use rusqlite::{params, Connection};

use crate::error::StoreError;
use crate::grants::{
    check_shelf_access_tx, load_active_grant_tx, now_rfc3339, record_audit_tx,
    require_rules_acked_tx,
};
use crate::models::{LessonRecord, RecordLessonRequest, TrioProject};
use crate::resources::validate_slug;
use crate::trio::load_project_tx;
use crate::StreamlineStore;

fn map_lesson_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LessonRecord> {
    let tags_json: String = row.get(2)?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    Ok(LessonRecord {
        lesson_id: row.get(0)?,
        project_id: row.get(1)?,
        tags,
        observed_fact: row.get(3)?,
        cause_kind: row.get(4)?,
        cause_note: row.get(5)?,
        forward_rule: row.get(6)?,
        validation_state: row.get(7)?,
        recorded_by: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn normalize_cause_kind(raw: &str) -> Result<String, StoreError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "demonstrated" => Ok("demonstrated".to_string()),
        "hypothesis" => Ok("hypothesis".to_string()),
        other => Err(StoreError::InvalidInput(format!(
            "invalid cause_kind '{other}'; expected 'demonstrated' or 'hypothesis'"
        ))),
    }
}

fn normalize_validation_state(raw: Option<&str>) -> Result<String, StoreError> {
    let Some(val) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok("worked_once".to_string());
    };
    match val.to_ascii_lowercase().as_str() {
        "worked_once" => Ok("worked_once".to_string()),
        "verified_reusable" => Ok("verified_reusable".to_string()),
        other => Err(StoreError::InvalidInput(format!(
            "invalid validation_state '{other}'; expected 'worked_once' or 'verified_reusable'"
        ))),
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw_tag in tags {
        let trimmed = raw_tag.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !out
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(trimmed))
        {
            out.push(trimmed.to_string());
        }
    }
    out
}

pub(crate) fn load_lessons_tx(
    conn: &Connection,
    tag_filter: Option<&str>,
    project_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<LessonRecord>, StoreError> {
    let effective_limit = if limit == 0 { 25 } else { limit.min(200) };
    let normalized_tag = tag_filter.map(str::trim).filter(|val| !val.is_empty());
    let normalized_project = project_filter.map(str::trim).filter(|val| !val.is_empty());

    let mut out = Vec::new();
    if let Some(pid) = normalized_project {
        let mut stmt = conn.prepare(
            "SELECT lesson_id, project_id, tags_json, observed_fact, cause_kind, cause_note,
                    forward_rule, validation_state, recorded_by, created_at
             FROM lessons_learned
             WHERE project_id = ?1
             ORDER BY created_at DESC, rowid DESC",
        )?;
        let rows = stmt.query_map(params![pid], map_lesson_row)?;
        for item in rows {
            let lesson = item?;
            if let Some(expected_tag) = normalized_tag {
                if !lesson
                    .tags
                    .iter()
                    .any(|tag| tag.eq_ignore_ascii_case(expected_tag))
                {
                    continue;
                }
            }
            out.push(lesson);
            if out.len() >= effective_limit {
                break;
            }
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT lesson_id, project_id, tags_json, observed_fact, cause_kind, cause_note,
                    forward_rule, validation_state, recorded_by, created_at
             FROM lessons_learned
             ORDER BY created_at DESC, rowid DESC",
        )?;
        let rows = stmt.query_map([], map_lesson_row)?;
        for item in rows {
            let lesson = item?;
            if let Some(expected_tag) = normalized_tag {
                if !lesson
                    .tags
                    .iter()
                    .any(|tag| tag.eq_ignore_ascii_case(expected_tag))
                {
                    continue;
                }
            }
            out.push(lesson);
            if out.len() >= effective_limit {
                break;
            }
        }
    }

    Ok(out)
}

pub(crate) fn load_all_lessons_tx(conn: &Connection) -> Result<Vec<LessonRecord>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT lesson_id, project_id, tags_json, observed_fact, cause_kind, cause_note,
                forward_rule, validation_state, recorded_by, created_at
         FROM lessons_learned
         ORDER BY created_at ASC, lesson_id ASC",
    )?;
    let rows = stmt.query_map([], map_lesson_row)?;
    let mut out = Vec::new();
    let mut budget = crate::limits::Budget::new("vault lessons", crate::limits::MAX_RESULT_BYTES);
    for item in rows {
        budget.push(&mut out, item?)?;
    }
    Ok(out)
}

impl StreamlineStore {
    /// Records a forward lesson into the central `lessons_learned` database and marks
    /// `lessons_satisfied = 1` on the associated `trio_projects` row when `project_id` is set.
    pub fn record_lesson(
        &self,
        grant_id: &str,
        req: RecordLessonRequest,
    ) -> Result<LessonRecord, StoreError> {
        let observed_fact = req.observed_fact.trim().to_string();
        let forward_rule = req.forward_rule.trim().to_string();
        if observed_fact.is_empty() || forward_rule.is_empty() {
            return Err(StoreError::InvalidInput(
                "observed_fact and forward_rule cannot be empty".to_string(),
            ));
        }

        let cause_kind = normalize_cause_kind(&req.cause_kind)?;
        let cause_note = req.cause_note.trim().to_string();
        let validation_state = normalize_validation_state(req.validation_state.as_deref())?;
        let tags = normalize_tags(req.tags);

        let lesson_id = match req
            .lesson_id
            .as_deref()
            .map(str::trim)
            .filter(|val| !val.is_empty())
        {
            Some(explicit) => validate_slug(explicit, "lesson_id")?,
            None => format!("lesson-{}", uuid::Uuid::new_v4()),
        };

        let project_id = req
            .project_id
            .as_deref()
            .map(str::trim)
            .filter(|val| !val.is_empty())
            .map(str::to_string);

        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }

        if let Some(ref pid) = project_id {
            check_shelf_access_tx(&tx, &grant, "projects", Some(pid))?;
            let _ = load_project_tx(&tx, pid)?;
        }

        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM lessons_learned WHERE lesson_id = ?1)",
            params![lesson_id],
            |row| row.get(0),
        )?;
        if exists {
            return Err(StoreError::Conflict(format!(
                "lesson '{lesson_id}' already exists"
            )));
        }

        let tags_json = serde_json::to_string(&tags)?;
        let created_at = now_rfc3339();
        let recorded_by = grant.principal_id.clone();

        tx.execute(
            "INSERT INTO lessons_learned (
                lesson_id, project_id, tags_json, observed_fact, cause_kind,
                cause_note, forward_rule, validation_state, recorded_by, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                lesson_id,
                project_id,
                tags_json,
                observed_fact,
                cause_kind,
                cause_note,
                forward_rule,
                validation_state,
                recorded_by,
                created_at,
            ],
        )?;

        if let Some(ref pid) = project_id {
            tx.execute(
                "UPDATE trio_projects SET lessons_satisfied = 1 WHERE project_id = ?1",
                params![pid],
            )?;
        }

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "record_lesson",
            &lesson_id,
            "ok",
            serde_json::json!({
                "project_id": project_id,
                "cause_kind": cause_kind,
                "validation_state": validation_state,
                "tags": tags,
            }),
        )?;

        tx.commit()?;
        Ok(LessonRecord {
            lesson_id,
            project_id,
            tags,
            observed_fact,
            cause_kind,
            cause_note,
            forward_rule,
            validation_state,
            recorded_by,
            created_at,
        })
    }

    /// Queries central forward lessons newest-first, optionally filtered by tag (case-insensitive)
    /// and/or `project_id`, clamped to 1..=200 (0 defaults to 25).
    pub fn query_lessons(
        &self,
        grant_id: &str,
        tag_filter: Option<&str>,
        project_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<LessonRecord>, StoreError> {
        let conn = self.lock_conn()?;
        let _grant = load_active_grant_tx(&conn, grant_id)?;
        load_lessons_tx(&conn, tag_filter, project_filter, limit)
    }

    /// Marks temporary and build files cleanup as verified for a Trio+ project.
    pub fn mark_project_cleanup_verified(
        &self,
        grant_id: &str,
        project_id: &str,
        detail: &str,
    ) -> Result<TrioProject, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        check_shelf_access_tx(&tx, &grant, "projects", Some(project_id))?;
        let _ = load_project_tx(&tx, project_id)?;

        tx.execute(
            "UPDATE trio_projects SET cleanup_verified = 1 WHERE project_id = ?1",
            params![project_id],
        )?;

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "mark_project_cleanup_verified",
            project_id,
            "ok",
            serde_json::json!({ "detail": detail.trim() }),
        )?;

        let updated = load_project_tx(&tx, project_id)?;
        tx.commit()?;
        Ok(updated)
    }

    /// Explicitly satisfies the lessons-learned closeout gate when a project produced
    /// no new reusable lessons, recording the operator's non-empty reason in the audit log.
    pub fn record_no_new_lessons(
        &self,
        grant_id: &str,
        project_id: &str,
        reason: &str,
    ) -> Result<TrioProject, StoreError> {
        let trimmed_reason = reason.trim();
        if trimmed_reason.is_empty() {
            return Err(StoreError::InvalidInput(
                "reason cannot be empty when recording no new lessons".to_string(),
            ));
        }

        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        check_shelf_access_tx(&tx, &grant, "projects", Some(project_id))?;
        let _ = load_project_tx(&tx, project_id)?;

        tx.execute(
            "UPDATE trio_projects SET lessons_satisfied = 1 WHERE project_id = ?1",
            params![project_id],
        )?;

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "record_no_new_lessons",
            project_id,
            "ok",
            serde_json::json!({ "reason": trimmed_reason }),
        )?;

        let updated = load_project_tx(&tx, project_id)?;
        tx.commit()?;
        Ok(updated)
    }
}
