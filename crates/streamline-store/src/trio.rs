use rusqlite::{params, Connection, OptionalExtension};
use streamline_core::{
    assess_work_with_modules, check_sync, validate_how_it_works, validate_stage_transition,
    ConveyorRun, CoreError, Divergence, DivergenceStatus, LifecycleTier, OwnDecision,
    PrincipalRole, ReqStage, RequirementRow, RunPhase, RunState, StageTransitionRequest,
    SyncVerdict, TriageState, TrioProfile, WorkKind,
};

use crate::error::StoreError;
use crate::grants::{
    check_shelf_access_tx, grant_has_tag_access, load_active_grant_tx, now_rfc3339,
    record_audit_tx, require_rules_acked_tx, sha256_hex,
};
use crate::models::{
    CreateTrioProjectRequest, RecordDivergenceRequest, ResourceRecord, SaveResourceRequest,
    SaveSyncReportRequest, SyncReport, TrioProject, TrioProjectBundle,
    UpdateRequirementStageRequest,
};
use crate::resources::{map_resource_row, read_resource_by_id_tx, save_resource_tx, validate_slug};
use crate::StreamlineStore;

pub(crate) fn map_trio_project_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrioProject> {
    let work_kind_str: String = row.get(3)?;
    let profile_str: String = row.get(6)?;
    let roles_json: String = row.get(7)?;
    let state_str: String = row.get(9)?;
    let cleanup_verified_int: i64 = row.get(11)?;
    let lessons_satisfied_int: i64 = row.get(12)?;
    Ok(TrioProject {
        project_id: row.get(0)?,
        create_id: row.get(1)?,
        title: row.get(2)?,
        work_kind: work_kind_str
            .parse::<WorkKind>()
            .unwrap_or(WorkKind::General),
        difficulty: row.get::<_, i64>(4)? as u8,
        risk: row.get::<_, i64>(5)? as u8,
        profile: profile_str
            .parse::<TrioProfile>()
            .unwrap_or(TrioProfile::P1),
        expected_roles: serde_json::from_str(&roles_json).unwrap_or_default(),
        plan_locked_rev: row.get(8)?,
        state: state_str.parse::<RunState>().unwrap_or(RunState::Staged),
        how_it_works_resource_id: row.get(10)?,
        cleanup_verified: cleanup_verified_int != 0,
        lessons_satisfied: lessons_satisfied_int != 0,
        created_at: row.get(13)?,
    })
}

pub(crate) fn load_project_tx(
    conn: &Connection,
    project_id: &str,
) -> Result<TrioProject, StoreError> {
    conn.query_row(
        "SELECT project_id, create_id, title, work_kind, difficulty, risk, profile,
                expected_roles_json, plan_locked_rev, state, how_it_works_resource_id,
                cleanup_verified, lessons_satisfied, created_at
         FROM trio_projects WHERE project_id = ?1",
        params![project_id],
        map_trio_project_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::NotFound(format!("trio project '{project_id}' not found")))
}

fn map_sync_report_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncReport> {
    let paired_json: String = row.get(4)?;
    let verdict_str: String = row.get(5)?;
    Ok(SyncReport {
        report_id: row.get(0)?,
        project_id: row.get(1)?,
        plan_rev: row.get(2)?,
        asbuilt_rev: row.get(3)?,
        paired_rows: serde_json::from_str(&paired_json).unwrap_or_default(),
        verdict: verdict_str
            .parse::<SyncVerdict>()
            .unwrap_or(SyncVerdict::Fail),
        acceptor_family: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn load_project_bundle_tx(
    conn: &Connection,
    project_id: &str,
) -> Result<TrioProjectBundle, StoreError> {
    let project = load_project_tx(conn, project_id)?;

    let mut own_decisions = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT project_id, own_id, statement, locked_at, approved_by
             FROM own_decisions WHERE project_id = ?1 ORDER BY own_id ASC",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            Ok(OwnDecision {
                project_id: row.get(0)?,
                own_id: row.get(1)?,
                statement: row.get(2)?,
                locked_at: row.get(3)?,
                approved_by: row.get(4)?,
            })
        })?;
        for item in rows {
            own_decisions.push(item?);
        }
    }

    let mut requirements = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT project_id, req_id, source_id, statement, stage, evidence_ref,
                    builder_principal, builder_family, acceptor_principal, acceptor_family, updated_at
             FROM requirement_rows WHERE project_id = ?1 ORDER BY req_id ASC",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            let stage_str: String = row.get(4)?;
            Ok(RequirementRow {
                project_id: row.get(0)?,
                req_id: row.get(1)?,
                source_id: row.get(2)?,
                statement: row.get(3)?,
                stage: stage_str.parse::<ReqStage>().unwrap_or(ReqStage::Unknown),
                evidence_ref: row.get(5)?,
                builder_principal: row.get(6)?,
                builder_family: row.get(7)?,
                acceptor_principal: row.get(8)?,
                acceptor_family: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?;
        for item in rows {
            requirements.push(item?);
        }
    }

    let mut divergences = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT project_id, div_id, designed, observed, resolution, status,
                    author_principal, created_at
             FROM divergences WHERE project_id = ?1 ORDER BY div_id ASC",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            let status_str: String = row.get(5)?;
            Ok(Divergence {
                project_id: row.get(0)?,
                div_id: row.get(1)?,
                designed: row.get(2)?,
                observed: row.get(3)?,
                resolution: row.get(4)?,
                status: status_str
                    .parse::<DivergenceStatus>()
                    .unwrap_or(DivergenceStatus::Open),
                author_principal: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        for item in rows {
            divergences.push(item?);
        }
    }

    let mut resources: Vec<ResourceRecord> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, shelf_slug, slug, project_id, trio_role, lifecycle_tier,
                    triage_state, title, current_rev, current_sha256, deleted_at, updated_at
             FROM resources
             WHERE project_id = ?1 AND deleted_at IS NULL
             ORDER BY slug ASC",
        )?;
        let rows = stmt.query_map(params![project_id], map_resource_row)?;
        for item in rows {
            resources.push(item?);
        }
    }

    let mut sync_reports = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT report_id, project_id, plan_rev, asbuilt_rev, paired_rows_json,
                    verdict, acceptor_family, created_at
             FROM sync_reports WHERE project_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![project_id], map_sync_report_row)?;
        for item in rows {
            sync_reports.push(item?);
        }
    }

    Ok(TrioProjectBundle {
        project,
        own_decisions,
        requirements,
        divergences,
        resources,
        sync_reports,
    })
}

fn recipe_digest(req: &CreateTrioProjectRequest) -> Result<String, StoreError> {
    let payload = serde_json::to_vec(req)?;
    Ok(sha256_hex(&payload))
}

fn map_conveyor_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConveyorRun> {
    let phase_str: String = row.get(2)?;
    let cleanup_int: i64 = row.get(7)?;
    let scan_int: i64 = row.get(8)?;
    let state_str: String = row.get(9)?;
    Ok(ConveyorRun {
        run_id: row.get(0)?,
        project_id: row.get(1)?,
        phase: phase_str.parse::<RunPhase>().unwrap_or(RunPhase::Build),
        harness_id: row.get(3)?,
        family: row.get(4)?,
        input_manifest_sha256: row.get(5)?,
        output_manifest_sha256: row.get(6)?,
        cleanup_clean: cleanup_int != 0,
        scan_clean: scan_int != 0,
        state: state_str.parse::<RunState>().unwrap_or(RunState::Staged),
        started_at: row.get(10)?,
        finished_at: row.get(11)?,
    })
}

impl StreamlineStore {
    /// Idempotently creates a Trio+ project bundle scaled to `P0..P4`.
    /// Returns the existing bundle when `project_id` / `create_id` and recipe match,
    /// or `StoreError::Conflict` when the identifier collides with a different recipe.
    pub fn create_trio_project(
        &self,
        grant_id: &str,
        req: CreateTrioProjectRequest,
    ) -> Result<TrioProjectBundle, StoreError> {
        let project_id = validate_slug(&req.project_id, "project_id")?;
        if req.title.trim().is_empty() {
            return Err(StoreError::InvalidInput(
                "project title cannot be empty".to_string(),
            ));
        }

        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        check_shelf_access_tx(&tx, &grant, "projects", Some(&project_id))?;

        let recipe_sha = recipe_digest(&req)?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT project_id, recipe_sha256
                 FROM trio_projects
                 WHERE project_id = ?1 OR (?2 IS NOT NULL AND create_id = ?2)",
                params![project_id, req.create_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        if let Some((existing_id, existing_recipe)) = existing {
            if existing_id == project_id && existing_recipe == recipe_sha {
                let bundle = load_project_bundle_tx(&tx, &existing_id)?;
                tx.commit()?;
                return Ok(bundle);
            }
            return Err(StoreError::Conflict(format!(
                "project '{existing_id}' already exists with a different recipe"
            )));
        }

        let outcome = assess_work_with_modules(&req.assessment, &req.opt_in_modules);
        let roles_json = serde_json::to_string(&outcome.expected_roles)?;
        let now = now_rfc3339();

        tx.execute(
            "INSERT INTO trio_projects (
                project_id, create_id, recipe_sha256, title, work_kind, difficulty,
                risk, profile, expected_roles_json, plan_locked_rev, state,
                how_it_works_resource_id, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, 'STAGED', NULL, ?10)",
            params![
                project_id,
                req.create_id,
                recipe_sha,
                req.title.trim(),
                req.assessment.work_kind.as_str(),
                outcome.difficulty as i64,
                outcome.risk as i64,
                outcome.profile.as_str(),
                roles_json,
                now,
            ],
        )?;

        for own in &req.initial_own_decisions {
            tx.execute(
                "INSERT INTO own_decisions (project_id, own_id, statement, locked_at, approved_by)
                 VALUES (?1, ?2, ?3, NULL, NULL)",
                params![project_id, own.own_id.trim(), own.statement.trim()],
            )?;
        }

        for row in &req.initial_requirements {
            tx.execute(
                "INSERT INTO requirement_rows (
                    project_id, req_id, source_id, statement, stage, evidence_ref,
                    builder_principal, builder_family, acceptor_principal, acceptor_family, updated_at
                ) VALUES (?1, ?2, ?3, ?4, 'D', '', NULL, NULL, NULL, NULL, ?5)",
                params![
                    project_id,
                    row.req_id.trim(),
                    row.source_id.trim(),
                    row.statement.trim(),
                    now,
                ],
            )?;
        }

        for role_name in &outcome.expected_roles {
            let slug = format!("{project_id}-{role_name}");
            let doc_title = format!("{}: {}", req.title.trim(), role_name);
            let starter_body = format!(
                "# {}\n\nProject: `{}`\nProfile: `{}` (`D{}` / `R{}`)\nRole: `{}`\n",
                doc_title, project_id, outcome.profile, outcome.difficulty, outcome.risk, role_name
            );
            save_resource_tx(
                &tx,
                &grant.principal_id,
                Some(&grant),
                SaveResourceRequest {
                    shelf_slug: "projects".to_string(),
                    slug,
                    title: doc_title,
                    body_md: starter_body,
                    project_id: Some(project_id.clone()),
                    trio_role: Some(role_name.clone()),
                    lifecycle_tier: Some(LifecycleTier::Active),
                    triage_state: Some(TriageState::Reviewed),
                    summary: format!("Initialize Trio+ role '{role_name}'"),
                    expected_rev: None,
                },
            )?;
        }

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "create_trio_project",
            &project_id,
            "ok",
            serde_json::json!({
                "profile": outcome.profile.as_str(),
                "difficulty": outcome.difficulty,
                "risk": outcome.risk,
            }),
        )?;

        let bundle = load_project_bundle_tx(&tx, &project_id)?;
        tx.commit()?;
        Ok(bundle)
    }

    /// Lists all Trio+ projects visible to the caller's granted tags.
    pub fn list_trio_projects(&self, grant_id: &str) -> Result<Vec<TrioProject>, StoreError> {
        let conn = self.lock_conn()?;
        let grant = load_active_grant_tx(&conn, grant_id)?;
        let mut stmt = conn.prepare(
            "SELECT project_id, create_id, title, work_kind, difficulty, risk, profile,
                    expected_roles_json, plan_locked_rev, state, how_it_works_resource_id,
                    cleanup_verified, lessons_satisfied, created_at
             FROM trio_projects ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], map_trio_project_row)?;
        let mut out = Vec::new();
        for item in rows {
            let proj = item?;
            if grant_has_tag_access(&grant, "projects", "projects", Some(&proj.project_id)) {
                out.push(proj);
            }
        }
        Ok(out)
    }

    /// Loads a full Trio+ project bundle (`project`, `OWN`, `REQ`, `DIV`, resources, sync reports).
    pub fn get_project_bundle(
        &self,
        grant_id: &str,
        project_id: &str,
    ) -> Result<TrioProjectBundle, StoreError> {
        let conn = self.lock_conn()?;
        let grant = load_active_grant_tx(&conn, grant_id)?;
        check_shelf_access_tx(&conn, &grant, "projects", Some(project_id))?;
        load_project_bundle_tx(&conn, project_id)
    }

    /// Inserts or updates an `OWN` decision before Plan Lock.
    /// Rejects mutations with `StoreError::OwnLocked` (`409` semantics) once locked.
    pub fn upsert_own_decision(
        &self,
        grant_id: &str,
        project_id: &str,
        own_id: &str,
        statement: &str,
        approved_by: Option<String>,
    ) -> Result<OwnDecision, StoreError> {
        if own_id.trim().is_empty() || statement.trim().is_empty() {
            return Err(StoreError::InvalidInput(
                "own_id and statement cannot be empty".to_string(),
            ));
        }

        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        check_shelf_access_tx(&tx, &grant, "projects", Some(project_id))?;

        let project = load_project_tx(&tx, project_id)?;
        if project.plan_locked_rev.is_some() {
            return Err(StoreError::OwnLocked);
        }

        let existing_locked: Option<Option<String>> = tx
            .query_row(
                "SELECT locked_at FROM own_decisions WHERE project_id = ?1 AND own_id = ?2",
                params![project_id, own_id.trim()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(Some(_)) = existing_locked {
            return Err(StoreError::OwnLocked);
        }

        tx.execute(
            "INSERT INTO own_decisions (project_id, own_id, statement, locked_at, approved_by)
             VALUES (?1, ?2, ?3, NULL, ?4)
             ON CONFLICT(project_id, own_id) DO UPDATE SET
                statement = excluded.statement,
                approved_by = COALESCE(excluded.approved_by, own_decisions.approved_by)",
            params![project_id, own_id.trim(), statement.trim(), approved_by],
        )?;

        let saved = tx.query_row(
            "SELECT project_id, own_id, statement, locked_at, approved_by
             FROM own_decisions WHERE project_id = ?1 AND own_id = ?2",
            params![project_id, own_id.trim()],
            |row| {
                Ok(OwnDecision {
                    project_id: row.get(0)?,
                    own_id: row.get(1)?,
                    statement: row.get(2)?,
                    locked_at: row.get(3)?,
                    approved_by: row.get(4)?,
                })
            },
        )?;

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "upsert_own_decision",
            &format!("{project_id}/{}", own_id.trim()),
            "ok",
            serde_json::json!({}),
        )?;

        tx.commit()?;
        Ok(saved)
    }

    /// Freezes `plan_locked_rev` on the project and locks all `OWN` decision rows.
    pub fn lock_project_plan(
        &self,
        grant_id: &str,
        project_id: &str,
        plan_rev: Option<i64>,
    ) -> Result<TrioProject, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        check_shelf_access_tx(&tx, &grant, "projects", Some(project_id))?;

        let project = load_project_tx(&tx, project_id)?;
        let plan_resource: Option<(String, i64)> = tx
            .query_row(
                "SELECT id, current_rev FROM resources
                 WHERE project_id = ?1 AND trio_role IN ('blueprint', 'fix_record')
                   AND deleted_at IS NULL
                 ORDER BY current_rev DESC LIMIT 1",
                params![project_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let resolved_rev = match plan_rev {
            Some(rev) => {
                if rev < 1 {
                    return Err(StoreError::InvalidInput(format!(
                        "plan revision {rev} does not exist for project '{project_id}'"
                    )));
                }
                let Some((plan_res_id, _)) = plan_resource else {
                    return Err(StoreError::InvalidInput(format!(
                        "plan revision {rev} does not exist for project '{project_id}'"
                    )));
                };
                let rev_exists: bool = tx.query_row(
                    "SELECT EXISTS(
                         SELECT 1 FROM resource_revisions
                         WHERE resource_id = ?1 AND rev = ?2
                     )",
                    params![plan_res_id, rev],
                    |row| row.get(0),
                )?;
                if !rev_exists {
                    return Err(StoreError::InvalidInput(format!(
                        "plan revision {rev} does not exist for project '{project_id}'"
                    )));
                }
                rev
            }
            None => {
                let Some((plan_res_id, current_rev)) = plan_resource else {
                    return Err(StoreError::InvalidInput(format!(
                        "plan revision does not exist for project '{project_id}'"
                    )));
                };
                if current_rev < 1 {
                    return Err(StoreError::InvalidInput(format!(
                        "plan revision {current_rev} does not exist for project '{project_id}'"
                    )));
                }
                let rev_exists: bool = tx.query_row(
                    "SELECT EXISTS(
                         SELECT 1 FROM resource_revisions
                         WHERE resource_id = ?1 AND rev = ?2
                     )",
                    params![plan_res_id, current_rev],
                    |row| row.get(0),
                )?;
                if !rev_exists {
                    return Err(StoreError::InvalidInput(format!(
                        "plan revision {current_rev} does not exist for project '{project_id}'"
                    )));
                }
                current_rev
            }
        };

        if let Some(existing_rev) = project.plan_locked_rev {
            if existing_rev != resolved_rev {
                return Err(StoreError::Conflict(format!(
                    "project '{project_id}' already locked at rev {existing_rev}"
                )));
            }
            tx.commit()?;
            return Ok(project);
        }

        let now = now_rfc3339();
        tx.execute(
            "UPDATE trio_projects SET plan_locked_rev = ?1 WHERE project_id = ?2",
            params![resolved_rev, project_id],
        )?;
        tx.execute(
            "UPDATE own_decisions
             SET locked_at = ?1, approved_by = COALESCE(approved_by, ?2)
             WHERE project_id = ?3 AND locked_at IS NULL",
            params![now, grant.principal_id, project_id],
        )?;

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "lock_project_plan",
            project_id,
            "ok",
            serde_json::json!({ "plan_locked_rev": resolved_rev }),
        )?;

        let updated = load_project_tx(&tx, project_id)?;
        tx.commit()?;
        Ok(updated)
    }

    /// Appends a `DIV-nnn` divergence record without mutating any locked `OWN` rows.
    pub fn record_divergence(
        &self,
        grant_id: &str,
        req: RecordDivergenceRequest,
    ) -> Result<Divergence, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        check_shelf_access_tx(&tx, &grant, "projects", Some(&req.project_id))?;
        let _ = load_project_tx(&tx, &req.project_id)?;

        let div_id = match req.div_id {
            Some(explicit) if !explicit.trim().is_empty() => explicit.trim().to_string(),
            _ => {
                let count: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM divergences WHERE project_id = ?1",
                    params![req.project_id],
                    |row| row.get(0),
                )?;
                format!("DIV-{:03}", count + 1)
            }
        };
        let now = now_rfc3339();

        tx.execute(
            "INSERT INTO divergences (
                project_id, div_id, designed, observed, resolution, status, author_principal, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                req.project_id,
                div_id,
                req.designed,
                req.observed,
                req.resolution,
                req.status.as_str(),
                grant.principal_id,
                now,
            ],
        )
        .map_err(|err| match err {
            rusqlite::Error::SqliteFailure(sqlite_err, _)
                if sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                StoreError::Conflict(format!("divergence '{div_id}' already exists"))
            }
            other => StoreError::Sqlite(other),
        })?;

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "record_divergence",
            &format!("{}/{}", req.project_id, div_id),
            "ok",
            serde_json::json!({ "status": req.status.as_str() }),
        )?;

        tx.commit()?;
        Ok(Divergence {
            project_id: req.project_id,
            div_id,
            designed: req.designed,
            observed: req.observed,
            resolution: req.resolution,
            status: req.status,
            author_principal: grant.principal_id,
            created_at: now,
        })
    }

    /// Inserts or updates a requirement definition row (`stage` starts at `D` for new rows).
    pub fn upsert_requirement_row(
        &self,
        grant_id: &str,
        project_id: &str,
        req_id: &str,
        source_id: &str,
        statement: &str,
    ) -> Result<RequirementRow, StoreError> {
        if req_id.trim().is_empty() || statement.trim().is_empty() {
            return Err(StoreError::InvalidInput(
                "req_id and statement cannot be empty".to_string(),
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

        let trimmed_req_id = req_id.trim();
        let trimmed_source_id = source_id.trim();
        let trimmed_statement = statement.trim();
        let now = now_rfc3339();

        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT source_id, statement FROM requirement_rows
                 WHERE project_id = ?1 AND req_id = ?2",
                params![project_id, trimmed_req_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        match existing {
            Some((existing_source_id, existing_statement)) => {
                if existing_source_id != trimmed_source_id
                    || existing_statement != trimmed_statement
                {
                    tx.execute(
                        "UPDATE requirement_rows
                         SET source_id = ?1,
                             statement = ?2,
                             stage = 'D',
                             evidence_ref = '',
                             builder_principal = NULL,
                             builder_family = NULL,
                             acceptor_principal = NULL,
                             acceptor_family = NULL,
                             updated_at = ?3
                         WHERE project_id = ?4 AND req_id = ?5",
                        params![
                            trimmed_source_id,
                            trimmed_statement,
                            now,
                            project_id,
                            trimmed_req_id,
                        ],
                    )?;
                } else {
                    tx.execute(
                        "UPDATE requirement_rows
                         SET updated_at = ?1
                         WHERE project_id = ?2 AND req_id = ?3",
                        params![now, project_id, trimmed_req_id],
                    )?;
                }
            }
            None => {
                tx.execute(
                    "INSERT INTO requirement_rows (
                        project_id, req_id, source_id, statement, stage, evidence_ref,
                        builder_principal, builder_family, acceptor_principal, acceptor_family, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, 'D', '', NULL, NULL, NULL, NULL, ?5)",
                    params![
                        project_id,
                        trimmed_req_id,
                        trimmed_source_id,
                        trimmed_statement,
                        now,
                    ],
                )?;
            }
        }

        let saved = load_requirement_row_tx(&tx, project_id, trimmed_req_id)?;
        tx.commit()?;
        Ok(saved)
    }

    /// Advances a requirement row through `D -> I -> T -> A -> L`, enforcing role, evidence,
    /// non-self acceptance, server-derived grant model family, cross-family acceptance for `R2+`,
    /// and approved `HOW-IT-WORKS` for `L`.
    pub fn update_requirement_stage(
        &self,
        grant_id: &str,
        req: UpdateRequirementStageRequest,
    ) -> Result<RequirementRow, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        check_shelf_access_tx(&tx, &grant, "projects", Some(&req.project_id))?;

        let project = load_project_tx(&tx, &req.project_id)?;
        let existing = load_requirement_row_tx(&tx, &req.project_id, &req.req_id)?;
        let how_it_works_approved =
            project.state == RunState::Done && project.how_it_works_resource_id.is_some();

        let bound_family = grant
            .model_family
            .as_deref()
            .map(str::trim)
            .filter(|val| !val.is_empty())
            .map(str::to_string);
        let claimed_family = req
            .actor_family
            .as_deref()
            .map(str::trim)
            .filter(|val| !val.is_empty())
            .map(str::to_string);

        let effective_family: Option<String> = if grant.principal_role == PrincipalRole::Owner {
            match (&bound_family, &claimed_family) {
                (Some(bound), Some(claimed)) if !bound.eq_ignore_ascii_case(claimed) => {
                    return Err(StoreError::ForbiddenRole(
                        "caller cannot spoof model_family; grant is bound to a different family"
                            .to_string(),
                    ));
                }
                (Some(bound), _) => Some(bound.clone()),
                (None, claimed) => claimed.clone(),
            }
        } else {
            if let Some(ref claimed) = claimed_family {
                let matches_bound = bound_family
                    .as_deref()
                    .map(|bound| bound.eq_ignore_ascii_case(claimed))
                    .unwrap_or(false);
                if !matches_bound {
                    return Err(StoreError::ForbiddenRole(
                        "caller cannot spoof model_family; grant is bound to a different family"
                            .to_string(),
                    ));
                }
            }
            bound_family.clone()
        };

        if project.risk >= 2
            && matches!(req.target_stage, ReqStage::I | ReqStage::T)
            && effective_family.is_none()
        {
            return Err(StoreError::InvalidState(
                "R2+ work requires grant bound to a trusted model_family".to_string(),
            ));
        }
        if project.risk >= 2
            && req.target_stage == ReqStage::A
            && grant.principal_role != PrincipalRole::Owner
            && bound_family.is_none()
        {
            return Err(StoreError::InvalidState(
                "R2+ work requires grant bound to a trusted model_family".to_string(),
            ));
        }

        let (builder_family, acceptor_family) = match req.target_stage {
            ReqStage::A => (existing.builder_family.clone(), effective_family.clone()),
            _ => (
                existing.builder_family.clone(),
                existing.acceptor_family.clone(),
            ),
        };

        validate_stage_transition(&StageTransitionRequest {
            current_stage: existing.stage,
            target_stage: req.target_stage,
            risk: project.risk,
            actor_principal: grant.principal_id.clone(),
            actor_role: grant.principal_role,
            actor_is_custodian: req.actor_is_custodian,
            builder_principal: existing.builder_principal.clone(),
            builder_family,
            acceptor_family,
            evidence_ref: req.evidence_ref.clone(),
            how_it_works_approved,
        })?;

        let now = now_rfc3339();
        match req.target_stage {
            ReqStage::I | ReqStage::T => {
                tx.execute(
                    "UPDATE requirement_rows
                     SET stage = ?1, evidence_ref = ?2,
                         builder_principal = ?3,
                         builder_family = COALESCE(?4, builder_family),
                         updated_at = ?5
                     WHERE project_id = ?6 AND req_id = ?7",
                    params![
                        req.target_stage.as_str(),
                        req.evidence_ref.trim(),
                        grant.principal_id,
                        effective_family,
                        now,
                        req.project_id,
                        req.req_id,
                    ],
                )?;
            }
            ReqStage::A => {
                tx.execute(
                    "UPDATE requirement_rows
                     SET stage = 'A', evidence_ref = ?1,
                         acceptor_principal = ?2,
                         acceptor_family = ?3,
                         updated_at = ?4
                     WHERE project_id = ?5 AND req_id = ?6",
                    params![
                        req.evidence_ref.trim(),
                        grant.principal_id,
                        effective_family,
                        now,
                        req.project_id,
                        req.req_id,
                    ],
                )?;
            }
            ReqStage::D | ReqStage::Unknown => {
                tx.execute(
                    "UPDATE requirement_rows
                     SET stage = ?1,
                         evidence_ref = '',
                         builder_principal = NULL,
                         builder_family = NULL,
                         acceptor_principal = NULL,
                         acceptor_family = NULL,
                         updated_at = ?2
                     WHERE project_id = ?3 AND req_id = ?4",
                    params![req.target_stage.as_str(), now, req.project_id, req.req_id,],
                )?;
            }
            ReqStage::L => {
                tx.execute(
                    "UPDATE requirement_rows
                     SET stage = 'L', evidence_ref = ?1, updated_at = ?2
                     WHERE project_id = ?3 AND req_id = ?4",
                    params![req.evidence_ref.trim(), now, req.project_id, req.req_id,],
                )?;
            }
        }

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "update_requirement_stage",
            &format!("{}/{}", req.project_id, req.req_id),
            "ok",
            serde_json::json!({
                "from": existing.stage.as_str(),
                "to": req.target_stage.as_str(),
            }),
        )?;

        let updated = load_requirement_row_tx(&tx, &req.project_id, &req.req_id)?;
        tx.commit()?;
        Ok(updated)
    }

    /// Computes paired rows and verdict server-side against the project's locked plan and
    /// `as_built` resource, derives `acceptor_family` from the authenticated grant, and persists
    /// the resulting `SyncReport`.
    pub fn compute_and_save_sync_report(
        &self,
        grant_id: &str,
        project_id: &str,
        asbuilt_slug_override: Option<&str>,
    ) -> Result<SyncReport, StoreError> {
        let project_id = validate_slug(project_id, "project_id")?;
        let slug_override = match asbuilt_slug_override
            .map(str::trim)
            .filter(|val| !val.is_empty())
        {
            Some(raw_slug) => Some(validate_slug(raw_slug, "asbuilt_slug")?),
            None => None,
        };

        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        check_shelf_access_tx(&tx, &grant, "projects", Some(&project_id))?;

        let bundle = load_project_bundle_tx(&tx, &project_id)?;
        let plan_rev = bundle.project.plan_locked_rev.ok_or_else(|| {
            StoreError::InvalidState(
                "project plan must be locked before saving a sync report".to_string(),
            )
        })?;

        let target_res = if let Some(ref slug_name) = slug_override {
            bundle
                .resources
                .iter()
                .find(|res| &res.slug == slug_name)
                .ok_or_else(|| {
                    StoreError::NotFound(format!(
                        "as_built resource '{slug_name}' not found in project '{project_id}'"
                    ))
                })?
        } else {
            bundle
                .resources
                .iter()
                .find(|res| res.trio_role.as_deref() == Some("as_built"))
                .or_else(|| {
                    bundle
                        .resources
                        .iter()
                        .find(|res| res.trio_role.as_deref() == Some("fix_record"))
                })
                .ok_or_else(|| {
                    StoreError::NotFound(format!(
                        "no as_built or fix_record resource found in project '{project_id}'"
                    ))
                })?
        };

        let asbuilt_doc = read_resource_by_id_tx(&tx, &target_res.id)?;
        let asbuilt_rev = asbuilt_doc.resource.current_rev;
        let summary = check_sync(
            &bundle.requirements,
            &bundle.own_decisions,
            &asbuilt_doc.body_md,
        );
        let paired_rows = summary.rows;
        let verdict = if paired_rows.is_empty() {
            SyncVerdict::Fail
        } else if paired_rows.iter().all(|row| row.matched) {
            SyncVerdict::Pass
        } else if paired_rows.iter().any(|row| row.matched) {
            SyncVerdict::Partial
        } else {
            SyncVerdict::Fail
        };

        let bound_family = grant
            .model_family
            .as_deref()
            .map(str::trim)
            .filter(|val| !val.is_empty())
            .map(str::to_string);

        let acceptor_family = if bundle.project.risk >= 2 {
            let fam = match bound_family {
                Some(family_name) => family_name,
                None if grant.principal_role == PrincipalRole::Owner => {
                    grant.principal_role.as_str().to_string()
                }
                None => {
                    return Err(StoreError::InvalidState(
                        "R2+ sync report requires grant bound to a trusted model_family"
                            .to_string(),
                    ));
                }
            };
            for req_row in &bundle.requirements {
                if let Some(ref bf) = req_row.builder_family {
                    let trimmed_bf = bf.trim();
                    if !trimmed_bf.is_empty() && trimmed_bf.eq_ignore_ascii_case(&fam) {
                        return Err(StoreError::Core(CoreError::SameFamilyForbidden {
                            builder_family: trimmed_bf.to_string(),
                            acceptor_family: fam,
                        }));
                    }
                }
            }
            fam
        } else {
            bound_family.unwrap_or_else(|| grant.principal_role.as_str().to_string())
        };

        let report_id = format!("sync-{}", uuid::Uuid::new_v4());
        let paired_json = serde_json::to_string(&paired_rows)?;
        let now = now_rfc3339();

        tx.execute(
            "INSERT INTO sync_reports (
                report_id, project_id, plan_rev, asbuilt_rev, paired_rows_json,
                verdict, acceptor_family, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                report_id,
                project_id,
                plan_rev,
                asbuilt_rev,
                paired_json,
                verdict.as_str(),
                acceptor_family,
                now,
            ],
        )?;

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "save_sync_report",
            &report_id,
            "ok",
            serde_json::json!({
                "project_id": project_id,
                "plan_rev": plan_rev,
                "asbuilt_rev": asbuilt_rev,
                "verdict": verdict.as_str(),
                "acceptor_family": acceptor_family,
            }),
        )?;

        tx.commit()?;
        Ok(SyncReport {
            report_id,
            project_id,
            plan_rev,
            asbuilt_rev,
            paired_rows,
            verdict,
            acceptor_family,
            created_at: now,
        })
    }

    /// Computes and saves a server-verified cross-model sync report for a Trio+ project,
    /// ignoring any caller-supplied verdict, paired rows, revision numbers, or claimed family.
    pub fn save_sync_report(
        &self,
        grant_id: &str,
        req: SaveSyncReportRequest,
    ) -> Result<SyncReport, StoreError> {
        self.compute_and_save_sync_report(grant_id, &req.project_id, None)
    }

    /// Reads a sync report by `report_id`, or the latest sync report for `project_id`.
    pub fn get_sync_report(
        &self,
        grant_id: &str,
        project_id: &str,
        report_id: Option<&str>,
    ) -> Result<Option<SyncReport>, StoreError> {
        let conn = self.lock_conn()?;
        let grant = load_active_grant_tx(&conn, grant_id)?;
        check_shelf_access_tx(&conn, &grant, "projects", Some(project_id))?;

        match report_id {
            Some(id) => conn
                .query_row(
                    "SELECT report_id, project_id, plan_rev, asbuilt_rev, paired_rows_json,
                            verdict, acceptor_family, created_at
                     FROM sync_reports WHERE project_id = ?1 AND report_id = ?2",
                    params![project_id, id],
                    map_sync_report_row,
                )
                .optional()
                .map_err(StoreError::from),
            None => conn
                .query_row(
                    "SELECT report_id, project_id, plan_rev, asbuilt_rev, paired_rows_json,
                            verdict, acceptor_family, created_at
                     FROM sync_reports WHERE project_id = ?1
                     ORDER BY created_at DESC LIMIT 1",
                    params![project_id],
                    map_sync_report_row,
                )
                .optional()
                .map_err(StoreError::from),
        }
    }

    /// Enforces the 3-part closeout gate (`cleanup_verified`, central `lessons_learned` or
    /// explicit `record_no_new_lessons`, and plain-English `HOW-IT-WORKS` validation), saves the
    /// guide, and transitions the project state to `DOC_PENDING_OWNER`.
    pub fn submit_how_it_works(
        &self,
        grant_id: &str,
        project_id: &str,
        markdown: &str,
        title: Option<String>,
    ) -> Result<TrioProject, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        check_shelf_access_tx(&tx, &grant, "projects", Some(project_id))?;

        let project = load_project_tx(&tx, project_id)?;
        if !project.cleanup_verified {
            return Err(StoreError::InvalidState(
                "temporary files cleanup must be verified before closeout".to_string(),
            ));
        }

        let has_project_lesson: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM lessons_learned WHERE project_id = ?1)",
            params![project_id],
            |row| row.get(0),
        )?;
        if !project.lessons_satisfied && !has_project_lesson {
            return Err(StoreError::InvalidState(
                "lessons learned must be recorded to the central learning database before closeout"
                    .to_string(),
            ));
        }

        if let Err(errs) = validate_how_it_works(markdown) {
            return Err(StoreError::InvalidHowItWorks(errs));
        }

        let slug = format!("{project_id}-how-it-works");
        let doc_title = title
            .filter(|val| !val.trim().is_empty())
            .unwrap_or_else(|| format!("{}: HOW-IT-WORKS", project.title));

        let saved = save_resource_tx(
            &tx,
            &grant.principal_id,
            Some(&grant),
            SaveResourceRequest {
                shelf_slug: "projects".to_string(),
                slug,
                title: doc_title,
                body_md: markdown.to_string(),
                project_id: Some(project_id.to_string()),
                trio_role: Some("how_it_works".to_string()),
                lifecycle_tier: Some(LifecycleTier::Active),
                triage_state: Some(TriageState::Reviewed),
                summary: "Submit HOW-IT-WORKS for owner approval".to_string(),
                expected_rev: None,
            },
        )?;

        tx.execute(
            "UPDATE trio_projects
             SET how_it_works_resource_id = ?1, state = 'DOC_PENDING_OWNER'
             WHERE project_id = ?2",
            params![saved.resource.id, project_id],
        )?;

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "submit_how_it_works",
            project_id,
            "ok",
            serde_json::json!({ "resource_id": saved.resource.id }),
        )?;

        let updated = load_project_tx(&tx, project_id)?;
        tx.commit()?;
        Ok(updated)
    }

    /// Owner-only release gate: verifies `DOC_PENDING_OWNER` state and valid `HOW-IT-WORKS`,
    /// transitions the project to `DONE`, and promotes accepted (`A`) requirement rows to `L`.
    pub fn approve_release(
        &self,
        grant_id: &str,
        project_id: &str,
    ) -> Result<TrioProjectBundle, StoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let grant = require_rules_acked_tx(&tx, grant_id)?;
        if !grant.can_write {
            return Err(StoreError::ForbiddenWrite);
        }
        if grant.principal_role != PrincipalRole::Owner {
            return Err(StoreError::ForbiddenRole(grant.principal_role.to_string()));
        }
        check_shelf_access_tx(&tx, &grant, "projects", Some(project_id))?;

        let project = load_project_tx(&tx, project_id)?;
        if project.state != RunState::DocPendingOwner {
            return Err(StoreError::InvalidState(format!(
                "project '{}' is in state {}, expected DOC_PENDING_OWNER",
                project_id, project.state
            )));
        }

        let how_res_id = project.how_it_works_resource_id.as_deref().ok_or_else(|| {
            StoreError::InvalidState(format!(
                "project '{project_id}' has no HOW-IT-WORKS resource attached"
            ))
        })?;
        let how_doc = read_resource_by_id_tx(&tx, how_res_id)?;
        if let Err(errs) = validate_how_it_works(&how_doc.body_md) {
            return Err(StoreError::InvalidHowItWorks(errs));
        }

        let now = now_rfc3339();
        tx.execute(
            "UPDATE trio_projects SET state = 'DONE' WHERE project_id = ?1",
            params![project_id],
        )?;
        tx.execute(
            "UPDATE requirement_rows
             SET stage = 'L',
                 evidence_ref = CASE
                    WHEN TRIM(evidence_ref) = '' THEN 'owner:release-approved'
                    ELSE evidence_ref
                 END,
                 updated_at = ?1
             WHERE project_id = ?2 AND stage = 'A'",
            params![now, project_id],
        )?;

        record_audit_tx(
            &tx,
            &grant.principal_id,
            "approve_release",
            project_id,
            "ok",
            serde_json::json!({ "how_it_works_resource_id": how_res_id }),
        )?;

        let bundle = load_project_bundle_tx(&tx, project_id)?;
        tx.commit()?;
        Ok(bundle)
    }

    /// Inserts or updates a conveyor run record.
    pub fn upsert_conveyor_run(&self, run: ConveyorRun) -> Result<ConveyorRun, StoreError> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO conveyor_runs (
                run_id, project_id, phase, harness_id, family, input_manifest_sha256,
                output_manifest_sha256, cleanup_clean, scan_clean, state, started_at, finished_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(run_id) DO UPDATE SET
                phase = excluded.phase,
                harness_id = excluded.harness_id,
                family = excluded.family,
                input_manifest_sha256 = excluded.input_manifest_sha256,
                output_manifest_sha256 = excluded.output_manifest_sha256,
                cleanup_clean = excluded.cleanup_clean,
                scan_clean = excluded.scan_clean,
                state = excluded.state,
                finished_at = excluded.finished_at",
            params![
                run.run_id,
                run.project_id,
                run.phase.as_str(),
                run.harness_id,
                run.family,
                run.input_manifest_sha256,
                run.output_manifest_sha256,
                if run.cleanup_clean { 1 } else { 0 },
                if run.scan_clean { 1 } else { 0 },
                run.state.as_str(),
                run.started_at,
                run.finished_at,
            ],
        )?;
        Ok(run)
    }

    /// Fetches one conveyor run by `run_id`.
    pub fn get_conveyor_run(&self, run_id: &str) -> Result<ConveyorRun, StoreError> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT run_id, project_id, phase, harness_id, family, input_manifest_sha256,
                    output_manifest_sha256, cleanup_clean, scan_clean, state, started_at, finished_at
             FROM conveyor_runs WHERE run_id = ?1",
            params![run_id],
            map_conveyor_run_row,
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("conveyor run '{run_id}' not found")))
    }

    /// Lists conveyor runs for a Trio+ project.
    pub fn list_conveyor_runs(&self, project_id: &str) -> Result<Vec<ConveyorRun>, StoreError> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT run_id, project_id, phase, harness_id, family, input_manifest_sha256,
                    output_manifest_sha256, cleanup_clean, scan_clean, state, started_at, finished_at
             FROM conveyor_runs WHERE project_id = ?1 ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map(params![project_id], map_conveyor_run_row)?;
        let mut runs = Vec::new();
        for item in rows {
            runs.push(item?);
        }
        Ok(runs)
    }
}

fn load_requirement_row_tx(
    conn: &Connection,
    project_id: &str,
    req_id: &str,
) -> Result<RequirementRow, StoreError> {
    conn.query_row(
        "SELECT project_id, req_id, source_id, statement, stage, evidence_ref,
                builder_principal, builder_family, acceptor_principal, acceptor_family, updated_at
         FROM requirement_rows WHERE project_id = ?1 AND req_id = ?2",
        params![project_id, req_id],
        |row| {
            let stage_str: String = row.get(4)?;
            Ok(RequirementRow {
                project_id: row.get(0)?,
                req_id: row.get(1)?,
                source_id: row.get(2)?,
                statement: row.get(3)?,
                stage: stage_str.parse::<ReqStage>().unwrap_or(ReqStage::Unknown),
                evidence_ref: row.get(5)?,
                builder_principal: row.get(6)?,
                builder_family: row.get(7)?,
                acceptor_principal: row.get(8)?,
                acceptor_family: row.get(9)?,
                updated_at: row.get(10)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| {
        StoreError::NotFound(format!(
            "requirement '{req_id}' not found in project '{project_id}'"
        ))
    })
}
