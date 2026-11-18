use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rusqlite::{params, types::ValueRef, Connection, OpenFlags, OptionalExtension};
use streamline_core::{FreshnessState, LifecycleTier, SourcePassport, TriageState};

use crate::error::StoreError;
use crate::grants::{now_rfc3339, record_audit_tx, sha256_hex};
use crate::models::{LegacyImportSummary, SaveResourceRequest};
use crate::resources::save_resource_tx;
use crate::StreamlineStore;

const SKIPPED_LEGACY_TABLES: [&str; 5] = [
    "audit_events",
    "freshness_check",
    "sessions",
    "oauth_codes",
    "access_tokens",
];

fn sanitize_legacy_slug(raw: &str, fallback_prefix: &str, digest: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            prev_dash = false;
        } else if !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        format!("{fallback_prefix}-{}", &digest[..8])
    } else if slug.len() > 80 {
        format!("{}-{}", &slug[..64], &digest[..6])
    } else {
        slug
    }
}

fn pick_col<'a>(
    row_map: &'a BTreeMap<String, Option<String>>,
    candidates: &[&str],
) -> Option<&'a str> {
    for candidate in candidates {
        if let Some(Some(val)) = row_map.get(*candidate) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn map_legacy_shelf_and_tier(kind_raw: &str, status_raw: &str) -> (&'static str, LifecycleTier) {
    let kind = kind_raw.trim().to_ascii_lowercase();
    let status = status_raw.trim().to_ascii_lowercase();

    let shelf = if kind.contains("standard") || kind.contains("rule") {
        "standards"
    } else if kind.contains("runbook") || kind.contains("procedure") {
        "runbooks"
    } else if kind.contains("arch") || kind.contains("design") || kind.contains("spec") {
        "architecture"
    } else if kind.contains("project") || kind.contains("product") {
        "projects"
    } else if kind.contains("skill") {
        "skills"
    } else if kind.contains("want") || kind.contains("idea") || kind.contains("inbox") {
        "inbox"
    } else {
        "archive"
    };

    let tier = if kind.contains("design")
        || kind.contains("product")
        || kind.contains("want")
        || kind.contains("idea")
        || status.contains("dormant")
    {
        LifecycleTier::Dormant
    } else {
        LifecycleTier::Legacy
    };

    (shelf, tier)
}

fn is_noise_row(kind_raw: &str, title: &str, body: &str) -> bool {
    let kind_l = kind_raw.trim().to_ascii_lowercase();
    let title_l = title.to_ascii_lowercase();
    let body_l = body.to_ascii_lowercase();

    if matches!(
        kind_l.as_str(),
        "audit" | "audit_event" | "freshness_check" | "telemetry" | "session" | "oauth" | "noise"
    ) {
        return true;
    }

    if title_l.contains("returns 404") || body_l.contains("returns 404") {
        return true;
    }

    title.trim().is_empty() && body.trim().is_empty()
}

fn value_ref_to_opt_string(val: ValueRef<'_>) -> Option<String> {
    match val {
        ValueRef::Null => None,
        ValueRef::Integer(num) => Some(num.to_string()),
        ValueRef::Real(num) => Some(num.to_string()),
        ValueRef::Text(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        ValueRef::Blob(_) => None,
    }
}

fn read_legacy_table_rows(
    legacy_conn: &Connection,
    table_name: &str,
) -> Result<Vec<BTreeMap<String, Option<String>>>, StoreError> {
    let pragma_sql = format!("PRAGMA table_info(\"{table_name}\")");
    let mut pragma_stmt = legacy_conn.prepare(&pragma_sql)?;
    let col_rows = pragma_stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut columns = Vec::new();
    for item in col_rows {
        columns.push(item?);
    }
    if columns.is_empty() {
        return Ok(Vec::new());
    }

    let quoted_cols: Vec<String> = columns
        .iter()
        .map(|col| format!("\"{}\"", col.replace('"', "\"\"")))
        .collect();
    let select_sql = format!("SELECT {} FROM \"{}\"", quoted_cols.join(", "), table_name);
    let mut stmt = legacy_conn.prepare(&select_sql)?;
    let mut rows_cursor = stmt.query([])?;
    let mut out = Vec::new();

    while let Some(row) = rows_cursor.next()? {
        let mut map = BTreeMap::new();
        for (idx, col_name) in columns.iter().enumerate() {
            let val = value_ref_to_opt_string(row.get_ref(idx)?);
            map.insert(col_name.to_ascii_lowercase(), val);
        }
        out.push(map);
    }
    Ok(out)
}

fn table_has_columns(
    conn: &Connection,
    table_name: &str,
    required: &[&str],
) -> Result<bool, StoreError> {
    let pragma_sql = format!("PRAGMA table_info(\"{}\")", table_name.replace('"', "\"\""));
    let mut stmt = conn.prepare(&pragma_sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut cols = BTreeSet::new();
    for item in rows {
        cols.insert(item?.to_ascii_lowercase());
    }
    Ok(required.iter().all(|col| cols.contains(*col)))
}

fn lookup_latest_document_revision_body(
    legacy_conn: &Connection,
    doc_rev_table: &str,
    slug: &str,
) -> Result<Option<String>, StoreError> {
    let sql = format!(
        "SELECT body FROM \"{}\" WHERE slug = ?1 ORDER BY revision DESC LIMIT 1",
        doc_rev_table.replace('"', "\"\"")
    );
    let mut stmt = legacy_conn.prepare(&sql)?;
    let body: Option<String> = stmt
        .query_row(params![slug], |row| {
            Ok(value_ref_to_opt_string(row.get_ref(0)?))
        })
        .optional()?
        .flatten();
    Ok(body
        .map(|val| val.trim().to_string())
        .filter(|val| !val.is_empty()))
}

impl StreamlineStore {
    /// Imports historical catalog items/entities from a legacy SQLite database opened strictly read-only.
    ///
    /// Skips telemetry/auth tables (`audit_events`, `freshness_check`, `sessions`, `oauth_codes`,
    /// `access_tokens`) and `"returns 404"` feedback noise, placing imported items in `Dormant` or
    /// `Legacy` tiers with `NeedsTriage` and preserved `SourcePassport` metadata so agent
    /// bootstrap remains isolated.
    pub fn import_legacy_sqlite(
        &self,
        legacy_db_path: &Path,
    ) -> Result<LegacyImportSummary, StoreError> {
        let legacy_conn =
            Connection::open_with_flags(legacy_db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        let mut tables = Vec::new();
        {
            let mut stmt = legacy_conn
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name ASC")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for item in rows {
                let name = item?;
                if !name.starts_with("sqlite_") {
                    tables.push(name);
                }
            }
        }

        let mut summary = LegacyImportSummary {
            detected_tables: Vec::new(),
            imported_count: 0,
            skipped_noise_count: 0,
            skipped_duplicate_count: 0,
            skipped_tables: Vec::new(),
            imported_resource_ids: Vec::new(),
        };

        let mut source_tables = Vec::new();
        let mut doc_rev_table: Option<String> = None;
        for table_name in &tables {
            let lower = table_name.to_ascii_lowercase();
            if SKIPPED_LEGACY_TABLES.contains(&lower.as_str()) {
                summary.skipped_tables.push(table_name.clone());
            } else if matches!(
                lower.as_str(),
                "catalog_items" | "entities" | "ideas" | "capability_cards"
            ) {
                summary.detected_tables.push(table_name.clone());
                source_tables.push(table_name.clone());
            } else if lower == "document_revision"
                && table_has_columns(&legacy_conn, table_name, &["slug", "body", "revision"])?
            {
                doc_rev_table = Some(table_name.clone());
            }
        }

        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let mut seen_hashes: BTreeSet<(String, String)> = BTreeSet::new();
        let now = now_rfc3339();

        for table_name in source_tables {
            let is_catalog_items = table_name.eq_ignore_ascii_case("catalog_items");
            let default_kind = if table_name.eq_ignore_ascii_case("ideas") {
                "idea"
            } else if table_name.eq_ignore_ascii_case("capability_cards") {
                "product"
            } else {
                "legacy"
            };
            let rows = read_legacy_table_rows(&legacy_conn, &table_name)?;
            for row_map in rows {
                let raw_kind = pick_col(
                    &row_map,
                    &[
                        "kind",
                        "category",
                        "item_type",
                        "entity_type",
                        "type",
                        "shelf",
                    ],
                )
                .unwrap_or(default_kind);
                let raw_status = pick_col(
                    &row_map,
                    &["status", "lifecycle", "lifecycle_tier", "state"],
                )
                .unwrap_or("");
                let raw_title = pick_col(&row_map, &["title", "name", "slug", "id"]).unwrap_or("");
                let full_body = pick_col(&row_map, &["body_md", "body", "content", "description"]);
                let rev_body = if full_body.is_none() && is_catalog_items {
                    if let (Some(ref rev_tbl), Some(item_slug)) =
                        (&doc_rev_table, pick_col(&row_map, &["slug"]))
                    {
                        lookup_latest_document_revision_body(&legacy_conn, rev_tbl, item_slug)?
                    } else {
                        None
                    }
                } else {
                    None
                };
                let raw_body = full_body
                    .or(rev_body.as_deref())
                    .or_else(|| pick_col(&row_map, &["summary", "blurb", "notes", "metadata_json"]))
                    .unwrap_or("");

                if is_noise_row(raw_kind, raw_title, raw_body) {
                    summary.skipped_noise_count += 1;
                    continue;
                }

                let title = if raw_title.is_empty() {
                    "Untitled Legacy Item".to_string()
                } else {
                    raw_title.to_string()
                };
                let body_md = if raw_body.is_empty() {
                    format!("# {title}\n\nImported from legacy table `{table_name}`.\n")
                } else {
                    raw_body.to_string()
                };
                let body_sha = sha256_hex(body_md.as_bytes());
                let (shelf_slug, lifecycle_tier) = map_legacy_shelf_and_tier(raw_kind, raw_status);

                if !seen_hashes.insert((shelf_slug.to_string(), body_sha.clone())) {
                    summary.skipped_duplicate_count += 1;
                    continue;
                }

                let raw_slug = pick_col(&row_map, &["slug", "handle", "code", "name", "title"])
                    .unwrap_or(&title);
                let base_slug = sanitize_legacy_slug(raw_slug, "legacy", &body_sha);

                let existing: Option<(String, String)> = tx
                    .query_row(
                        "SELECT id, current_sha256 FROM resources WHERE shelf_slug = ?1 AND slug = ?2",
                        params![shelf_slug, base_slug],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;

                let final_slug = match existing {
                    Some((_, existing_sha)) if existing_sha == body_sha => {
                        summary.skipped_duplicate_count += 1;
                        continue;
                    }
                    Some(_) => format!("{base_slug}-{}", &body_sha[..6]),
                    None => base_slug,
                };

                let saved = save_resource_tx(
                    &tx,
                    "legacy-import",
                    SaveResourceRequest {
                        shelf_slug: shelf_slug.to_string(),
                        slug: final_slug,
                        title,
                        body_md,
                        project_id: None,
                        trio_role: None,
                        lifecycle_tier: Some(lifecycle_tier),
                        triage_state: Some(TriageState::NeedsTriage),
                        summary: format!("Imported from legacy table '{table_name}'"),
                        expected_rev: None,
                    },
                )?;

                let raw_id =
                    pick_col(&row_map, &["id", "item_id", "entity_id"]).unwrap_or("unknown");
                let steward = pick_col(&row_map, &["steward", "owner", "author"])
                    .unwrap_or("legacy-import")
                    .to_string();
                let last_verified_at = pick_col(
                    &row_map,
                    &[
                        "last_verified_at",
                        "verified_at",
                        "updated_at",
                        "created_at",
                    ],
                )
                .unwrap_or(&now)
                .to_string();
                let freshness_state = pick_col(&row_map, &["freshness_state", "freshness"])
                    .and_then(|val| val.parse::<FreshnessState>().ok())
                    .unwrap_or(FreshnessState::Unknown);
                let lineage_ref = pick_col(
                    &row_map,
                    &[
                        "lineage_ref",
                        "source_ref",
                        "source_url",
                        "source_path",
                        "url",
                    ],
                )
                .map(str::to_string)
                .unwrap_or_else(|| format!("legacy:{table_name}:{raw_id}"));
                let next_action = pick_col(&row_map, &["next_action"])
                    .unwrap_or("Triage legacy item and promote or retire")
                    .to_string();

                let passport = SourcePassport {
                    resource_id: saved.resource.id.clone(),
                    steward,
                    last_verified_at,
                    freshness_state,
                    lineage_ref,
                    access_scope: shelf_slug.to_string(),
                    next_action,
                };
                tx.execute(
                    "INSERT INTO source_passports (
                        resource_id, steward, last_verified_at, freshness_state,
                        lineage_ref, access_scope, next_action
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                    ON CONFLICT(resource_id) DO UPDATE SET
                        steward = excluded.steward,
                        last_verified_at = excluded.last_verified_at,
                        freshness_state = excluded.freshness_state,
                        lineage_ref = excluded.lineage_ref,
                        access_scope = excluded.access_scope,
                        next_action = excluded.next_action",
                    params![
                        passport.resource_id,
                        passport.steward,
                        passport.last_verified_at,
                        passport.freshness_state.as_str(),
                        passport.lineage_ref,
                        passport.access_scope,
                        passport.next_action,
                    ],
                )?;

                summary.imported_count += 1;
                summary.imported_resource_ids.push(saved.resource.id);
            }
        }

        record_audit_tx(
            &tx,
            "legacy-import",
            "import_legacy_sqlite",
            &legacy_db_path.display().to_string(),
            "ok",
            serde_json::json!({
                "imported_count": summary.imported_count,
                "skipped_noise_count": summary.skipped_noise_count,
                "skipped_duplicate_count": summary.skipped_duplicate_count,
            }),
        )?;

        tx.commit()?;
        Ok(summary)
    }
}
