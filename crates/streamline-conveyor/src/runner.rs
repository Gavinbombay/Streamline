use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use streamline_core::{ConveyorRun, HarnessSpec, RunPhase, RunState, StreamlineConfig};
use streamline_safe::{inspect_text, RedactionPolicy, ViolationKind};

use crate::error::ConveyorError;
use crate::staging::{
    compute_tree_manifest, freeze_readonly_tree, stage_readonly_inputs, unfreeze_readonly_dir,
    verify_cleanup_clean,
};

const SAFE_ENV_VARS: [&str; 4] = ["PATH", "HOME", "TMPDIR", "LANG"];

/// Request parameters for a blocking hands-off conveyor dispatch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DispatchRequest {
    pub project_id: String,
    pub input_paths: Vec<PathBuf>,
    pub preferred_builder_harness: Option<String>,
    pub preferred_acceptor_harness: Option<String>,
    pub risk_band: u8,
}

/// Complete outcome of a blocking conveyor execution including build and optional
/// cross-family acceptance runs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DispatchOutcome {
    pub build_run: ConveyorRun,
    pub accept_run: Option<ConveyorRun>,
    pub final_state: RunState,
    pub escalation_markdown: Option<String>,
}

struct RunContext<'a> {
    config: &'a StreamlineConfig,
    req: &'a DispatchRequest,
    builder: &'a HarnessSpec,
    acceptor: Option<&'a HarnessSpec>,
    build_run_id: String,
    build_started_at: String,
    run_dir: PathBuf,
    stage_dir: PathBuf,
    work_dir: PathBuf,
    out_dir: PathBuf,
}

struct OutputScanFinding {
    rel_path: String,
    line: usize,
    kind: ViolationKind,
    reason: String,
}

/// Validates that an independent acceptance pass for R2+ work uses a distinct model family.
pub fn verify_cross_family(
    risk_band: u8,
    builder_family: &str,
    acceptor_family: &str,
) -> Result<(), ConveyorError> {
    if risk_band >= 2
        && builder_family
            .trim()
            .eq_ignore_ascii_case(acceptor_family.trim())
    {
        return Err(ConveyorError::SameFamilyForbidden(
            builder_family.to_string(),
        ));
    }
    Ok(())
}

/// Runs the full hands-off build, cleanup check, redaction scan, and optional
/// cross-family acceptance workflow on the blocking thread pool.
pub fn execute_conveyor_blocking(
    config: &StreamlineConfig,
    req: &DispatchRequest,
) -> Result<DispatchOutcome, ConveyorError> {
    if req.project_id.trim().is_empty() || req.project_id.contains('\0') {
        return Err(ConveyorError::InvalidConfig(
            "project_id must not be empty".to_string(),
        ));
    }
    let runs_root = config.runs_dir.trim();
    if runs_root.is_empty() {
        return Err(ConveyorError::InvalidConfig(
            "runs_dir must not be empty".to_string(),
        ));
    }

    let (builder, acceptor) = select_harnesses(config, req)?;

    let build_run_id = format!("run-{}", uuid::Uuid::new_v4());
    let build_started_at = utc_now_rfc3339();
    let run_dir = Path::new(runs_root).join(&build_run_id);
    let stage_dir = run_dir.join("stage");
    let work_dir = run_dir.join("work");
    let out_dir = run_dir.join("out");

    fs::create_dir_all(&stage_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    fs::create_dir_all(&work_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    fs::create_dir_all(&out_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

    let ctx = RunContext {
        config,
        req,
        builder,
        acceptor,
        build_run_id,
        build_started_at,
        run_dir: run_dir.clone(),
        stage_dir,
        work_dir,
        out_dir,
    };

    let outcome = execute_conveyor_inner(&ctx);
    if let Err(unfreeze_err) = unfreeze_readonly_dir(&run_dir) {
        if outcome.is_ok() {
            return Err(unfreeze_err);
        }
    }
    outcome
}

fn select_harnesses<'a>(
    config: &'a StreamlineConfig,
    req: &DispatchRequest,
) -> Result<(&'a HarnessSpec, Option<&'a HarnessSpec>), ConveyorError> {
    if config.harnesses.is_empty() {
        return Err(ConveyorError::InvalidConfig(
            "no harnesses configured".to_string(),
        ));
    }

    let builder = match req.preferred_builder_harness.as_deref() {
        Some(pref_id) => config
            .harnesses
            .iter()
            .find(|h| h.id == pref_id)
            .ok_or_else(|| {
                ConveyorError::InvalidConfig(format!(
                    "preferred builder harness '{pref_id}' not found"
                ))
            })?,
        None => &config.harnesses[0],
    };
    validate_harness_command(builder, "builder")?;

    let acceptor = match req.preferred_acceptor_harness.as_deref() {
        Some(acc_id) => {
            let candidate = config
                .harnesses
                .iter()
                .find(|h| h.id == acc_id)
                .ok_or_else(|| {
                    ConveyorError::InvalidConfig(format!(
                        "preferred acceptor harness '{acc_id}' not found"
                    ))
                })?;
            verify_cross_family(req.risk_band, &builder.family, &candidate.family)?;
            validate_harness_command(candidate, "acceptor")?;
            Some(candidate)
        }
        None => {
            let distinct = config.harnesses.iter().find(|h| {
                !h.family
                    .trim()
                    .eq_ignore_ascii_case(builder.family.trim())
            });
            match distinct {
                Some(candidate) => {
                    validate_harness_command(candidate, "acceptor")?;
                    Some(candidate)
                }
                None if req.risk_band >= 2 => {
                    return Err(ConveyorError::SameFamilyForbidden(builder.family.clone()));
                }
                None => None,
            }
        }
    };

    Ok((builder, acceptor))
}

fn validate_harness_command(harness: &HarnessSpec, role: &str) -> Result<(), ConveyorError> {
    if harness.command.is_empty() || harness.command[0].trim().is_empty() {
        return Err(ConveyorError::InvalidConfig(format!(
            "{role} harness '{}' has an empty command",
            harness.id
        )));
    }
    Ok(())
}

fn execute_conveyor_inner(ctx: &RunContext<'_>) -> Result<DispatchOutcome, ConveyorError> {
    let staged_manifest = match stage_readonly_inputs(&ctx.req.input_paths, &ctx.stage_dir) {
        Ok(manifest) => manifest,
        Err(ConveyorError::MissingInput(missing_path)) => {
            return record_missing_input_impasse(ctx, &missing_path);
        }
        Err(other) => return Err(other),
    };

    let build_env = [
        ("STREAMLINE_PROJECT_ID", OsStr::new(ctx.req.project_id.as_str())),
        ("STREAMLINE_PHASE", OsStr::new("build")),
        ("STREAMLINE_STAGE_DIR", ctx.stage_dir.as_os_str()),
        ("STREAMLINE_WORK_DIR", ctx.work_dir.as_os_str()),
        ("STREAMLINE_OUT_DIR", ctx.out_dir.as_os_str()),
    ];

    let status1 = run_harness_command(ctx.builder, &ctx.work_dir, &build_env)?;
    if !status1.success() {
        tracing::warn!(
            run_id = %ctx.build_run_id,
            harness_id = %ctx.builder.id,
            status = %format_exit_status(status1),
            "builder attempt 1 exited nonzero; retrying once"
        );
        let status2 = run_harness_command(ctx.builder, &ctx.work_dir, &build_env)?;
        if !status2.success() {
            return record_two_strike_impasse(
                ctx,
                &staged_manifest.manifest_sha256,
                status1,
                status2,
            );
        }
    }

    match verify_cleanup_clean(&ctx.work_dir) {
        Ok(()) => {}
        Err(ConveyorError::DirtyScratch(dirty_rel)) => {
            tracing::warn!(
                run_id = %ctx.build_run_id,
                dirty_path = %dirty_rel,
                "builder scratch directory failed cleanup verification; quarantining run"
            );
            let out_manifest = compute_tree_manifest(&ctx.out_dir)?;
            let build_run = ConveyorRun {
                run_id: ctx.build_run_id.clone(),
                project_id: ctx.req.project_id.clone(),
                phase: RunPhase::Build,
                harness_id: ctx.builder.id.clone(),
                family: ctx.builder.family.clone(),
                input_manifest_sha256: staged_manifest.manifest_sha256,
                output_manifest_sha256: out_manifest.manifest_sha256,
                cleanup_clean: false,
                scan_clean: true,
                state: RunState::Quarantined,
                started_at: ctx.build_started_at.clone(),
                finished_at: Some(utc_now_rfc3339()),
            };
            return Ok(DispatchOutcome {
                build_run,
                accept_run: None,
                final_state: RunState::Quarantined,
                escalation_markdown: None,
            });
        }
        Err(other) => return Err(other),
    }

    let policy = RedactionPolicy {
        public_safe_mode: ctx.config.public_safe_mode,
    };
    let findings = scan_utf8_outputs(&ctx.out_dir, &policy)?;
    if !findings.is_empty() {
        let escalation = format_redaction_escalation(
            &ctx.req.project_id,
            &ctx.build_run_id,
            "build",
            &ctx.builder.id,
            &ctx.builder.family,
            &findings,
        );
        fs::write(ctx.out_dir.join("ESCALATION.md"), escalation.as_bytes())
            .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        let out_manifest = compute_tree_manifest(&ctx.out_dir)?;
        let build_run = ConveyorRun {
            run_id: ctx.build_run_id.clone(),
            project_id: ctx.req.project_id.clone(),
            phase: RunPhase::Build,
            harness_id: ctx.builder.id.clone(),
            family: ctx.builder.family.clone(),
            input_manifest_sha256: staged_manifest.manifest_sha256,
            output_manifest_sha256: out_manifest.manifest_sha256,
            cleanup_clean: true,
            scan_clean: false,
            state: RunState::Impasse,
            started_at: ctx.build_started_at.clone(),
            finished_at: Some(utc_now_rfc3339()),
        };
        return Ok(DispatchOutcome {
            build_run,
            accept_run: None,
            final_state: RunState::Impasse,
            escalation_markdown: Some(escalation),
        });
    }

    let build_out_manifest = compute_tree_manifest(&ctx.out_dir)?;
    let build_finished_at = utc_now_rfc3339();

    let Some(acceptor) = ctx.acceptor else {
        let build_run = ConveyorRun {
            run_id: ctx.build_run_id.clone(),
            project_id: ctx.req.project_id.clone(),
            phase: RunPhase::Build,
            harness_id: ctx.builder.id.clone(),
            family: ctx.builder.family.clone(),
            input_manifest_sha256: staged_manifest.manifest_sha256,
            output_manifest_sha256: build_out_manifest.manifest_sha256,
            cleanup_clean: true,
            scan_clean: true,
            state: RunState::DocPendingOwner,
            started_at: ctx.build_started_at.clone(),
            finished_at: Some(build_finished_at),
        };
        return Ok(DispatchOutcome {
            build_run,
            accept_run: None,
            final_state: RunState::DocPendingOwner,
            escalation_markdown: None,
        });
    };

    let build_run = ConveyorRun {
        run_id: ctx.build_run_id.clone(),
        project_id: ctx.req.project_id.clone(),
        phase: RunPhase::Build,
        harness_id: ctx.builder.id.clone(),
        family: ctx.builder.family.clone(),
        input_manifest_sha256: staged_manifest.manifest_sha256,
        output_manifest_sha256: build_out_manifest.manifest_sha256,
        cleanup_clean: true,
        scan_clean: true,
        state: RunState::Accepting,
        started_at: ctx.build_started_at.clone(),
        finished_at: Some(build_finished_at),
    };

    run_acceptance_pass(ctx, build_run, acceptor, &policy)
}

fn record_missing_input_impasse(
    ctx: &RunContext<'_>,
    missing_path: &str,
) -> Result<DispatchOutcome, ConveyorError> {
    let escalation = format!(
        "# Conveyor Escalation Report\n\n\
         - Reason: `MISSING_INPUT`\n\
         - Project ID: `{}`\n\
         - Run ID: `{}`\n\
         - Harness: `{}` (`{}`)\n\
         - Missing path: `{}`\n\n\
         Declared input path was not found on disk. No builder execution strike was counted.\n",
        ctx.req.project_id, ctx.build_run_id, ctx.builder.id, ctx.builder.family, missing_path
    );
    fs::write(ctx.out_dir.join("ESCALATION.md"), escalation.as_bytes())
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    let out_manifest = compute_tree_manifest(&ctx.out_dir)?;
    let build_run = ConveyorRun {
        run_id: ctx.build_run_id.clone(),
        project_id: ctx.req.project_id.clone(),
        phase: RunPhase::Build,
        harness_id: ctx.builder.id.clone(),
        family: ctx.builder.family.clone(),
        input_manifest_sha256: String::new(),
        output_manifest_sha256: out_manifest.manifest_sha256,
        cleanup_clean: true,
        scan_clean: true,
        state: RunState::Impasse,
        started_at: ctx.build_started_at.clone(),
        finished_at: Some(utc_now_rfc3339()),
    };
    Ok(DispatchOutcome {
        build_run,
        accept_run: None,
        final_state: RunState::Impasse,
        escalation_markdown: Some(escalation),
    })
}

fn record_two_strike_impasse(
    ctx: &RunContext<'_>,
    input_manifest_sha256: &str,
    status1: ExitStatus,
    status2: ExitStatus,
) -> Result<DispatchOutcome, ConveyorError> {
    let escalation = format!(
        "# Conveyor Escalation Report\n\n\
         - Reason: `BUILDER_TWO_STRIKE_FAILURE`\n\
         - Project ID: `{}`\n\
         - Run ID: `{}`\n\
         - Phase: `build`\n\
         - Harness: `{}` (`{}`)\n\
         - Attempt 1 status: `{}`\n\
         - Attempt 2 status: `{}`\n\n\
         Builder failed on both attempt 1 and attempt 2. Escalated to impasse.\n",
        ctx.req.project_id,
        ctx.build_run_id,
        ctx.builder.id,
        ctx.builder.family,
        format_exit_status(status1),
        format_exit_status(status2),
    );
    fs::write(ctx.out_dir.join("ESCALATION.md"), escalation.as_bytes())
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    let out_manifest = compute_tree_manifest(&ctx.out_dir)?;
    let cleanup_clean = verify_cleanup_clean(&ctx.work_dir).is_ok();
    let build_run = ConveyorRun {
        run_id: ctx.build_run_id.clone(),
        project_id: ctx.req.project_id.clone(),
        phase: RunPhase::Build,
        harness_id: ctx.builder.id.clone(),
        family: ctx.builder.family.clone(),
        input_manifest_sha256: input_manifest_sha256.to_string(),
        output_manifest_sha256: out_manifest.manifest_sha256,
        cleanup_clean,
        scan_clean: true,
        state: RunState::Impasse,
        started_at: ctx.build_started_at.clone(),
        finished_at: Some(utc_now_rfc3339()),
    };
    Ok(DispatchOutcome {
        build_run,
        accept_run: None,
        final_state: RunState::Impasse,
        escalation_markdown: Some(escalation),
    })
}

fn run_acceptance_pass(
    ctx: &RunContext<'_>,
    mut build_run: ConveyorRun,
    acceptor: &HarnessSpec,
    policy: &RedactionPolicy,
) -> Result<DispatchOutcome, ConveyorError> {
    let work_accept_dir = ctx.run_dir.join("work_accept");
    let out_accept_dir = ctx.run_dir.join("out_accept");
    fs::create_dir_all(&work_accept_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    fs::create_dir_all(&out_accept_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

    // Freeze builder outputs read-only during acceptance so acceptor cannot mutate them.
    freeze_readonly_tree(&ctx.out_dir)?;

    let accept_run_id = format!("run-{}", uuid::Uuid::new_v4());
    let accept_started_at = utc_now_rfc3339();
    let accept_env = [
        ("STREAMLINE_PROJECT_ID", OsStr::new(ctx.req.project_id.as_str())),
        ("STREAMLINE_PHASE", OsStr::new("accept")),
        ("STREAMLINE_STAGE_DIR", ctx.stage_dir.as_os_str()),
        ("STREAMLINE_BUILD_OUT_DIR", ctx.out_dir.as_os_str()),
        ("STREAMLINE_WORK_DIR", work_accept_dir.as_os_str()),
        ("STREAMLINE_OUT_DIR", out_accept_dir.as_os_str()),
    ];

    let accept_status = run_harness_command(acceptor, &work_accept_dir, &accept_env)?;
    if !accept_status.success() {
        let escalation = format!(
            "# Conveyor Escalation Report\n\n\
             - Reason: `ACCEPTOR_REJECTED`\n\
             - Project ID: `{}`\n\
             - Build Run ID: `{}`\n\
             - Accept Run ID: `{}`\n\
             - Harness: `{}` (`{}`)\n\
             - Exit status: `{}`\n\n\
             Independent acceptance pass exited nonzero. Escalated to impasse.\n",
            ctx.req.project_id,
            ctx.build_run_id,
            accept_run_id,
            acceptor.id,
            acceptor.family,
            format_exit_status(accept_status),
        );
        fs::write(out_accept_dir.join("ESCALATION.md"), escalation.as_bytes())
            .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        let cleanup_clean = verify_cleanup_clean(&work_accept_dir).is_ok();
        let scan_clean = scan_utf8_outputs(&out_accept_dir, policy)?.is_empty();
        let out_accept_manifest = compute_tree_manifest(&out_accept_dir)?;
        let accept_run = ConveyorRun {
            run_id: accept_run_id,
            project_id: ctx.req.project_id.clone(),
            phase: RunPhase::Accept,
            harness_id: acceptor.id.clone(),
            family: acceptor.family.clone(),
            input_manifest_sha256: build_run.output_manifest_sha256.clone(),
            output_manifest_sha256: out_accept_manifest.manifest_sha256,
            cleanup_clean,
            scan_clean,
            state: RunState::Impasse,
            started_at: accept_started_at,
            finished_at: Some(utc_now_rfc3339()),
        };
        return Ok(DispatchOutcome {
            build_run,
            accept_run: Some(accept_run),
            final_state: RunState::Impasse,
            escalation_markdown: Some(escalation),
        });
    }

    match verify_cleanup_clean(&work_accept_dir) {
        Ok(()) => {}
        Err(ConveyorError::DirtyScratch(dirty_rel)) => {
            tracing::warn!(
                run_id = %accept_run_id,
                dirty_path = %dirty_rel,
                "acceptor scratch directory failed cleanup verification; quarantining run"
            );
            let out_accept_manifest = compute_tree_manifest(&out_accept_dir)?;
            let accept_run = ConveyorRun {
                run_id: accept_run_id,
                project_id: ctx.req.project_id.clone(),
                phase: RunPhase::Accept,
                harness_id: acceptor.id.clone(),
                family: acceptor.family.clone(),
                input_manifest_sha256: build_run.output_manifest_sha256.clone(),
                output_manifest_sha256: out_accept_manifest.manifest_sha256,
                cleanup_clean: false,
                scan_clean: true,
                state: RunState::Quarantined,
                started_at: accept_started_at,
                finished_at: Some(utc_now_rfc3339()),
            };
            return Ok(DispatchOutcome {
                build_run,
                accept_run: Some(accept_run),
                final_state: RunState::Quarantined,
                escalation_markdown: None,
            });
        }
        Err(other) => return Err(other),
    }

    let findings = scan_utf8_outputs(&out_accept_dir, policy)?;
    if !findings.is_empty() {
        let escalation = format_redaction_escalation(
            &ctx.req.project_id,
            &accept_run_id,
            "accept",
            &acceptor.id,
            &acceptor.family,
            &findings,
        );
        fs::write(out_accept_dir.join("ESCALATION.md"), escalation.as_bytes())
            .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        let out_accept_manifest = compute_tree_manifest(&out_accept_dir)?;
        let accept_run = ConveyorRun {
            run_id: accept_run_id,
            project_id: ctx.req.project_id.clone(),
            phase: RunPhase::Accept,
            harness_id: acceptor.id.clone(),
            family: acceptor.family.clone(),
            input_manifest_sha256: build_run.output_manifest_sha256.clone(),
            output_manifest_sha256: out_accept_manifest.manifest_sha256,
            cleanup_clean: true,
            scan_clean: false,
            state: RunState::Impasse,
            started_at: accept_started_at,
            finished_at: Some(utc_now_rfc3339()),
        };
        return Ok(DispatchOutcome {
            build_run,
            accept_run: Some(accept_run),
            final_state: RunState::Impasse,
            escalation_markdown: Some(escalation),
        });
    }

    let out_accept_manifest = compute_tree_manifest(&out_accept_dir)?;
    build_run.state = RunState::DocPendingOwner;
    let accept_run = ConveyorRun {
        run_id: accept_run_id,
        project_id: ctx.req.project_id.clone(),
        phase: RunPhase::Accept,
        harness_id: acceptor.id.clone(),
        family: acceptor.family.clone(),
        input_manifest_sha256: build_run.output_manifest_sha256.clone(),
        output_manifest_sha256: out_accept_manifest.manifest_sha256,
        cleanup_clean: true,
        scan_clean: true,
        state: RunState::DocPendingOwner,
        started_at: accept_started_at,
        finished_at: Some(utc_now_rfc3339()),
    };

    Ok(DispatchOutcome {
        build_run,
        accept_run: Some(accept_run),
        final_state: RunState::DocPendingOwner,
        escalation_markdown: None,
    })
}

fn run_harness_command(
    harness: &HarnessSpec,
    cwd: &Path,
    extra_env: &[(&str, &OsStr)],
) -> Result<ExitStatus, ConveyorError> {
    let mut cmd = Command::new(&harness.command[0]);
    if harness.command.len() > 1 {
        cmd.args(&harness.command[1..]);
    }
    cmd.current_dir(cwd);
    cmd.env_clear();
    for var_name in SAFE_ENV_VARS {
        if let Some(val) = std::env::var_os(var_name) {
            cmd.env(var_name, val);
        }
    }
    for (var_name, val) in extra_env {
        cmd.env(var_name, val);
    }

    let output = cmd.output().map_err(|err| {
        ConveyorError::RunnerFailure(format!(
            "failed to spawn harness '{}': {err}",
            harness.id
        ))
    })?;
    Ok(output.status)
}

fn scan_utf8_outputs(
    out_dir: &Path,
    policy: &RedactionPolicy,
) -> Result<Vec<OutputScanFinding>, ConveyorError> {
    let mut files = Vec::new();
    collect_regular_files(out_dir, &mut files)?;
    files.sort();

    let mut findings = Vec::new();
    for file_path in files {
        let bytes = fs::read(&file_path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let report = inspect_text(text, policy);
        if !report.ok {
            let rel_path = file_path
                .strip_prefix(out_dir)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| file_path.display().to_string());
            for violation in report.violations {
                findings.push(OutputScanFinding {
                    rel_path: rel_path.clone(),
                    line: violation.line,
                    kind: violation.kind,
                    reason: violation.reason,
                });
            }
        }
    }
    Ok(findings)
}

fn collect_regular_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ConveyorError> {
    if !dir.exists() {
        return Ok(());
    }
    let rd = fs::read_dir(dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in rd {
        let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        entries.push(entry.path());
    }
    entries.sort();

    for path in entries {
        let link_meta =
            fs::symlink_metadata(&path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        let ft = link_meta.file_type();
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            collect_regular_files(&path, out)?;
        } else if ft.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn format_redaction_escalation(
    project_id: &str,
    run_id: &str,
    phase: &str,
    harness_id: &str,
    family: &str,
    findings: &[OutputScanFinding],
) -> String {
    let mut md = format!(
        "# Conveyor Escalation Report\n\n\
         - Reason: `REDACTION_SCAN_FAILED`\n\
         - Project ID: `{project_id}`\n\
         - Run ID: `{run_id}`\n\
         - Phase: `{phase}`\n\
         - Harness: `{harness_id}` (`{family}`)\n\n\
         ## Violations Detected (Raw Values Omitted)\n\n"
    );
    for finding in findings {
        md.push_str(&format!(
            "- `{}` line {} ({:?}): {}\n",
            finding.rel_path, finding.line, finding.kind, finding.reason
        ));
    }
    md
}

fn format_exit_status(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(sig) = status.signal() {
                    return format!("terminated by signal {sig}");
                }
            }
            "terminated abnormally".to_string()
        }
    }
}

fn utc_now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
