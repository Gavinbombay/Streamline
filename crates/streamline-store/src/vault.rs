use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use streamline_core::{LifecycleTier, TriageState};

use crate::error::StoreError;
use crate::grants::{now_rfc3339, record_audit_tx, sha256_hex};
use crate::lessons::load_all_lessons_tx;
use crate::models::{
    LessonRecord, ResourceRecord, ResourceWithPassport, SaveResourceRequest, VaultConflictRecord,
    VaultSyncSummary,
};
use crate::resources::{save_resource_tx, validate_slug};
use crate::StreamlineStore;

struct ParsedVaultFrontmatter {
    slug: Option<String>,
    title: Option<String>,
    shelf: Option<String>,
    lifecycle_tier: Option<LifecycleTier>,
    rev: Option<i64>,
    base_sha256: Option<String>,
}

fn unquote_yaml_scalar(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        if let Ok(decoded) = serde_json::from_str::<String>(trimmed) {
            return decoded;
        }
        return trimmed[1..trimmed.len() - 1].to_string();
    }
    if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        return trimmed[1..trimmed.len() - 1].to_string();
    }
    trimmed.to_string()
}

fn parse_vault_markdown(raw: &str) -> (ParsedVaultFrontmatter, String) {
    let normalized = raw.replace("\r\n", "\n");
    if let Some(after_open) = normalized.strip_prefix("---\n") {
        let end_match = if let Some(pos) = after_open.find("\n---\n") {
            Some((pos, pos + 5))
        } else {
            after_open
                .strip_suffix("\n---")
                .map(|stripped| (stripped.len(), after_open.len()))
        };

        if let Some((fm_end, body_start)) = end_match {
            let fm_block = &after_open[..fm_end];
            let body = after_open[body_start..].to_string();
            let mut fm = ParsedVaultFrontmatter {
                slug: None,
                title: None,
                shelf: None,
                lifecycle_tier: None,
                rev: None,
                base_sha256: None,
            };

            for line in fm_block.lines() {
                let Some((field_name, field_val)) = line.split_once(':') else {
                    continue;
                };
                let val = unquote_yaml_scalar(field_val);
                match field_name.trim() {
                    "slug" if !val.is_empty() => fm.slug = Some(val),
                    "title" if !val.is_empty() => fm.title = Some(val),
                    "shelf" if !val.is_empty() => fm.shelf = Some(val),
                    "lifecycle_tier" => fm.lifecycle_tier = val.parse::<LifecycleTier>().ok(),
                    "rev" => fm.rev = val.parse::<i64>().ok(),
                    "base_sha256" if !val.is_empty() => fm.base_sha256 = Some(val),
                    _ => {}
                }
            }
            return (fm, body);
        }
    }

    (
        ParsedVaultFrontmatter {
            slug: None,
            title: None,
            shelf: None,
            lifecycle_tier: None,
            rev: None,
            base_sha256: None,
        },
        normalized,
    )
}

fn format_vault_markdown(resource: &ResourceRecord, body_md: &str) -> Result<String, StoreError> {
    let quoted_title = serde_json::to_string(&resource.title)?;
    Ok(format!(
        "---\nslug: {}\ntitle: {}\nshelf: {}\nlifecycle_tier: {}\nrev: {}\nbase_sha256: {}\n---\n{}",
        resource.slug,
        quoted_title,
        resource.shelf_slug,
        resource.lifecycle_tier.as_str(),
        resource.current_rev,
        resource.current_sha256,
        body_md
    ))
}

fn format_lesson_markdown(lesson: &LessonRecord) -> Result<String, StoreError> {
    let project_display = lesson.project_id.as_deref().unwrap_or("central");
    let quoted_project = serde_json::to_string(project_display)?;
    let tags_json = serde_json::to_string(&lesson.tags)?;
    let tags_display = if lesson.tags.is_empty() {
        "none".to_string()
    } else {
        lesson.tags.join(", ")
    };
    let cause_note = if lesson.cause_note.trim().is_empty() {
        "None recorded."
    } else {
        lesson.cause_note.trim()
    };

    Ok(format!(
        "---\n\
         lesson_id: {}\n\
         project_id: {}\n\
         tags: {}\n\
         cause_kind: {}\n\
         validation_state: {}\n\
         recorded_by: {}\n\
         created_at: {}\n\
         ---\n\
         # Lesson: {}\n\n\
         - **Project:** {}\n\
         - **Tags:** {}\n\
         - **Cause Kind:** {}\n\
         - **Validation State:** {}\n\
         - **Recorded By:** {}\n\
         - **Created At:** {}\n\n\
         ## Observed Fact\n\n\
         {}\n\n\
         ## Cause Note\n\n\
         {}\n\n\
         ## Forward Rule\n\n\
         {}\n",
        lesson.lesson_id,
        quoted_project,
        tags_json,
        lesson.cause_kind,
        lesson.validation_state,
        lesson.recorded_by,
        lesson.created_at,
        lesson.lesson_id,
        project_display,
        tags_display,
        lesson.cause_kind,
        lesson.validation_state,
        lesson.recorded_by,
        lesson.created_at,
        lesson.observed_fact.trim(),
        cause_note,
        lesson.forward_rule.trim(),
    ))
}

fn infer_title_from_body(body_md: &str, fallback_slug: &str) -> String {
    for line in body_md.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("# ") {
            let clean = heading.trim();
            if !clean.is_empty() {
                return clean.to_string();
            }
        }
    }
    fallback_slug.to_string()
}

fn load_shelf_slugs_tx(conn: &Connection) -> Result<BTreeSet<String>, StoreError> {
    let mut set = BTreeSet::new();
    let mut stmt = conn.prepare("SELECT slug FROM shelves ORDER BY slug ASC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for item in rows {
        set.insert(item?);
    }
    Ok(set)
}

fn build_conflict_markdown(
    shelf_slug: &str,
    slug: &str,
    db_item: &ResourceWithPassport,
    disk_sha256: &str,
    disk_body: &str,
    detected_at: &str,
) -> String {
    format!(
        "# Vault Sync Conflict: {shelf_slug}/{slug}\n\n\
         Detected at: `{detected_at}`\n\
         DB revision: `rev {}` (`{}`)\n\
         Disk sha256: `{}`\n\n\
         Both the database resource and the vault markdown file changed since the last synced base digest.\n\
         Resolve manually by choosing or merging the two versions below and saving the primary resource.\n\n\
         ## Database Version (rev {})\n\n\
         ```markdown\n{}\n```\n\n\
         ## Vault Disk Version\n\n\
         ```markdown\n{}\n```\n",
        db_item.resource.current_rev,
        db_item.resource.current_sha256,
        disk_sha256,
        db_item.resource.current_rev,
        db_item.body_md,
        disk_body
    )
}

fn verify_entry_inside_vault(path: &Path, canon_vault: &Path) -> Result<fs::Metadata, StoreError> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(StoreError::InvalidInput(
            "symlinks are forbidden inside vault_sync_dir".to_string(),
        ));
    }
    let canon_path = fs::canonicalize(path)?;
    if !canon_path.starts_with(canon_vault) {
        return Err(StoreError::InvalidInput(
            "path escapes vault_sync_dir".to_string(),
        ));
    }
    Ok(meta)
}

fn ensure_verified_vault_subdir(dir: &Path, canon_vault: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(dir) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(StoreError::InvalidInput(
                    "symlinks are forbidden inside vault_sync_dir".to_string(),
                ));
            }
            if !meta.is_dir() {
                return Err(StoreError::InvalidInput(format!(
                    "expected directory inside vault_sync_dir at '{}'",
                    dir.display()
                )));
            }
            if !fs::canonicalize(dir)?.starts_with(canon_vault) {
                return Err(StoreError::InvalidInput(
                    "path escapes vault_sync_dir".to_string(),
                ));
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(dir)?;
            let meta = fs::symlink_metadata(dir)?;
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(StoreError::InvalidInput(
                    "symlinks are forbidden inside vault_sync_dir".to_string(),
                ));
            }
            if !fs::canonicalize(dir)?.starts_with(canon_vault) {
                return Err(StoreError::InvalidInput(
                    "path escapes vault_sync_dir".to_string(),
                ));
            }
            Ok(())
        }
        Err(err) => Err(StoreError::Io(err)),
    }
}

fn atomic_write_vault_file(
    target_path: &Path,
    canon_vault: &Path,
    contents: &str,
) -> Result<(), StoreError> {
    let parent_dir = target_path.parent().ok_or_else(|| {
        StoreError::InvalidInput("vault target path missing parent directory".to_string())
    })?;
    ensure_verified_vault_subdir(parent_dir, canon_vault)?;

    match fs::symlink_metadata(target_path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(StoreError::InvalidInput(
                    "symlinks are forbidden inside vault_sync_dir".to_string(),
                ));
            }
            if !meta.is_file() {
                return Err(StoreError::InvalidInput(format!(
                    "expected regular file inside vault_sync_dir at '{}'",
                    target_path.display()
                )));
            }
            if !fs::canonicalize(target_path)?.starts_with(canon_vault) {
                return Err(StoreError::InvalidInput(
                    "path escapes vault_sync_dir".to_string(),
                ));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(StoreError::Io(err)),
    }

    let tmp_path = parent_dir.join(format!(".tmp-{}", uuid::Uuid::new_v4()));
    let mut tmp_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)?;
    if let Err(err) = tmp_file
        .write_all(contents.as_bytes())
        .and_then(|()| tmp_file.sync_all())
    {
        drop(tmp_file);
        let _ = fs::remove_file(&tmp_path);
        return Err(StoreError::Io(err));
    }
    drop(tmp_file);
    if let Err(err) = fs::rename(&tmp_path, target_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(StoreError::Io(err));
    }
    Ok(())
}

impl StreamlineStore {
    /// Two-way sync between SQLite resources and an Obsidian-compatible markdown directory.
    ///
    /// - Exports resources to `<vault_dir>/<shelf_slug>/<slug>.md` with YAML frontmatter.
    /// - Imports clean disk edits when `base_sha256` matches the current DB digest.
    /// - Preserves concurrent DB + disk edits by writing an explicit conflict record in `inbox`
    ///   without overwriting either copy.
    /// - Refuses symlinks anywhere inside `vault_dir` and writes files atomically via `.tmp-*`.
    pub fn sync_vault_dir(&self, vault_dir: &Path) -> Result<VaultSyncSummary, StoreError> {
        match fs::symlink_metadata(vault_dir) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(StoreError::InvalidInput(
                        "symlinks are forbidden inside vault_sync_dir".to_string(),
                    ));
                }
                if !meta.is_dir() {
                    return Err(StoreError::InvalidInput(
                        "vault_sync_dir exists and is not a directory".to_string(),
                    ));
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir_all(vault_dir)?;
                let meta = fs::symlink_metadata(vault_dir)?;
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    return Err(StoreError::InvalidInput(
                        "symlinks are forbidden inside vault_sync_dir".to_string(),
                    ));
                }
            }
            Err(err) => return Err(StoreError::Io(err)),
        }
        let canon_vault = fs::canonicalize(vault_dir)?;

        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;

        let shelf_slugs = load_shelf_slugs_tx(&tx)?;
        let db_items = Self::load_all_non_deleted_with_bodies(&tx)?;

        let mut summary = VaultSyncSummary {
            exported: 0,
            imported: 0,
            unchanged: 0,
            conflicts: Vec::new(),
        };
        let mut seen_on_disk: BTreeSet<(String, String)> = BTreeSet::new();
        let mut pending_exports: BTreeMap<(String, String), ResourceWithPassport> = BTreeMap::new();

        for shelf_slug in &shelf_slugs {
            let shelf_dir = vault_dir.join(shelf_slug);
            match fs::symlink_metadata(&shelf_dir) {
                Ok(meta) => {
                    if meta.file_type().is_symlink() {
                        return Err(StoreError::InvalidInput(
                            "symlinks are forbidden inside vault_sync_dir".to_string(),
                        ));
                    }
                    if !meta.is_dir() {
                        continue;
                    }
                    if !fs::canonicalize(&shelf_dir)?.starts_with(&canon_vault) {
                        return Err(StoreError::InvalidInput(
                            "path escapes vault_sync_dir".to_string(),
                        ));
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => return Err(StoreError::Io(err)),
            }

            let mut entries: Vec<PathBuf> = Vec::new();
            for entry_res in fs::read_dir(&shelf_dir)? {
                let entry = entry_res?;
                let entry_path = entry.path();
                let meta = verify_entry_inside_vault(&entry_path, &canon_vault)?;
                if meta.is_file() && entry_path.extension().and_then(|e| e.to_str()) == Some("md") {
                    entries.push(entry_path);
                }
            }
            entries.sort();

            for file_path in entries {
                let Some(file_stem) = file_path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if file_stem.starts_with('.') {
                    continue;
                }
                verify_entry_inside_vault(&file_path, &canon_vault)?;
                let raw_disk = fs::read_to_string(&file_path)?;
                let (fm, disk_body) = parse_vault_markdown(&raw_disk);
                let Some(slug) = fm
                    .slug
                    .as_deref()
                    .filter(|val| validate_slug(val, "slug").is_ok())
                    .or_else(|| {
                        if validate_slug(file_stem, "slug").is_ok() {
                            Some(file_stem)
                        } else {
                            None
                        }
                    })
                    .map(str::to_string)
                else {
                    continue;
                };
                let effective_shelf = fm
                    .shelf
                    .as_deref()
                    .filter(|val| shelf_slugs.contains(*val))
                    .unwrap_or(shelf_slug)
                    .to_string();

                let coord = (effective_shelf.clone(), slug.clone());
                seen_on_disk.insert(coord.clone());
                let disk_sha256 = sha256_hex(disk_body.as_bytes());

                if let Some(db_item) = db_items.get(&coord) {
                    let db_sha = &db_item.resource.current_sha256;
                    if disk_sha256 == *db_sha {
                        let expected_fm_matches = fm.base_sha256.as_deref()
                            == Some(db_sha.as_str())
                            && fm.rev == Some(db_item.resource.current_rev);
                        if !expected_fm_matches {
                            let rendered =
                                format_vault_markdown(&db_item.resource, &db_item.body_md)?;
                            atomic_write_vault_file(&file_path, &canon_vault, &rendered)?;
                        }
                        summary.unchanged += 1;
                    } else if fm.base_sha256.as_deref() == Some(db_sha.as_str()) {
                        let title = fm.title.unwrap_or_else(|| db_item.resource.title.clone());
                        let saved = save_resource_tx(
                            &tx,
                            "vault-sync",
                            None,
                            SaveResourceRequest {
                                shelf_slug: effective_shelf.clone(),
                                slug: slug.clone(),
                                title,
                                body_md: disk_body,
                                project_id: db_item.resource.project_id.clone(),
                                trio_role: db_item.resource.trio_role.clone(),
                                lifecycle_tier: fm
                                    .lifecycle_tier
                                    .or(Some(db_item.resource.lifecycle_tier)),
                                triage_state: Some(db_item.resource.triage_state),
                                summary: "Import clean Obsidian vault edit".to_string(),
                                expected_rev: Some(db_item.resource.current_rev),
                            },
                        )?;
                        let rendered = format_vault_markdown(&saved.resource, &saved.body_md)?;
                        atomic_write_vault_file(&file_path, &canon_vault, &rendered)?;
                        summary.imported += 1;
                    } else if fm.base_sha256.as_deref() == Some(disk_sha256.as_str()) {
                        let rendered = format_vault_markdown(&db_item.resource, &db_item.body_md)?;
                        atomic_write_vault_file(&file_path, &canon_vault, &rendered)?;
                        summary.exported += 1;
                    } else {
                        let conflict_slug = format!(
                            "conflict-{}-{}-r{}-{}",
                            effective_shelf,
                            slug,
                            db_item.resource.current_rev,
                            &disk_sha256[..8]
                        );
                        let now = now_rfc3339();
                        let conflict_body = build_conflict_markdown(
                            &effective_shelf,
                            &slug,
                            db_item,
                            &disk_sha256,
                            &disk_body,
                            &now,
                        );
                        let conflict_coord = ("inbox".to_string(), conflict_slug.clone());
                        let conflict_resource_id =
                            if let Some(existing_conflict) = db_items.get(&conflict_coord) {
                                existing_conflict.resource.id.clone()
                            } else {
                                let saved_conflict = save_resource_tx(
                                    &tx,
                                    "vault-sync",
                                    None,
                                    SaveResourceRequest {
                                        shelf_slug: "inbox".to_string(),
                                        slug: conflict_slug.clone(),
                                        title: format!("Sync Conflict: {effective_shelf}/{slug}"),
                                        body_md: conflict_body,
                                        project_id: db_item.resource.project_id.clone(),
                                        trio_role: None,
                                        lifecycle_tier: Some(LifecycleTier::Active),
                                        triage_state: Some(TriageState::NeedsTriage),
                                        summary: format!(
                                            "Concurrent vault conflict on {effective_shelf}/{slug}"
                                        ),
                                        expected_rev: None,
                                    },
                                )?;
                                pending_exports.insert(conflict_coord, saved_conflict.clone());
                                saved_conflict.resource.id
                            };

                        record_audit_tx(
                            &tx,
                            "vault-sync",
                            "vault_conflict",
                            &db_item.resource.id,
                            "conflict",
                            serde_json::json!({
                                "shelf_slug": effective_shelf,
                                "slug": slug,
                                "conflict_resource_id": conflict_resource_id,
                                "db_sha256": db_sha,
                                "disk_sha256": disk_sha256,
                            }),
                        )?;

                        summary.conflicts.push(VaultConflictRecord {
                            shelf_slug: effective_shelf,
                            slug,
                            conflict_resource_id,
                            conflict_slug,
                            db_rev: db_item.resource.current_rev,
                            db_sha256: db_sha.clone(),
                            disk_sha256,
                        });
                    }
                } else {
                    let title = fm
                        .title
                        .unwrap_or_else(|| infer_title_from_body(&disk_body, &slug));
                    let saved = save_resource_tx(
                        &tx,
                        "vault-sync",
                        None,
                        SaveResourceRequest {
                            shelf_slug: effective_shelf,
                            slug,
                            title,
                            body_md: disk_body,
                            project_id: None,
                            trio_role: None,
                            lifecycle_tier: Some(
                                fm.lifecycle_tier.unwrap_or(LifecycleTier::Active),
                            ),
                            triage_state: Some(TriageState::Reviewed),
                            summary: "Import new vault markdown file".to_string(),
                            expected_rev: None,
                        },
                    )?;
                    let rendered = format_vault_markdown(&saved.resource, &saved.body_md)?;
                    atomic_write_vault_file(&file_path, &canon_vault, &rendered)?;
                    summary.imported += 1;
                }
            }
        }

        for (coord, item) in db_items.into_iter().chain(pending_exports) {
            if seen_on_disk.contains(&coord) {
                continue;
            }
            let safe_shelf = validate_slug(&item.resource.shelf_slug, "shelf_slug")?;
            let safe_slug = validate_slug(&item.resource.slug, "slug")?;
            let shelf_dir = vault_dir.join(&safe_shelf);
            ensure_verified_vault_subdir(&shelf_dir, &canon_vault)?;
            let target_path = shelf_dir.join(format!("{safe_slug}.md"));
            let rendered = format_vault_markdown(&item.resource, &item.body_md)?;
            atomic_write_vault_file(&target_path, &canon_vault, &rendered)?;
            summary.exported += 1;
        }

        let lessons_dir = vault_dir.join("lessons");
        match fs::symlink_metadata(&lessons_dir) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(StoreError::InvalidInput(
                        "symlinks are forbidden inside vault_sync_dir".to_string(),
                    ));
                }
                if meta.is_dir() {
                    if !fs::canonicalize(&lessons_dir)?.starts_with(&canon_vault) {
                        return Err(StoreError::InvalidInput(
                            "path escapes vault_sync_dir".to_string(),
                        ));
                    }
                    for entry_res in fs::read_dir(&lessons_dir)? {
                        let entry = entry_res?;
                        let _ = verify_entry_inside_vault(&entry.path(), &canon_vault)?;
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(StoreError::Io(err)),
        }

        let lessons = load_all_lessons_tx(&tx)?;
        if !lessons.is_empty() {
            ensure_verified_vault_subdir(&lessons_dir, &canon_vault)?;
            for lesson in lessons {
                let safe_id = validate_slug(&lesson.lesson_id, "lesson_id")?;
                let target_path = lessons_dir.join(format!("{safe_id}.md"));
                let rendered = format_lesson_markdown(&lesson)?;
                let existing_matches = match fs::symlink_metadata(&target_path) {
                    Ok(meta) => {
                        if meta.file_type().is_symlink() {
                            return Err(StoreError::InvalidInput(
                                "symlinks are forbidden inside vault_sync_dir".to_string(),
                            ));
                        }
                        if !fs::canonicalize(&target_path)?.starts_with(&canon_vault) {
                            return Err(StoreError::InvalidInput(
                                "path escapes vault_sync_dir".to_string(),
                            ));
                        }
                        meta.is_file()
                            && fs::read_to_string(&target_path).ok().as_deref()
                                == Some(rendered.as_str())
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
                    Err(err) => return Err(StoreError::Io(err)),
                };
                if !existing_matches {
                    atomic_write_vault_file(&target_path, &canon_vault, &rendered)?;
                    summary.exported += 1;
                }
            }
        }

        tx.commit()?;
        Ok(summary)
    }
}
