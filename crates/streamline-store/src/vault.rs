use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rusqlite::Connection;
use streamline_core::{LifecycleTier, PrincipalRole, TriageState};
use streamline_safe::PinnedDir;

use crate::error::StoreError;
use crate::grants::{
    check_shelf_access_tx, now_rfc3339, record_audit_tx, require_rules_acked_tx, sha256_hex,
};
use crate::lessons::load_all_lessons_tx;
use crate::limits::{Budget, MAX_RESOURCE_BODY_BYTES, MAX_VAULT_BYTES};
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
    let rendered = format!(
        "---\nslug: {}\ntitle: {}\nshelf: {}\nlifecycle_tier: {}\nrev: {}\nbase_sha256: {}\n---\n{}",
        resource.slug,
        quoted_title,
        resource.shelf_slug,
        resource.lifecycle_tier.as_str(),
        resource.current_rev,
        resource.current_sha256,
        body_md
    );
    if rendered.len() > MAX_RESOURCE_BODY_BYTES {
        return Err(StoreError::InvalidInput(
            "rendered vault file exceeds 16 MiB".into(),
        ));
    }
    Ok(rendered)
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

fn map_vault_io(err: std::io::Error) -> StoreError {
    if err.kind() == std::io::ErrorKind::InvalidInput {
        StoreError::InvalidInput(err.to_string())
    } else {
        StoreError::Io(err)
    }
}

impl StreamlineStore {
    /// Two-way sync between SQLite resources and an Obsidian-compatible markdown directory.
    ///
    /// - Exports resources to `<vault_dir>/<shelf_slug>/<slug>.md` with YAML frontmatter.
    /// - Imports clean disk edits when `base_sha256` matches the current DB digest.
    /// - Preserves concurrent DB + disk edits by writing an explicit conflict record in `inbox`
    ///   without overwriting either copy.
    /// - Refuses symlinks anywhere inside `vault_dir` via descriptor-pinned `PinnedDir`
    ///   (`openat`/`O_NOFOLLOW`/`renameat`).
    /// Trusted local operator entrypoint, not for authenticated remote requests.
    pub fn sync_vault_dir(&self, vault_dir: &Path) -> Result<VaultSyncSummary, StoreError> {
        self.sync_vault(vault_dir, None, true)
    }

    /// Two-way sync requested by an authenticated owner. Grant checks happen in the
    /// same transaction as imports; the local operator CLI uses sync_vault_dir instead.
    pub fn sync_vault_dir_as_owner(
        &self,
        vault_dir: &Path,
        grant_id: &str,
    ) -> Result<VaultSyncSummary, StoreError> {
        self.sync_vault(vault_dir, Some(grant_id), true)
    }

    /// Automatic release exports must never import disk changes or create conflict rows.
    /// Preserve local edits and unknown files for an explicit two-way sync.
    pub fn export_vault_dir(&self, vault_dir: &Path) -> Result<VaultSyncSummary, StoreError> {
        self.sync_vault(vault_dir, None, false)
    }

    fn sync_vault(
        &self,
        vault_dir: &Path,
        grant_id: Option<&str>,
        allow_import: bool,
    ) -> Result<VaultSyncSummary, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = grant_id
            .map(|id| require_rules_acked_tx(&tx, id))
            .transpose()?;
        if let Some(ref grant) = grant {
            if grant.principal_role != PrincipalRole::Owner {
                return Err(StoreError::ForbiddenRole(grant.principal_role.to_string()));
            }
            if !grant.can_write {
                return Err(StoreError::ForbiddenWrite);
            }
        }
        let author = grant
            .as_ref()
            .map_or("vault-sync", |grant| grant.principal_id.as_str());
        let vault_fd = PinnedDir::open_or_create_dir(vault_dir).map_err(map_vault_io)?;

        let shelf_slugs = load_shelf_slugs_tx(&tx)?;
        let db_items = Self::load_all_non_deleted_with_bodies(&tx)?;

        let mut summary = VaultSyncSummary {
            exported: 0,
            imported: 0,
            unchanged: 0,
            conflicts: Vec::new(),
        };
        let mut seen_on_disk: BTreeSet<(String, String)> = BTreeSet::new();
        let mut disk_budget = Budget::new("vault disk corpus", MAX_VAULT_BYTES);
        let mut pending_budget = Budget::new("vault conflict corpus", MAX_VAULT_BYTES);
        let mut pending_exports: BTreeMap<(String, String), ResourceWithPassport> = BTreeMap::new();

        for shelf_slug in &shelf_slugs {
            let shelf_fd = match vault_fd.open_subdir(shelf_slug) {
                Ok(dir) => dir,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => return Err(map_vault_io(err)),
            };

            let mut entry_names = shelf_fd.list_entry_names().map_err(map_vault_io)?;
            entry_names.sort();

            for entry_name in entry_names {
                let is_reg = shelf_fd
                    .is_regular_file_nofollow(&entry_name)
                    .map_err(map_vault_io)?;
                if !is_reg || entry_name.starts_with('.') || !entry_name.ends_with(".md") {
                    continue;
                }
                let file_stem = entry_name.strip_suffix(".md").unwrap_or(&entry_name);
                if file_stem.is_empty() || file_stem.starts_with('.') {
                    continue;
                }

                let raw_disk = shelf_fd
                    .read_to_string_nofollow(&entry_name)
                    .map_err(map_vault_io)?;
                disk_budget.add(raw_disk.len())?;
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
                            shelf_fd
                                .atomic_write_nofollow(&entry_name, rendered.as_bytes(), 0o644)
                                .map_err(map_vault_io)?;
                        }
                        summary.unchanged += 1;
                    } else if allow_import && fm.base_sha256.as_deref() == Some(db_sha.as_str()) {
                        let title = fm.title.unwrap_or_else(|| db_item.resource.title.clone());
                        let saved = save_resource_tx(
                            &tx,
                            author,
                            grant.as_ref(),
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
                        shelf_fd
                            .atomic_write_nofollow(&entry_name, rendered.as_bytes(), 0o644)
                            .map_err(map_vault_io)?;
                        summary.imported += 1;
                    } else if fm.base_sha256.as_deref() == Some(disk_sha256.as_str()) {
                        let rendered = format_vault_markdown(&db_item.resource, &db_item.body_md)?;
                        shelf_fd
                            .atomic_write_nofollow(&entry_name, rendered.as_bytes(), 0o644)
                            .map_err(map_vault_io)?;
                        summary.exported += 1;
                    } else if allow_import {
                        if let Some(ref grant) = grant {
                            check_shelf_access_tx(
                                &tx,
                                grant,
                                &effective_shelf,
                                db_item.resource.project_id.as_deref(),
                            )?;
                        }
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
                        let conflict_resource_id = if let Some(existing_conflict) =
                            db_items.get(&conflict_coord)
                        {
                            existing_conflict.resource.id.clone()
                        } else {
                            let saved_conflict = save_resource_tx(
                                &tx,
                                author,
                                grant.as_ref(),
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
                            pending_budget.add(crate::limits::serialized_len(&saved_conflict)?)?;
                            pending_exports.insert(conflict_coord, saved_conflict.clone());
                            saved_conflict.resource.id
                        };

                        record_audit_tx(
                            &tx,
                            author,
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
                } else if allow_import {
                    let title = fm
                        .title
                        .unwrap_or_else(|| infer_title_from_body(&disk_body, &slug));
                    let saved = save_resource_tx(
                        &tx,
                        author,
                        grant.as_ref(),
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
                    shelf_fd
                        .atomic_write_nofollow(&entry_name, rendered.as_bytes(), 0o644)
                        .map_err(map_vault_io)?;
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
            let target_shelf_fd = vault_fd
                .open_or_create_subdir(&safe_shelf, 0o755)
                .map_err(map_vault_io)?;
            let target_name = format!("{safe_slug}.md");
            let rendered = format_vault_markdown(&item.resource, &item.body_md)?;
            target_shelf_fd
                .atomic_write_nofollow(&target_name, rendered.as_bytes(), 0o644)
                .map_err(map_vault_io)?;
            summary.exported += 1;
        }

        let existing_lessons_fd = match vault_fd.open_subdir("lessons") {
            Ok(dir) => {
                for entry_name in dir.list_entry_names().map_err(map_vault_io)? {
                    let _ = dir
                        .is_regular_file_nofollow(&entry_name)
                        .map_err(map_vault_io)?;
                }
                Some(dir)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => return Err(map_vault_io(err)),
        };

        let lessons = load_all_lessons_tx(&tx)?;
        if !lessons.is_empty() {
            let lessons_fd = match existing_lessons_fd {
                Some(dir) => dir,
                None => vault_fd
                    .open_or_create_subdir("lessons", 0o755)
                    .map_err(map_vault_io)?,
            };
            for lesson in lessons {
                let safe_id = validate_slug(&lesson.lesson_id, "lesson_id")?;
                let file_name = format!("{safe_id}.md");
                let rendered = format_lesson_markdown(&lesson)?;
                if rendered.len() > MAX_RESOURCE_BODY_BYTES {
                    return Err(StoreError::InvalidInput(
                        "rendered vault lesson exceeds 16 MiB".into(),
                    ));
                }
                let existing_matches = match lessons_fd.is_regular_file_nofollow(&file_name) {
                    Ok(true) => match lessons_fd.read_to_string_nofollow(&file_name) {
                        Ok(existing_text) => {
                            disk_budget.add(existing_text.len())?;
                            existing_text == rendered
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::InvalidInput => {
                            return Err(map_vault_io(err));
                        }
                        Err(_) => false,
                    },
                    Ok(false) => {
                        return Err(StoreError::InvalidInput(format!(
                            "expected regular file inside vault_sync_dir/lessons at '{file_name}'"
                        )));
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
                    Err(err) => return Err(map_vault_io(err)),
                };
                if !existing_matches {
                    lessons_fd
                        .atomic_write_nofollow(&file_name, rendered.as_bytes(), 0o644)
                        .map_err(map_vault_io)?;
                    summary.exported += 1;
                }
            }
        }

        tx.commit()?;
        Ok(summary)
    }
}
