use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use chrono::DateTime;
use streamline_core::{
    HarnessSpec, RunPhase, RunState, StreamlineConfig, SyncVerdict, TrioProfile,
};
use streamline_safe::RedactionPolicy;

use crate::{
    append_trajectory_jsonl, compute_tree_manifest, execute_conveyor_blocking,
    stage_readonly_inputs, unfreeze_readonly_dir, verify_cross_family, ConveyorError,
    DispatchRequest, TrajectoryExportInput,
};

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = unfreeze_readonly_dir(&self.path);
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn sh_harness(id: &str, family: &str, script: &str) -> HarnessSpec {
    HarnessSpec {
        id: id.to_string(),
        family: family.to_string(),
        command: vec!["/bin/sh".to_string(), "-c".to_string(), script.to_string()],
        max_concurrency: 1,
    }
}

fn base_config(runs_dir: &std::path::Path, harnesses: Vec<HarnessSpec>) -> StreamlineConfig {
    let allowed_root = runs_dir
        .parent()
        .unwrap_or(runs_dir)
        .to_string_lossy()
        .into_owned();
    StreamlineConfig {
        vault_sync_dir: allowed_root.clone(),
        runs_dir: runs_dir.to_string_lossy().into_owned(),
        allowed_input_roots: vec![allowed_root],
        public_safe_mode: true,
        harnesses,
        ..StreamlineConfig::default()
    }
}

#[test]
fn enforces_cross_family_on_r2_and_above() {
    assert!(verify_cross_family(2, "openai-codex", "openai-codex").is_err());
    assert!(verify_cross_family(2, "openai-codex", "OpenAI-Codex").is_err());
    assert!(verify_cross_family(2, "openai-codex", "google-gemini").is_ok());
    assert!(verify_cross_family(1, "openai-codex", "openai-codex").is_ok());
}

#[test]
fn readonly_staging_blocks_rogue_builder_mutation() {
    let tmp = TempDirGuard::new("conveyor-ro-stage");
    let src_dir = tmp.path.join("src_inputs");
    let nested_dir = src_dir.join("bundle");
    fs::create_dir_all(&nested_dir).expect("create nested input dir");

    let spec_path = src_dir.join("spec.md");
    fs::write(&spec_path, "Original specification\n").expect("write spec.md");

    let script_path = src_dir.join("tool.sh");
    fs::write(&script_path, "#!/bin/sh\nexit 0\n").expect("write tool.sh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
            .expect("chmod tool.sh");
    }

    fs::write(nested_dir.join("notes.txt"), "Bundle note\n").expect("write notes.txt");

    let direct_stage_dir = tmp.path.join("direct_stage");
    let manifest = stage_readonly_inputs(
        &[spec_path.clone(), script_path.clone(), nested_dir.clone()],
        &direct_stage_dir,
    )
    .expect("stage readonly inputs");
    assert_eq!(manifest.file_count, 3);

    let sums_path = direct_stage_dir.join("INPUTS").join("SHA256SUMS");
    let sums_text = fs::read_to_string(&sums_path).expect("read SHA256SUMS");
    assert!(sums_text.contains("  spec.md\n"));
    assert!(sums_text.contains("  tool.sh\n"));
    assert!(sums_text.contains("  bundle/notes.txt\n"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let inputs_mode = fs::metadata(direct_stage_dir.join("INPUTS"))
            .expect("inputs dir meta")
            .permissions()
            .mode()
            & 0o777;
        let spec_mode = fs::metadata(direct_stage_dir.join("INPUTS").join("spec.md"))
            .expect("spec meta")
            .permissions()
            .mode()
            & 0o777;
        let script_mode = fs::metadata(direct_stage_dir.join("INPUTS").join("tool.sh"))
            .expect("script meta")
            .permissions()
            .mode()
            & 0o777;
        let nested_mode = fs::metadata(direct_stage_dir.join("INPUTS").join("bundle"))
            .expect("nested dir meta")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(inputs_mode, 0o555);
        assert_eq!(nested_mode, 0o555);
        assert_eq!(spec_mode, 0o444);
        assert_eq!(script_mode, 0o555);
    }

    unfreeze_readonly_dir(&direct_stage_dir).expect("unfreeze staged dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let inputs_mode = fs::metadata(direct_stage_dir.join("INPUTS"))
            .expect("unfrozen inputs meta")
            .permissions()
            .mode()
            & 0o777;
        let spec_mode = fs::metadata(direct_stage_dir.join("INPUTS").join("spec.md"))
            .expect("unfrozen spec meta")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(inputs_mode, 0o755);
        assert_eq!(spec_mode, 0o644);
    }

    let runs_dir = tmp.path.join("runs");
    let rogue_builder = sh_harness(
        "rogue-builder",
        "openai-codex",
        "printf 'Tamper\\n' > \"$STREAMLINE_STAGE_DIR/INPUTS/spec.md\"",
    );
    let cfg = base_config(&runs_dir, vec![rogue_builder]);
    let req = DispatchRequest {
        project_id: "proj-ro-test".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 1,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("execute rogue builder run");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert!(outcome.accept_run.is_none());

    let staged_inputs = runs_dir
        .join(&outcome.build_run.run_id)
        .join("stage")
        .join("INPUTS");
    let observed_spec =
        fs::read_to_string(staged_inputs.join("spec.md")).expect("read staged spec");
    assert_eq!(observed_spec, "Original specification\n");

    let recomputed = compute_tree_manifest(&staged_inputs).expect("recompute staged manifest");
    assert_eq!(
        recomputed.manifest_sha256,
        outcome.build_run.input_manifest_sha256
    );
}

#[test]
fn full_conveyor_build_cleanup_scan_and_cross_family_accept_reaches_doc_pending() {
    let tmp = TempDirGuard::new("conveyor-full-e2e");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "# Design Spec\nBuild clean artifact.\n").expect("write spec.md");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "set -eu
         [ \"$STREAMLINE_PHASE\" = \"build\" ]
         [ \"$STREAMLINE_PROJECT_ID\" = \"proj-full\" ]
         cat \"$STREAMLINE_STAGE_DIR/INPUTS/spec.md\" > \"$STREAMLINE_OUT_DIR/artifact.md\"
         printf '\\nBuilt cleanly.\\n' >> \"$STREAMLINE_OUT_DIR/artifact.md\"",
    );

    let acceptor = sh_harness(
        "agy-main",
        "google-gemini",
        "set -eu
         [ \"$STREAMLINE_PHASE\" = \"accept\" ]
         [ \"$STREAMLINE_PROJECT_ID\" = \"proj-full\" ]
         test -f \"$STREAMLINE_BUILD_OUT_DIR/artifact.md\"
         if printf 'Tamper\\n' >> \"$STREAMLINE_BUILD_OUT_DIR/artifact.md\" 2>/dev/null; then
             exit 99
         fi
         printf 'ACCEPT_OK\\n' > \"$STREAMLINE_OUT_DIR/verdict.txt\"",
    );

    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![builder, acceptor]);
    let req = DispatchRequest {
        project_id: "proj-full".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("execute full conveyor");
    assert_eq!(outcome.final_state, RunState::DocPendingOwner);
    assert_eq!(outcome.build_run.phase, RunPhase::Build);
    assert_eq!(outcome.build_run.harness_id, "codex-main");
    assert_eq!(outcome.build_run.family, "openai-codex");
    assert!(outcome.build_run.cleanup_clean);
    assert!(outcome.build_run.scan_clean);
    assert_eq!(outcome.build_run.state, RunState::DocPendingOwner);
    assert!(!outcome.build_run.input_manifest_sha256.is_empty());
    assert!(!outcome.build_run.output_manifest_sha256.is_empty());
    assert!(outcome.escalation_markdown.is_none());

    let accept = outcome.accept_run.as_ref().expect("accept_run must exist");
    assert_eq!(accept.phase, RunPhase::Accept);
    assert_eq!(accept.harness_id, "agy-main");
    assert_eq!(accept.family, "google-gemini");
    assert!(accept.cleanup_clean);
    assert!(accept.scan_clean);
    assert_eq!(accept.state, RunState::DocPendingOwner);
    assert_eq!(
        accept.input_manifest_sha256,
        outcome.build_run.output_manifest_sha256
    );
    assert!(!accept.output_manifest_sha256.is_empty());

    let verdict_path = runs_dir
        .join(&outcome.build_run.run_id)
        .join("out_accept")
        .join("verdict.txt");
    assert_eq!(
        fs::read_to_string(verdict_path).expect("read verdict"),
        "ACCEPT_OK\n"
    );
}

#[test]
fn dirty_work_dir_quarantines_run_before_acceptance() {
    let tmp = TempDirGuard::new("conveyor-dirty-work");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "mkdir -p \"$STREAMLINE_WORK_DIR/target\"
         printf 'binary' > \"$STREAMLINE_WORK_DIR/target/artifact\"
         printf 'ok\\n' > \"$STREAMLINE_OUT_DIR/result.txt\"",
    );

    let acceptor = sh_harness(
        "agy-main",
        "google-gemini",
        "printf 'ran\\n' > \"$STREAMLINE_OUT_DIR/unexpected.txt\"",
    );

    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![builder, acceptor]);
    let req = DispatchRequest {
        project_id: "proj-dirty".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("execute dirty build");
    assert_eq!(outcome.final_state, RunState::Quarantined);
    assert_eq!(outcome.build_run.state, RunState::Quarantined);
    assert!(!outcome.build_run.cleanup_clean);
    assert!(outcome.accept_run.is_none());

    let out_accept_dir = runs_dir.join(&outcome.build_run.run_id).join("out_accept");
    assert!(!out_accept_dir.exists());
}

#[test]
fn two_strike_builder_failure_records_impasse_and_escalation() {
    let tmp = TempDirGuard::new("conveyor-two-strike");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "printf 'attempt\\n' >> \"$STREAMLINE_WORK_DIR/attempts.log\"
         exit 7",
    );

    let acceptor = sh_harness("agy-main", "google-gemini", "exit 0");
    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![builder, acceptor]);
    let req = DispatchRequest {
        project_id: "proj-two-strike".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("execute failing builder");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert_eq!(outcome.build_run.state, RunState::Impasse);
    assert!(outcome.accept_run.is_none());

    let escalation = outcome
        .escalation_markdown
        .as_ref()
        .expect("escalation markdown present");
    assert!(escalation.contains("BUILDER_TWO_STRIKE_FAILURE"));
    assert!(escalation.contains("exit code 7"));

    let run_dir = runs_dir.join(&outcome.build_run.run_id);
    let escalation_file =
        fs::read_to_string(run_dir.join("out").join("ESCALATION.md")).expect("read ESCALATION.md");
    assert_eq!(&escalation_file, escalation);

    let attempts_log =
        fs::read_to_string(run_dir.join("work").join("attempts.log")).expect("read attempts.log");
    assert_eq!(attempts_log.lines().count(), 2);
}

#[test]
fn trajectory_export_writes_verified_jsonl_and_blocks_unsafe_text() {
    let tmp = TempDirGuard::new("conveyor-trajectory");
    let runs_dir = tmp.path.join("runs");
    let policy = RedactionPolicy {
        public_safe_mode: true,
    };

    let input_one = TrajectoryExportInput {
        project_id: "proj-alpha".to_string(),
        title: "Alpha Release".to_string(),
        profile: TrioProfile::P2,
        own_statements: vec!["OWN-001: Keep staging immutable".to_string()],
        div_summaries: vec!["DIV-001: Accepted minor copy adjustment".to_string()],
        lesson_rules: vec!["Always run cleanup check before acceptance".to_string()],
        sync_verdict: Some(SyncVerdict::Pass),
        how_it_works_sha256: Some("a".repeat(64)),
    };

    let path_one = append_trajectory_jsonl(&runs_dir, &policy, &input_one)
        .expect("append first trajectory line");
    assert_eq!(path_one, runs_dir.join("trajectories.jsonl"));

    let input_two = TrajectoryExportInput {
        project_id: "proj-beta".to_string(),
        title: "Beta Fix".to_string(),
        profile: TrioProfile::P0,
        own_statements: vec![],
        div_summaries: vec![],
        lesson_rules: vec![],
        sync_verdict: None,
        how_it_works_sha256: None,
    };
    append_trajectory_jsonl(&runs_dir, &policy, &input_two).expect("append second trajectory line");

    let raw_contents = fs::read_to_string(&path_one).expect("read trajectories.jsonl");
    let lines: Vec<&str> = raw_contents.lines().collect();
    assert_eq!(lines.len(), 2);

    let parsed: serde_json::Value = serde_json::from_str(lines[0]).expect("parse first JSONL line");
    assert_eq!(parsed["project_id"], "proj-alpha");
    assert_eq!(parsed["profile"], "P2");
    assert_eq!(parsed["sync_verdict"], "pass");
    let exported_at = parsed["exported_at"]
        .as_str()
        .expect("exported_at string present");
    DateTime::parse_from_rfc3339(exported_at).expect("valid RFC3339 timestamp");

    let bad_cred = ["ghp_", "Demo123456AbCdEf"].concat();
    let unsafe_cred_input = TrajectoryExportInput {
        project_id: "proj-unsafe".to_string(),
        title: "Unsafe".to_string(),
        profile: TrioProfile::P1,
        own_statements: vec![format!("credential={bad_cred}")],
        div_summaries: vec![],
        lesson_rules: vec![],
        sync_verdict: None,
        how_it_works_sha256: None,
    };
    let cred_err = append_trajectory_jsonl(&runs_dir, &policy, &unsafe_cred_input)
        .expect_err("expected redaction rejection");
    assert!(matches!(cred_err, ConveyorError::RedactionBlocked(_)));
    assert!(!cred_err.to_string().contains("Demo123456AbCdEf"));

    let bad_ip = format!("{}.{}.{}.{}", 192, 168, 50, 12);
    let unsafe_ip_input = TrajectoryExportInput {
        project_id: "proj-unsafe-ip".to_string(),
        title: "Unsafe IP".to_string(),
        profile: TrioProfile::P1,
        own_statements: vec![format!("endpoint={bad_ip}")],
        div_summaries: vec![],
        lesson_rules: vec![],
        sync_verdict: None,
        how_it_works_sha256: None,
    };
    let ip_err = append_trajectory_jsonl(&runs_dir, &policy, &unsafe_ip_input)
        .expect_err("expected private IP rejection");
    assert!(matches!(ip_err, ConveyorError::RedactionBlocked(_)));
    assert!(!ip_err.to_string().contains(&bad_ip));

    let after_contents = fs::read_to_string(&path_one).expect("re-read trajectories.jsonl");
    assert_eq!(after_contents.lines().count(), 2);
}

#[test]
fn missing_input_records_impasse_without_builder_strike() {
    let tmp = TempDirGuard::new("conveyor-missing-input");
    let missing_path = tmp.path.join("does-not-exist.md");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "printf 'ran\\n' > \"$STREAMLINE_WORK_DIR/ran.flag\"",
    );
    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![builder]);
    let req = DispatchRequest {
        project_id: "proj-missing".to_string(),
        input_paths: vec![missing_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 1,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("missing input returns Impasse");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert_eq!(outcome.build_run.state, RunState::Impasse);
    assert!(outcome.accept_run.is_none());

    let escalation = outcome
        .escalation_markdown
        .as_ref()
        .expect("escalation markdown present");
    assert!(escalation.contains("MISSING_INPUT"));

    let run_dir = runs_dir.join(&outcome.build_run.run_id);
    assert!(!run_dir.join("work").join("ran.flag").exists());
    assert!(run_dir.join("out").join("ESCALATION.md").exists());
}

#[test]
fn redaction_hit_in_build_output_records_impasse_without_leaking() {
    let tmp = TempDirGuard::new("conveyor-scan-hit");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let leaked_sample = ["ghp_", "AbCdEf123456GhIj"].concat();
    let script = format!(
        "printf '%s\\n' '{leaked_sample}' > \"$STREAMLINE_OUT_DIR/report.txt\"
         printf '%s\\n' '{leaked_sample}' > \"$STREAMLINE_OUT_DIR/{leaked_sample}.txt\""
    );
    let builder = sh_harness("codex-main", "openai-codex", &script);
    let acceptor = sh_harness("agy-main", "google-gemini", "exit 0");

    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![builder, acceptor]);
    let req = DispatchRequest {
        project_id: "proj-scan-hit".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("execute redaction hit run");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert!(!outcome.build_run.scan_clean);
    assert!(outcome.accept_run.is_none());

    let escalation = outcome
        .escalation_markdown
        .as_ref()
        .expect("escalation markdown present");
    assert!(escalation.contains("CredentialPrefix"));
    assert!(escalation.contains("report.txt"));
    assert!(escalation.contains("redacted-path-"));
    assert!(!escalation.contains(&leaked_sample));

    // Also verify clean file content under a sensitive filename is caught and redacted in ESCALATION.md.
    let name_only_script =
        format!("printf 'clean\\n' > \"$STREAMLINE_OUT_DIR/{leaked_sample}_clean.txt\"");
    let name_builder = sh_harness("codex-main", "openai-codex", &name_only_script);
    let name_acceptor = sh_harness("agy-main", "google-gemini", "exit 0");
    let runs_name_dir = tmp.path.join("runs_name_only");
    let spec_path_2 = tmp.path.join("spec2.md");
    fs::write(&spec_path_2, "Input\n").expect("write spec2.md");
    let cfg_name = base_config(&runs_name_dir, vec![name_builder, name_acceptor]);
    let req_name = DispatchRequest {
        project_id: "proj-scan-name".to_string(),
        input_paths: vec![spec_path_2],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };
    let name_outcome =
        execute_conveyor_blocking(&cfg_name, &req_name).expect("execute filename scan hit");
    assert_eq!(name_outcome.final_state, RunState::Impasse);
    assert!(!name_outcome.build_run.scan_clean);
    assert!(name_outcome.accept_run.is_none());
    let name_esc = name_outcome
        .escalation_markdown
        .as_ref()
        .expect("filename escalation present");
    assert!(name_esc.contains("redacted-path-"));
    assert!(!name_esc.contains(&leaked_sample));
}

#[test]
fn same_family_rejected_upfront_for_r2_plus() {
    let tmp = TempDirGuard::new("conveyor-same-family");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let h1 = sh_harness("codex-a", "openai-codex", "exit 0");
    let h2 = sh_harness("codex-b", "OpenAI-Codex", "exit 0");
    let runs_dir = tmp.path.join("runs");

    let cfg_two = base_config(&runs_dir, vec![h1.clone(), h2]);
    let req = DispatchRequest {
        project_id: "proj-same-fam".to_string(),
        input_paths: vec![spec_path.clone()],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };
    let err = execute_conveyor_blocking(&cfg_two, &req).expect_err("expected SameFamilyForbidden");
    assert!(matches!(err, ConveyorError::SameFamilyForbidden(_)));

    let cfg_one = base_config(&runs_dir, vec![h1]);
    let err_one =
        execute_conveyor_blocking(&cfg_one, &req).expect_err("expected SameFamilyForbidden");
    assert!(matches!(err_one, ConveyorError::SameFamilyForbidden(_)));

    let req_r1 = DispatchRequest {
        project_id: "proj-r1".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 1,
        caller_has_wildcard_scope: true,
    };
    let ok_r1 = execute_conveyor_blocking(&cfg_one, &req_r1).expect("R1 single harness succeeds");
    assert_eq!(ok_r1.final_state, RunState::DocPendingOwner);
    assert!(ok_r1.accept_run.is_none());
}

#[test]
fn scrubbed_environment_excludes_unrelated_parent_vars() {
    let tmp = TempDirGuard::new("conveyor-scrub-env");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "env | cut -d= -f1 > \"$STREAMLINE_OUT_DIR/env_names.txt\"",
    );
    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![builder]);
    let req = DispatchRequest {
        project_id: "proj-env".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 0,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("execute scrubbed env check");
    assert_eq!(outcome.final_state, RunState::DocPendingOwner);

    let env_names_text = fs::read_to_string(
        runs_dir
            .join(&outcome.build_run.run_id)
            .join("out")
            .join("env_names.txt"),
    )
    .expect("read env_names.txt");

    let names: HashSet<String> = env_names_text.lines().map(str::to_string).collect();
    for required in [
        "STREAMLINE_PROJECT_ID",
        "STREAMLINE_PHASE",
        "STREAMLINE_STAGE_DIR",
        "STREAMLINE_WORK_DIR",
        "STREAMLINE_OUT_DIR",
    ] {
        assert!(names.contains(required), "missing env var {required}");
    }

    let safe_parent_vars: HashSet<&str> = ["PATH", "HOME", "TMPDIR", "LANG"].into_iter().collect();
    for (parent_var, _) in std::env::vars() {
        if !safe_parent_vars.contains(parent_var.as_str())
            && !parent_var.starts_with("STREAMLINE_")
            && !matches!(parent_var.as_str(), "PWD" | "SHLVL" | "_" | "LC_CTYPE")
        {
            assert!(
                !names.contains(&parent_var),
                "unrelated parent env var leaked to child process: {parent_var}"
            );
        }
    }
}

#[test]
fn acceptor_nonzero_exit_records_impasse_and_escalation() {
    let tmp = TempDirGuard::new("conveyor-accept-fail");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "printf 'built\\n' > \"$STREAMLINE_OUT_DIR/out.txt\"",
    );
    let acceptor = sh_harness(
        "agy-main",
        "google-gemini",
        "printf 'reject\\n' > \"$STREAMLINE_OUT_DIR/notes.txt\"
         exit 3",
    );

    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![builder, acceptor]);
    let req = DispatchRequest {
        project_id: "proj-accept-fail".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("execute acceptor failure run");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert_eq!(outcome.build_run.state, RunState::Accepting);
    let accept = outcome.accept_run.as_ref().expect("accept_run present");
    assert_eq!(accept.state, RunState::Impasse);

    let escalation = outcome
        .escalation_markdown
        .as_ref()
        .expect("escalation markdown present");
    assert!(escalation.contains("ACCEPTOR_REJECTED"));
    assert!(escalation.contains("exit code 3"));

    let escalation_path = runs_dir
        .join(&outcome.build_run.run_id)
        .join("out_accept")
        .join("ESCALATION.md");
    assert_eq!(
        fs::read_to_string(escalation_path).expect("read accept ESCALATION.md"),
        *escalation
    );
}

#[test]
fn r2_preferred_same_family_acceptor_rejected_and_r1_allowed() {
    let tmp = TempDirGuard::new("conveyor-pref-acceptor");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let h1 = sh_harness(
        "codex-a",
        "openai-codex",
        "printf 'ok\\n' > \"$STREAMLINE_OUT_DIR/a.txt\"",
    );
    let h2 = sh_harness(
        "codex-b",
        "openai-codex",
        "printf 'accept\\n' > \"$STREAMLINE_OUT_DIR/b.txt\"",
    );
    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![h1, h2]);

    let req_r2 = DispatchRequest {
        project_id: "proj-pref-r2".to_string(),
        input_paths: vec![spec_path.clone()],
        preferred_builder_harness: Some("codex-a".to_string()),
        preferred_acceptor_harness: Some("codex-b".to_string()),
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };
    let err = execute_conveyor_blocking(&cfg, &req_r2).expect_err("R2 same-family rejected");
    assert!(matches!(err, ConveyorError::SameFamilyForbidden(_)));

    let req_r1 = DispatchRequest {
        project_id: "proj-pref-r1".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: Some("codex-a".to_string()),
        preferred_acceptor_harness: Some("codex-b".to_string()),
        risk_band: 1,
        caller_has_wildcard_scope: true,
    };
    let ok = execute_conveyor_blocking(&cfg, &req_r1).expect("R1 same-family allowed");
    assert_eq!(ok.final_state, RunState::DocPendingOwner);
    assert_eq!(
        ok.accept_run
            .as_ref()
            .expect("accept_run present")
            .harness_id,
        "codex-b"
    );
}

#[test]
fn input_path_outside_allowed_roots_is_blocked_at_impasse() {
    let tmp = TempDirGuard::new("conveyor-outside-root");
    let allowed_dir = tmp.path.join("allowed_inputs");
    let outside_dir = tmp.path.join("outside_inputs");
    let vault_dir = tmp.path.join("vault");
    let runs_dir = tmp.path.join("runs");
    fs::create_dir_all(&allowed_dir).expect("create allowed_inputs");
    fs::create_dir_all(&outside_dir).expect("create outside_inputs");
    fs::create_dir_all(&vault_dir).expect("create vault");

    let outside_file = outside_dir.join("outside.txt");
    fs::write(&outside_file, "OUTSIDE_PAYLOAD_DO_NOT_COPY\n").expect("write outside file");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "printf 'ran\\n' > \"$STREAMLINE_WORK_DIR/ran.flag\"",
    );
    let cfg = StreamlineConfig {
        vault_sync_dir: vault_dir.to_string_lossy().into_owned(),
        runs_dir: runs_dir.to_string_lossy().into_owned(),
        allowed_input_roots: vec![allowed_dir.to_string_lossy().into_owned()],
        public_safe_mode: true,
        harnesses: vec![builder],
        ..StreamlineConfig::default()
    };
    let req = DispatchRequest {
        project_id: "proj-outside-root".to_string(),
        input_paths: vec![outside_file],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 1,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("outside root returns Impasse");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert_eq!(outcome.build_run.state, RunState::Impasse);
    assert!(outcome.accept_run.is_none());

    let escalation = outcome
        .escalation_markdown
        .as_ref()
        .expect("escalation markdown present");
    assert!(escalation.contains("INPUT_OUTSIDE_ALLOWED_ROOT"));

    let run_dir = runs_dir.join(&outcome.build_run.run_id);
    assert!(!run_dir.join("stage").join("INPUTS").exists());
    assert!(!run_dir.join("work").join("ran.flag").exists());
    assert!(run_dir.join("out").join("ESCALATION.md").exists());
}

#[cfg(unix)]
#[test]
fn symlink_input_path_is_rejected_without_copying() {
    use std::os::unix::fs::symlink;

    let tmp = TempDirGuard::new("conveyor-symlink-input");
    let allowed_dir = tmp.path.join("allowed_inputs");
    let outside_dir = tmp.path.join("outside_inputs");
    let vault_dir = tmp.path.join("vault");
    let runs_dir = tmp.path.join("runs");
    fs::create_dir_all(&allowed_dir).expect("create allowed_inputs");
    fs::create_dir_all(&outside_dir).expect("create outside_inputs");
    fs::create_dir_all(&vault_dir).expect("create vault");

    let outside_file = outside_dir.join("outside.txt");
    fs::write(&outside_file, "SYMLINK_TARGET_DO_NOT_COPY\n").expect("write outside file");

    let link_path = allowed_dir.join("link.txt");
    symlink(&outside_file, &link_path).expect("create symlink input");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "printf 'ran\\n' > \"$STREAMLINE_WORK_DIR/ran.flag\"",
    );
    let cfg = StreamlineConfig {
        vault_sync_dir: vault_dir.to_string_lossy().into_owned(),
        runs_dir: runs_dir.to_string_lossy().into_owned(),
        allowed_input_roots: vec![allowed_dir.to_string_lossy().into_owned()],
        public_safe_mode: true,
        harnesses: vec![builder],
        ..StreamlineConfig::default()
    };
    let req = DispatchRequest {
        project_id: "proj-symlink-file".to_string(),
        input_paths: vec![link_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 1,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("symlink input returns Impasse");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert!(outcome.accept_run.is_none());
    let escalation = outcome
        .escalation_markdown
        .as_ref()
        .expect("escalation markdown present");
    assert!(escalation.contains("SYMLINK_INPUT_REJECTED"));

    let run_dir = runs_dir.join(&outcome.build_run.run_id);
    assert!(!run_dir.join("stage").join("INPUTS").exists());
    assert!(!run_dir.join("work").join("ran.flag").exists());

    let bundle_dir = allowed_dir.join("bundle");
    fs::create_dir_all(&bundle_dir).expect("create bundle dir");
    fs::write(bundle_dir.join("clean.txt"), "clean\n").expect("write clean.txt");
    symlink(&outside_file, bundle_dir.join("escape.txt")).expect("create nested symlink");

    let req_bundle = DispatchRequest {
        project_id: "proj-symlink-dir".to_string(),
        input_paths: vec![bundle_dir],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 1,
        caller_has_wildcard_scope: true,
    };
    let bundle_outcome =
        execute_conveyor_blocking(&cfg, &req_bundle).expect("nested symlink returns Impasse");
    assert_eq!(bundle_outcome.final_state, RunState::Impasse);
    assert!(bundle_outcome
        .escalation_markdown
        .as_deref()
        .unwrap_or_default()
        .contains("SYMLINK_INPUT_REJECTED"));
    let bundle_run_dir = runs_dir.join(&bundle_outcome.build_run.run_id);
    assert!(!bundle_run_dir.join("stage").join("INPUTS").exists());
}

#[cfg(unix)]
#[test]
fn trajectory_export_refuses_symlinked_target_and_runs_dir() {
    use std::os::unix::fs::symlink;

    let tmp = TempDirGuard::new("conveyor-trajectory-symlink");
    let victim_file = tmp.path.join("victim.txt");
    fs::write(&victim_file, "VICTIM_ORIGINAL_DATA\n").expect("write victim file");

    let runs_dir = tmp.path.join("runs");
    fs::create_dir_all(&runs_dir).expect("create runs_dir");
    let symlink_target = runs_dir.join("trajectories.jsonl");
    symlink(&victim_file, &symlink_target).expect("plant symlink at trajectories.jsonl");

    let policy = RedactionPolicy {
        public_safe_mode: true,
    };
    let input = TrajectoryExportInput {
        project_id: "proj-symlink-traj".to_string(),
        title: "Symlink Guard".to_string(),
        profile: TrioProfile::P1,
        own_statements: vec!["OWN-001: Safe check".to_string()],
        div_summaries: vec![],
        lesson_rules: vec![],
        sync_verdict: Some(SyncVerdict::Pass),
        how_it_works_sha256: None,
    };

    let err_target = append_trajectory_jsonl(&runs_dir, &policy, &input)
        .expect_err("symlinked trajectories.jsonl must fail");
    assert!(matches!(err_target, ConveyorError::StageFailure(_)));
    assert_eq!(
        fs::read_to_string(&victim_file).expect("read victim file"),
        "VICTIM_ORIGINAL_DATA\n"
    );

    let outside_runs = tmp.path.join("outside_runs");
    fs::create_dir_all(&outside_runs).expect("create outside_runs");
    let symlink_runs_dir = tmp.path.join("runs_link");
    symlink(&outside_runs, &symlink_runs_dir).expect("plant symlink at runs_dir");

    let err_runs_dir = append_trajectory_jsonl(&symlink_runs_dir, &policy, &input)
        .expect_err("symlinked runs_dir must fail");
    assert!(matches!(err_runs_dir, ConveyorError::StageFailure(_)));
    assert!(!outside_runs.join("trajectories.jsonl").exists());
}

#[test]
fn project_scoped_dispatch_restricts_inputs_to_project_subdir() {
    let tmp = TempDirGuard::new("conveyor-project-scope");
    let allowed_root = tmp.path.join("allowed_root");
    let proj_a_dir = allowed_root.join("projects").join("proj-a");
    let proj_b_dir = allowed_root.join("projects").join("proj-b");
    let vault_dir = tmp.path.join("vault");
    let runs_dir = tmp.path.join("runs");

    fs::create_dir_all(&proj_a_dir).expect("create proj-a dir");
    fs::create_dir_all(&proj_b_dir).expect("create proj-b dir");
    fs::create_dir_all(&vault_dir).expect("create vault dir");
    fs::create_dir_all(&runs_dir).expect("create runs dir");

    let proj_a_spec = proj_a_dir.join("spec.md");
    let proj_b_spec = proj_b_dir.join("spec.md");
    let root_level_spec = allowed_root.join("shared.md");
    fs::write(&proj_a_spec, "Project A specification\n").expect("write proj-a spec");
    fs::write(&proj_b_spec, "Project B private payload\n").expect("write proj-b spec");
    fs::write(&root_level_spec, "Top-level shared payload\n").expect("write shared.md");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "set -eu
         printf 'ran\\n' > \"$STREAMLINE_WORK_DIR/ran.flag\"
         cat \"$STREAMLINE_STAGE_DIR/INPUTS/spec.md\" > \"$STREAMLINE_OUT_DIR/artifact.md\"",
    );

    let cfg = StreamlineConfig {
        vault_sync_dir: vault_dir.to_string_lossy().into_owned(),
        runs_dir: runs_dir.to_string_lossy().into_owned(),
        allowed_input_roots: vec![allowed_root.to_string_lossy().into_owned()],
        public_safe_mode: true,
        harnesses: vec![builder],
        ..StreamlineConfig::default()
    };

    let cross_project_req = DispatchRequest {
        project_id: "proj-a".to_string(),
        input_paths: vec![proj_b_spec],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 1,
        caller_has_wildcard_scope: false,
    };
    let blocked_outcome = execute_conveyor_blocking(&cfg, &cross_project_req)
        .expect("cross-project input returns Impasse");
    assert_eq!(blocked_outcome.final_state, RunState::Impasse);
    assert_eq!(blocked_outcome.build_run.state, RunState::Impasse);
    assert!(blocked_outcome.accept_run.is_none());
    let escalation = blocked_outcome
        .escalation_markdown
        .as_deref()
        .expect("escalation markdown present");
    assert!(escalation.contains("INPUT_OUTSIDE_ALLOWED_ROOT"));

    let blocked_run_dir = runs_dir.join(&blocked_outcome.build_run.run_id);
    assert!(!blocked_run_dir.join("stage").join("INPUTS").exists());
    assert!(!blocked_run_dir.join("work").join("ran.flag").exists());

    let root_level_req = DispatchRequest {
        project_id: "proj-a".to_string(),
        input_paths: vec![root_level_spec],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 1,
        caller_has_wildcard_scope: false,
    };
    let root_level_outcome = execute_conveyor_blocking(&cfg, &root_level_req)
        .expect("root-level input outside project subdir returns Impasse");
    assert_eq!(root_level_outcome.final_state, RunState::Impasse);
    assert!(root_level_outcome
        .escalation_markdown
        .as_deref()
        .unwrap_or_default()
        .contains("INPUT_OUTSIDE_ALLOWED_ROOT"));
    assert!(!runs_dir
        .join(&root_level_outcome.build_run.run_id)
        .join("stage")
        .join("INPUTS")
        .exists());

    let traversal_req = DispatchRequest {
        project_id: "../proj-b".to_string(),
        input_paths: vec![proj_a_spec.clone()],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 1,
        caller_has_wildcard_scope: false,
    };
    let traversal_err = execute_conveyor_blocking(&cfg, &traversal_req)
        .expect_err("project_id with traversal must be rejected");
    assert!(matches!(traversal_err, ConveyorError::InvalidConfig(_)));

    let allowed_req = DispatchRequest {
        project_id: "proj-a".to_string(),
        input_paths: vec![proj_a_spec],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 1,
        caller_has_wildcard_scope: false,
    };
    let ok_outcome =
        execute_conveyor_blocking(&cfg, &allowed_req).expect("own project input succeeds");
    assert_eq!(ok_outcome.final_state, RunState::DocPendingOwner);
    assert!(ok_outcome.escalation_markdown.is_none());
    let artifact_path = runs_dir
        .join(&ok_outcome.build_run.run_id)
        .join("out")
        .join("artifact.md");
    assert_eq!(
        fs::read_to_string(artifact_path).expect("read artifact.md"),
        "Project A specification\n"
    );
}

#[cfg(unix)]
#[test]
fn intermediate_symlink_under_allowed_root_is_rejected() {
    use std::os::unix::fs::symlink;

    let tmp = TempDirGuard::new("conveyor-intermediate-symlink");
    let allowed_dir = tmp.path.join("allowed_inputs");
    let real_sub = allowed_dir.join("real_sub");
    let outside_dir = tmp.path.join("outside_inputs");
    let vault_dir = tmp.path.join("vault");
    let runs_dir = tmp.path.join("runs");

    fs::create_dir_all(&real_sub).expect("create real_sub");
    fs::create_dir_all(&outside_dir).expect("create outside_dir");
    fs::create_dir_all(&vault_dir).expect("create vault_dir");
    fs::create_dir_all(&runs_dir).expect("create runs_dir");

    fs::write(real_sub.join("inside.txt"), "INSIDE_DATA\n").expect("write inside.txt");
    fs::write(outside_dir.join("outside.txt"), "OUTSIDE_DATA\n").expect("write outside.txt");

    symlink(&real_sub, allowed_dir.join("internal_link")).expect("create internal symlink");
    symlink(&outside_dir, allowed_dir.join("escape_link")).expect("create escape symlink");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "printf 'ran\\n' > \"$STREAMLINE_WORK_DIR/ran.flag\"",
    );
    let cfg = StreamlineConfig {
        vault_sync_dir: vault_dir.to_string_lossy().into_owned(),
        runs_dir: runs_dir.to_string_lossy().into_owned(),
        allowed_input_roots: vec![allowed_dir.to_string_lossy().into_owned()],
        public_safe_mode: true,
        harnesses: vec![builder],
        ..StreamlineConfig::default()
    };

    for bad_input in [
        allowed_dir.join("internal_link").join("inside.txt"),
        allowed_dir.join("escape_link").join("outside.txt"),
    ] {
        let req = DispatchRequest {
            project_id: "proj-inter-sym".to_string(),
            input_paths: vec![bad_input],
            preferred_builder_harness: None,
            preferred_acceptor_harness: None,
            risk_band: 1,
            caller_has_wildcard_scope: true,
        };
        let outcome =
            execute_conveyor_blocking(&cfg, &req).expect("intermediate symlink returns Impasse");
        assert_eq!(outcome.final_state, RunState::Impasse);
        assert!(outcome
            .escalation_markdown
            .as_deref()
            .unwrap_or_default()
            .contains("SYMLINK_INPUT_REJECTED"));
        let run_dir = runs_dir.join(&outcome.build_run.run_id);
        assert!(!run_dir.join("stage").join("INPUTS").exists());
        assert!(!run_dir.join("work").join("ran.flag").exists());
    }
}

#[cfg(unix)]
#[test]
fn trajectory_export_refuses_intermediate_symlinked_ancestor() {
    use std::os::unix::fs::symlink;

    let tmp = TempDirGuard::new("conveyor-traj-anc-symlink");
    let outside = tmp.path.join("outside");
    let existing_nested = outside.join("nested");
    fs::create_dir_all(&existing_nested).expect("create outside/nested");

    let ancestor_link = tmp.path.join("ancestor_link");
    symlink(&outside, &ancestor_link).expect("create ancestor_link");

    let policy = RedactionPolicy {
        public_safe_mode: true,
    };
    let input = TrajectoryExportInput {
        project_id: "proj-anc-symlink".to_string(),
        title: "Ancestor Symlink Guard".to_string(),
        profile: TrioProfile::P1,
        own_statements: vec!["OWN-001: Ancestor guard".to_string()],
        div_summaries: vec![],
        lesson_rules: vec![],
        sync_verdict: Some(SyncVerdict::Pass),
        how_it_works_sha256: None,
    };

    let runs_under_existing = ancestor_link.join("nested").join("runs");
    let err_existing = append_trajectory_jsonl(&runs_under_existing, &policy, &input)
        .expect_err("intermediate symlinked ancestor must fail");
    assert!(matches!(err_existing, ConveyorError::StageFailure(_)));
    assert!(!existing_nested.join("runs").exists());

    let runs_under_missing = ancestor_link.join("uncreated").join("runs");
    let err_missing = append_trajectory_jsonl(&runs_under_missing, &policy, &input)
        .expect_err("missing nested dir under symlinked ancestor must fail");
    assert!(matches!(err_missing, ConveyorError::StageFailure(_)));
    assert!(!outside.join("uncreated").exists());
}

#[test]
fn oversized_harness_output_records_quota_impasse() {
    let tmp = TempDirGuard::new("conveyor-oversized-out");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "dd if=/dev/zero of=\"$STREAMLINE_OUT_DIR/big.bin\" bs=1048576 count=5 2>/dev/null",
    );
    let acceptor = sh_harness("agy-main", "google-gemini", "exit 0");

    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![builder, acceptor]);
    let req = DispatchRequest {
        project_id: "proj-oversized-out".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("oversized output returns Impasse");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert_eq!(outcome.build_run.state, RunState::Impasse);
    assert!(!outcome.build_run.scan_clean);
    assert!(outcome.accept_run.is_none());

    let escalation = outcome
        .escalation_markdown
        .as_ref()
        .expect("escalation markdown present");
    assert!(escalation.contains("OUTPUT_QUOTA_EXCEEDED"));

    let escalation_path = runs_dir
        .join(&outcome.build_run.run_id)
        .join("out")
        .join("ESCALATION.md");
    assert_eq!(
        fs::read_to_string(escalation_path).expect("read ESCALATION.md"),
        *escalation
    );
}

#[test]
fn too_many_output_files_records_quota_impasse() {
    let tmp = TempDirGuard::new("conveyor-too-many-out");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let builder = sh_harness(
        "codex-main",
        "openai-codex",
        "i=1
         while [ \"$i\" -le 513 ]; do
             : > \"$STREAMLINE_OUT_DIR/f_$i.txt\"
             i=$((i + 1))
         done",
    );
    let acceptor = sh_harness("agy-main", "google-gemini", "exit 0");

    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![builder, acceptor]);
    let req = DispatchRequest {
        project_id: "proj-many-files".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };

    let outcome =
        execute_conveyor_blocking(&cfg, &req).expect("too many output files returns Impasse");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert_eq!(outcome.build_run.state, RunState::Impasse);
    assert!(!outcome.build_run.scan_clean);
    assert!(outcome.accept_run.is_none());

    let escalation = outcome
        .escalation_markdown
        .as_ref()
        .expect("escalation markdown present");
    assert!(escalation.contains("OUTPUT_QUOTA_EXCEEDED"));
}

#[test]
fn redaction_scan_caps_findings_at_max_scan_findings() {
    let tmp = TempDirGuard::new("conveyor-cap-findings");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let leaked_sample = ["ghp_", "AbCdEf123456GhIj"].concat();
    let script = format!(
        "i=1; while [ \"$i\" -le 100 ]; do printf '%s\\n' '{leaked_sample}' >> \"$STREAMLINE_OUT_DIR/noisy.txt\"; i=$((i + 1)); done"
    );
    let builder = sh_harness("codex-main", "openai-codex", &script);
    let acceptor = sh_harness("agy-main", "google-gemini", "exit 0");

    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![builder, acceptor]);
    let req = DispatchRequest {
        project_id: "proj-cap-findings".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("capped findings returns Impasse");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert!(!outcome.build_run.scan_clean);

    let escalation = outcome
        .escalation_markdown
        .as_ref()
        .expect("escalation markdown present");
    assert!(escalation.contains("REDACTION_SCAN_FAILED"));
    let finding_lines = escalation
        .lines()
        .filter(|line| line.starts_with("- `noisy.txt` "))
        .count();
    assert_eq!(finding_lines, crate::runner::MAX_SCAN_FINDINGS);
}

#[test]
fn compute_tree_manifest_enforces_file_count_limit() {
    let tmp = TempDirGuard::new("conveyor-manifest-cap");
    for idx in 0..=crate::staging::MAX_MANIFEST_FILES {
        fs::write(tmp.path.join(format!("item_{idx:04}.txt")), b"x").expect("write manifest file");
    }

    let err = compute_tree_manifest(&tmp.path)
        .expect_err("compute_tree_manifest must reject > MAX_MANIFEST_FILES");
    assert!(matches!(err, ConveyorError::StageFailure(msg) if msg.contains("exceeds maximum")));
}

#[test]
fn high_fanout_and_deep_output_trees_record_quota_impasse() {
    let tmp = TempDirGuard::new("conveyor-fanout-depth");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let fanout_builder = sh_harness(
        "codex-main",
        "openai-codex",
        "dirs=\"\"
         i=1
         while [ \"$i\" -le 513 ]; do
             dirs=\"$dirs d_$i\"
             i=$((i + 1))
         done
         cd \"$STREAMLINE_OUT_DIR\" && mkdir $dirs",
    );
    let acceptor = sh_harness("agy-main", "google-gemini", "exit 0");

    let runs_dir = tmp.path.join("runs_fanout");
    let cfg_fanout = base_config(&runs_dir, vec![fanout_builder, acceptor.clone()]);
    let req_fanout = DispatchRequest {
        project_id: "proj-fanout".to_string(),
        input_paths: vec![spec_path.clone()],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };

    let fanout_outcome =
        execute_conveyor_blocking(&cfg_fanout, &req_fanout).expect("fanout returns Impasse");
    assert_eq!(fanout_outcome.final_state, RunState::Impasse);
    assert_eq!(fanout_outcome.build_run.state, RunState::Impasse);
    assert!(!fanout_outcome.build_run.scan_clean);
    assert!(fanout_outcome.accept_run.is_none());
    let fanout_esc = fanout_outcome
        .escalation_markdown
        .as_ref()
        .expect("fanout escalation present");
    assert!(fanout_esc.contains("OUTPUT_QUOTA_EXCEEDED"));

    let deep_builder = sh_harness(
        "codex-main",
        "openai-codex",
        "p=\"$STREAMLINE_OUT_DIR\"
         i=1
         while [ \"$i\" -le 34 ]; do
             p=\"$p/d$i\"
             i=$((i + 1))
         done
         mkdir -p \"$p\"",
    );
    let runs_deep_dir = tmp.path.join("runs_deep");
    let cfg_deep = base_config(&runs_deep_dir, vec![deep_builder, acceptor]);
    let req_deep = DispatchRequest {
        project_id: "proj-deep".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };

    let deep_outcome =
        execute_conveyor_blocking(&cfg_deep, &req_deep).expect("deep nesting returns Impasse");
    assert_eq!(deep_outcome.final_state, RunState::Impasse);
    assert_eq!(deep_outcome.build_run.state, RunState::Impasse);
    assert!(!deep_outcome.build_run.scan_clean);
    assert!(deep_outcome.accept_run.is_none());
    let deep_esc = deep_outcome
        .escalation_markdown
        .as_ref()
        .expect("deep escalation present");
    assert!(deep_esc.contains("OUTPUT_QUOTA_EXCEEDED"));
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_output_filename_and_binary_prefixed_payload_blocked_before_accept() {
    let tmp = TempDirGuard::new("conveyor-non-utf8-out");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let non_utf8_builder = sh_harness(
        "codex-main",
        "openai-codex",
        "python3 -c 'import os; p = os.fsencode(os.environ[\"STREAMLINE_OUT_DIR\"]) + b\"/\\xff_bad.txt\"; open(p, \"wb\").write(b\"ok\\n\")'",
    );
    let acceptor = sh_harness(
        "agy-main",
        "google-gemini",
        "printf 'accepted\\n' > \"$STREAMLINE_OUT_DIR/verdict.txt\"",
    );

    let runs_dir = tmp.path.join("runs_non_utf8");
    let cfg = base_config(&runs_dir, vec![non_utf8_builder, acceptor.clone()]);
    let req = DispatchRequest {
        project_id: "proj-non-utf8".to_string(),
        input_paths: vec![spec_path.clone()],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("non-utf8 output returns Impasse");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert!(!outcome.build_run.scan_clean);
    assert!(outcome.accept_run.is_none());
    let esc = outcome
        .escalation_markdown
        .as_ref()
        .expect("escalation present");
    assert!(esc.contains("UTF-8"));

    let bad_prefix = ["ghp_", "Demo123456AbCdEf"].concat();
    let binary_stuffed_builder = sh_harness(
        "codex-main",
        "openai-codex",
        &format!(
            "python3 -c 'import os; p = os.path.join(os.environ[\"STREAMLINE_OUT_DIR\"], \"mixed.bin\"); open(p, \"wb\").write(b\"\\xff\\n{bad_prefix}\\n\")'"
        ),
    );
    let runs_mixed = tmp.path.join("runs_mixed");
    let cfg_mixed = base_config(&runs_mixed, vec![binary_stuffed_builder, acceptor]);
    let req_mixed = DispatchRequest {
        project_id: "proj-mixed-bytes".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };
    let mixed_outcome =
        execute_conveyor_blocking(&cfg_mixed, &req_mixed).expect("mixed bytes returns Impasse");
    assert_eq!(mixed_outcome.final_state, RunState::Impasse);
    assert!(!mixed_outcome.build_run.scan_clean);
    assert!(mixed_outcome.accept_run.is_none());
    assert!(mixed_outcome
        .escalation_markdown
        .as_deref()
        .unwrap_or_default()
        .contains("REDACTION_SCAN_FAILED"));
}

#[test]
fn oversized_staged_input_redacts_sensitive_entry_name_in_error() {
    let tmp = TempDirGuard::new("conveyor-stage-redact-err");
    let bundle_dir = tmp.path.join("bundle");
    fs::create_dir_all(&bundle_dir).expect("create bundle dir");

    let sensitive_name = format!("{}.bin", ["ghp_", "StageLeak123456AbCd"].concat());
    let big_path = bundle_dir.join(&sensitive_name);
    let oversized_len = (crate::staging::MAX_MANIFEST_FILE_BYTES + 1024) as u64;
    let f = fs::File::create(&big_path).expect("create oversized file");
    f.set_len(oversized_len)
        .expect("set oversized sparse length");

    let stage_dir = tmp.path.join("stage");
    let err = stage_readonly_inputs(&[bundle_dir], &stage_dir)
        .expect_err("oversized descendant input must fail staging");
    let msg = err.to_string();
    assert!(msg.contains("redacted-path-"));
    assert!(!msg.contains("StageLeak123456AbCd"));
}

#[test]
fn harness_discards_large_stdio_enforces_timeout_and_concurrency_cap() {
    use std::time::Duration;

    let tmp = TempDirGuard::new("conveyor-stdio-timeout-cap");
    let spec_path = tmp.path.join("spec.md");
    fs::write(&spec_path, "Input\n").expect("write spec.md");

    let noisy_builder = sh_harness(
        "codex-noisy",
        "openai-codex",
        "dd if=/dev/zero bs=1048576 count=8 2>/dev/null
         dd if=/dev/zero bs=1048576 count=8 >&2 2>/dev/null || true
         printf 'ok\\n' > \"$STREAMLINE_OUT_DIR/done.txt\"",
    );
    let acceptor = sh_harness("agy-main", "google-gemini", "exit 0");
    let runs_dir = tmp.path.join("runs");
    let cfg = base_config(&runs_dir, vec![noisy_builder, acceptor]);
    let req = DispatchRequest {
        project_id: "proj-noisy-stdio".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };
    let outcome = execute_conveyor_blocking(&cfg, &req).expect("noisy stdio run succeeds");
    assert_eq!(outcome.final_state, RunState::DocPendingOwner);

    let slow_harness = sh_harness("codex-slow", "openai-codex", "sleep 5");
    let work_dir = tmp.path.join("slow_work");
    fs::create_dir_all(&work_dir).expect("create slow_work");
    let timed = crate::runner::run_harness_command_with_timeout(
        &runs_dir.to_string_lossy(),
        &slow_harness,
        &work_dir,
        &[],
        Duration::from_millis(60),
    )
    .expect("timeout returns HarnessExit::TimedOut");
    assert!(!timed.is_success());
    assert!(matches!(timed, crate::runner::HarnessExit::TimedOut(_)));

    let capped = sh_harness("codex-cap", "openai-codex", "exit 0");
    let slot_one =
        crate::runner::acquire_harness_slot(&runs_dir.to_string_lossy(), &capped).expect("slot 1");
    let slot_two = crate::runner::acquire_harness_slot(&runs_dir.to_string_lossy(), &capped);
    assert!(matches!(slot_two, Err(ConveyorError::RunnerFailure(_))));
    drop(slot_one);
    let slot_three =
        crate::runner::acquire_harness_slot(&runs_dir.to_string_lossy(), &capped).expect("slot 3");
    drop(slot_three);

    // Background descendants in the harness process group must be terminated on normal exit
    // before output scanning or post-run stages proceed.
    let bg_spec = tmp.path.join("bg_spec.md");
    fs::write(&bg_spec, "Input\n").expect("write bg_spec.md");
    let bg_builder = sh_harness(
        "codex-bg",
        "openai-codex",
        "(sleep 0.25 && printf 'late\\n' > \"$STREAMLINE_OUT_DIR/late_descendant.txt\") &
         printf 'immediate\\n' > \"$STREAMLINE_OUT_DIR/immediate.txt\"",
    );
    let bg_acceptor = sh_harness("agy-bg", "google-gemini", "exit 0");
    let bg_runs_dir = tmp.path.join("runs_bg");
    let bg_cfg = base_config(&bg_runs_dir, vec![bg_builder, bg_acceptor]);
    let bg_req = DispatchRequest {
        project_id: "proj-bg-descendant".to_string(),
        input_paths: vec![bg_spec],
        preferred_builder_harness: None,
        preferred_acceptor_harness: None,
        risk_band: 2,
        caller_has_wildcard_scope: true,
    };
    let bg_outcome = execute_conveyor_blocking(&bg_cfg, &bg_req).expect("bg descendant run");
    assert_eq!(bg_outcome.final_state, RunState::DocPendingOwner);
    std::thread::sleep(Duration::from_millis(350));
    let bg_out_dir = bg_runs_dir.join(&bg_outcome.build_run.run_id).join("out");
    assert!(bg_out_dir.join("immediate.txt").exists());
    assert!(
        !bg_out_dir.join("late_descendant.txt").exists(),
        "background process-group descendant must be killed on harness exit"
    );
}
