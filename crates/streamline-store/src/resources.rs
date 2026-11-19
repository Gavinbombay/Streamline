use std::collections::BTreeMap;

use rusqlite::{params, types::Value, Connection, OptionalExtension};
use streamline_core::{AuditEvent, FreshnessState, LifecycleTier, SourcePassport, TriageState};

use crate::error::StoreError;
use crate::grants::{
    check_shelf_access_tx, current_rules_sha256_tx, grant_has_tag_access, load_active_grant_tx,
    missing_tag_for_scope, now_rfc3339, record_audit_tx, require_rules_acked_tx, sha256_hex,
};
use crate::lessons::load_lessons_tx;
use crate::models::{
    AccessGrant, BootstrapView, DiffLine, ResourceFilter, ResourceRecord, ResourceRevision,
    ResourceSearchHit, ResourceWithPassport, RevisionDiffSummary, SaveResourceRequest,
    ShelfSummary, WordChange,
};
use crate::trio::map_trio_project_row;
use crate::StreamlineStore;

pub(crate) fn validate_slug(raw: &str, field_name: &str) -> Result<String, StoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
    {
        return Err(StoreError::InvalidInput(format!(
            "{field_name} '{raw}' is invalid"
        )));
    }
    Ok(trimmed.to_string())
}

pub(crate) fn map_resource_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResourceRecord> {
    let tier_str: String = row.get(5)?;
    let triage_str: String = row.get(6)?;
    Ok(ResourceRecord {
        id: row.get(0)?,
        shelf_slug: row.get(1)?,
        slug: row.get(2)?,
        project_id: row.get(3)?,
        trio_role: row.get(4)?,
        lifecycle_tier: tier_str
            .parse::<LifecycleTier>()
            .unwrap_or(LifecycleTier::Active),
        triage_state: triage_str
            .parse::<TriageState>()
            .unwrap_or(TriageState::Reviewed),
        title: row.get(7)?,
        current_rev: row.get(8)?,
        current_sha256: row.get(9)?,
        deleted_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

pub(crate) fn load_passport_tx(
    conn: &Connection,
    resource_id: &str,
) -> Result<Option<SourcePassport>, StoreError> {
    conn.query_row(
        "SELECT resource_id, steward, last_verified_at, freshness_state,
                lineage_ref, access_scope, next_action
         FROM source_passports WHERE resource_id = ?1",
        params![resource_id],
        |row| {
            let freshness_str: String = row.get(3)?;
            Ok(SourcePassport {
                resource_id: row.get(0)?,
                steward: row.get(1)?,
                last_verified_at: row.get(2)?,
                freshness_state: freshness_str
                    .parse::<FreshnessState>()
                    .unwrap_or(FreshnessState::Unknown),
                lineage_ref: row.get(4)?,
                access_scope: row.get(5)?,
                next_action: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(StoreError::from)
}

pub(crate) fn save_resource_tx(
    conn: &Connection,
    author_principal: &str,
    grant: Option<&AccessGrant>,
    req: SaveResourceRequest,
) -> Result<ResourceWithPassport, StoreError> {
    let shelf_slug = validate_slug(&req.shelf_slug, "shelf_slug")?;
    let slug = validate_slug(&req.slug, "slug")?;
    let dest_project_id = match req.project_id {
        Some(raw_pid) => Some(validate_slug(&raw_pid, "project_id")?),
        None => None,
    };

    let shelf_required_tag: String = conn
        .query_row(
            "SELECT required_tag FROM shelves WHERE slug = ?1",
            params![shelf_slug],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("shelf '{shelf_slug}' not found")))?;

    let existing = conn
        .query_row(
            "SELECT id, shelf_slug, slug, project_id, trio_role, lifecycle_tier,
                    triage_state, title, current_rev, current_sha256, deleted_at, updated_at
             FROM resources WHERE shelf_slug = ?1 AND slug = ?2",
            params![shelf_slug, slug],
            map_resource_row,
        )
        .optional()?;

    if let Some(active_grant) = grant {
        if let Some(ref prev) = existing {
            if !grant_has_tag_access(
                active_grant,
                &shelf_slug,
                &shelf_required_tag,
                prev.project_id.as_deref(),
            ) {
                return Err(StoreError::ForbiddenTag {
                    required_tag: missing_tag_for_scope(
                        &shelf_slug,
                        &shelf_required_tag,
                        prev.project_id.as_deref(),
                    ),
                });
            }
        }
        if !grant_has_tag_access(
            active_grant,
            &shelf_slug,
            &shelf_required_tag,
            dest_project_id.as_deref(),
        ) {
            return Err(StoreError::ForbiddenTag {
                required_tag: missing_tag_for_scope(
                    &shelf_slug,
                    &shelf_required_tag,
                    dest_project_id.as_deref(),
                ),
            });
        }
    }

    if req.title.trim().is_empty() {
        return Err(StoreError::InvalidInput(
            "resource title cannot be empty".to_string(),
        ));
    }

    let sha256 = sha256_hex(req.body_md.as_bytes());
    let now = now_rfc3339();

    let resource_id = match existing {
        Some(prev) => {
            if let Some(expected_rev) = req.expected_rev {
                if prev.current_rev != expected_rev {
                    return Err(StoreError::Conflict(format!(
                        "expected rev {} for {}/{}, found {}",
                        expected_rev, shelf_slug, slug, prev.current_rev
                    )));
                }
            }
            let next_rev = prev.current_rev + 1;
            let next_tier = req.lifecycle_tier.unwrap_or(prev.lifecycle_tier);
            let next_triage = req.triage_state.unwrap_or(prev.triage_state);
            let next_project = dest_project_id.or(prev.project_id);
            let next_role = req.trio_role.or(prev.trio_role);

            conn.execute(
                "UPDATE resources
                 SET title = ?1, project_id = ?2, trio_role = ?3, lifecycle_tier = ?4,
                     triage_state = ?5, current_rev = ?6, current_sha256 = ?7,
                     deleted_at = NULL, updated_at = ?8
                 WHERE id = ?9",
                params![
                    req.title.trim(),
                    next_project,
                    next_role,
                    next_tier.as_str(),
                    next_triage.as_str(),
                    next_rev,
                    sha256,
                    now,
                    prev.id
                ],
            )?;

            conn.execute(
                "INSERT INTO resource_revisions (
                    resource_id, rev, body_md, sha256, author_principal, summary, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    prev.id,
                    next_rev,
                    req.body_md,
                    sha256,
                    author_principal,
                    req.summary,
                    now
                ],
            )?;

            prev.id
        }
        None => {
            if let Some(expected_rev) = req.expected_rev {
                if expected_rev != 0 {
                    return Err(StoreError::Conflict(format!(
                        "expected rev {} for new resource {}/{}",
                        expected_rev, shelf_slug, slug
                    )));
                }
            }
            let new_id = format!("res-{}", uuid::Uuid::new_v4());
            let tier = req.lifecycle_tier.unwrap_or(LifecycleTier::Active);
            let triage = req.triage_state.unwrap_or(TriageState::Reviewed);

            conn.execute(
                "INSERT INTO resources (
                    id, shelf_slug, slug, project_id, trio_role, lifecycle_tier,
                    triage_state, title, current_rev, current_sha256, deleted_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, NULL, ?10)",
                params![
                    new_id,
                    shelf_slug,
                    slug,
                    dest_project_id,
                    req.trio_role,
                    tier.as_str(),
                    triage.as_str(),
                    req.title.trim(),
                    sha256,
                    now
                ],
            )?;

            conn.execute(
                "INSERT INTO resource_revisions (
                    resource_id, rev, body_md, sha256, author_principal, summary, created_at
                ) VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    new_id,
                    req.body_md,
                    sha256,
                    author_principal,
                    req.summary,
                    now
                ],
            )?;

            new_id
        }
    };

    record_audit_tx(
        conn,
        author_principal,
        "save_resource",
        &resource_id,
        "ok",
        serde_json::json!({
            "shelf_slug": shelf_slug,
            "slug": slug,
            "sha256": sha256,
        }),
    )?;

    read_resource_by_id_tx(conn, &resource_id)
}

pub(crate) fn read_resource_by_id_tx(
    conn: &Connection,
    resource_id: &str,
) -> Result<ResourceWithPassport, StoreError> {
    let resource = conn
        .query_row(
            "SELECT id, shelf_slug, slug, project_id, trio_role, lifecycle_tier,
                    triage_state, title, current_rev, current_sha256, deleted_at, updated_at
             FROM resources WHERE id = ?1",
            params![resource_id],
            map_resource_row,
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("resource '{resource_id}' not found")))?;

    let body_md: String = conn.query_row(
        "SELECT body_md FROM resource_revisions WHERE resource_id = ?1 AND rev = ?2",
        params![resource.id, resource.current_rev],
        |row| row.get(0),
    )?;
    let passport = load_passport_tx(conn, &resource.id)?;

    Ok(ResourceWithPassport {
        resource,
        body_md,
        passport,
    })
}

fn can_read_resource_row(
    grant: &AccessGrant,
    shelf_required_tag: &str,
    resource: &ResourceRecord,
) -> bool {
    grant_has_tag_access(
        grant,
        &resource.shelf_slug,
        shelf_required_tag,
        resource.project_id.as_deref(),
    )
}

fn check_shelf_entry_gate_tx(
    conn: &Connection,
    grant: &AccessGrant,
    shelf_slug: &str,
) -> Result<(), StoreError> {
    let required_tag: String = conn
        .query_row(
            "SELECT required_tag FROM shelves WHERE slug = ?1",
            params![shelf_slug],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("shelf '{shelf_slug}' not found")))?;

    if grant_has_tag_access(grant, shelf_slug, &required_tag, None) {
        return Ok(());
    }

    if shelf_slug == "projects"
        && grant
            .granted_tags
            .iter()
            .any(|t| t.starts_with("project:") && t.len() > "project:".len())
    {
        return Ok(());
    }

    Err(StoreError::ForbiddenTag { required_tag })
}

fn verify_shelf_resource_access_tx(
    conn: &Connection,
    grant: &AccessGrant,
    shelf_slug: &str,
    slug: &str,
    project_id: Option<&str>,
) -> Result<(), StoreError> {
    match check_shelf_access_tx(conn, grant, shelf_slug, project_id) {
        Ok(()) => Ok(()),
        Err(StoreError::ForbiddenTag { .. }) => Err(StoreError::NotFound(format!(
            "resource '{shelf_slug}/{slug}' not found"
        ))),
        Err(other) => Err(other),
    }
}

fn authorize_resource_by_id_tx(
    conn: &Connection,
    grant: &AccessGrant,
    resource_id: &str,
) -> Result<(), StoreError> {
    let scope: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT shelf_slug, project_id FROM resources WHERE id = ?1 AND deleted_at IS NULL",
            params![resource_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let Some((shelf_slug, project_id)) = scope else {
        return Err(StoreError::NotFound(format!(
            "resource '{resource_id}' not found"
        )));
    };

    match check_shelf_access_tx(conn, grant, &shelf_slug, project_id.as_deref()) {
        Ok(()) => Ok(()),
        Err(StoreError::ForbiddenTag { .. }) => Err(StoreError::NotFound(format!(
            "resource '{resource_id}' not found"
        ))),
        Err(other) => Err(other),
    }
}

fn compute_lcs_diff<T: PartialEq + Clone>(left: &[T], right: &[T]) -> Vec<(i8, T)> {
    let n = left.len();
    let m = right.len();
    if n * m > 400_000 {
        let mut out = Vec::with_capacity(n + m);
        for item in left {
            out.push((-1, item.clone()));
        }
        for item in right {
            out.push((1, item.clone()));
        }
        return out;
    }

    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            if left[i] == right[j] {
                dp[i][j] = dp[i + 1][j + 1] + 1;
            } else {
                dp[i][j] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    let mut out = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < n && j < m {
        if left[i] == right[j] {
            out.push((0, left[i].clone()));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push((-1, left[i].clone()));
            i += 1;
        } else {
            out.push((1, right[j].clone()));
            j += 1;
        }
    }
    while i < n {
        out.push((-1, left[i].clone()));
        i += 1;
    }
    while j < m {
        out.push((1, right[j].clone()));
        j += 1;
    }
    out
}

/// Computes a structured line and word diff between two markdown strings.
pub fn compare_texts(
    left_rev: i64,
    right_rev: i64,
    left_text: &str,
    right_text: &str,
) -> RevisionDiffSummary {
    let left_lines: Vec<&str> = left_text.lines().collect();
    let right_lines: Vec<&str> = right_text.lines().collect();
    let line_ops = compute_lcs_diff(&left_lines, &right_lines);

    let mut unchanged_lines = 0usize;
    let mut added_lines = Vec::new();
    let mut removed_lines = Vec::new();
    let mut line_hunks = Vec::new();

    for (op, line) in line_ops {
        let text = line.to_string();
        match op {
            0 => {
                unchanged_lines += 1;
                line_hunks.push(DiffLine::Context(text));
            }
            1 => {
                added_lines.push(text.clone());
                line_hunks.push(DiffLine::Added(text));
            }
            _ => {
                removed_lines.push(text.clone());
                line_hunks.push(DiffLine::Removed(text));
            }
        }
    }

    let left_words: Vec<&str> = left_text.split_whitespace().collect();
    let right_words: Vec<&str> = right_text.split_whitespace().collect();
    let word_ops = compute_lcs_diff(&left_words, &right_words);
    let mut changed_words = Vec::new();
    for (op, word) in word_ops {
        let w = word.to_string();
        match op {
            0 => changed_words.push(WordChange::Context(w)),
            1 => changed_words.push(WordChange::Added(w)),
            _ => changed_words.push(WordChange::Removed(w)),
        }
    }

    let summary = format!(
        "+{} lines / -{} lines ({} unchanged)",
        added_lines.len(),
        removed_lines.len(),
        unchanged_lines
    );

    RevisionDiffSummary {
        left_rev,
        right_rev,
        left_sha256: sha256_hex(left_text.as_bytes()),
        right_sha256: sha256_hex(right_text.as_bytes()),
        unchanged_lines,
        added_lines,
        removed_lines,
        line_hunks,
        changed_words,
        summary,
    }
}

fn build_snippet(body_md: &str, needle_lower: &str) -> String {
    if let Some(pos) = body_md.to_ascii_lowercase().find(needle_lower) {
        let start = body_md[..pos]
            .char_indices()
            .rev()
            .nth(40)
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        let end = body_md[pos..]
            .char_indices()
            .nth(120)
            .map(|(idx, _)| pos + idx)
            .unwrap_or(body_md.len());
        return body_md[start..end].replace('\n', " ");
    }
    body_md
        .chars()
        .take(140)
        .collect::<String>()
        .replace('\n', " ")
}

impl StreamlineStore {
    /// Returns the active-only governance view for bootstrap.
    /// Dormant, Legacy, and Retired items never appear in `active_resources`.
    pub fn bootstrap_view(&self, grant_id: &str) -> Result<BootstrapView, StoreError> {
        let conn = self.lock_conn()?;
        let grant = load_active_grant_tx(&conn, grant_id)?;
        let rules_sha256 = current_rules_sha256_tx(&conn)?;
        let rules_acked = grant.acked_rules_sha256.as_deref() == Some(rules_sha256.as_str());

        let mut active_resources = Vec::new();
        {
            let mut stmt = conn.prepare(
                "SELECT r.id, r.shelf_slug, r.slug, r.project_id, r.trio_role, r.lifecycle_tier,
                        r.triage_state, r.title, r.current_rev, r.current_sha256, r.deleted_at,
                        r.updated_at, s.required_tag
                 FROM resources r
                 JOIN shelves s ON s.slug = r.shelf_slug
                 WHERE r.lifecycle_tier = 'active' AND r.deleted_at IS NULL
                 ORDER BY r.shelf_slug ASC, r.slug ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                let res = map_resource_row(row)?;
                let req_tag: String = row.get(12)?;
                Ok((res, req_tag))
            })?;
            for item in rows {
                let (res, req_tag) = item?;
                if can_read_resource_row(&grant, &req_tag, &res) {
                    active_resources.push(res);
                }
            }
        }

        let mut required_onboarding_slugs: Vec<String> = vec!["standards/rules".to_string()];
        for res in &active_resources {
            if res.shelf_slug == "standards" {
                let full_slug = format!("{}/{}", res.shelf_slug, res.slug);
                if !required_onboarding_slugs.contains(&full_slug) {
                    required_onboarding_slugs.push(full_slug);
                }
            }
        }

        let mut active_projects = Vec::new();
        {
            let mut stmt = conn.prepare(
                "SELECT project_id, create_id, title, work_kind, difficulty, risk, profile,
                        expected_roles_json, plan_locked_rev, state, how_it_works_resource_id,
                        cleanup_verified, lessons_satisfied, github_release, audit_passed_commit,
                        created_at
                 FROM trio_projects
                 WHERE state != 'DONE'
                 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], map_trio_project_row)?;
            for item in rows {
                let proj = item?;
                if grant_has_tag_access(&grant, "projects", "projects", Some(&proj.project_id)) {
                    active_projects.push(proj);
                }
            }
        }

        let forward_lessons = load_lessons_tx(&conn, None, None, 25)?;

        Ok(BootstrapView {
            rules_sha256,
            rules_acked,
            granted_tags: grant.granted_tags,
            required_onboarding_slugs,
            active_resources,
            active_projects,
            forward_lessons,
        })
    }

    /// Lists shelves authorized for the caller along with resource and freshness counts.
    pub fn list_shelves(&self, grant_id: &str) -> Result<Vec<ShelfSummary>, StoreError> {
        let conn = self.lock_conn()?;
        let grant = load_active_grant_tx(&conn, grant_id)?;

        let mut shelves: Vec<(String, String, String)> = Vec::new();
        {
            let mut stmt =
                conn.prepare("SELECT slug, title, required_tag FROM shelves ORDER BY slug ASC")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            for item in rows {
                let (slug, title, required_tag) = item?;
                let project_scoped = slug == "projects"
                    && grant
                        .granted_tags
                        .iter()
                        .any(|tag_item| tag_item.starts_with("project:"));
                if grant_has_tag_access(&grant, &slug, &required_tag, None) || project_scoped {
                    shelves.push((slug, title, required_tag));
                }
            }
        }

        let mut summaries = Vec::with_capacity(shelves.len());
        for (slug, title, required_tag) in shelves {
            let mut total_count = 0usize;
            let mut active_count = 0usize;
            let mut verified_count = 0usize;
            let mut stale_count = 0usize;
            let mut drift_count = 0usize;
            let mut unknown_count = 0usize;

            let mut stmt = conn.prepare(
                "SELECT r.project_id, r.lifecycle_tier, COALESCE(sp.freshness_state, 'unknown')
                 FROM resources r
                 LEFT JOIN source_passports sp ON sp.resource_id = r.id
                 WHERE r.shelf_slug = ?1 AND r.deleted_at IS NULL",
            )?;
            let rows = stmt.query_map(params![slug], |row| {
                let project_id: Option<String> = row.get(0)?;
                let tier_str: String = row.get(1)?;
                let freshness_str: String = row.get(2)?;
                Ok((project_id, tier_str, freshness_str))
            })?;
            for item in rows {
                let (project_id, tier_str, freshness_str) = item?;
                if !grant_has_tag_access(&grant, &slug, &required_tag, project_id.as_deref()) {
                    continue;
                }
                total_count += 1;
                if tier_str == "active" {
                    active_count += 1;
                }
                match freshness_str
                    .parse::<FreshnessState>()
                    .unwrap_or(FreshnessState::Unknown)
                {
                    FreshnessState::Verified => verified_count += 1,
                    FreshnessState::Stale => stale_count += 1,
                    FreshnessState::Drift => drift_count += 1,
                    FreshnessState::Unknown => unknown_count += 1,
                }
            }

            summaries.push(ShelfSummary {
                slug,
                title,
                required_tag,
                total_count,
                active_count,
                verified_count,
                stale_count,
                drift_count,
                unknown_count,
            });
        }

        Ok(summaries)
    }

    /// Lists resources matching `filter` within the caller's authorized tags.
    pub fn list_resources(
        &self,
        grant_id: &str,
        filter: &ResourceFilter,
    ) -> Result<Vec<ResourceRecord>, StoreError> {
        let conn = self.lock_conn()?;
        let grant = load_active_grant_tx(&conn, grant_id)?;

        if let Some(ref shelf_slug) = filter.shelf_slug {
            let required_tag: String = conn
                .query_row(
                    "SELECT required_tag FROM shelves WHERE slug = ?1",
                    params![shelf_slug],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| StoreError::NotFound(format!("shelf '{shelf_slug}' not found")))?;

            let has_shelf_tag = grant_has_tag_access(
                &grant,
                shelf_slug,
                &required_tag,
                filter.project_id.as_deref(),
            );
            let has_any_project_tag = shelf_slug == "projects"
                && filter.project_id.is_none()
                && grant
                    .granted_tags
                    .iter()
                    .any(|tag_item| tag_item.starts_with("project:"));
            if !has_shelf_tag && !has_any_project_tag {
                let missing = if shelf_slug == "projects" {
                    if let Some(ref pid) = filter.project_id {
                        format!("project:{pid}")
                    } else {
                        required_tag
                    }
                } else {
                    required_tag
                };
                return Err(StoreError::ForbiddenTag {
                    required_tag: missing,
                });
            }
        }

        let mut sql = String::from(
            "SELECT r.id, r.shelf_slug, r.slug, r.project_id, r.trio_role, r.lifecycle_tier,
                    r.triage_state, r.title, r.current_rev, r.current_sha256, r.deleted_at,
                    r.updated_at, s.required_tag
             FROM resources r
             JOIN shelves s ON s.slug = r.shelf_slug
             LEFT JOIN source_passports sp ON sp.resource_id = r.id
             WHERE 1 = 1",
        );
        let mut values: Vec<Value> = Vec::new();

        if !filter.include_deleted {
            sql.push_str(" AND r.deleted_at IS NULL");
        }
        if let Some(ref shelf_slug) = filter.shelf_slug {
            sql.push_str(" AND r.shelf_slug = ?");
            values.push(Value::from(shelf_slug.clone()));
        }
        if let Some(tier) = filter.lifecycle_tier {
            sql.push_str(" AND r.lifecycle_tier = ?");
            values.push(Value::from(tier.as_str().to_string()));
        }
        if let Some(triage) = filter.triage_state {
            sql.push_str(" AND r.triage_state = ?");
            values.push(Value::from(triage.as_str().to_string()));
        }
        if let Some(ref project_id) = filter.project_id {
            sql.push_str(" AND r.project_id = ?");
            values.push(Value::from(project_id.clone()));
        }
        if let Some(freshness) = filter.freshness_state {
            sql.push_str(" AND COALESCE(sp.freshness_state, 'unknown') = ?");
            values.push(Value::from(freshness.as_str().to_string()));
        }
        sql.push_str(" ORDER BY r.shelf_slug ASC, r.slug ASC");

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            let res = map_resource_row(row)?;
            let req_tag: String = row.get(12)?;
            Ok((res, req_tag))
        })?;

        let mut out = Vec::new();
        for item in rows {
            let (res, req_tag) = item?;
            if can_read_resource_row(&grant, &req_tag, &res) {
                out.push(res);
            }
        }
        Ok(out)
    }

    /// Reads one resource by `(shelf_slug, slug)` along with its markdown body and Source Passport.
    pub fn read_resource(
        &self,
        grant_id: &str,
        shelf_slug: &str,
        slug: &str,
    ) -> Result<ResourceWithPassport, StoreError> {
        let conn = self.lock_conn()?;
        let grant = load_active_grant_tx(&conn, grant_id)?;
        check_shelf_entry_gate_tx(&conn, &grant, shelf_slug)?;

        let (resource_id, project_id): (String, Option<String>) = conn
            .query_row(
                "SELECT id, project_id FROM resources
                 WHERE shelf_slug = ?1 AND slug = ?2 AND deleted_at IS NULL",
                params![shelf_slug, slug],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::NotFound(format!("resource '{shelf_slug}/{slug}' not found"))
            })?;

        verify_shelf_resource_access_tx(&conn, &grant, shelf_slug, slug, project_id.as_deref())?;
        read_resource_by_id_tx(&conn, &resource_id)
    }

    /// Reads one resource by `resource_id`, enforcing tag authorization.
    pub fn read_resource_by_id(
        &self,
        grant_id: &str,
        resource_id: &str,
    ) -> Result<ResourceWithPassport, StoreError> {
        let conn = self.lock_conn()?;
        let grant = load_active_grant_tx(&conn, grant_id)?;
        authorize_resource_by_id_tx(&conn, &grant, resource_id)?;
        read_resource_by_id_tx(&conn, resource_id)
    }

    /// Full-text substring search across authorized resources and current revisions.
    pub fn search_resources(
        &self,
        grant_id: &str,
        query: &str,
        filter: &ResourceFilter,
    ) -> Result<Vec<ResourceSearchHit>, StoreError> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return Err(StoreError::InvalidInput(
                "search query must be non-empty".to_string(),
            ));
        }

        let resources = self.list_resources(grant_id, filter)?;
        let conn = self.lock_conn()?;
        let mut hits = Vec::new();
        for res in resources {
            let body_md: String = conn.query_row(
                "SELECT body_md FROM resource_revisions WHERE resource_id = ?1 AND rev = ?2",
                params![res.id, res.current_rev],
                |row| row.get(0),
            )?;
            let matches = res.title.to_ascii_lowercase().contains(&needle)
                || res.slug.to_ascii_lowercase().contains(&needle)
                || body_md.to_ascii_lowercase().contains(&needle);
            if matches {
                let passport = load_passport_tx(&conn, &res.id)?;
                let snippet = build_snippet(&body_md, &needle);
                hits.push(ResourceSearchHit {
                    resource: res,
                    snippet,
                    passport,
                });
            }
        }
        Ok(hits)
    }

    /// Saves a resource as an append-only revision after enforcing the rule-ack gate,
    /// write permission, tag access, and optional optimistic revision check.
    pub fn save_resource(
        &self,
        grant_id: &str,
        req: SaveResourceRequest,
    ) -> Result<ResourceWithPassport, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        let saved = save_resource_tx(&tx, &grant.principal_id, Some(&grant), req)?;
        tx.commit()?;
        Ok(saved)
    }

    /// Returns append-only revision history for a resource, newest revision first.
    pub fn resource_history(
        &self,
        grant_id: &str,
        shelf_slug: &str,
        slug: &str,
    ) -> Result<Vec<ResourceRevision>, StoreError> {
        let conn = self.lock_conn()?;
        let grant = load_active_grant_tx(&conn, grant_id)?;
        check_shelf_entry_gate_tx(&conn, &grant, shelf_slug)?;

        let (resource_id, project_id): (String, Option<String>) = conn
            .query_row(
                "SELECT id, project_id FROM resources WHERE shelf_slug = ?1 AND slug = ?2",
                params![shelf_slug, slug],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::NotFound(format!("resource '{shelf_slug}/{slug}' not found"))
            })?;

        verify_shelf_resource_access_tx(&conn, &grant, shelf_slug, slug, project_id.as_deref())?;

        let mut stmt = conn.prepare(
            "SELECT resource_id, rev, body_md, sha256, author_principal, summary, created_at
             FROM resource_revisions WHERE resource_id = ?1 ORDER BY rev DESC",
        )?;
        let rows = stmt.query_map(params![resource_id], |row| {
            Ok(ResourceRevision {
                resource_id: row.get(0)?,
                rev: row.get(1)?,
                body_md: row.get(2)?,
                sha256: row.get(3)?,
                author_principal: row.get(4)?,
                summary: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut history = Vec::new();
        for item in rows {
            history.push(item?);
        }
        Ok(history)
    }

    /// Restores a prior revision's content by appending a brand-new revision.
    pub fn restore_resource(
        &self,
        grant_id: &str,
        shelf_slug: &str,
        slug: &str,
        target_rev: i64,
        summary: Option<String>,
    ) -> Result<ResourceWithPassport, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        check_shelf_entry_gate_tx(&tx, &grant, shelf_slug)?;

        let res = tx
            .query_row(
                "SELECT id, shelf_slug, slug, project_id, trio_role, lifecycle_tier,
                        triage_state, title, current_rev, current_sha256, deleted_at, updated_at
                 FROM resources WHERE shelf_slug = ?1 AND slug = ?2",
                params![shelf_slug, slug],
                map_resource_row,
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::NotFound(format!("resource '{shelf_slug}/{slug}' not found"))
            })?;

        verify_shelf_resource_access_tx(&tx, &grant, shelf_slug, slug, res.project_id.as_deref())?;

        let target_body: String = tx
            .query_row(
                "SELECT body_md FROM resource_revisions WHERE resource_id = ?1 AND rev = ?2",
                params![res.id, target_rev],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::NotFound(format!(
                    "revision {target_rev} not found for '{shelf_slug}/{slug}'"
                ))
            })?;

        let saved = save_resource_tx(
            &tx,
            &grant.principal_id,
            Some(&grant),
            SaveResourceRequest {
                shelf_slug: shelf_slug.to_string(),
                slug: slug.to_string(),
                title: res.title,
                body_md: target_body,
                project_id: res.project_id,
                trio_role: res.trio_role,
                lifecycle_tier: Some(res.lifecycle_tier),
                triage_state: Some(res.triage_state),
                summary: summary.unwrap_or_else(|| format!("Restore from rev {target_rev}")),
                expected_rev: Some(res.current_rev),
            },
        )?;
        tx.commit()?;
        Ok(saved)
    }

    /// Computes a structured line and word diff between two revisions of the same resource.
    pub fn compare_revisions(
        &self,
        grant_id: &str,
        shelf_slug: &str,
        slug: &str,
        left_rev: i64,
        right_rev: i64,
    ) -> Result<RevisionDiffSummary, StoreError> {
        let conn = self.lock_conn()?;
        let grant = load_active_grant_tx(&conn, grant_id)?;
        check_shelf_entry_gate_tx(&conn, &grant, shelf_slug)?;

        let (resource_id, project_id): (String, Option<String>) = conn
            .query_row(
                "SELECT id, project_id FROM resources WHERE shelf_slug = ?1 AND slug = ?2",
                params![shelf_slug, slug],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::NotFound(format!("resource '{shelf_slug}/{slug}' not found"))
            })?;
        verify_shelf_resource_access_tx(&conn, &grant, shelf_slug, slug, project_id.as_deref())?;

        let left_body: String = conn
            .query_row(
                "SELECT body_md FROM resource_revisions WHERE resource_id = ?1 AND rev = ?2",
                params![resource_id, left_rev],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("revision {left_rev} not found")))?;

        let right_body: String = conn
            .query_row(
                "SELECT body_md FROM resource_revisions WHERE resource_id = ?1 AND rev = ?2",
                params![resource_id, right_rev],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("revision {right_rev} not found")))?;

        Ok(compare_texts(left_rev, right_rev, &left_body, &right_body))
    }

    /// Creates or updates a resource's Source Passport.
    pub fn upsert_source_passport(
        &self,
        grant_id: &str,
        passport: SourcePassport,
    ) -> Result<SourcePassport, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }

        authorize_resource_by_id_tx(&tx, &grant, &passport.resource_id)?;

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

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "upsert_source_passport",
            &passport.resource_id,
            "ok",
            serde_json::json!({ "freshness_state": passport.freshness_state.as_str() }),
        )?;

        tx.commit()?;
        Ok(passport)
    }

    /// Reads the Source Passport for a resource if present.
    pub fn get_source_passport(
        &self,
        grant_id: &str,
        resource_id: &str,
    ) -> Result<Option<SourcePassport>, StoreError> {
        let conn = self.lock_conn()?;
        let grant = load_active_grant_tx(&conn, grant_id)?;
        authorize_resource_by_id_tx(&conn, &grant, resource_id)?;
        load_passport_tx(&conn, resource_id)
    }

    /// Updates a resource's lifecycle tier, triage state, or linked Trio+ project.
    pub fn set_lifecycle_tier(
        &self,
        grant_id: &str,
        shelf_slug: &str,
        slug: &str,
        lifecycle_tier: LifecycleTier,
        triage_state: Option<TriageState>,
        target_project_id: Option<String>,
    ) -> Result<ResourceRecord, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        check_shelf_entry_gate_tx(&tx, &grant, shelf_slug)?;

        let prev = tx
            .query_row(
                "SELECT id, shelf_slug, slug, project_id, trio_role, lifecycle_tier,
                        triage_state, title, current_rev, current_sha256, deleted_at, updated_at
                 FROM resources WHERE shelf_slug = ?1 AND slug = ?2",
                params![shelf_slug, slug],
                map_resource_row,
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::NotFound(format!("resource '{shelf_slug}/{slug}' not found"))
            })?;

        verify_shelf_resource_access_tx(&tx, &grant, shelf_slug, slug, prev.project_id.as_deref())?;
        let clean_target_project_id = match target_project_id {
            Some(raw_pid) => {
                let clean = validate_slug(&raw_pid, "target_project_id")?;
                check_shelf_access_tx(&tx, &grant, shelf_slug, Some(&clean))?;
                Some(clean)
            }
            None => None,
        };

        let next_triage = triage_state.unwrap_or(prev.triage_state);
        let next_project = clean_target_project_id.or(prev.project_id);
        let now = now_rfc3339();

        tx.execute(
            "UPDATE resources
             SET lifecycle_tier = ?1, triage_state = ?2, project_id = ?3, updated_at = ?4
             WHERE id = ?5",
            params![
                lifecycle_tier.as_str(),
                next_triage.as_str(),
                next_project,
                now,
                prev.id
            ],
        )?;

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "set_lifecycle_tier",
            &prev.id,
            "ok",
            serde_json::json!({
                "lifecycle_tier": lifecycle_tier.as_str(),
                "triage_state": next_triage.as_str(),
                "project_id": next_project,
            }),
        )?;

        let updated = tx.query_row(
            "SELECT id, shelf_slug, slug, project_id, trio_role, lifecycle_tier,
                    triage_state, title, current_rev, current_sha256, deleted_at, updated_at
             FROM resources WHERE id = ?1",
            params![prev.id],
            map_resource_row,
        )?;
        tx.commit()?;
        Ok(updated)
    }

    /// Lists recent audit events (newest first) for diagnostics.
    pub fn list_audit_events(&self, limit: usize) -> Result<Vec<AuditEvent>, StoreError> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT event_id, principal_id, action, target_id, outcome, detail_json, created_at
             FROM audit_events ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(AuditEvent {
                event_id: row.get(0)?,
                principal_id: row.get(1)?,
                action: row.get(2)?,
                target_id: row.get(3)?,
                outcome: row.get(4)?,
                detail_json: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        let mut events = Vec::new();
        for item in rows {
            events.push(item?);
        }
        Ok(events)
    }

    /// Helper returning all non-deleted resources grouped cleanly for internal exporters.
    pub(crate) fn load_all_non_deleted_with_bodies(
        conn: &Connection,
    ) -> Result<BTreeMap<(String, String), ResourceWithPassport>, StoreError> {
        let mut map = BTreeMap::new();
        let mut stmt = conn.prepare(
            "SELECT r.id, r.shelf_slug, r.slug, r.project_id, r.trio_role, r.lifecycle_tier,
                    r.triage_state, r.title, r.current_rev, r.current_sha256, r.deleted_at,
                    r.updated_at, rr.body_md
             FROM resources r
             JOIN resource_revisions rr
               ON rr.resource_id = r.id AND rr.rev = r.current_rev
             WHERE r.deleted_at IS NULL
             ORDER BY r.shelf_slug ASC, r.slug ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let res = map_resource_row(row)?;
            let body_md: String = row.get(12)?;
            Ok((res, body_md))
        })?;
        for item in rows {
            let (res, body_md) = item?;
            let passport = load_passport_tx(conn, &res.id)?;
            map.insert(
                (res.shelf_slug.clone(), res.slug.clone()),
                ResourceWithPassport {
                    resource: res,
                    body_md,
                    passport,
                },
            );
        }
        Ok(map)
    }
}
