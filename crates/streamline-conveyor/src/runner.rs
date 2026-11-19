use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use streamline_core::{ConveyorRun, HarnessSpec, RunPhase, RunState, StreamlineConfig};
use streamline_safe::{
    inspect_text, kill_process_group, PinnedDir, RedactionPolicy, ViolationKind,
};

use crate::error::ConveyorError;
use crate::staging::{
    compute_tree_manifest, freeze_readonly_tree, stage_pinned_inputs,
    trim_trailing_slashes_and_dot, unfreeze_readonly_dir, verify_cleanup_clean,
    verify_no_symlink_components, PinnedInputEntry,
};

const SAFE_ENV_VARS: [&str; 4] = ["PATH", "HOME", "TMPDIR", "LANG"];
pub const MAX_OUTPUT_FILES: usize = 512;
pub const MAX_DIR_ENTRIES: usize = 512;
pub const MAX_TREE_DEPTH: usize = 32;
pub const MAX_SINGLE_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TOTAL_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SCAN_FINDINGS: usize = 64;
pub const MAX_HARNESS_RUNTIME: Duration = Duration::from_secs(300);
pub const MAX_CONCURRENT_DISPATCHES: usize = 32;

static ACTIVE_DISPATCHES: AtomicUsize = AtomicUsize::new(0);
static HARNESS_ACTIVE_SLOTS: LazyLock<Mutex<HashMap<(String, String), usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) struct HarnessSlotGuard {
    slot_pair: (String, String),
}

impl Drop for HarnessSlotGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = HARNESS_ACTIVE_SLOTS.lock() {
            if let Some(count) = map.get_mut(&self.slot_pair) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    map.remove(&self.slot_pair);
                }
            }
        }
        ACTIVE_DISPATCHES.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) fn acquire_harness_slot(
    runs_root: &str,
    harness: &HarnessSpec,
) -> Result<HarnessSlotGuard, ConveyorError> {
    let prev = ACTIVE_DISPATCHES.fetch_add(1, Ordering::SeqCst);
    if prev >= MAX_CONCURRENT_DISPATCHES {
        ACTIVE_DISPATCHES.fetch_sub(1, Ordering::SeqCst);
        return Err(ConveyorError::RunnerFailure(format!(
            "conveyor global concurrency limit ({MAX_CONCURRENT_DISPATCHES}) reached"
        )));
    }

    let cap = (harness.max_concurrency.max(1) as usize).min(MAX_CONCURRENT_DISPATCHES);
    let slot_pair = (runs_root.to_string(), harness.id.clone());
    let mut map = match HARNESS_ACTIVE_SLOTS.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let current = map.get(&slot_pair).copied().unwrap_or(0);
    if current >= cap {
        drop(map);
        ACTIVE_DISPATCHES.fetch_sub(1, Ordering::SeqCst);
        return Err(ConveyorError::RunnerFailure(format!(
            "harness '{}' max_concurrency ({cap}) reached",
            harness.id
        )));
    }
    map.insert(slot_pair.clone(), current + 1);
    Ok(HarnessSlotGuard { slot_pair })
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HarnessExit {
    Exited(ExitStatus),
    TimedOut(Duration),
}

impl HarnessExit {
    pub(crate) fn is_success(self) -> bool {
        match self {
            Self::Exited(status) => status.success(),
            Self::TimedOut(_) => false,
        }
    }
}

/// Request parameters for a blocking hands-off conveyor dispatch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DispatchRequest {
    pub project_id: String,
    pub input_paths: Vec<PathBuf>,
    pub preferred_builder_harness: Option<String>,
    pub preferred_acceptor_harness: Option<String>,
    pub risk_band: u8,
    #[serde(default = "default_wildcard_scope")]
    pub caller_has_wildcard_scope: bool,
}

fn default_wildcard_scope() -> bool {
    true
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

enum OutputScanOutcome {
    Clean,
    Violations(Vec<OutputScanFinding>),
    QuotaExceeded(String),
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
    if !req.caller_has_wildcard_scope {
        validate_scoped_project_id(&req.project_id)?;
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

    PinnedDir::open_or_create_dir(&stage_dir)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    PinnedDir::open_or_create_dir(&work_dir)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    PinnedDir::open_or_create_dir(&out_dir)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

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
    let unfreeze_stage = unfreeze_readonly_dir(&ctx.stage_dir.join("INPUTS"));
    let unfreeze_out = if matches!(&outcome, Ok(o) if o.accept_run.is_some()) || outcome.is_err() {
        unfreeze_readonly_dir(&ctx.out_dir)
    } else {
        Ok(())
    };
    if outcome.is_ok() {
        unfreeze_stage?;
        unfreeze_out?;
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
            let distinct = config
                .harnesses
                .iter()
                .find(|h| !h.family.trim().eq_ignore_ascii_case(builder.family.trim()));
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
    let pinned_entries = match validate_and_pin_inputs(ctx.config, ctx.req) {
        Ok(entries) => entries,
        Err(violation) => {
            return match violation {
                InputPinError::InvalidConfig(msg) => Err(ConveyorError::InvalidConfig(msg)),
                InputPinError::SymlinkRejected(path_str) => record_input_rejection_impasse(
                    ctx,
                    "SYMLINK_INPUT_REJECTED",
                    "Rejected path",
                    &path_str,
                    "Symbolic links are forbidden in staged inputs. No builder execution strike was counted.",
                ),
                InputPinError::OutsideAllowedRoot(path_str) => record_input_rejection_impasse(
                    ctx,
                    "INPUT_OUTSIDE_ALLOWED_ROOT",
                    "Rejected path",
                    &path_str,
                    "Declared input path is outside allowed input roots. No builder execution strike was counted.",
                ),
                InputPinError::MissingInput(path_str) => record_input_rejection_impasse(
                    ctx,
                    "MISSING_INPUT",
                    "Missing path",
                    &path_str,
                    "Declared input path was not found on disk. No builder execution strike was counted.",
                ),
            };
        }
    };

    let staged_manifest = match stage_pinned_inputs(&pinned_entries, &ctx.stage_dir) {
        Ok(manifest) => manifest,
        Err(ConveyorError::MissingInput(missing_path)) => {
            return record_input_rejection_impasse(
                ctx,
                "MISSING_INPUT",
                "Missing path",
                &missing_path,
                "Declared input path was not found on disk. No builder execution strike was counted.",
            );
        }
        Err(other) => return Err(other),
    };

    let build_env = [
        (
            "STREAMLINE_PROJECT_ID",
            OsStr::new(ctx.req.project_id.as_str()),
        ),
        ("STREAMLINE_PHASE", OsStr::new("build")),
        ("STREAMLINE_STAGE_DIR", ctx.stage_dir.as_os_str()),
        ("STREAMLINE_WORK_DIR", ctx.work_dir.as_os_str()),
        ("STREAMLINE_OUT_DIR", ctx.out_dir.as_os_str()),
    ];

    let status1 = run_harness_command(
        ctx.config.runs_dir.trim(),
        ctx.builder,
        &ctx.work_dir,
        &build_env,
    )?;
    if !status1.is_success() {
        tracing::warn!(
            run_id = %ctx.build_run_id,
            harness_id = %ctx.builder.id,
            status = %format_harness_exit(status1),
            "builder attempt 1 exited nonzero; retrying once"
        );
        let status2 = run_harness_command(
            ctx.config.runs_dir.trim(),
            ctx.builder,
            &ctx.work_dir,
            &build_env,
        )?;
        if !status2.is_success() {
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
    match scan_utf8_outputs(&ctx.out_dir, &policy)? {
        OutputScanOutcome::Clean => {}
        OutputScanOutcome::Violations(findings) => {
            let escalation = format_redaction_escalation(
                &ctx.req.project_id,
                &ctx.build_run_id,
                "build",
                &ctx.builder.id,
                &ctx.builder.family,
                &findings,
            );
            return record_build_scan_impasse(
                ctx,
                &staged_manifest.manifest_sha256,
                escalation,
                false,
            );
        }
        OutputScanOutcome::QuotaExceeded(reason) => {
            let escalation = format_quota_escalation(
                &ctx.req.project_id,
                &ctx.build_run_id,
                "build",
                &ctx.builder.id,
                &ctx.builder.family,
                &reason,
            );
            return record_build_scan_impasse(
                ctx,
                &staged_manifest.manifest_sha256,
                escalation,
                true,
            );
        }
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

enum InputPinError {
    InvalidConfig(String),
    SymlinkRejected(String),
    OutsideAllowedRoot(String),
    MissingInput(String),
}

struct PinnedRoot {
    abs_raw_root: PathBuf,
    canon_root: PathBuf,
    dir: PinnedDir,
}

struct SymlinkedRoot {
    abs_raw_root: PathBuf,
    canon_root: Option<PathBuf>,
}

fn validate_scoped_project_id(project_id: &str) -> Result<(), ConveyorError> {
    if project_id.is_empty()
        || project_id != project_id.trim()
        || project_id == "."
        || project_id == ".."
        || project_id.contains("..")
        || project_id.contains('/')
        || project_id.contains('\\')
        || project_id.contains('\0')
    {
        return Err(ConveyorError::InvalidConfig(
            "project_id contains invalid path characters for project-scoped dispatch".to_string(),
        ));
    }
    Ok(())
}

fn validate_and_pin_inputs(
    config: &StreamlineConfig,
    req: &DispatchRequest,
) -> Result<Vec<PinnedInputEntry>, InputPinError> {
    if !req.caller_has_wildcard_scope {
        validate_scoped_project_id(&req.project_id)
            .map_err(|e| InputPinError::InvalidConfig(e.to_string()))?;
    }

    if req.input_paths.is_empty() {
        return Ok(Vec::new());
    }

    let base_roots = config.effective_allowed_input_roots();
    let mut pinned_roots = Vec::new();
    let mut symlinked_roots = Vec::new();

    for base_root in &base_roots {
        if req.caller_has_wildcard_scope {
            collect_wildcard_root(base_root, &mut pinned_roots, &mut symlinked_roots);
        } else {
            collect_project_scoped_root(
                base_root,
                &req.project_id,
                &mut pinned_roots,
                &mut symlinked_roots,
            );
        }
    }

    if pinned_roots.is_empty() && symlinked_roots.is_empty() {
        return Err(InputPinError::OutsideAllowedRoot(
            req.input_paths[0].display().to_string(),
        ));
    }

    let mut pinned_entries = Vec::with_capacity(req.input_paths.len());
    for input_path in &req.input_paths {
        let display_path = input_path.display().to_string();
        let trimmed_input = trim_trailing_slashes_and_dot(input_path);
        let abs_input = to_abs_path(&trimmed_input)
            .map_err(|_| InputPinError::OutsideAllowedRoot(display_path.clone()))?;

        match fs::symlink_metadata(&trimmed_input) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(InputPinError::SymlinkRejected(display_path));
                }

                let canonical_input = fs::canonicalize(&trimmed_input)
                    .map_err(|_| InputPinError::OutsideAllowedRoot(display_path.clone()))?;

                if matches_symlinked_root(&abs_input, Some(&canonical_input), &symlinked_roots) {
                    return Err(InputPinError::SymlinkRejected(display_path));
                }

                let matched_root = select_deepest_matching_root(&pinned_roots, &canonical_input);

                let Some(matched_root) = matched_root else {
                    if let Some(err) =
                        check_lexical_symlink_escape(&abs_input, &pinned_roots, &display_path)
                    {
                        return Err(err);
                    }
                    return Err(InputPinError::OutsideAllowedRoot(display_path));
                };

                let segments = resolve_walk_segments(
                    &trimmed_input,
                    &abs_input,
                    &canonical_input,
                    matched_root,
                    &display_path,
                )?;
                let entry = pin_entry_from_root(matched_root, &segments, &display_path)?;
                pinned_entries.push(entry);
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                if matches_symlinked_root(&abs_input, None, &symlinked_roots) {
                    return Err(InputPinError::SymlinkRejected(display_path));
                }
                validate_missing_path_scope(&trimmed_input, &pinned_roots, &symlinked_roots)?;
                return Err(InputPinError::MissingInput(display_path));
            }
            Err(_) => {
                if matches_symlinked_root(&abs_input, None, &symlinked_roots) {
                    return Err(InputPinError::SymlinkRejected(display_path));
                }
                return Err(InputPinError::OutsideAllowedRoot(display_path));
            }
        }
    }

    Ok(pinned_entries)
}

fn select_deepest_matching_root<'a>(
    pinned_roots: &'a [PinnedRoot],
    canonical_input: &Path,
) -> Option<&'a PinnedRoot> {
    let mut best: Option<(&PinnedRoot, usize)> = None;
    for item in pinned_roots {
        if !canonical_input.starts_with(&item.canon_root) {
            continue;
        }
        let depth = item.canon_root.components().count();
        match best {
            None => best = Some((item, depth)),
            Some((_, best_depth)) if depth > best_depth => best = Some((item, depth)),
            _ => {}
        }
    }
    best.map(|(item, _)| item)
}

fn collect_wildcard_root(
    raw_root: &Path,
    pinned_roots: &mut Vec<PinnedRoot>,
    symlinked_roots: &mut Vec<SymlinkedRoot>,
) {
    let trimmed = trim_trailing_slashes_and_dot(raw_root);
    let Ok(abs_raw_root) = to_abs_path(&trimmed) else {
        return;
    };
    match PinnedDir::open_existing_dir(&trimmed) {
        Ok(dir) => {
            if let Ok(canon_root) = fs::canonicalize(&trimmed) {
                pinned_roots.push(PinnedRoot {
                    abs_raw_root,
                    canon_root,
                    dir,
                });
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => {
            let is_sym = is_symlink_io_err(&err)
                || fs::symlink_metadata(&trimmed)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);
            if is_sym {
                symlinked_roots.push(SymlinkedRoot {
                    abs_raw_root,
                    canon_root: fs::canonicalize(&trimmed).ok(),
                });
            }
        }
    }
}

fn collect_project_scoped_root(
    base_root: &Path,
    project_id: &str,
    pinned_roots: &mut Vec<PinnedRoot>,
    symlinked_roots: &mut Vec<SymlinkedRoot>,
) {
    let trimmed_base = trim_trailing_slashes_and_dot(base_root);
    let scoped_raw = trimmed_base.join("projects").join(project_id);
    let Ok(abs_raw_root) = to_abs_path(&scoped_raw) else {
        return;
    };

    let base_dir = match PinnedDir::open_existing_dir(&trimmed_base) {
        Ok(dir) => dir,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return,
        Err(err) => {
            let is_sym = is_symlink_io_err(&err)
                || fs::symlink_metadata(&trimmed_base)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);
            if is_sym {
                symlinked_roots.push(SymlinkedRoot {
                    abs_raw_root,
                    canon_root: fs::canonicalize(&scoped_raw).ok(),
                });
            }
            return;
        }
    };

    let projects_dir = match base_dir.open_subdir("projects") {
        Ok(dir) => dir,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return,
        Err(err) => {
            if is_symlink_io_err(&err) {
                symlinked_roots.push(SymlinkedRoot {
                    abs_raw_root,
                    canon_root: fs::canonicalize(&scoped_raw).ok(),
                });
            }
            return;
        }
    };

    match projects_dir.open_subdir(project_id) {
        Ok(proj_dir) => {
            if let Ok(canon_root) = fs::canonicalize(&scoped_raw) {
                pinned_roots.push(PinnedRoot {
                    abs_raw_root,
                    canon_root,
                    dir: proj_dir,
                });
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => {
            if is_symlink_io_err(&err) {
                symlinked_roots.push(SymlinkedRoot {
                    abs_raw_root,
                    canon_root: fs::canonicalize(&scoped_raw).ok(),
                });
            }
        }
    }
}

fn is_symlink_io_err(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::InvalidInput && err.to_string().contains("symlinks are forbidden")
}

fn to_abs_path(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn matches_symlinked_root(
    abs_input: &Path,
    canonical_input: Option<&Path>,
    symlinked_roots: &[SymlinkedRoot],
) -> bool {
    symlinked_roots.iter().any(|root| {
        abs_input.starts_with(&root.abs_raw_root)
            || match (canonical_input, root.canon_root.as_ref()) {
                (Some(c_in), Some(c_root)) => c_in.starts_with(c_root),
                _ => false,
            }
    })
}

fn extract_normal_segments(rel: &Path) -> Option<Vec<String>> {
    let mut segs = Vec::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(seg) => segs.push(seg.to_str()?.to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(segs)
}

fn resolve_walk_segments(
    trimmed_input: &Path,
    abs_input: &Path,
    canonical_input: &Path,
    matched_root: &PinnedRoot,
    display_path: &str,
) -> Result<Vec<String>, InputPinError> {
    if let Some(segs) = abs_input
        .strip_prefix(&matched_root.abs_raw_root)
        .ok()
        .and_then(extract_normal_segments)
    {
        return Ok(segs);
    }

    if let Some(segs) = abs_input
        .strip_prefix(&matched_root.canon_root)
        .ok()
        .and_then(extract_normal_segments)
    {
        return Ok(segs);
    }

    if let Err(err) = verify_no_symlink_components(trimmed_input) {
        return match err {
            ConveyorError::MissingInput(_) => {
                Err(InputPinError::MissingInput(display_path.to_string()))
            }
            _ => Err(InputPinError::SymlinkRejected(display_path.to_string())),
        };
    }

    canonical_input
        .strip_prefix(&matched_root.canon_root)
        .ok()
        .and_then(extract_normal_segments)
        .ok_or_else(|| InputPinError::OutsideAllowedRoot(display_path.to_string()))
}

fn check_lexical_symlink_escape(
    abs_input: &Path,
    pinned_roots: &[PinnedRoot],
    display_path: &str,
) -> Option<InputPinError> {
    for root in pinned_roots {
        let segs = abs_input
            .strip_prefix(&root.abs_raw_root)
            .ok()
            .and_then(extract_normal_segments)
            .or_else(|| {
                abs_input
                    .strip_prefix(&root.canon_root)
                    .ok()
                    .and_then(extract_normal_segments)
            });
        if let Some(segs) = segs {
            if let Err(err @ InputPinError::SymlinkRejected(_)) =
                pin_entry_from_root(root, &segs, display_path)
            {
                return Some(err);
            }
        }
    }
    None
}

fn pin_entry_from_root(
    matched_root: &PinnedRoot,
    segments: &[String],
    display_path: &str,
) -> Result<PinnedInputEntry, InputPinError> {
    if segments.is_empty() {
        let parent_path = matched_root
            .canon_root
            .parent()
            .ok_or_else(|| InputPinError::OutsideAllowedRoot(display_path.to_string()))?;
        let leaf_name = matched_root
            .canon_root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| InputPinError::OutsideAllowedRoot(display_path.to_string()))?
            .to_string();
        let parent_dir =
            PinnedDir::open_existing_dir(parent_path).map_err(|e| map_walk_err(e, display_path))?;
        verify_no_symlinks_pinned(&matched_root.dir, display_path, 0, &mut 0)?;
        return Ok(PinnedInputEntry {
            parent_dir,
            leaf_name: leaf_name.clone(),
            staged_basename: leaf_name,
        });
    }

    let (parent_dir, leaf_name) = if segments.len() == 1 {
        let parent_dir = matched_root
            .dir
            .try_clone()
            .map_err(|e| map_walk_err(e, display_path))?;
        (parent_dir, segments[0].clone())
    } else {
        let mut cur = matched_root
            .dir
            .open_subdir(&segments[0])
            .map_err(|e| map_walk_err(e, display_path))?;
        for seg in &segments[1..segments.len() - 1] {
            cur = cur
                .open_subdir(seg)
                .map_err(|e| map_walk_err(e, display_path))?;
        }
        (cur, segments[segments.len() - 1].clone())
    };

    match parent_dir.is_regular_file_nofollow(&leaf_name) {
        Ok(true) => {}
        Ok(false) => {
            let sub_dir = parent_dir
                .open_subdir(&leaf_name)
                .map_err(|e| map_walk_err(e, display_path))?;
            verify_no_symlinks_pinned(&sub_dir, display_path, 0, &mut 0)?;
        }
        Err(err) => return Err(map_walk_err(err, display_path)),
    }

    Ok(PinnedInputEntry {
        parent_dir,
        leaf_name: leaf_name.clone(),
        staged_basename: leaf_name,
    })
}

fn verify_no_symlinks_pinned(
    dir: &PinnedDir,
    display_path: &str,
    depth: usize,
    seen_entries: &mut usize,
) -> Result<(), InputPinError> {
    if depth > MAX_TREE_DEPTH {
        return Err(InputPinError::InvalidConfig(format!(
            "input directory '{display_path}' exceeds maximum directory depth ({MAX_TREE_DEPTH})"
        )));
    }
    let remaining = MAX_DIR_ENTRIES.saturating_sub(*seen_entries);
    let names = dir.list_entry_names_bounded(remaining).map_err(|e| {
        if e.kind() == io::ErrorKind::InvalidData {
            InputPinError::InvalidConfig(format!(
                "input directory '{display_path}' exceeds maximum entry budget ({MAX_DIR_ENTRIES})"
            ))
        } else {
            map_walk_err(e, display_path)
        }
    })?;
    *seen_entries = seen_entries.saturating_add(names.len());
    for name in names {
        match dir.is_regular_file_nofollow(&name) {
            Ok(true) => {}
            Ok(false) => {
                let sub = dir
                    .open_subdir(&name)
                    .map_err(|e| map_walk_err(e, display_path))?;
                verify_no_symlinks_pinned(&sub, display_path, depth + 1, seen_entries)?;
            }
            Err(err) => return Err(map_walk_err(err, display_path)),
        }
    }
    Ok(())
}

fn map_walk_err(err: io::Error, display_path: &str) -> InputPinError {
    if err.kind() == io::ErrorKind::NotFound {
        InputPinError::MissingInput(display_path.to_string())
    } else if err.kind() == io::ErrorKind::InvalidInput {
        InputPinError::SymlinkRejected(display_path.to_string())
    } else {
        InputPinError::OutsideAllowedRoot(display_path.to_string())
    }
}

fn validate_missing_path_scope(
    input_path: &Path,
    pinned_roots: &[PinnedRoot],
    symlinked_roots: &[SymlinkedRoot],
) -> Result<(), InputPinError> {
    let display_path = input_path.display().to_string();
    let abs_path = to_abs_path(input_path)
        .map_err(|_| InputPinError::OutsideAllowedRoot(display_path.clone()))?;

    let mut existing = abs_path.as_path();
    let mut tail_segments: Vec<OsString> = Vec::new();
    let existing_meta = loop {
        match fs::symlink_metadata(existing) {
            Ok(meta) => break meta,
            Err(_) => {
                let mut comps = existing.components();
                let Some(last) = comps.next_back() else {
                    return Err(InputPinError::OutsideAllowedRoot(display_path));
                };
                match last {
                    Component::Normal(seg) => tail_segments.push(seg.to_os_string()),
                    Component::ParentDir => tail_segments.push(OsString::from("..")),
                    Component::CurDir => {}
                    Component::RootDir | Component::Prefix(_) => {
                        return Err(InputPinError::OutsideAllowedRoot(display_path));
                    }
                }
                existing = comps.as_path();
                if existing.as_os_str().is_empty() {
                    return Err(InputPinError::OutsideAllowedRoot(display_path));
                }
            }
        }
    };

    if existing_meta.file_type().is_symlink() {
        return Err(InputPinError::SymlinkRejected(display_path));
    }

    let mut resolved = fs::canonicalize(existing)
        .map_err(|_| InputPinError::OutsideAllowedRoot(display_path.clone()))?;
    for seg in tail_segments.into_iter().rev() {
        if seg == ".." {
            if !resolved.pop() {
                return Err(InputPinError::OutsideAllowedRoot(display_path));
            }
        } else if seg != "." {
            resolved.push(seg);
        }
    }

    if matches_symlinked_root(&abs_path, Some(&resolved), symlinked_roots) {
        return Err(InputPinError::SymlinkRejected(display_path));
    }

    if !pinned_roots
        .iter()
        .any(|root| resolved.starts_with(&root.canon_root))
    {
        return Err(InputPinError::OutsideAllowedRoot(display_path));
    }
    Ok(())
}

fn record_input_rejection_impasse(
    ctx: &RunContext<'_>,
    reason_code: &str,
    path_label: &str,
    rejected_path: &str,
    detail: &str,
) -> Result<DispatchOutcome, ConveyorError> {
    let policy = RedactionPolicy {
        public_safe_mode: ctx.config.public_safe_mode,
    };
    let safe_rejected_path = sanitize_escalation_path(rejected_path, &policy);
    let escalation = format!(
        "# Conveyor Escalation Report\n\n\
         - Reason: `{reason_code}`\n\
         - Project ID: `{}`\n\
         - Run ID: `{}`\n\
         - Harness: `{}` (`{}`)\n\
         - {path_label}: `{safe_rejected_path}`\n\n\
         {detail}\n",
        ctx.req.project_id, ctx.build_run_id, ctx.builder.id, ctx.builder.family
    );
    write_escalation_file(&ctx.out_dir, &escalation)?;
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
    status1: HarnessExit,
    status2: HarnessExit,
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
        format_harness_exit(status1),
        format_harness_exit(status2),
    );
    write_escalation_file(&ctx.out_dir, &escalation)?;
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
    PinnedDir::open_or_create_dir(&work_accept_dir)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    PinnedDir::open_or_create_dir(&out_accept_dir)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

    // Freeze builder outputs read-only during acceptance so acceptor cannot mutate them.
    freeze_readonly_tree(&ctx.out_dir)?;

    let accept_run_id = format!("run-{}", uuid::Uuid::new_v4());
    let accept_started_at = utc_now_rfc3339();
    let accept_env = [
        (
            "STREAMLINE_PROJECT_ID",
            OsStr::new(ctx.req.project_id.as_str()),
        ),
        ("STREAMLINE_PHASE", OsStr::new("accept")),
        ("STREAMLINE_STAGE_DIR", ctx.stage_dir.as_os_str()),
        ("STREAMLINE_BUILD_OUT_DIR", ctx.out_dir.as_os_str()),
        ("STREAMLINE_WORK_DIR", work_accept_dir.as_os_str()),
        ("STREAMLINE_OUT_DIR", out_accept_dir.as_os_str()),
    ];

    let accept_status = run_harness_command(
        ctx.config.runs_dir.trim(),
        acceptor,
        &work_accept_dir,
        &accept_env,
    )?;
    if !accept_status.is_success() {
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
            format_harness_exit(accept_status),
        );
        write_escalation_file(&out_accept_dir, &escalation)?;
        let cleanup_clean = verify_cleanup_clean(&work_accept_dir).is_ok();
        let scan_clean = matches!(
            scan_utf8_outputs(&out_accept_dir, policy)?,
            OutputScanOutcome::Clean
        );
        let out_accept_manifest_sha256 = compute_output_manifest_sha256(&out_accept_dir, true)?;
        let accept_run = ConveyorRun {
            run_id: accept_run_id,
            project_id: ctx.req.project_id.clone(),
            phase: RunPhase::Accept,
            harness_id: acceptor.id.clone(),
            family: acceptor.family.clone(),
            input_manifest_sha256: build_run.output_manifest_sha256.clone(),
            output_manifest_sha256: out_accept_manifest_sha256,
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

    match scan_utf8_outputs(&out_accept_dir, policy)? {
        OutputScanOutcome::Clean => {}
        OutputScanOutcome::Violations(findings) => {
            let escalation = format_redaction_escalation(
                &ctx.req.project_id,
                &accept_run_id,
                "accept",
                &acceptor.id,
                &acceptor.family,
                &findings,
            );
            return record_accept_scan_impasse(
                ctx,
                build_run,
                accept_run_id,
                accept_started_at,
                acceptor,
                &out_accept_dir,
                escalation,
                false,
            );
        }
        OutputScanOutcome::QuotaExceeded(reason) => {
            let escalation = format_quota_escalation(
                &ctx.req.project_id,
                &accept_run_id,
                "accept",
                &acceptor.id,
                &acceptor.family,
                &reason,
            );
            return record_accept_scan_impasse(
                ctx,
                build_run,
                accept_run_id,
                accept_started_at,
                acceptor,
                &out_accept_dir,
                escalation,
                true,
            );
        }
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
    runs_root: &str,
    harness: &HarnessSpec,
    cwd: &Path,
    extra_env: &[(&str, &OsStr)],
) -> Result<HarnessExit, ConveyorError> {
    run_harness_command_with_timeout(runs_root, harness, cwd, extra_env, MAX_HARNESS_RUNTIME)
}

pub(crate) fn run_harness_command_with_timeout(
    runs_root: &str,
    harness: &HarnessSpec,
    cwd: &Path,
    extra_env: &[(&str, &OsStr)],
    timeout: Duration,
) -> Result<HarnessExit, ConveyorError> {
    let _slot_guard = acquire_harness_slot(runs_root, harness)?;

    let mut cmd = Command::new(&harness.command[0]);
    if harness.command.len() > 1 {
        cmd.args(&harness.command[1..]);
    }
    cmd.current_dir(cwd);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.env_clear();
    for var_name in SAFE_ENV_VARS {
        if let Some(val) = std::env::var_os(var_name) {
            cmd.env(var_name, val);
        }
    }
    for (var_name, val) in extra_env {
        cmd.env(var_name, val);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd.spawn().map_err(|err| {
        ConveyorError::RunnerFailure(format!("failed to spawn harness '{}': {err}", harness.id))
    })?;
    let child_pgid = child.id();

    struct ProcessGroupCleanupGuard(u32);
    impl Drop for ProcessGroupCleanupGuard {
        fn drop(&mut self) {
            kill_process_group(self.0);
        }
    }
    let _pg_cleanup = ProcessGroupCleanupGuard(child_pgid);

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                kill_process_group(child_pgid);
                return Ok(HarnessExit::Exited(status));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    kill_process_group(child_pgid);
                    let _ = child.kill();
                    let _ = child.wait();
                    kill_process_group(child_pgid);
                    return Ok(HarnessExit::TimedOut(timeout));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => {
                kill_process_group(child_pgid);
                let _ = child.kill();
                let _ = child.wait();
                kill_process_group(child_pgid);
                return Err(ConveyorError::RunnerFailure(format!(
                    "failed waiting on harness '{}': {err}",
                    harness.id
                )));
            }
        }
    }
}

#[derive(Default)]
struct OutputScanState {
    seen_entries: usize,
    seen_files: usize,
    total_bytes: usize,
    findings: Vec<OutputScanFinding>,
}

fn scan_utf8_outputs(
    out_dir: &Path,
    policy: &RedactionPolicy,
) -> Result<OutputScanOutcome, ConveyorError> {
    let root_fd = match PinnedDir::open_existing_dir(out_dir) {
        Ok(fd) => fd,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(OutputScanOutcome::Clean),
        Err(err) => return Err(ConveyorError::StageFailure(err.to_string())),
    };

    let mut state = OutputScanState::default();
    if let Some(quota_reason) = scan_pinned_dir(&root_fd, "", 0, policy, &mut state)? {
        return Ok(OutputScanOutcome::QuotaExceeded(quota_reason));
    }
    if !state.findings.is_empty() {
        return Ok(OutputScanOutcome::Violations(state.findings));
    }
    Ok(OutputScanOutcome::Clean)
}

fn scan_pinned_dir(
    dir_fd: &PinnedDir,
    prefix: &str,
    depth: usize,
    policy: &RedactionPolicy,
    state: &mut OutputScanState,
) -> Result<Option<String>, ConveyorError> {
    if depth > MAX_TREE_DEPTH {
        return Ok(Some(format!(
            "output directory depth exceeds maximum {MAX_TREE_DEPTH}"
        )));
    }

    let remaining_entries = MAX_OUTPUT_FILES.saturating_sub(state.seen_entries);
    let names = match dir_fd.list_entry_names_bounded(remaining_entries) {
        Ok(names) => names,
        Err(err) if err.kind() == io::ErrorKind::InvalidData => {
            return Ok(Some(format!(
                "output directory listing rejected ({MAX_DIR_ENTRIES} entry cap): {err}"
            )));
        }
        Err(err) => return Err(ConveyorError::StageFailure(err.to_string())),
    };
    state.seen_entries = state.seen_entries.saturating_add(names.len());

    for name in names {
        let rel_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let safe_rel_path = sanitize_escalation_path(&rel_path, policy);
        if state.findings.len() < MAX_SCAN_FINDINGS {
            let name_report = inspect_text(&name, policy);
            if !name_report.ok {
                for violation in name_report.violations {
                    if state.findings.len() >= MAX_SCAN_FINDINGS {
                        break;
                    }
                    state.findings.push(OutputScanFinding {
                        rel_path: safe_rel_path.clone(),
                        line: 1,
                        kind: violation.kind,
                        reason: format!(
                            "output entry name failed redaction check: {}",
                            violation.reason
                        ),
                    });
                }
            }
        }
        match dir_fd.is_regular_file_nofollow(&name) {
            Ok(true) => {
                if state.seen_files >= MAX_OUTPUT_FILES {
                    return Ok(Some(format!(
                        "output file count exceeded maximum budget of {MAX_OUTPUT_FILES} files"
                    )));
                }
                state.seen_files += 1;

                let remaining = MAX_TOTAL_OUTPUT_BYTES.saturating_sub(state.total_bytes);
                let per_file_limit = MAX_SINGLE_OUTPUT_BYTES.min(remaining);
                let bytes = match dir_fd.read_bytes_nofollow(&name, per_file_limit) {
                    Ok(bytes) => bytes,
                    Err(err)
                        if err.kind() == io::ErrorKind::InvalidInput
                            && err
                                .to_string()
                                .contains("exceeds maximum allowed byte size") =>
                    {
                        let reason = if remaining < MAX_SINGLE_OUTPUT_BYTES {
                            format!(
                                "total output bytes exceeded maximum budget of {MAX_TOTAL_OUTPUT_BYTES} bytes at '{safe_rel_path}'"
                            )
                        } else {
                            format!(
                                "output file '{safe_rel_path}' exceeded single-file budget of {MAX_SINGLE_OUTPUT_BYTES} bytes"
                            )
                        };
                        return Ok(Some(reason));
                    }
                    Err(err) => return Err(ConveyorError::StageFailure(err.to_string())),
                };
                state.total_bytes = state.total_bytes.saturating_add(bytes.len());

                if state.findings.len() < MAX_SCAN_FINDINGS {
                    let text = String::from_utf8_lossy(&bytes);
                    let report = inspect_text(&text, policy);
                    if !report.ok {
                        for violation in report.violations {
                            if state.findings.len() >= MAX_SCAN_FINDINGS {
                                break;
                            }
                            state.findings.push(OutputScanFinding {
                                rel_path: safe_rel_path.clone(),
                                line: violation.line,
                                kind: violation.kind,
                                reason: violation.reason,
                            });
                        }
                    }
                }
            }
            Ok(false) => {
                let sub_fd = dir_fd
                    .open_subdir(&name)
                    .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                if let Some(reason) = scan_pinned_dir(&sub_fd, &rel_path, depth + 1, policy, state)?
                {
                    return Ok(Some(reason));
                }
            }
            Err(err) if err.kind() == io::ErrorKind::InvalidInput => {
                return Err(ConveyorError::StageFailure(
                    "symlinks are forbidden in harness output directories".to_string(),
                ));
            }
            Err(err) => return Err(ConveyorError::StageFailure(err.to_string())),
        }
    }

    Ok(None)
}

fn write_escalation_file(out_dir: &Path, escalation: &str) -> Result<(), ConveyorError> {
    let out_fd = PinnedDir::open_existing_dir(out_dir)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    out_fd
        .atomic_write_nofollow("ESCALATION.md", escalation.as_bytes(), 0o644)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))
}

fn compute_output_manifest_sha256(
    out_dir: &Path,
    allow_quota_empty: bool,
) -> Result<String, ConveyorError> {
    match compute_tree_manifest(out_dir) {
        Ok(manifest) => Ok(manifest.manifest_sha256),
        Err(ConveyorError::StageFailure(msg))
            if allow_quota_empty
                && (msg.contains("exceeds maximum") || msg.contains("not valid UTF-8")) =>
        {
            Ok(String::new())
        }
        Err(err) => Err(err),
    }
}

fn record_build_scan_impasse(
    ctx: &RunContext<'_>,
    input_manifest_sha256: &str,
    escalation: String,
    allow_quota_empty: bool,
) -> Result<DispatchOutcome, ConveyorError> {
    write_escalation_file(&ctx.out_dir, &escalation)?;
    let out_manifest_sha256 = compute_output_manifest_sha256(&ctx.out_dir, allow_quota_empty)?;
    let build_run = ConveyorRun {
        run_id: ctx.build_run_id.clone(),
        project_id: ctx.req.project_id.clone(),
        phase: RunPhase::Build,
        harness_id: ctx.builder.id.clone(),
        family: ctx.builder.family.clone(),
        input_manifest_sha256: input_manifest_sha256.to_string(),
        output_manifest_sha256: out_manifest_sha256,
        cleanup_clean: true,
        scan_clean: false,
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

#[allow(clippy::too_many_arguments)]
fn record_accept_scan_impasse(
    ctx: &RunContext<'_>,
    build_run: ConveyorRun,
    accept_run_id: String,
    accept_started_at: String,
    acceptor: &HarnessSpec,
    out_accept_dir: &Path,
    escalation: String,
    allow_quota_empty: bool,
) -> Result<DispatchOutcome, ConveyorError> {
    write_escalation_file(out_accept_dir, &escalation)?;
    let out_accept_manifest_sha256 =
        compute_output_manifest_sha256(out_accept_dir, allow_quota_empty)?;
    let accept_run = ConveyorRun {
        run_id: accept_run_id,
        project_id: ctx.req.project_id.clone(),
        phase: RunPhase::Accept,
        harness_id: acceptor.id.clone(),
        family: acceptor.family.clone(),
        input_manifest_sha256: build_run.output_manifest_sha256.clone(),
        output_manifest_sha256: out_accept_manifest_sha256,
        cleanup_clean: true,
        scan_clean: false,
        state: RunState::Impasse,
        started_at: accept_started_at,
        finished_at: Some(utc_now_rfc3339()),
    };
    Ok(DispatchOutcome {
        build_run,
        accept_run: Some(accept_run),
        final_state: RunState::Impasse,
        escalation_markdown: Some(escalation),
    })
}

fn format_quota_escalation(
    project_id: &str,
    run_id: &str,
    phase: &str,
    harness_id: &str,
    family: &str,
    reason: &str,
) -> String {
    format!(
        "# Conveyor Escalation Report\n\n\
         - Reason: `OUTPUT_QUOTA_EXCEEDED`\n\
         - Project ID: `{project_id}`\n\
         - Run ID: `{run_id}`\n\
         - Phase: `{phase}`\n\
         - Harness: `{harness_id}` (`{family}`)\n\
         - Detail: {reason}\n\n\
         Harness output exceeded safety scan budgets (max files: {MAX_OUTPUT_FILES}, \
         max single file: {MAX_SINGLE_OUTPUT_BYTES} bytes, max total: {MAX_TOTAL_OUTPUT_BYTES} bytes). \
         Escalated to impasse.\n"
    )
}

pub(crate) fn sanitize_path_label(raw_path: &str) -> String {
    let strict_policy = RedactionPolicy {
        public_safe_mode: true,
    };
    sanitize_escalation_path(raw_path, &strict_policy)
}

pub(crate) fn sanitize_escalation_path(raw_path: &str, policy: &RedactionPolicy) -> String {
    let strict_policy = RedactionPolicy {
        public_safe_mode: true,
    };
    let chars_ok = !raw_path.is_empty()
        && raw_path.len() <= 128
        && raw_path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-'));
    if chars_ok && inspect_text(raw_path, policy).ok && inspect_text(raw_path, &strict_policy).ok {
        raw_path.to_string()
    } else {
        let digest = streamline_safe::sha256_hex(raw_path.as_bytes());
        format!("redacted-path-{}", &digest[..12])
    }
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

fn format_harness_exit(exit: HarnessExit) -> String {
    match exit {
        HarnessExit::Exited(status) => format_exit_status(status),
        HarnessExit::TimedOut(limit) => {
            format!("timed out after {} ms", limit.as_millis())
        }
    }
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
