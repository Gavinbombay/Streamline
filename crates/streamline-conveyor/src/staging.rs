use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::ConveyorError;

const BANNED_SCRATCH_DIRS: [&str; 5] = ["target", ".venv", "venv", "node_modules", "__pycache__"];

/// Deterministic SHA-256 manifest for a staged input or output directory tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedManifest {
    pub manifest_sha256: String,
    pub file_count: usize,
    pub entries: Vec<(String, String)>,
}

/// Copies declared `input_paths` into `<stage_dir>/INPUTS/<basename>`, writes
/// `<stage_dir>/INPUTS/SHA256SUMS`, freezes permissions read-only (`0o444`/`0o555`
/// on Unix), and returns the deterministic [`StagedManifest`].
pub fn stage_readonly_inputs(
    input_paths: &[PathBuf],
    stage_dir: &Path,
) -> Result<StagedManifest, ConveyorError> {
    let mut seen_basenames = HashSet::new();
    let mut planned = Vec::with_capacity(input_paths.len());

    for raw_path in input_paths {
        verify_no_symlink_components(raw_path)?;
        let base_name = resolve_input_basename(raw_path)?;
        if !seen_basenames.insert(base_name.clone()) {
            return Err(ConveyorError::StageFailure(format!(
                "duplicate staged input basename '{base_name}'"
            )));
        }
        planned.push((raw_path.as_path(), base_name));
    }

    fs::create_dir_all(stage_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    let inputs_dir = stage_dir.join("INPUTS");
    if inputs_dir.exists() {
        unfreeze_readonly_dir(&inputs_dir)?;
        fs::remove_dir_all(&inputs_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    }
    fs::create_dir_all(&inputs_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

    for (src_path, base_name) in planned {
        let dst_path = inputs_dir.join(&base_name);
        copy_staged_entry(src_path, &dst_path)?;
    }

    let manifest = compute_tree_manifest(&inputs_dir)?;
    let mut sums_text = String::new();
    for (rel_path, digest) in &manifest.entries {
        sums_text.push_str(digest);
        sums_text.push_str("  ");
        sums_text.push_str(rel_path);
        sums_text.push('\n');
    }
    fs::write(inputs_dir.join("SHA256SUMS"), sums_text.as_bytes())
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

    freeze_readonly_tree(&inputs_dir)?;
    Ok(manifest)
}

/// Recursively restores writable owner permissions (`0o755` for directories and
/// executables, `0o644` for regular files on Unix) so staged or frozen trees can
/// be inspected or cleaned up without permission errors.
pub fn unfreeze_readonly_dir(dir: &Path) -> Result<(), ConveyorError> {
    if !dir.exists() {
        return Ok(());
    }
    unfreeze_recursive(dir, true)
}

/// Recursively sets read-only permissions (`0o555` for directories and executable
/// files, `0o444` for regular non-executable files on Unix) bottom-up.
pub(crate) fn freeze_readonly_tree(path: &Path) -> Result<(), ConveyorError> {
    if !path.exists() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let meta =
            fs::symlink_metadata(path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        if meta.file_type().is_symlink() {
            return Ok(());
        }
        if meta.is_dir() {
            let rd = fs::read_dir(path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
            for entry in rd {
                let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                freeze_readonly_tree(&entry.path())?;
            }
            fs::set_permissions(path, fs::Permissions::from_mode(0o555))
                .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        } else if meta.is_file() {
            let is_exec = meta.permissions().mode() & 0o111 != 0;
            let mode = if is_exec { 0o555 } else { 0o444 };
            fs::set_permissions(path, fs::Permissions::from_mode(mode))
                .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        }
    }

    #[cfg(not(unix))]
    {
        let meta =
            fs::symlink_metadata(path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        if meta.file_type().is_symlink() {
            return Ok(());
        }
        if meta.is_dir() {
            let rd = fs::read_dir(path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
            for entry in rd {
                let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                freeze_readonly_tree(&entry.path())?;
            }
        }
        let mut perms = meta.permissions();
        perms.set_readonly(true);
        fs::set_permissions(path, perms).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
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

/// Verifies that `path` and, when it is a directory, all of its walked
/// children are free of symbolic links.
pub fn verify_no_symlink_components(path: &Path) -> Result<(), ConveyorError> {
    let meta = fs::symlink_metadata(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            ConveyorError::MissingInput(path.display().to_string())
        } else {
            ConveyorError::StageFailure(err.to_string())
        }
    })?;
    if meta.file_type().is_symlink() {
        return Err(ConveyorError::StageFailure(
            "symlinks are forbidden in staged inputs".to_string(),
        ));
    }
    if meta.is_dir() {
        let rd = fs::read_dir(path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        for entry in rd {
            let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
            verify_no_symlink_components(&entry.path())?;
        }
    }
    Ok(())
}

/// Verifies no stray build or virtual-environment directories remain in a scratch tree.
pub fn verify_cleanup_clean(work_dir: &Path) -> Result<(), ConveyorError> {
    if !work_dir.exists() {
        return Ok(());
    }
    check_banned(work_dir, work_dir, &BANNED_SCRATCH_DIRS)
}

fn unfreeze_recursive(path: &Path, is_root: bool) -> Result<(), ConveyorError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let meta = if is_root {
            fs::metadata(path)
        } else {
            fs::symlink_metadata(path)
        }
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

        if !is_root && meta.file_type().is_symlink() {
            return Ok(());
        }
        if meta.is_dir() {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
            let rd = fs::read_dir(path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
            for entry in rd {
                let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                unfreeze_recursive(&entry.path(), false)?;
            }
        } else if meta.is_file() {
            let is_exec = meta.permissions().mode() & 0o111 != 0;
            let mode = if is_exec { 0o755 } else { 0o644 };
            fs::set_permissions(path, fs::Permissions::from_mode(mode))
                .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        }
    }

    #[cfg(not(unix))]
    {
        let meta = if is_root {
            fs::metadata(path)
        } else {
            fs::symlink_metadata(path)
        }
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

        if !is_root && meta.file_type().is_symlink() {
            return Ok(());
        }
        let mut perms = meta.permissions();
        perms.set_readonly(false);
        fs::set_permissions(path, perms).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        if meta.is_dir() {
            let rd = fs::read_dir(path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
            for entry in rd {
                let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                unfreeze_recursive(&entry.path(), false)?;
            }
        }
    }

    Ok(())
}

fn resolve_input_basename(path: &Path) -> Result<String, ConveyorError> {
    let os_name = match path.file_name() {
        Some(name) => name.to_os_string(),
        None => {
            let canonical =
                fs::canonicalize(path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
            canonical
                .file_name()
                .map(|n| n.to_os_string())
                .ok_or_else(|| {
                    ConveyorError::StageFailure(format!(
                        "input path '{}' has no basename",
                        path.display()
                    ))
                })?
        }
    };
    let text = os_name.to_str().ok_or_else(|| {
        ConveyorError::StageFailure(format!(
            "input path '{}' basename is not valid UTF-8",
            path.display()
        ))
    })?;
    if text.is_empty() || text == "." || text == ".." {
        return Err(ConveyorError::StageFailure(format!(
            "invalid staged input basename '{text}'"
        )));
    }
    Ok(text.to_string())
}

fn copy_staged_entry(src: &Path, dst: &Path) -> Result<(), ConveyorError> {
    let meta = fs::symlink_metadata(src).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    if meta.file_type().is_symlink() {
        return Err(ConveyorError::StageFailure(
            "symlinks are forbidden in staged inputs".to_string(),
        ));
    }
    if meta.is_dir() {
        copy_dir_recursive(src, dst)
    } else if meta.is_file() {
        copy_single_file(src, dst, &meta)
    } else {
        Err(ConveyorError::StageFailure(format!(
            "unsupported input entry type at '{}'",
            src.display()
        )))
    }
}

fn copy_dir_recursive(src_dir: &Path, dst_dir: &Path) -> Result<(), ConveyorError> {
    fs::create_dir_all(dst_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    let rd = fs::read_dir(src_dir).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    let mut children: Vec<PathBuf> = Vec::new();
    for entry in rd {
        let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        children.push(entry.path());
    }
    children.sort();

    for child_src in children {
        let Some(name) = child_src.file_name() else {
            continue;
        };
        let child_dst = dst_dir.join(name);
        let link_meta = fs::symlink_metadata(&child_src)
            .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        let ft = link_meta.file_type();
        if ft.is_symlink() {
            return Err(ConveyorError::StageFailure(
                "symlinks are forbidden in staged inputs".to_string(),
            ));
        } else if ft.is_dir() {
            copy_dir_recursive(&child_src, &child_dst)?;
        } else if ft.is_file() {
            copy_single_file(&child_src, &child_dst, &link_meta)?;
        }
    }

    Ok(())
}

fn copy_single_file(src: &Path, dst: &Path, src_meta: &fs::Metadata) -> Result<(), ConveyorError> {
    fs::copy(src, dst).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let is_exec = src_meta.permissions().mode() & 0o111 != 0;
        let mode = if is_exec { 0o755 } else { 0o644 };
        fs::set_permissions(dst, fs::Permissions::from_mode(mode))
            .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = src_meta;
        if let Ok(dst_meta) = fs::metadata(dst) {
            let mut perms = dst_meta.permissions();
            perms.set_readonly(false);
            let _ = fs::set_permissions(dst, perms);
        }
    }

    Ok(())
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
        let meta =
            fs::symlink_metadata(&path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        if meta.file_type().is_symlink() {
            return Err(ConveyorError::StageFailure(
                "symlinks are forbidden in manifest trees".to_string(),
            ));
        }
        if meta.is_dir() {
            collect_files(root, &path, out)?;
        } else if meta.is_file() {
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

fn check_banned(root: &Path, current: &Path, banned: &[&str]) -> Result<(), ConveyorError> {
    let rd = fs::read_dir(current).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_dir() {
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
