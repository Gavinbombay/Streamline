use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use streamline_safe::PinnedDir;

use crate::error::ConveyorError;

const BANNED_SCRATCH_DIRS: [&str; 5] = ["target", ".venv", "venv", "node_modules", "__pycache__"];
pub const MAX_MANIFEST_FILES: usize = crate::runner::MAX_OUTPUT_FILES;
pub const MAX_MANIFEST_FILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_MANIFEST_TOTAL_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_SCRATCH_ENTRIES: usize = 4_096;
const MAX_STAGED_FILE_BYTES: usize = MAX_MANIFEST_FILE_BYTES;

/// Deterministic SHA-256 manifest for a staged input or output directory tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedManifest {
    pub manifest_sha256: String,
    pub file_count: usize,
    pub entries: Vec<(String, String)>,
}

/// A descriptor-pinned input entry validated in scope for staging without
/// subsequent pathname resolution.
pub(crate) struct PinnedInputEntry {
    pub(crate) parent_dir: PinnedDir,
    pub(crate) leaf_name: String,
    pub(crate) staged_basename: String,
}

/// Copies descriptor-pinned `entries` into `<stage_dir>/INPUTS/<staged_basename>`,
/// writes `<stage_dir>/INPUTS/SHA256SUMS`, freezes permissions read-only
/// (`0o444`/`0o555` on Unix), and returns the deterministic [`StagedManifest`].
pub(crate) fn stage_pinned_inputs(
    entries: &[PinnedInputEntry],
    stage_dir: &Path,
) -> Result<StagedManifest, ConveyorError> {
    let mut seen_basenames = HashSet::new();
    for entry in entries {
        validate_single_segment(&entry.staged_basename)?;
        if !seen_basenames.insert(entry.staged_basename.as_str()) {
            return Err(ConveyorError::StageFailure(format!(
                "duplicate staged input basename '{}'",
                entry.staged_basename
            )));
        }
    }

    let stage_fd = PinnedDir::open_or_create_dir(stage_dir)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

    match stage_fd.is_regular_file_nofollow("INPUTS") {
        Ok(false) => {
            let inputs_dir = stage_dir.join("INPUTS");
            unfreeze_readonly_dir(&inputs_dir)?;
            fs::remove_dir_all(&inputs_dir)
                .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        }
        Ok(true) => {
            return Err(ConveyorError::StageFailure(
                "INPUTS exists and is not a directory".to_string(),
            ));
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(ConveyorError::StageFailure(err.to_string())),
    }

    let inputs_fd = stage_fd
        .open_or_create_subdir("INPUTS", 0o755)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

    if entries.len() > MAX_MANIFEST_FILES {
        return Err(ConveyorError::StageFailure(format!(
            "staged input entry count exceeds maximum of {MAX_MANIFEST_FILES} entries"
        )));
    }
    let mut copy_budget = StageCopyBudget {
        seen_entries: entries.len(),
        ..StageCopyBudget::default()
    };
    for entry in entries {
        match entry.parent_dir.is_regular_file_nofollow(&entry.leaf_name) {
            Ok(true) => {
                copy_pinned_file(
                    &entry.parent_dir,
                    &entry.leaf_name,
                    &inputs_fd,
                    &entry.staged_basename,
                    &mut copy_budget,
                )?;
            }
            Ok(false) => {
                let sub_src = entry
                    .parent_dir
                    .open_subdir(&entry.leaf_name)
                    .map_err(|e| map_pinned_input_err(e, &entry.leaf_name))?;
                let sub_dst = inputs_fd
                    .open_or_create_subdir(&entry.staged_basename, 0o755)
                    .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                copy_pinned_dir_recursive(&sub_src, &sub_dst, 1, &mut copy_budget)?;
            }
            Err(err) => return Err(map_pinned_input_err(err, &entry.leaf_name)),
        }
    }

    let manifest = compute_pinned_manifest(&inputs_fd)?;
    let mut sums_text = String::new();
    for (rel_path, digest) in &manifest.entries {
        sums_text.push_str(digest);
        sums_text.push_str("  ");
        sums_text.push_str(rel_path);
        sums_text.push('\n');
    }
    inputs_fd
        .atomic_write_nofollow("SHA256SUMS", sums_text.as_bytes(), 0o444)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;

    let inputs_dir = stage_dir.join("INPUTS");
    freeze_readonly_tree(&inputs_dir)?;
    Ok(manifest)
}

/// Copies declared `input_paths` into `<stage_dir>/INPUTS/<basename>`, writes
/// `<stage_dir>/INPUTS/SHA256SUMS`, freezes permissions read-only (`0o444`/`0o555`
/// on Unix), and returns the deterministic [`StagedManifest`].
pub fn stage_readonly_inputs(
    input_paths: &[PathBuf],
    stage_dir: &Path,
) -> Result<StagedManifest, ConveyorError> {
    let mut seen_basenames = HashSet::new();
    let mut pinned = Vec::with_capacity(input_paths.len());

    for raw_path in input_paths {
        let trimmed = trim_trailing_slashes_and_dot(raw_path);
        verify_no_symlink_components(&trimmed)?;
        let staged_basename = resolve_input_basename(&trimmed)?;
        if !seen_basenames.insert(staged_basename.clone()) {
            return Err(ConveyorError::StageFailure(format!(
                "duplicate staged input basename '{staged_basename}'"
            )));
        }

        let (parent_dir, leaf_name) = open_parent_and_leaf(&trimmed, raw_path)?;
        pinned.push(PinnedInputEntry {
            parent_dir,
            leaf_name,
            staged_basename,
        });
    }

    stage_pinned_inputs(&pinned, stage_dir)
}

/// Recursively restores writable owner permissions (`0o755` for directories and
/// executables, `0o644` for regular files on Unix) so staged or frozen trees can
/// be inspected or cleaned up without permission errors.
pub fn unfreeze_readonly_dir(dir: &Path) -> Result<(), ConveyorError> {
    if !dir.exists() {
        return Ok(());
    }
    let mut seen_entries = 0;
    unfreeze_recursive(dir, true, 0, &mut seen_entries)
}

/// Recursively sets read-only permissions (`0o555` for directories and executable
/// files, `0o444` for regular non-executable files on Unix) bottom-up.
pub(crate) fn freeze_readonly_tree(path: &Path) -> Result<(), ConveyorError> {
    if !path.exists() {
        return Ok(());
    }
    let mut seen_entries = 0;
    freeze_readonly_tree_bounded(path, 0, &mut seen_entries)
}

fn freeze_readonly_tree_bounded(
    path: &Path,
    depth: usize,
    seen_entries: &mut usize,
) -> Result<(), ConveyorError> {
    if depth > crate::runner::MAX_TREE_DEPTH {
        return Err(ConveyorError::StageFailure(format!(
            "freeze tree depth exceeds maximum of {}",
            crate::runner::MAX_TREE_DEPTH
        )));
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
                if *seen_entries >= MAX_SCRATCH_ENTRIES {
                    return Err(ConveyorError::StageFailure(format!(
                        "freeze tree entry count exceeds maximum of {MAX_SCRATCH_ENTRIES}"
                    )));
                }
                *seen_entries += 1;
                let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                freeze_readonly_tree_bounded(&entry.path(), depth + 1, seen_entries)?;
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
                if *seen_entries >= MAX_SCRATCH_ENTRIES {
                    return Err(ConveyorError::StageFailure(format!(
                        "freeze tree entry count exceeds maximum of {MAX_SCRATCH_ENTRIES}"
                    )));
                }
                *seen_entries += 1;
                let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                freeze_readonly_tree_bounded(&entry.path(), depth + 1, seen_entries)?;
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
    let trimmed = trim_trailing_slashes_and_dot(root);
    match PinnedDir::open_existing_dir(&trimmed) {
        Ok(root_fd) => compute_pinned_manifest(&root_fd),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(StagedManifest {
            manifest_sha256: hex::encode(Sha256::new().finalize()),
            file_count: 0,
            entries: Vec::new(),
        }),
        Err(err) => Err(ConveyorError::StageFailure(err.to_string())),
    }
}

/// Verifies that `path` and, when it is a directory, all of its walked
/// children are free of symbolic links.
pub fn verify_no_symlink_components(path: &Path) -> Result<(), ConveyorError> {
    let trimmed = trim_trailing_slashes_and_dot(path);
    let meta = fs::symlink_metadata(&trimmed).map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
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
        let dir_fd = PinnedDir::open_existing_dir(&trimmed)
            .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        let mut seen_entries = 0;
        verify_no_symlinks_in_pinned_dir(&dir_fd, 0, &mut seen_entries)?;
    }
    Ok(())
}

/// Verifies no stray build or virtual-environment directories remain in a scratch tree.
pub fn verify_cleanup_clean(work_dir: &Path) -> Result<(), ConveyorError> {
    if !work_dir.exists() {
        return Ok(());
    }
    let mut seen_entries = 0;
    check_banned(
        work_dir,
        work_dir,
        &BANNED_SCRATCH_DIRS,
        0,
        &mut seen_entries,
    )
}

pub(crate) fn trim_trailing_slashes_and_dot(path: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let mut b = path.as_os_str().as_bytes();
        loop {
            if b.len() > 1 && b.ends_with(b"/") {
                b = &b[..b.len() - 1];
                continue;
            }
            if b.len() > 2 && b.ends_with(b"/.") {
                b = &b[..b.len() - 2];
                continue;
            }
            if b == b"/." {
                b = b"/";
                continue;
            }
            break;
        }
        PathBuf::from(OsStr::from_bytes(b))
    }
    #[cfg(not(unix))]
    {
        path.to_path_buf()
    }
}

fn verify_no_symlinks_in_pinned_dir(
    dir_fd: &PinnedDir,
    depth: usize,
    seen_entries: &mut usize,
) -> Result<(), ConveyorError> {
    if depth > crate::runner::MAX_TREE_DEPTH {
        return Err(ConveyorError::StageFailure(format!(
            "input directory depth exceeds maximum of {}",
            crate::runner::MAX_TREE_DEPTH
        )));
    }
    let remaining = MAX_MANIFEST_FILES.saturating_sub(*seen_entries);
    let names = dir_fd.list_entry_names_bounded(remaining).map_err(|e| {
        if e.kind() == io::ErrorKind::InvalidData {
            ConveyorError::StageFailure(format!(
                "input directory entry count exceeds maximum of {MAX_MANIFEST_FILES}"
            ))
        } else {
            ConveyorError::StageFailure(e.to_string())
        }
    })?;
    *seen_entries = seen_entries.saturating_add(names.len());
    for name in names {
        match dir_fd.is_regular_file_nofollow(&name) {
            Ok(true) => {}
            Ok(false) => {
                let sub_fd = dir_fd
                    .open_subdir(&name)
                    .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                verify_no_symlinks_in_pinned_dir(&sub_fd, depth + 1, seen_entries)?;
            }
            Err(err) => return Err(ConveyorError::StageFailure(err.to_string())),
        }
    }
    Ok(())
}

fn open_parent_and_leaf(
    trimmed: &Path,
    display_path: &Path,
) -> Result<(PinnedDir, String), ConveyorError> {
    if let (Some(parent), Some(name)) = (trimmed.parent(), trimmed.file_name()) {
        let leaf_name = name
            .to_str()
            .ok_or_else(|| {
                ConveyorError::StageFailure(format!(
                    "input path '{}' basename is not valid UTF-8",
                    display_path.display()
                ))
            })?
            .to_string();
        let parent_ref = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        let parent_dir = PinnedDir::open_existing_dir(parent_ref)
            .map_err(|e| map_pinned_input_err(e, &display_path.display().to_string()))?;
        return Ok((parent_dir, leaf_name));
    }

    let canonical = fs::canonicalize(trimmed).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ConveyorError::MissingInput(display_path.display().to_string())
        } else {
            ConveyorError::StageFailure(e.to_string())
        }
    })?;
    let parent = canonical.parent().ok_or_else(|| {
        ConveyorError::StageFailure(format!(
            "input path '{}' has no parent directory",
            display_path.display()
        ))
    })?;
    let leaf_name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            ConveyorError::StageFailure(format!(
                "input path '{}' has no valid UTF-8 basename",
                display_path.display()
            ))
        })?
        .to_string();
    let parent_dir = PinnedDir::open_existing_dir(parent)
        .map_err(|e| map_pinned_input_err(e, &display_path.display().to_string()))?;
    Ok((parent_dir, leaf_name))
}

#[derive(Default)]
struct StageCopyBudget {
    seen_entries: usize,
    copied_files: usize,
    copied_bytes: usize,
}

fn copy_pinned_dir_recursive(
    src_dir: &PinnedDir,
    dst_dir: &PinnedDir,
    depth: usize,
    budget: &mut StageCopyBudget,
) -> Result<(), ConveyorError> {
    if depth > crate::runner::MAX_TREE_DEPTH {
        return Err(ConveyorError::StageFailure(format!(
            "staged input directory depth exceeds maximum of {}",
            crate::runner::MAX_TREE_DEPTH
        )));
    }
    let remaining_entries = MAX_MANIFEST_FILES.saturating_sub(budget.seen_entries);
    let child_names = src_dir
        .list_entry_names_bounded(remaining_entries)
        .map_err(|e| {
            if e.kind() == io::ErrorKind::InvalidData {
                ConveyorError::StageFailure(format!(
                    "staged input entry count exceeds maximum of {MAX_MANIFEST_FILES}"
                ))
            } else {
                ConveyorError::StageFailure(e.to_string())
            }
        })?;
    budget.seen_entries = budget.seen_entries.saturating_add(child_names.len());
    for child in child_names {
        match src_dir.is_regular_file_nofollow(&child) {
            Ok(true) => copy_pinned_file(src_dir, &child, dst_dir, &child, budget)?,
            Ok(false) => {
                let sub_src = src_dir
                    .open_subdir(&child)
                    .map_err(|e| map_pinned_input_err(e, &child))?;
                let sub_dst = dst_dir
                    .open_or_create_subdir(&child, 0o755)
                    .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                copy_pinned_dir_recursive(&sub_src, &sub_dst, depth + 1, budget)?;
            }
            Err(err) => return Err(map_pinned_input_err(err, &child)),
        }
    }
    Ok(())
}

fn copy_pinned_file(
    src_dir: &PinnedDir,
    src_name: &str,
    dst_dir: &PinnedDir,
    dst_name: &str,
    budget: &mut StageCopyBudget,
) -> Result<(), ConveyorError> {
    if budget.copied_files >= MAX_MANIFEST_FILES {
        return Err(ConveyorError::StageFailure(format!(
            "staged input file count exceeds maximum of {MAX_MANIFEST_FILES} files"
        )));
    }
    let remaining = MAX_MANIFEST_TOTAL_BYTES.saturating_sub(budget.copied_bytes);

    let mut src_file = src_dir
        .open_file_nofollow(src_name)
        .map_err(|e| map_pinned_input_err(e, src_name))?;
    let src_meta = src_file
        .metadata()
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    if src_meta.len() > MAX_STAGED_FILE_BYTES as u64 {
        return Err(ConveyorError::StageFailure(format!(
            "input file '{src_name}' exceeds maximum staged size"
        )));
    }
    if src_meta.len() > remaining as u64 {
        return Err(ConveyorError::StageFailure(format!(
            "staged input total size exceeds maximum of {MAX_MANIFEST_TOTAL_BYTES} bytes at '{src_name}'"
        )));
    }

    #[cfg(unix)]
    let file_mode = {
        use std::os::unix::fs::PermissionsExt;
        if src_meta.permissions().mode() & 0o111 != 0 {
            0o755
        } else {
            0o644
        }
    };
    #[cfg(not(unix))]
    let file_mode = 0o644;

    let read_cap = MAX_STAGED_FILE_BYTES.min(remaining);
    let cap = usize::try_from(src_meta.len()).unwrap_or(0).min(read_cap);
    let mut bytes = Vec::with_capacity(cap);
    src_file
        .by_ref()
        .take((read_cap as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    if bytes.len() > MAX_STAGED_FILE_BYTES {
        return Err(ConveyorError::StageFailure(format!(
            "input file '{src_name}' exceeds maximum staged size"
        )));
    }
    if bytes.len() > remaining {
        return Err(ConveyorError::StageFailure(format!(
            "staged input total size exceeds maximum of {MAX_MANIFEST_TOTAL_BYTES} bytes at '{src_name}'"
        )));
    }

    budget.copied_files += 1;
    budget.copied_bytes = budget.copied_bytes.saturating_add(bytes.len());

    dst_dir
        .atomic_write_nofollow(dst_name, &bytes, file_mode)
        .map_err(|e| ConveyorError::StageFailure(e.to_string()))
}

struct ManifestAccumulator {
    entries: Vec<(String, String)>,
    seen_entries: usize,
    total_bytes: usize,
}

fn compute_pinned_manifest(root_fd: &PinnedDir) -> Result<StagedManifest, ConveyorError> {
    let mut acc = ManifestAccumulator {
        entries: Vec::new(),
        seen_entries: 0,
        total_bytes: 0,
    };
    collect_files_pinned(root_fd, "", 0, &mut acc)?;
    acc.entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut combined = Sha256::new();
    for (rel, digest) in &acc.entries {
        combined.update(digest.as_bytes());
        combined.update(b"  ");
        combined.update(rel.as_bytes());
        combined.update(b"\n");
    }

    Ok(StagedManifest {
        manifest_sha256: hex::encode(combined.finalize()),
        file_count: acc.entries.len(),
        entries: acc.entries,
    })
}

fn collect_files_pinned(
    dir_fd: &PinnedDir,
    prefix: &str,
    depth: usize,
    acc: &mut ManifestAccumulator,
) -> Result<(), ConveyorError> {
    if depth > crate::runner::MAX_TREE_DEPTH {
        return Err(ConveyorError::StageFailure(format!(
            "manifest directory depth exceeds maximum of {}",
            crate::runner::MAX_TREE_DEPTH
        )));
    }
    let remaining_entries = MAX_MANIFEST_FILES.saturating_sub(acc.seen_entries);
    let names = dir_fd
        .list_entry_names_bounded(remaining_entries)
        .map_err(|e| {
            ConveyorError::StageFailure(format!("manifest directory listing failed: {e}"))
        })?;
    acc.seen_entries = acc.seen_entries.saturating_add(names.len());
    for name in names {
        if name == "SHA256SUMS" || name == ".git" {
            continue;
        }
        let rel = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        match dir_fd.is_regular_file_nofollow(&name) {
            Ok(true) => {
                if acc.entries.len() >= MAX_MANIFEST_FILES {
                    return Err(ConveyorError::StageFailure(format!(
                        "manifest file count exceeds maximum of {MAX_MANIFEST_FILES} files"
                    )));
                }
                let remaining = MAX_MANIFEST_TOTAL_BYTES.saturating_sub(acc.total_bytes);
                let per_file_limit = MAX_MANIFEST_FILE_BYTES.min(remaining);
                let bytes = match dir_fd.read_bytes_nofollow(&name, per_file_limit) {
                    Ok(bytes) => bytes,
                    Err(err)
                        if err.kind() == io::ErrorKind::InvalidInput
                            && err
                                .to_string()
                                .contains("exceeds maximum allowed byte size") =>
                    {
                        if remaining < MAX_MANIFEST_FILE_BYTES {
                            return Err(ConveyorError::StageFailure(format!(
                                "manifest total size exceeds maximum of {MAX_MANIFEST_TOTAL_BYTES} bytes at '{rel}'"
                            )));
                        }
                        return Err(ConveyorError::StageFailure(format!(
                            "manifest file '{rel}' exceeds maximum of {MAX_MANIFEST_FILE_BYTES} bytes"
                        )));
                    }
                    Err(err) => return Err(ConveyorError::StageFailure(err.to_string())),
                };
                acc.total_bytes = acc.total_bytes.saturating_add(bytes.len());
                let digest = hex::encode(Sha256::digest(&bytes));
                acc.entries.push((rel, digest));
            }
            Ok(false) => {
                let sub_fd = dir_fd
                    .open_subdir(&name)
                    .map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                collect_files_pinned(&sub_fd, &rel, depth + 1, acc)?;
            }
            Err(err) => return Err(ConveyorError::StageFailure(err.to_string())),
        }
    }
    Ok(())
}

fn map_pinned_input_err(err: io::Error, path_label: &str) -> ConveyorError {
    if err.kind() == io::ErrorKind::NotFound {
        ConveyorError::MissingInput(path_label.to_string())
    } else {
        ConveyorError::StageFailure(err.to_string())
    }
}

fn validate_single_segment(name: &str) -> Result<(), ConveyorError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return Err(ConveyorError::StageFailure(format!(
            "invalid staged input basename '{name}'"
        )));
    }
    Ok(())
}

fn unfreeze_recursive(
    path: &Path,
    is_root: bool,
    depth: usize,
    seen_entries: &mut usize,
) -> Result<(), ConveyorError> {
    if depth > crate::runner::MAX_TREE_DEPTH {
        return Err(ConveyorError::StageFailure(format!(
            "unfreeze tree depth exceeds maximum of {}",
            crate::runner::MAX_TREE_DEPTH
        )));
    }

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
                if *seen_entries >= MAX_SCRATCH_ENTRIES {
                    return Err(ConveyorError::StageFailure(format!(
                        "unfreeze tree entry count exceeds maximum of {MAX_SCRATCH_ENTRIES}"
                    )));
                }
                *seen_entries += 1;
                let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                unfreeze_recursive(&entry.path(), false, depth + 1, seen_entries)?;
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
                if *seen_entries >= MAX_SCRATCH_ENTRIES {
                    return Err(ConveyorError::StageFailure(format!(
                        "unfreeze tree entry count exceeds maximum of {MAX_SCRATCH_ENTRIES}"
                    )));
                }
                *seen_entries += 1;
                let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
                unfreeze_recursive(&entry.path(), false, depth + 1, seen_entries)?;
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

fn check_banned(
    root: &Path,
    current: &Path,
    banned: &[&str],
    depth: usize,
    seen_entries: &mut usize,
) -> Result<(), ConveyorError> {
    if depth > crate::runner::MAX_TREE_DEPTH {
        return Err(ConveyorError::DirtyScratch(format!(
            "scratch directory depth exceeds maximum of {}",
            crate::runner::MAX_TREE_DEPTH
        )));
    }
    let rd = fs::read_dir(current).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
    for entry in rd {
        if *seen_entries >= MAX_SCRATCH_ENTRIES {
            return Err(ConveyorError::DirtyScratch(format!(
                "scratch entry count exceeds maximum of {MAX_SCRATCH_ENTRIES}"
            )));
        }
        *seen_entries += 1;
        let entry = entry.map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        let path = entry.path();
        let meta =
            fs::symlink_metadata(&path).map_err(|e| ConveyorError::StageFailure(e.to_string()))?;
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return Err(ConveyorError::DirtyScratch(
                "scratch directory contains non-UTF-8 entry name".to_string(),
            ));
        };
        if meta.file_type().is_dir() {
            if banned.contains(&name) {
                let rel = path
                    .strip_prefix(root)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| name.to_string());
                return Err(ConveyorError::DirtyScratch(rel));
            }
            check_banned(root, &path, banned, depth + 1, seen_entries)?;
        }
    }
    Ok(())
}
