use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use streamline_core::{SyncVerdict, TrioProfile};
use streamline_safe::{require_safe_text, RedactionPolicy};

use crate::error::ConveyorError;

/// Summary payload exported as one compact JSONL trajectory line upon release approval.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrajectoryExportInput {
    pub project_id: String,
    pub title: String,
    pub profile: TrioProfile,
    pub own_statements: Vec<String>,
    pub div_summaries: Vec<String>,
    pub lesson_rules: Vec<String>,
    pub sync_verdict: Option<SyncVerdict>,
    pub how_it_works_sha256: Option<String>,
}

#[derive(Serialize)]
struct TrajectoryJsonlRow<'a> {
    #[serde(flatten)]
    input: &'a TrajectoryExportInput,
    exported_at: String,
}

/// Serializes `input` plus an RFC-3339 `exported_at` timestamp into compact single-line
/// JSON, verifies the line passes [`require_safe_text`], and appends it to
/// `<runs_dir>/trajectories.jsonl`.
pub fn append_trajectory_jsonl(
    runs_dir: &Path,
    policy: &RedactionPolicy,
    input: &TrajectoryExportInput,
) -> Result<PathBuf, ConveyorError> {
    if runs_dir.as_os_str().is_empty() {
        return Err(ConveyorError::InvalidConfig(
            "runs_dir must not be empty".to_string(),
        ));
    }

    let record = TrajectoryJsonlRow {
        input,
        exported_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    };
    let line = serde_json::to_string(&record).map_err(|e| {
        ConveyorError::StageFailure(format!("failed to serialize trajectory line: {e}"))
    })?;

    if let Err(violations) = require_safe_text(&line, policy) {
        let summary = violations
            .iter()
            .map(|item| format!("line {} ({:?}): {}", item.line, item.kind, item.reason))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ConveyorError::RedactionBlocked(summary));
    }

    ensure_safe_runs_dir(runs_dir)?;

    let target = runs_dir.join("trajectories.jsonl");
    let existing_bytes = match fs::symlink_metadata(&target) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(ConveyorError::StageFailure(
                    "refusing to write through symlinked trajectories.jsonl".to_string(),
                ));
            }
            if !meta.is_file() {
                return Err(ConveyorError::StageFailure(
                    "trajectories.jsonl exists and is not a regular file".to_string(),
                ));
            }
            fs::read(&target).map_err(|e| ConveyorError::StageFailure(e.to_string()))?
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(err) => return Err(ConveyorError::StageFailure(err.to_string())),
    };

    let tmp_path = runs_dir.join(format!(".trajectories.{}.tmp", uuid::Uuid::new_v4()));
    let write_result = (|| -> Result<(), ConveyorError> {
        let mut tmp_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        if !existing_bytes.is_empty() {
            tmp_file
                .write_all(&existing_bytes)
                .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        }
        tmp_file
            .write_all(line.as_bytes())
            .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        tmp_file
            .write_all(b"\n")
            .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        tmp_file
            .sync_all()
            .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    if let Ok(meta) = fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() {
            let _ = fs::remove_file(&tmp_path);
            return Err(ConveyorError::StageFailure(
                "refusing to write through symlinked trajectories.jsonl".to_string(),
            ));
        }
    }

    if let Err(err) = fs::rename(&tmp_path, &target) {
        let _ = fs::remove_file(&tmp_path);
        return Err(ConveyorError::StageFailure(err.to_string()));
    }

    Ok(target)
}

fn ensure_safe_runs_dir(runs_dir: &Path) -> Result<(), ConveyorError> {
    if let Some(parent) = runs_dir.parent() {
        if !parent.as_os_str().is_empty() {
            if let Ok(parent_meta) = fs::symlink_metadata(parent) {
                if parent_meta.file_type().is_symlink() {
                    return Err(ConveyorError::StageFailure(
                        "refusing to write trajectory under symlinked parent directory".to_string(),
                    ));
                }
            }
        }
    }

    match fs::symlink_metadata(runs_dir) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(ConveyorError::StageFailure(
                    "refusing to write trajectory into symlinked runs_dir".to_string(),
                ));
            }
            if !meta.is_dir() {
                return Err(ConveyorError::StageFailure(
                    "runs_dir exists and is not a directory".to_string(),
                ));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            for anc in runs_dir.ancestors().skip(1) {
                if anc.as_os_str().is_empty() {
                    break;
                }
                match fs::symlink_metadata(anc) {
                    Ok(anc_meta) => {
                        if anc_meta.file_type().is_symlink() {
                            return Err(ConveyorError::StageFailure(
                                "refusing to create runs_dir under symlinked parent directory"
                                    .to_string(),
                            ));
                        }
                        if !anc_meta.is_dir() {
                            return Err(ConveyorError::StageFailure(
                                "runs_dir parent exists and is not a directory".to_string(),
                            ));
                        }
                        break;
                    }
                    Err(anc_err) if anc_err.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(anc_err) => return Err(ConveyorError::StageFailure(anc_err.to_string())),
                }
            }
            fs::create_dir_all(runs_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        }
        Err(err) => return Err(ConveyorError::StageFailure(err.to_string())),
    }

    let final_meta =
        fs::symlink_metadata(runs_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    if final_meta.file_type().is_symlink() || !final_meta.is_dir() {
        return Err(ConveyorError::StageFailure(
            "runs_dir must be a real directory and not a symlink".to_string(),
        ));
    }

    Ok(())
}
