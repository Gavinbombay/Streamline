//! Hands-off input staging, runner interface, cleanup verification, and cross-family check guards.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConveyorError {
    #[error("builder family '{0}' cannot act as its own acceptor for R2+ work")]
    SameFamilyForbidden(String),
    #[error("cleanup failed: stray artifact found at '{0}'")]
    DirtyScratch(String),
    #[error("staging failed: {0}")]
    StageFailure(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedManifest {
    pub manifest_sha256: String,
    pub file_count: usize,
    pub entries: Vec<(String, String)>,
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

/// Computes a deterministic sorted SHA-256 manifest for a directory tree.
pub fn compute_tree_manifest(root: &Path) -> Result<StagedManifest, ConveyorError> {
    let mut entries = Vec::new();
    collect_files(root, root, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut combined = Sha256::new();
    for (rel, digest) in &entries {
        combined.update(digest.as_bytes());
        combined.update(b"  ");
        combined.update(rel.as_bytes());
        combined.update(b"\n");
    }

    Ok(StagedManifest {
        manifest_sha256: hex::encode(combined.finalize()),
        file_count: entries.len(),
        entries,
    })
}

fn collect_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, String)>,
) -> Result<(), ConveyorError> {
    if !current.exists() {
        return Ok(());
    }
    let rd = fs::read_dir(current).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|d| d.path())).collect();
    paths.sort();

    for path in paths {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name == "SHA256SUMS" || name == ".git" {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if path.is_file() {
            let bytes = fs::read(&path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
            let digest = hex::encode(Sha256::digest(&bytes));
            let rel = path
                .strip_prefix(root)
                .map_err(|e| ConveyorError::StageFailure(e.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, digest));
        }
    }
    Ok(())
}

/// Verifies no stray build or virtual-environment directories remain in a scratch tree.
pub fn verify_cleanup_clean(work_dir: &Path) -> Result<(), ConveyorError> {
    if !work_dir.exists() {
        return Ok(());
    }
    let banned_dirs = ["target", ".venv", "venv", "node_modules", "__pycache__"];
    check_banned(work_dir, work_dir, &banned_dirs)
}

fn check_banned(root: &Path, current: &Path, banned: &[&str]) -> Result<(), ConveyorError> {
    let rd = fs::read_dir(current).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if banned.contains(&name) {
                let rel = path
                    .strip_prefix(root)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| name.to_string());
                return Err(ConveyorError::DirtyScratch(rel));
            }
            check_banned(root, &path, banned)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_cross_family_on_r2_and_above() {
        assert!(verify_cross_family(2, "openai-codex", "openai-codex").is_err());
        assert!(verify_cross_family(2, "openai-codex", "google-gemini").is_ok());
        assert!(verify_cross_family(1, "openai-codex", "openai-codex").is_ok());
    }
}
