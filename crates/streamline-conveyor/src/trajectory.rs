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

    fs::create_dir_all(runs_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    let jsonl_path = runs_dir.join("trajectories.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&jsonl_path)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    file.write_all(line.as_bytes())
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    file.write_all(b"\n")
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    file.flush()
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

    Ok(jsonl_path)
}
