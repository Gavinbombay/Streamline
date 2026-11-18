use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use chrono::DateTime;
use streamline_core::{HarnessSpec, RunPhase, RunState, StreamlineConfig, SyncVerdict, TrioProfile};
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
        command: vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            script.to_string(),
        ],
        max_concurrency: 1,
    }
}

fn base_config(runs_dir: &std::path::Path, harnesses: Vec<HarnessSpec>) -> StreamlineConfig {
    StreamlineConfig {
        runs_dir: runs_dir.to_string_lossy().into_owned(),
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
    };

    let outcome = execute_conveyor_blocking(&cfg, &req).expect("execute rogue builder run");
    assert_eq!(outcome.final_state, RunState::Impasse);
    assert!(outcome.accept_run.is_none());

    let staged_inputs = runs_dir
        .join(&outcome.build_run.run_id)
        .join("stage")
        .join("INPUTS");
    let observed_spec = fs::read_to_string(staged_inputs.join("spec.md")).expect("read staged spec");
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

    let parsed: serde_json::Value =
        serde_json::from_str(lines[0]).expect("parse first JSONL line");
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
    let script = format!("printf '%s\\n' '{leaked_sample}' > \"$STREAMLINE_OUT_DIR/report.txt\"");
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
    assert!(escalation.contains("line 1"));
    assert!(!escalation.contains(&leaked_sample));
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
    };
    let err = execute_conveyor_blocking(&cfg, &req_r2).expect_err("R2 same-family rejected");
    assert!(matches!(err, ConveyorError::SameFamilyForbidden(_)));

    let req_r1 = DispatchRequest {
        project_id: "proj-pref-r1".to_string(),
        input_paths: vec![spec_path],
        preferred_builder_harness: Some("codex-a".to_string()),
        preferred_acceptor_harness: Some("codex-b".to_string()),
        risk_band: 1,
    };
    let ok = execute_conveyor_blocking(&cfg, &req_r1).expect("R1 same-family allowed");
    assert_eq!(ok.final_state, RunState::DocPendingOwner);
    assert_eq!(
        ok.accept_run.as_ref().expect("accept_run present").harness_id,
        "codex-b"
    );
}
