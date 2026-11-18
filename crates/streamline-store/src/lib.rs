pub mod error;
pub mod grants;
pub mod legacy;
pub mod lessons;
pub mod migrations;
pub mod models;
pub mod resources;
pub mod trio;
pub mod vault;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

pub use error::StoreError;
pub use models::{
    AccessGrant, BootstrapView, CreateGrantRequest, CreateTrioProjectRequest, DiffLine,
    LegacyImportSummary, LessonRecord, OwnDecisionInput, Principal, RecordDivergenceRequest,
    RecordLessonRequest, RequirementRowInput, ResourceFilter, ResourceRecord, ResourceRevision,
    ResourceSearchHit, ResourceWithPassport, RevisionDiffSummary, SaveResourceRequest,
    SaveSyncReportRequest, ShelfSummary, SyncReport, TrioProject, TrioProjectBundle,
    UpdateRequirementStageRequest, VaultConflictRecord, VaultSyncSummary, WordChange,
};
pub use resources::compare_texts;

/// Single-writer SQLite and Obsidian markdown vault store for Streamline.
pub struct StreamlineStore {
    conn: Mutex<Connection>,
}

impl StreamlineStore {
    /// Opens (or creates) a file-backed SQLite database with WAL mode, foreign keys,
    /// and idempotent schema migrations + default shelf/rules seeds.
    pub fn open(db_path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut conn = Connection::open(db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        migrations::run_migrations(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Opens an isolated in-memory SQLite database for tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let mut conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        migrations::run_migrations(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub(crate) fn lock_conn(&self) -> Result<MutexGuard<'_, Connection>, StoreError> {
        self.conn.lock().map_err(|_| StoreError::LockPoisoned)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use rusqlite::{params, Connection};
    use streamline_core::{
        AssessmentInput, CoreError, DivergenceStatus, FreshnessState, LifecycleTier, PairedSyncRow,
        PrincipalRole, ReqStage, RunState, SyncVerdict, TriageState, TrioProfile, WorkKind,
    };

    use super::*;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn seed_owner_grant(store: &StreamlineStore) -> AccessGrant {
        store
            .upsert_principal("owner-1", "owner", PrincipalRole::Owner)
            .expect("create owner");
        store
            .create_grant(CreateGrantRequest {
                grant_id: Some("grant-owner".to_string()),
                principal_id: "owner-1".to_string(),
                raw_credential: "cred-owner-12345".to_string(),
                granted_tags: vec!["*".to_string()],
                can_write: true,
                can_dispatch: true,
                expires_at: None,
                model_family: None,
            })
            .expect("create owner grant")
    }

    fn seed_agent_grant_with_family(
        store: &StreamlineStore,
        id: &str,
        cred: &str,
        tags: Vec<String>,
        model_family: Option<&str>,
    ) -> AccessGrant {
        store
            .upsert_principal(id, id, PrincipalRole::Agent)
            .expect("create agent");
        store
            .create_grant(CreateGrantRequest {
                grant_id: Some(format!("grant-{id}")),
                principal_id: id.to_string(),
                raw_credential: cred.to_string(),
                granted_tags: tags,
                can_write: true,
                can_dispatch: false,
                expires_at: None,
                model_family: model_family.map(str::to_string),
            })
            .expect("create agent grant")
    }

    fn seed_agent_grant(
        store: &StreamlineStore,
        id: &str,
        cred: &str,
        tags: Vec<String>,
    ) -> AccessGrant {
        seed_agent_grant_with_family(store, id, cred, tags, None)
    }

    fn sample_assessment() -> AssessmentInput {
        AssessmentInput {
            d1_scope: 2,
            d2_interfaces: 1,
            d3_uncertainty: 1,
            d4_ui: 0,
            d5_novelty: 1,
            r1_exposure: 2,
            r2_irreversibility: 1,
            r3_blast_radius: 1,
            r4_evidence: 1,
            r5_custody: 1,
            work_kind: WorkKind::Software,
            has_ui: false,
            is_game: false,
            under_accepted_design: false,
        }
    }

    fn valid_how_it_works() -> &'static str {
        "\
## Overview and Purpose
This service stores project governance records and tracks Trio+ verification stages.

## Runtime Location
The binary binds loopback 127.0.0.1:5880 and stores SQLite state under /var/lib/streamline.

## Step-by-Step Flow
Callers authenticate via grant hash, acknowledge active rules, lock OWN decisions, and verify sync.

## Health and Logs
Inspect GET /ready for health status and check structured server logs for audit history.

## Failure Modes and Recovery
Unacknowledged writes return 412, locked OWN edits return 409, and SQLite backups recover state.
"
    }

    #[test]
    fn rules_ack_gate_and_invalidation_on_rules_update() {
        let store = StreamlineStore::open_in_memory().expect("open store");
        let owner = seed_owner_grant(&store);

        let authed = store
            .authenticate_grant("cred-owner-12345")
            .expect("auth grant");
        assert_eq!(authed.grant_id, owner.grant_id);

        let rules_sha_v1 = store.current_rules_sha256().expect("rules sha");

        let blocked = store.save_resource(
            &owner.grant_id,
            SaveResourceRequest {
                shelf_slug: "standards".to_string(),
                slug: "style-guide".to_string(),
                title: "Style Guide".to_string(),
                body_md: "Plain English prose.".to_string(),
                project_id: None,
                trio_role: None,
                lifecycle_tier: Some(LifecycleTier::Active),
                triage_state: Some(TriageState::Reviewed),
                summary: "Add style guide".to_string(),
                expected_rev: None,
            },
        );
        assert!(matches!(
            blocked,
            Err(StoreError::RulesAckRequired { ref current_rules_sha256 })
                if current_rules_sha256 == &rules_sha_v1
        ));

        let acked = store
            .ack_rules(&owner.grant_id, &rules_sha_v1)
            .expect("ack rules");
        assert_eq!(
            acked.acked_rules_sha256.as_deref(),
            Some(rules_sha_v1.as_str())
        );

        store
            .save_resource(
                &owner.grant_id,
                SaveResourceRequest {
                    shelf_slug: "standards".to_string(),
                    slug: "rules".to_string(),
                    title: "Streamline Governance Rules".to_string(),
                    body_md: "# Updated Rules v2\n\nAll agents must re-acknowledge.\n".to_string(),
                    project_id: None,
                    trio_role: None,
                    lifecycle_tier: Some(LifecycleTier::Active),
                    triage_state: Some(TriageState::Reviewed),
                    summary: "Update rules to v2".to_string(),
                    expected_rev: Some(1),
                },
            )
            .expect("update rules");

        let rules_sha_v2 = store.current_rules_sha256().expect("rules sha v2");
        assert_ne!(rules_sha_v1, rules_sha_v2);

        let gate_after_update = store.require_rules_acked(&owner.grant_id);
        assert!(matches!(
            gate_after_update,
            Err(StoreError::RulesAckRequired { ref current_rules_sha256 })
                if current_rules_sha256 == &rules_sha_v2
        ));
    }

    #[test]
    fn tag_isolation_and_active_bootstrap_fencing() {
        let store = StreamlineStore::open_in_memory().expect("open store");
        let owner = seed_owner_grant(&store);
        let rules_sha = store.current_rules_sha256().expect("sha");
        store
            .ack_rules(&owner.grant_id, &rules_sha)
            .expect("ack owner");

        store
            .save_resource(
                &owner.grant_id,
                SaveResourceRequest {
                    shelf_slug: "runbooks".to_string(),
                    slug: "deploy".to_string(),
                    title: "Deploy Runbook".to_string(),
                    body_md: "Restart service cleanly.".to_string(),
                    project_id: None,
                    trio_role: None,
                    lifecycle_tier: Some(LifecycleTier::Active),
                    triage_state: Some(TriageState::Reviewed),
                    summary: "Seed deploy runbook".to_string(),
                    expected_rev: None,
                },
            )
            .expect("save runbook");

        store
            .save_resource(
                &owner.grant_id,
                SaveResourceRequest {
                    shelf_slug: "standards".to_string(),
                    slug: "dormant-note".to_string(),
                    title: "Dormant Note".to_string(),
                    body_md: "Historical idea kept fenced.".to_string(),
                    project_id: None,
                    trio_role: None,
                    lifecycle_tier: Some(LifecycleTier::Dormant),
                    triage_state: Some(TriageState::NeedsTriage),
                    summary: "Add dormant note".to_string(),
                    expected_rev: None,
                },
            )
            .expect("save dormant");

        let scoped = seed_agent_grant(
            &store,
            "scoped-agent",
            "cred-scoped-999",
            vec!["project:demo".to_string(), "standards".to_string()],
        );
        store
            .ack_rules(&scoped.grant_id, &rules_sha)
            .expect("ack scoped");

        let boot = store.bootstrap_view(&scoped.grant_id).expect("bootstrap");
        assert!(boot.active_resources.iter().any(|r| r.slug == "rules"));
        assert!(!boot
            .active_resources
            .iter()
            .any(|r| r.slug == "dormant-note"));
        assert!(!boot.active_resources.iter().any(|r| r.slug == "deploy"));

        let ungranted = store.read_resource(&scoped.grant_id, "runbooks", "deploy");
        assert!(matches!(
            ungranted,
            Err(StoreError::ForbiddenTag { ref required_tag }) if required_tag == "runbooks"
        ));

        let other_project_write = store.save_resource(
            &scoped.grant_id,
            SaveResourceRequest {
                shelf_slug: "projects".to_string(),
                slug: "bounded-other".to_string(),
                title: "Other".to_string(),
                body_md: "Should fail tag check.".to_string(),
                project_id: Some("other".to_string()),
                trio_role: None,
                lifecycle_tier: Some(LifecycleTier::Active),
                triage_state: Some(TriageState::Reviewed),
                summary: "Unauthorized project".to_string(),
                expected_rev: None,
            },
        );
        assert!(matches!(
            other_project_write,
            Err(StoreError::ForbiddenTag { ref required_tag }) if required_tag == "project:other"
        ));

        let demo_write = store.save_resource(
            &scoped.grant_id,
            SaveResourceRequest {
                shelf_slug: "projects".to_string(),
                slug: "demo-notes".to_string(),
                title: "Demo Notes".to_string(),
                body_md: "Bounded demo notes.".to_string(),
                project_id: Some("demo".to_string()),
                trio_role: None,
                lifecycle_tier: Some(LifecycleTier::Active),
                triage_state: Some(TriageState::Reviewed),
                summary: "Demo project notes".to_string(),
                expected_rev: None,
            },
        );
        assert!(demo_write.is_ok());

        store
            .set_lifecycle_tier(
                &owner.grant_id,
                "standards",
                "dormant-note",
                LifecycleTier::Active,
                Some(TriageState::Reviewed),
                None,
            )
            .expect("promote dormant to active");
        let boot_after = store.bootstrap_view(&scoped.grant_id).expect("bootstrap 2");
        assert!(boot_after
            .active_resources
            .iter()
            .any(|r| r.slug == "dormant-note"));
    }

    #[test]
    fn trio_project_idempotency_own_lock_div_and_release_gate() {
        let store = StreamlineStore::open_in_memory().expect("open store");
        let owner = seed_owner_grant(&store);
        let worker = seed_agent_grant_with_family(
            &store,
            "worker-a",
            "cred-worker-a",
            vec!["*".to_string()],
            Some("openai-codex"),
        );
        let acceptor = seed_agent_grant_with_family(
            &store,
            "acceptor-b",
            "cred-acceptor-b",
            vec!["*".to_string()],
            Some("google-gemini"),
        );

        let rules_sha = store.current_rules_sha256().expect("sha");
        store
            .ack_rules(&owner.grant_id, &rules_sha)
            .expect("ack owner");
        store
            .ack_rules(&worker.grant_id, &rules_sha)
            .expect("ack worker");
        store
            .ack_rules(&acceptor.grant_id, &rules_sha)
            .expect("ack acceptor");

        let req = CreateTrioProjectRequest {
            project_id: "demo-p1".to_string(),
            create_id: Some("req-create-001".to_string()),
            title: "Demo Service".to_string(),
            assessment: sample_assessment(),
            opt_in_modules: Vec::new(),
            initial_own_decisions: vec![OwnDecisionInput {
                own_id: "OWN-001".to_string(),
                statement: "Bind loopback 127.0.0.1:5880 only".to_string(),
            }],
            initial_requirements: vec![RequirementRowInput {
                req_id: "REQ-01".to_string(),
                source_id: "OWN-001".to_string(),
                statement: "Serve GET /ready on 127.0.0.1:5880".to_string(),
            }],
        };

        let bundle1 = store
            .create_trio_project(&owner.grant_id, req.clone())
            .expect("create project");
        assert_eq!(bundle1.project.profile, TrioProfile::P1);
        assert_eq!(bundle1.resources.len(), 3);

        let bundle_retry = store
            .create_trio_project(&owner.grant_id, req.clone())
            .expect("idempotent retry");
        assert_eq!(bundle_retry.project.project_id, "demo-p1");

        let mut conflicting = req;
        conflicting.title = "Different Title".to_string();
        assert!(matches!(
            store.create_trio_project(&owner.grant_id, conflicting),
            Err(StoreError::Conflict(_))
        ));

        store
            .lock_project_plan(&owner.grant_id, "demo-p1", None)
            .expect("lock plan");

        let own_edit = store.upsert_own_decision(
            &worker.grant_id,
            "demo-p1",
            "OWN-001",
            "Bind 0.0.0.0 instead",
            None,
        );
        assert!(matches!(own_edit, Err(StoreError::OwnLocked)));

        let div = store
            .record_divergence(
                &worker.grant_id,
                RecordDivergenceRequest {
                    project_id: "demo-p1".to_string(),
                    div_id: None,
                    designed: "Port 5880".to_string(),
                    observed: "Tested ephemeral port in CI".to_string(),
                    resolution: "Keep 5880 default in config".to_string(),
                    status: DivergenceStatus::Accepted,
                },
            )
            .expect("record divergence");
        assert_eq!(div.div_id, "DIV-001");

        store
            .update_requirement_stage(
                &worker.grant_id,
                UpdateRequirementStageRequest {
                    project_id: "demo-p1".to_string(),
                    req_id: "REQ-01".to_string(),
                    target_stage: ReqStage::T,
                    evidence_ref: "cargo test ready".to_string(),
                    actor_family: Some("openai-codex".to_string()),
                    actor_is_custodian: false,
                },
            )
            .expect("mark tested");

        let same_family_accept = store.update_requirement_stage(
            &acceptor.grant_id,
            UpdateRequirementStageRequest {
                project_id: "demo-p1".to_string(),
                req_id: "REQ-01".to_string(),
                target_stage: ReqStage::A,
                evidence_ref: "qc pass".to_string(),
                actor_family: Some("openai-codex".to_string()),
                actor_is_custodian: false,
            },
        );
        assert!(same_family_accept.is_err());

        store
            .update_requirement_stage(
                &acceptor.grant_id,
                UpdateRequirementStageRequest {
                    project_id: "demo-p1".to_string(),
                    req_id: "REQ-01".to_string(),
                    target_stage: ReqStage::A,
                    evidence_ref: "qc pass".to_string(),
                    actor_family: Some("google-gemini".to_string()),
                    actor_is_custodian: false,
                },
            )
            .expect("cross-family accept");

        store
            .save_sync_report(
                &acceptor.grant_id,
                SaveSyncReportRequest {
                    project_id: "demo-p1".to_string(),
                    plan_rev: 1,
                    asbuilt_rev: 1,
                    paired_rows: vec![PairedSyncRow {
                        req_id: "REQ-01".to_string(),
                        planned: "Serve GET /ready".to_string(),
                        observed: "200 OK".to_string(),
                        stage: ReqStage::A,
                        matched: true,
                    }],
                    verdict: SyncVerdict::Pass,
                    acceptor_family: "google-gemini".to_string(),
                },
            )
            .expect("save sync report");

        let blocked_no_cleanup =
            store.submit_how_it_works(&worker.grant_id, "demo-p1", valid_how_it_works(), None);
        assert!(matches!(
            blocked_no_cleanup,
            Err(StoreError::InvalidState(ref msg)) if msg.contains("cleanup")
        ));

        let cleaned = store
            .mark_project_cleanup_verified(
                &worker.grant_id,
                "demo-p1",
                "Removed temporary build artifacts",
            )
            .expect("verify cleanup");
        assert!(cleaned.cleanup_verified);
        assert!(!cleaned.lessons_satisfied);

        let blocked_no_lesson =
            store.submit_how_it_works(&worker.grant_id, "demo-p1", valid_how_it_works(), None);
        assert!(matches!(
            blocked_no_lesson,
            Err(StoreError::InvalidState(ref msg)) if msg.contains("lessons learned")
        ));

        let lesson = store
            .record_lesson(
                &worker.grant_id,
                RecordLessonRequest {
                    lesson_id: Some("loopback-port-check".to_string()),
                    project_id: Some("demo-p1".to_string()),
                    tags: vec!["Rust".to_string(), "http".to_string()],
                    observed_fact: "Health probe stayed deterministic on loopback 5880."
                        .to_string(),
                    cause_kind: "demonstrated".to_string(),
                    cause_note: "Explicit loopback bind avoids interface drift.".to_string(),
                    forward_rule: "Always verify GET /ready on 127.0.0.1 before closeout."
                        .to_string(),
                    validation_state: Some("verified_reusable".to_string()),
                },
            )
            .expect("record lesson");
        assert_eq!(lesson.lesson_id, "loopback-port-check");
        assert_eq!(lesson.cause_kind, "demonstrated");
        assert_eq!(lesson.validation_state, "verified_reusable");

        let queried = store
            .query_lessons(&owner.grant_id, Some("rust"), Some("demo-p1"), 10)
            .expect("query lessons");
        assert_eq!(queried.len(), 1);
        assert_eq!(queried[0].lesson_id, "loopback-port-check");

        let boot = store.bootstrap_view(&owner.grant_id).expect("bootstrap");
        assert!(boot
            .forward_lessons
            .iter()
            .any(|item| item.lesson_id == "loopback-port-check"));

        let proj_pending = store
            .submit_how_it_works(&worker.grant_id, "demo-p1", valid_how_it_works(), None)
            .expect("submit how it works");
        assert_eq!(proj_pending.state, RunState::DocPendingOwner);
        assert!(proj_pending.cleanup_verified);
        assert!(proj_pending.lessons_satisfied);

        let agent_approve = store.approve_release(&worker.grant_id, "demo-p1");
        assert!(matches!(agent_approve, Err(StoreError::ForbiddenRole(_))));

        let released = store
            .approve_release(&owner.grant_id, "demo-p1")
            .expect("owner approve release");
        assert_eq!(released.project.state, RunState::Done);
        assert_eq!(released.requirements[0].stage, ReqStage::L);
    }

    #[test]
    fn vault_sync_imports_clean_edits_and_records_conflicts() {
        let dir = TempDirGuard::new("streamline-vault-test");
        let db_path = dir.path.join("streamline.sqlite3");
        let vault_dir = dir.path.join("vault");

        let store = StreamlineStore::open(&db_path).expect("open file store");
        let owner = seed_owner_grant(&store);
        let rules_sha = store.current_rules_sha256().expect("sha");
        store.ack_rules(&owner.grant_id, &rules_sha).expect("ack");

        store
            .save_resource(
                &owner.grant_id,
                SaveResourceRequest {
                    shelf_slug: "architecture".to_string(),
                    slug: "notes".to_string(),
                    title: "Architecture Notes".to_string(),
                    body_md: "Line one\nLine two\n".to_string(),
                    project_id: None,
                    trio_role: None,
                    lifecycle_tier: Some(LifecycleTier::Active),
                    triage_state: Some(TriageState::Reviewed),
                    summary: "Initial notes".to_string(),
                    expected_rev: None,
                },
            )
            .expect("save notes");

        let sync1 = store.sync_vault_dir(&vault_dir).expect("initial export");
        assert!(sync1.exported >= 2);
        assert!(sync1.conflicts.is_empty());

        let md_path = vault_dir.join("architecture").join("notes.md");
        let exported_text = fs::read_to_string(&md_path).expect("read exported md");
        let clean_edit = exported_text.replace("Line two", "Line two edited externally");
        fs::write(&md_path, clean_edit).expect("write clean edit");

        let sync2 = store.sync_vault_dir(&vault_dir).expect("clean import");
        assert_eq!(sync2.imported, 1);
        assert!(sync2.conflicts.is_empty());

        let loaded = store
            .read_resource(&owner.grant_id, "architecture", "notes")
            .expect("read imported");
        assert_eq!(loaded.resource.current_rev, 2);
        assert!(loaded.body_md.contains("Line two edited externally"));

        let diff = store
            .compare_revisions(&owner.grant_id, "architecture", "notes", 1, 2)
            .expect("compare revs");
        assert_eq!(diff.added_lines, vec!["Line two edited externally"]);

        let base_disk_text = fs::read_to_string(&md_path).expect("read synced rev 2");
        store
            .save_resource(
                &owner.grant_id,
                SaveResourceRequest {
                    shelf_slug: "architecture".to_string(),
                    slug: "notes".to_string(),
                    title: "Architecture Notes".to_string(),
                    body_md: "Line one\nDB changed concurrently\n".to_string(),
                    project_id: None,
                    trio_role: None,
                    lifecycle_tier: Some(LifecycleTier::Active),
                    triage_state: Some(TriageState::Reviewed),
                    summary: "DB concurrent edit".to_string(),
                    expected_rev: Some(2),
                },
            )
            .expect("db concurrent edit");

        let disk_concurrent =
            base_disk_text.replace("Line two edited externally", "Disk changed concurrently");
        fs::write(&md_path, disk_concurrent).expect("write disk concurrent edit");

        let sync3 = store.sync_vault_dir(&vault_dir).expect("conflict sync");
        assert_eq!(sync3.conflicts.len(), 1);
        let conflict = &sync3.conflicts[0];
        let inbox_doc = store
            .read_resource(&owner.grant_id, "inbox", &conflict.conflict_slug)
            .expect("read conflict doc");
        assert!(inbox_doc.body_md.contains("DB changed concurrently"));
        assert!(inbox_doc.body_md.contains("Disk changed concurrently"));
    }

    #[test]
    fn import_legacy_sqlite_fences_dormant_and_skips_noise() {
        let dir = TempDirGuard::new("streamline-legacy-test");
        let legacy_path = dir.path.join("legacy-explorer.sqlite3");

        {
            let lconn = Connection::open(&legacy_path).expect("open legacy fixture");
            lconn
                .execute_batch(
                    "
                    CREATE TABLE catalog_items (
                        id TEXT PRIMARY KEY,
                        slug TEXT,
                        title TEXT,
                        kind TEXT,
                        body_md TEXT,
                        steward TEXT,
                        freshness_state TEXT,
                        lineage_ref TEXT,
                        verified_at TEXT
                    );
                    CREATE TABLE entities (
                        id TEXT PRIMARY KEY,
                        name TEXT,
                        entity_type TEXT,
                        description TEXT,
                        owner TEXT,
                        source_ref TEXT
                    );
                    CREATE TABLE audit_events (id TEXT, msg TEXT);
                    CREATE TABLE freshness_check (id TEXT, status TEXT);
                    CREATE TABLE sessions (id TEXT, principal TEXT);
                    CREATE TABLE oauth_codes (code TEXT);
                    CREATE TABLE access_tokens (grant_ref TEXT);
                    ",
                )
                .expect("create legacy schema");

            lconn
                .execute(
                    "INSERT INTO catalog_items VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        "cat-1",
                        "old-gateway-design",
                        "Legacy Gateway Design",
                        "design",
                        "Detailed historical gateway topology.",
                        "owner",
                        "stale",
                        "legacy:catalog:1",
                        "2026-01-10T00:00:00Z"
                    ],
                )
                .expect("insert design");
            lconn
                .execute(
                    "INSERT INTO catalog_items VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        "cat-noise-1",
                        "noise-404",
                        "Endpoint /foo returns 404",
                        "feedback",
                        "GET /foo returns 404 when unauthenticated",
                        "bot",
                        "unknown",
                        "legacy:noise:1",
                        "2026-01-11T00:00:00Z"
                    ],
                )
                .expect("insert noise 1");
            lconn
                .execute(
                    "INSERT INTO catalog_items VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        "cat-noise-2",
                        "noise-404-dup",
                        "Endpoint /bar returns 404",
                        "feedback",
                        "Duplicate returns 404 log entry",
                        "bot",
                        "unknown",
                        "legacy:noise:2",
                        "2026-01-12T00:00:00Z"
                    ],
                )
                .expect("insert noise 2");
            lconn
                .execute(
                    "INSERT INTO entities VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        "ent-1",
                        "Voice Assistant Want",
                        "want",
                        "Idea to capture voice notes into inbox.",
                        "owner",
                        "legacy:entity:ent-1"
                    ],
                )
                .expect("insert entity");
        }

        let store = StreamlineStore::open_in_memory().expect("open target store");
        let owner = seed_owner_grant(&store);
        let summary = store
            .import_legacy_sqlite(&legacy_path)
            .expect("import legacy");

        assert_eq!(summary.imported_count, 2);
        assert_eq!(summary.skipped_noise_count, 2);
        assert_eq!(summary.skipped_tables.len(), 5);

        let boot = store.bootstrap_view(&owner.grant_id).expect("bootstrap");
        assert!(!boot
            .active_resources
            .iter()
            .any(|res| res.slug == "old-gateway-design" || res.slug == "voice-assistant-want"));

        let dormant_items = store
            .list_resources(
                &owner.grant_id,
                &ResourceFilter {
                    lifecycle_tier: Some(LifecycleTier::Dormant),
                    triage_state: Some(TriageState::NeedsTriage),
                    ..Default::default()
                },
            )
            .expect("list dormant");
        assert_eq!(dormant_items.len(), 2);

        let passport = store
            .get_source_passport(&owner.grant_id, &dormant_items[0].id)
            .expect("get passport")
            .expect("passport present");
        assert_eq!(passport.steward, "owner");
        assert!(
            passport.freshness_state == FreshnessState::Stale
                || passport.freshness_state == FreshnessState::Unknown
        );
    }

    #[test]
    fn no_new_lessons_and_vault_lessons_export_and_legacy_revision_lookup() {
        let dir = TempDirGuard::new("streamline-lessons-vault-legacy");
        let db_path = dir.path.join("streamline.sqlite3");
        let vault_dir = dir.path.join("vault");
        let legacy_path = dir.path.join("legacy-v2.sqlite3");

        let store = StreamlineStore::open(&db_path).expect("open store");
        let owner = seed_owner_grant(&store);
        let rules_sha = store.current_rules_sha256().expect("sha");
        store.ack_rules(&owner.grant_id, &rules_sha).expect("ack");

        store
            .create_trio_project(
                &owner.grant_id,
                CreateTrioProjectRequest {
                    project_id: "proj-no-lesson".to_string(),
                    create_id: None,
                    title: "Routine Patch".to_string(),
                    assessment: sample_assessment(),
                    opt_in_modules: Vec::new(),
                    initial_own_decisions: Vec::new(),
                    initial_requirements: Vec::new(),
                },
            )
            .expect("create project");

        store
            .mark_project_cleanup_verified(&owner.grant_id, "proj-no-lesson", "No temp files left")
            .expect("verify cleanup");

        let empty_reason = store.record_no_new_lessons(&owner.grant_id, "proj-no-lesson", "   ");
        assert!(matches!(empty_reason, Err(StoreError::InvalidInput(_))));

        let satisfied = store
            .record_no_new_lessons(
                &owner.grant_id,
                "proj-no-lesson",
                "Existing runbook already covers this routine patch.",
            )
            .expect("record no new lessons");
        assert!(satisfied.lessons_satisfied);

        let pending = store
            .submit_how_it_works(
                &owner.grant_id,
                "proj-no-lesson",
                valid_how_it_works(),
                None,
            )
            .expect("submit how it works");
        assert_eq!(pending.state, RunState::DocPendingOwner);

        store
            .record_lesson(
                &owner.grant_id,
                RecordLessonRequest {
                    lesson_id: Some("central-vault-rule".to_string()),
                    project_id: None,
                    tags: vec!["vault".to_string()],
                    observed_fact: "Central lessons export to vault directory.".to_string(),
                    cause_kind: "demonstrated".to_string(),
                    cause_note: "Verified during closeout sync.".to_string(),
                    forward_rule: "Check <vault>/lessons after every release.".to_string(),
                    validation_state: None,
                },
            )
            .expect("record central lesson");

        let sync_res = store.sync_vault_dir(&vault_dir).expect("sync vault");
        assert!(sync_res.exported >= 1);
        let lesson_md = fs::read_to_string(vault_dir.join("lessons").join("central-vault-rule.md"))
            .expect("read exported lesson markdown");
        assert!(lesson_md.contains("central-vault-rule"));
        assert!(lesson_md.contains("Check <vault>/lessons after every release."));

        {
            let lconn = Connection::open(&legacy_path).expect("open legacy");
            lconn
                .execute_batch(
                    "
                    CREATE TABLE catalog_items (
                        id TEXT PRIMARY KEY,
                        slug TEXT,
                        title TEXT,
                        kind TEXT,
                        summary TEXT
                    );
                    CREATE TABLE document_revision (
                        slug TEXT,
                        revision INTEGER,
                        body TEXT
                    );
                    CREATE TABLE ideas (
                        id TEXT PRIMARY KEY,
                        title TEXT,
                        blurb TEXT
                    );
                    CREATE TABLE capability_cards (
                        id TEXT PRIMARY KEY,
                        title TEXT,
                        summary TEXT
                    );
                    ",
                )
                .expect("create legacy schema");
            lconn
                .execute(
                    "INSERT INTO catalog_items VALUES ('c1', 'service-guide', 'Service Guide', 'design', 'Short blurb')",
                    [],
                )
                .expect("insert catalog item");
            lconn
                .execute(
                    "INSERT INTO document_revision VALUES ('service-guide', 1, 'Older draft body')",
                    [],
                )
                .expect("insert rev 1");
            lconn
                .execute(
                    "INSERT INTO document_revision VALUES ('service-guide', 2, 'Latest revision body from document_revision')",
                    [],
                )
                .expect("insert rev 2");
            lconn
                .execute(
                    "INSERT INTO ideas VALUES ('idea-1', 'Voice Capture Idea', 'Capture quick voice notes into inbox.')",
                    [],
                )
                .expect("insert idea");
            lconn
                .execute(
                    "INSERT INTO capability_cards VALUES ('cap-1', 'Local Search Card', 'Fast offline FTS card.')",
                    [],
                )
                .expect("insert capability card");
        }

        let legacy_summary = store
            .import_legacy_sqlite(&legacy_path)
            .expect("import legacy");
        assert_eq!(legacy_summary.imported_count, 3);
        assert!(legacy_summary
            .detected_tables
            .iter()
            .any(|name| name == "ideas"));
        assert!(legacy_summary
            .detected_tables
            .iter()
            .any(|name| name == "capability_cards"));

        let imported_guide = store
            .read_resource(&owner.grant_id, "architecture", "service-guide")
            .expect("read imported guide");
        assert_eq!(
            imported_guide.body_md,
            "Latest revision body from document_revision"
        );
    }

    #[test]
    fn save_resource_blocks_cross_project_and_unscoped_overwrite() {
        let store = StreamlineStore::open_in_memory().expect("open store");
        let owner = seed_owner_grant(&store);
        let rules_sha = store.current_rules_sha256().expect("rules sha");
        store
            .ack_rules(&owner.grant_id, &rules_sha)
            .expect("ack owner");

        store
            .save_resource(
                &owner.grant_id,
                SaveResourceRequest {
                    shelf_slug: "projects".to_string(),
                    slug: "other-doc".to_string(),
                    title: "Other Project Doc".to_string(),
                    body_md: "Protected other project content.".to_string(),
                    project_id: Some("other".to_string()),
                    trio_role: None,
                    lifecycle_tier: Some(LifecycleTier::Active),
                    triage_state: Some(TriageState::Reviewed),
                    summary: "Seed other project doc".to_string(),
                    expected_rev: None,
                },
            )
            .expect("save other project doc");

        store
            .save_resource(
                &owner.grant_id,
                SaveResourceRequest {
                    shelf_slug: "projects".to_string(),
                    slug: "unscoped-doc".to_string(),
                    title: "Unscoped Projects Doc".to_string(),
                    body_md: "Protected unscoped content.".to_string(),
                    project_id: None,
                    trio_role: None,
                    lifecycle_tier: Some(LifecycleTier::Active),
                    triage_state: Some(TriageState::Reviewed),
                    summary: "Seed unscoped project doc".to_string(),
                    expected_rev: None,
                },
            )
            .expect("save unscoped project doc");

        let demo_agent = seed_agent_grant(
            &store,
            "demo-agent",
            "cred-demo-agent",
            vec!["project:demo".to_string()],
        );
        store
            .ack_rules(&demo_agent.grant_id, &rules_sha)
            .expect("ack demo agent");

        let overwrite_other = store.save_resource(
            &demo_agent.grant_id,
            SaveResourceRequest {
                shelf_slug: "projects".to_string(),
                slug: "other-doc".to_string(),
                title: "Hijacked Title".to_string(),
                body_md: "Hijacked body.".to_string(),
                project_id: Some("demo".to_string()),
                trio_role: None,
                lifecycle_tier: Some(LifecycleTier::Active),
                triage_state: Some(TriageState::Reviewed),
                summary: "Attempt overwrite".to_string(),
                expected_rev: None,
            },
        );
        assert!(matches!(
            overwrite_other,
            Err(StoreError::ForbiddenTag { ref required_tag }) if required_tag == "project:other"
        ));

        let overwrite_unscoped = store.save_resource(
            &demo_agent.grant_id,
            SaveResourceRequest {
                shelf_slug: "projects".to_string(),
                slug: "unscoped-doc".to_string(),
                title: "Hijacked Unscoped".to_string(),
                body_md: "Hijacked unscoped body.".to_string(),
                project_id: Some("demo".to_string()),
                trio_role: None,
                lifecycle_tier: Some(LifecycleTier::Active),
                triage_state: Some(TriageState::Reviewed),
                summary: "Attempt unscoped overwrite".to_string(),
                expected_rev: None,
            },
        );
        assert!(matches!(
            overwrite_unscoped,
            Err(StoreError::ForbiddenTag { ref required_tag }) if required_tag == "projects"
        ));

        store
            .save_resource(
                &demo_agent.grant_id,
                SaveResourceRequest {
                    shelf_slug: "projects".to_string(),
                    slug: "demo-own".to_string(),
                    title: "Demo Own".to_string(),
                    body_md: "Demo content.".to_string(),
                    project_id: Some("demo".to_string()),
                    trio_role: None,
                    lifecycle_tier: Some(LifecycleTier::Active),
                    triage_state: Some(TriageState::Reviewed),
                    summary: "Demo write".to_string(),
                    expected_rev: None,
                },
            )
            .expect("create demo doc");

        let move_to_other = store.save_resource(
            &demo_agent.grant_id,
            SaveResourceRequest {
                shelf_slug: "projects".to_string(),
                slug: "demo-own".to_string(),
                title: "Moved".to_string(),
                body_md: "Moved.".to_string(),
                project_id: Some("other".to_string()),
                trio_role: None,
                lifecycle_tier: Some(LifecycleTier::Active),
                triage_state: Some(TriageState::Reviewed),
                summary: "Attempt move".to_string(),
                expected_rev: None,
            },
        );
        assert!(matches!(
            move_to_other,
            Err(StoreError::ForbiddenTag { ref required_tag }) if required_tag == "project:other"
        ));

        let other_after = store
            .read_resource(&owner.grant_id, "projects", "other-doc")
            .expect("read other-doc");
        assert_eq!(other_after.body_md, "Protected other project content.");
        assert_eq!(other_after.resource.project_id.as_deref(), Some("other"));
    }

    #[test]
    fn lock_project_plan_rejects_nonexistent_revision_without_locking_own() {
        let store = StreamlineStore::open_in_memory().expect("open store");
        let owner = seed_owner_grant(&store);
        let rules_sha = store.current_rules_sha256().expect("rules sha");
        store
            .ack_rules(&owner.grant_id, &rules_sha)
            .expect("ack owner");

        store
            .create_trio_project(
                &owner.grant_id,
                CreateTrioProjectRequest {
                    project_id: "proj-rev-check".to_string(),
                    create_id: None,
                    title: "Plan Rev Check".to_string(),
                    assessment: sample_assessment(),
                    opt_in_modules: Vec::new(),
                    initial_own_decisions: vec![OwnDecisionInput {
                        own_id: "OWN-001".to_string(),
                        statement: "Original choice".to_string(),
                    }],
                    initial_requirements: Vec::new(),
                },
            )
            .expect("create project");

        let phantom = store.lock_project_plan(&owner.grant_id, "proj-rev-check", Some(999));
        assert!(matches!(phantom, Err(StoreError::InvalidInput(_))));

        let zero_rev = store.lock_project_plan(&owner.grant_id, "proj-rev-check", Some(0));
        assert!(matches!(zero_rev, Err(StoreError::InvalidInput(_))));

        let bundle_before = store
            .get_project_bundle(&owner.grant_id, "proj-rev-check")
            .expect("bundle before valid lock");
        assert_eq!(bundle_before.project.plan_locked_rev, None);
        assert!(bundle_before.own_decisions[0].locked_at.is_none());

        store
            .upsert_own_decision(
                &owner.grant_id,
                "proj-rev-check",
                "OWN-001",
                "Refined choice before lock",
                None,
            )
            .expect("OWN must remain editable after rejected lock");

        let locked = store
            .lock_project_plan(&owner.grant_id, "proj-rev-check", Some(1))
            .expect("lock existing rev 1");
        assert_eq!(locked.plan_locked_rev, Some(1));
    }

    #[test]
    fn update_requirement_stage_enforces_grant_model_family_and_blocks_spoofing() {
        let store = StreamlineStore::open_in_memory().expect("open store");
        let owner = seed_owner_grant(&store);
        let unbound_worker = seed_agent_grant_with_family(
            &store,
            "unbound-worker",
            "cred-unbound-w",
            vec!["*".to_string()],
            None,
        );
        let codex_worker = seed_agent_grant_with_family(
            &store,
            "codex-worker",
            "cred-codex-w",
            vec!["*".to_string()],
            Some("openai-codex"),
        );
        let codex_peer = seed_agent_grant_with_family(
            &store,
            "codex-peer",
            "cred-codex-p",
            vec!["*".to_string()],
            Some("openai-codex"),
        );
        let gemini_acceptor = seed_agent_grant_with_family(
            &store,
            "gemini-acceptor",
            "cred-gemini-a",
            vec!["*".to_string()],
            Some("google-gemini"),
        );

        let rules_sha = store.current_rules_sha256().expect("rules sha");
        for g in [
            &owner.grant_id,
            &unbound_worker.grant_id,
            &codex_worker.grant_id,
            &codex_peer.grant_id,
            &gemini_acceptor.grant_id,
        ] {
            store.ack_rules(g, &rules_sha).expect("ack rules");
        }

        store
            .create_trio_project(
                &owner.grant_id,
                CreateTrioProjectRequest {
                    project_id: "proj-family-gate".to_string(),
                    create_id: None,
                    title: "Model Family Gate".to_string(),
                    assessment: sample_assessment(),
                    opt_in_modules: Vec::new(),
                    initial_own_decisions: Vec::new(),
                    initial_requirements: vec![RequirementRowInput {
                        req_id: "REQ-01".to_string(),
                        source_id: "PLAN".to_string(),
                        statement: "Enforce grant-bound family provenance".to_string(),
                    }],
                },
            )
            .expect("create R2 project");

        let unbound_spoof = store.update_requirement_stage(
            &unbound_worker.grant_id,
            UpdateRequirementStageRequest {
                project_id: "proj-family-gate".to_string(),
                req_id: "REQ-01".to_string(),
                target_stage: ReqStage::I,
                evidence_ref: "commit-1".to_string(),
                actor_family: Some("openai-codex".to_string()),
                actor_is_custodian: false,
            },
        );
        assert!(matches!(unbound_spoof, Err(StoreError::ForbiddenRole(_))));

        let unbound_none = store.update_requirement_stage(
            &unbound_worker.grant_id,
            UpdateRequirementStageRequest {
                project_id: "proj-family-gate".to_string(),
                req_id: "REQ-01".to_string(),
                target_stage: ReqStage::I,
                evidence_ref: "commit-1".to_string(),
                actor_family: None,
                actor_is_custodian: false,
            },
        );
        assert!(matches!(unbound_none, Err(StoreError::ForbiddenRole(_))));

        let built = store
            .update_requirement_stage(
                &codex_worker.grant_id,
                UpdateRequirementStageRequest {
                    project_id: "proj-family-gate".to_string(),
                    req_id: "REQ-01".to_string(),
                    target_stage: ReqStage::T,
                    evidence_ref: "unit tests passed".to_string(),
                    actor_family: None,
                    actor_is_custodian: false,
                },
            )
            .expect("bound worker marks T without caller actor_family");
        assert_eq!(built.builder_family.as_deref(), Some("openai-codex"));

        let owner_spoof_accept = store.update_requirement_stage(
            &owner.grant_id,
            UpdateRequirementStageRequest {
                project_id: "proj-family-gate".to_string(),
                req_id: "REQ-01".to_string(),
                target_stage: ReqStage::A,
                evidence_ref: "owner unbound spoof".to_string(),
                actor_family: Some("google-gemini".to_string()),
                actor_is_custodian: false,
            },
        );
        assert!(matches!(
            owner_spoof_accept,
            Err(StoreError::ForbiddenRole(_))
        ));

        let owner_unbound_accept = store.update_requirement_stage(
            &owner.grant_id,
            UpdateRequirementStageRequest {
                project_id: "proj-family-gate".to_string(),
                req_id: "REQ-01".to_string(),
                target_stage: ReqStage::A,
                evidence_ref: "owner unbound none".to_string(),
                actor_family: None,
                actor_is_custodian: false,
            },
        );
        assert!(matches!(
            owner_unbound_accept,
            Err(StoreError::ForbiddenRole(_))
        ));

        let spoof_accept = store.update_requirement_stage(
            &codex_peer.grant_id,
            UpdateRequirementStageRequest {
                project_id: "proj-family-gate".to_string(),
                req_id: "REQ-01".to_string(),
                target_stage: ReqStage::A,
                evidence_ref: "spoofed family".to_string(),
                actor_family: Some("google-gemini".to_string()),
                actor_is_custodian: false,
            },
        );
        assert!(matches!(spoof_accept, Err(StoreError::ForbiddenRole(_))));

        let same_family = store.update_requirement_stage(
            &codex_peer.grant_id,
            UpdateRequirementStageRequest {
                project_id: "proj-family-gate".to_string(),
                req_id: "REQ-01".to_string(),
                target_stage: ReqStage::A,
                evidence_ref: "same family".to_string(),
                actor_family: None,
                actor_is_custodian: false,
            },
        );
        assert!(matches!(
            same_family,
            Err(StoreError::Core(CoreError::SameFamilyForbidden { .. }))
        ));

        let accepted = store
            .update_requirement_stage(
                &gemini_acceptor.grant_id,
                UpdateRequirementStageRequest {
                    project_id: "proj-family-gate".to_string(),
                    req_id: "REQ-01".to_string(),
                    target_stage: ReqStage::A,
                    evidence_ref: "verified cross-family".to_string(),
                    actor_family: None,
                    actor_is_custodian: false,
                },
            )
            .expect("cross-family bound acceptor succeeds");
        assert_eq!(accepted.stage, ReqStage::A);
        assert_eq!(accepted.acceptor_family.as_deref(), Some("google-gemini"));
    }

    #[test]
    fn save_sync_report_computes_server_verdict_and_ignores_forged_pass() {
        let store = StreamlineStore::open_in_memory().expect("open store");
        let owner = seed_owner_grant(&store);
        let worker = seed_agent_grant_with_family(
            &store,
            "sync-worker",
            "cred-sync-w",
            vec!["*".to_string()],
            Some("openai-codex"),
        );
        let acceptor = seed_agent_grant_with_family(
            &store,
            "sync-acceptor",
            "cred-sync-a",
            vec!["*".to_string()],
            Some("google-gemini"),
        );
        let rules_sha = store.current_rules_sha256().expect("rules sha");
        store.ack_rules(&owner.grant_id, &rules_sha).unwrap();
        store.ack_rules(&worker.grant_id, &rules_sha).unwrap();
        store.ack_rules(&acceptor.grant_id, &rules_sha).unwrap();

        store
            .create_trio_project(
                &owner.grant_id,
                CreateTrioProjectRequest {
                    project_id: "proj-sync-verify".to_string(),
                    create_id: None,
                    title: "Sync Verify".to_string(),
                    assessment: sample_assessment(),
                    opt_in_modules: Vec::new(),
                    initial_own_decisions: vec![OwnDecisionInput {
                        own_id: "OWN-001".to_string(),
                        statement: "Loopback only".to_string(),
                    }],
                    initial_requirements: vec![RequirementRowInput {
                        req_id: "REQ-01".to_string(),
                        source_id: "OWN-001".to_string(),
                        statement: "Bind 127.0.0.1".to_string(),
                    }],
                },
            )
            .expect("create project");
        store
            .lock_project_plan(&owner.grant_id, "proj-sync-verify", None)
            .expect("lock plan");
        store
            .update_requirement_stage(
                &worker.grant_id,
                UpdateRequirementStageRequest {
                    project_id: "proj-sync-verify".to_string(),
                    req_id: "REQ-01".to_string(),
                    target_stage: ReqStage::T,
                    evidence_ref: "tested".to_string(),
                    actor_family: None,
                    actor_is_custodian: false,
                },
            )
            .expect("mark T");
        store
            .update_requirement_stage(
                &acceptor.grant_id,
                UpdateRequirementStageRequest {
                    project_id: "proj-sync-verify".to_string(),
                    req_id: "REQ-01".to_string(),
                    target_stage: ReqStage::A,
                    evidence_ref: "accepted".to_string(),
                    actor_family: None,
                    actor_is_custodian: false,
                },
            )
            .expect("mark A");

        let forged_attempt = store
            .save_sync_report(
                &acceptor.grant_id,
                SaveSyncReportRequest {
                    project_id: "proj-sync-verify".to_string(),
                    plan_rev: 99,
                    asbuilt_rev: 99,
                    paired_rows: vec![PairedSyncRow {
                        req_id: "REQ-01".to_string(),
                        planned: "forged".to_string(),
                        observed: "forged".to_string(),
                        stage: ReqStage::A,
                        matched: true,
                    }],
                    verdict: SyncVerdict::Pass,
                    acceptor_family: "forged-family".to_string(),
                },
            )
            .expect("computes server verdict instead of trusting forged payload");
        assert_eq!(forged_attempt.verdict, SyncVerdict::Fail);
        assert_eq!(forged_attempt.plan_rev, 1);
        assert_eq!(forged_attempt.asbuilt_rev, 1);
        assert_eq!(forged_attempt.acceptor_family, "google-gemini");
        assert_eq!(forged_attempt.paired_rows.len(), 1);
        assert!(!forged_attempt.paired_rows[0].matched);

        let same_family_sync =
            store.compute_and_save_sync_report(&worker.grant_id, "proj-sync-verify", None);
        assert!(matches!(
            same_family_sync,
            Err(StoreError::Core(CoreError::SameFamilyForbidden { .. }))
        ));

        store
            .save_resource(
                &worker.grant_id,
                SaveResourceRequest {
                    shelf_slug: "projects".to_string(),
                    slug: "proj-sync-verify-as_built".to_string(),
                    title: "Sync Verify: as_built".to_string(),
                    body_md: "# As Built\n\n| REQ-01 | A | Bound 127.0.0.1 | test:loopback |\n"
                        .to_string(),
                    project_id: Some("proj-sync-verify".to_string()),
                    trio_role: Some("as_built".to_string()),
                    lifecycle_tier: Some(LifecycleTier::Active),
                    triage_state: Some(TriageState::Reviewed),
                    summary: "Document REQ-01 in as_built".to_string(),
                    expected_rev: Some(1),
                },
            )
            .expect("update as_built");

        let real_pass = store
            .compute_and_save_sync_report(&acceptor.grant_id, "proj-sync-verify", None)
            .expect("compute real pass");
        assert_eq!(real_pass.verdict, SyncVerdict::Pass);
        assert_eq!(real_pass.asbuilt_rev, 2);
        assert!(real_pass.paired_rows[0].matched);

        let owner_unbound_sync =
            store.compute_and_save_sync_report(&owner.grant_id, "proj-sync-verify", None);
        assert!(matches!(
            owner_unbound_sync,
            Err(StoreError::ForbiddenRole(_))
        ));

        store
            .save_resource(
                &worker.grant_id,
                SaveResourceRequest {
                    shelf_slug: "projects".to_string(),
                    slug: "proj-sync-verify-as_built".to_string(),
                    title: "Sync Verify: as_built".to_string(),
                    body_md: "# As Built\n\n\
                              | REQ-01 | A | Bound 127.0.0.1 | test:loopback |\n\
                              | REQ-99 | A | Extra unmapped claim | test:extra |\n"
                        .to_string(),
                    project_id: Some("proj-sync-verify".to_string()),
                    trio_role: Some("as_built".to_string()),
                    lifecycle_tier: Some(LifecycleTier::Active),
                    triage_state: Some(TriageState::Reviewed),
                    summary: "Add unmapped REQ-99 claim in as_built".to_string(),
                    expected_rev: Some(2),
                },
            )
            .expect("update as_built with unmapped claim");

        let partial_report = store
            .save_sync_report(
                &acceptor.grant_id,
                SaveSyncReportRequest {
                    project_id: "proj-sync-verify".to_string(),
                    plan_rev: 1,
                    asbuilt_rev: 3,
                    paired_rows: Vec::new(),
                    verdict: SyncVerdict::Pass,
                    acceptor_family: "google-gemini".to_string(),
                },
            )
            .expect("save partial sync report");
        assert_eq!(partial_report.verdict, SyncVerdict::Partial);
        let fetched_partial = store
            .get_sync_report(
                &acceptor.grant_id,
                "proj-sync-verify",
                Some(&partial_report.report_id),
            )
            .expect("get partial report")
            .expect("partial report persisted");
        assert_eq!(fetched_partial.verdict, SyncVerdict::Partial);
    }

    #[test]
    fn upsert_requirement_row_and_stage_reset_clear_prior_acceptance() {
        let store = StreamlineStore::open_in_memory().expect("open store");
        let owner = seed_owner_grant(&store);
        let worker = seed_agent_grant_with_family(
            &store,
            "req-worker",
            "cred-req-w",
            vec!["*".to_string()],
            Some("openai-codex"),
        );
        let acceptor = seed_agent_grant_with_family(
            &store,
            "req-acceptor",
            "cred-req-a",
            vec!["*".to_string()],
            Some("google-gemini"),
        );
        let rules_sha = store.current_rules_sha256().expect("rules sha");
        store.ack_rules(&owner.grant_id, &rules_sha).unwrap();
        store.ack_rules(&worker.grant_id, &rules_sha).unwrap();
        store.ack_rules(&acceptor.grant_id, &rules_sha).unwrap();

        store
            .create_trio_project(
                &owner.grant_id,
                CreateTrioProjectRequest {
                    project_id: "proj-req-reset".to_string(),
                    create_id: None,
                    title: "Req Reset".to_string(),
                    assessment: sample_assessment(),
                    opt_in_modules: Vec::new(),
                    initial_own_decisions: Vec::new(),
                    initial_requirements: vec![RequirementRowInput {
                        req_id: "REQ-01".to_string(),
                        source_id: "OWN-001".to_string(),
                        statement: "Original statement".to_string(),
                    }],
                },
            )
            .expect("create project");

        store
            .update_requirement_stage(
                &worker.grant_id,
                UpdateRequirementStageRequest {
                    project_id: "proj-req-reset".to_string(),
                    req_id: "REQ-01".to_string(),
                    target_stage: ReqStage::T,
                    evidence_ref: "test-ok".to_string(),
                    actor_family: None,
                    actor_is_custodian: false,
                },
            )
            .expect("mark T");
        let accepted = store
            .update_requirement_stage(
                &acceptor.grant_id,
                UpdateRequirementStageRequest {
                    project_id: "proj-req-reset".to_string(),
                    req_id: "REQ-01".to_string(),
                    target_stage: ReqStage::A,
                    evidence_ref: "accept-ok".to_string(),
                    actor_family: None,
                    actor_is_custodian: false,
                },
            )
            .expect("mark A");
        assert_eq!(accepted.stage, ReqStage::A);
        assert_eq!(accepted.acceptor_family.as_deref(), Some("google-gemini"));

        let same_def = store
            .upsert_requirement_row(
                &owner.grant_id,
                "proj-req-reset",
                "REQ-01",
                "OWN-001",
                "Original statement",
            )
            .expect("identical upsert preserves stage");
        assert_eq!(same_def.stage, ReqStage::A);

        let changed_def = store
            .upsert_requirement_row(
                &owner.grant_id,
                "proj-req-reset",
                "REQ-01",
                "OWN-001",
                "Changed requirement statement",
            )
            .expect("changed statement resets stage");
        assert_eq!(changed_def.stage, ReqStage::D);
        assert_eq!(changed_def.evidence_ref, "");
        assert_eq!(changed_def.builder_principal, None);
        assert_eq!(changed_def.builder_family, None);
        assert_eq!(changed_def.acceptor_principal, None);
        assert_eq!(changed_def.acceptor_family, None);

        store
            .update_requirement_stage(
                &worker.grant_id,
                UpdateRequirementStageRequest {
                    project_id: "proj-req-reset".to_string(),
                    req_id: "REQ-01".to_string(),
                    target_stage: ReqStage::I,
                    evidence_ref: "impl-ref".to_string(),
                    actor_family: None,
                    actor_is_custodian: false,
                },
            )
            .expect("mark I");
        let reset_stage = store
            .update_requirement_stage(
                &worker.grant_id,
                UpdateRequirementStageRequest {
                    project_id: "proj-req-reset".to_string(),
                    req_id: "REQ-01".to_string(),
                    target_stage: ReqStage::D,
                    evidence_ref: String::new(),
                    actor_family: None,
                    actor_is_custodian: false,
                },
            )
            .expect("reset stage to D");
        assert_eq!(reset_stage.stage, ReqStage::D);
        assert_eq!(reset_stage.evidence_ref, "");
        assert_eq!(reset_stage.builder_principal, None);
        assert_eq!(reset_stage.builder_family, None);
    }

    #[cfg(unix)]
    #[test]
    fn vault_sync_rejects_symlinks_without_reading_or_writing_outside_target() {
        let dir = TempDirGuard::new("streamline-vault-symlink");
        let db_path = dir.path.join("streamline.sqlite3");
        let vault_dir = dir.path.join("vault");
        let outside_file = dir.path.join("outside.txt");
        fs::write(&outside_file, "DO_NOT_READ_OR_OVERWRITE\n").expect("write outside");

        let store = StreamlineStore::open(&db_path).expect("open store");
        let owner = seed_owner_grant(&store);

        let standards_dir = vault_dir.join("standards");
        fs::create_dir_all(&standards_dir).expect("create standards dir");
        std::os::unix::fs::symlink(&outside_file, standards_dir.join("escape.md"))
            .expect("plant symlink");

        let err = store.sync_vault_dir(&vault_dir);
        assert!(matches!(
            err,
            Err(StoreError::InvalidInput(ref msg)) if msg.contains("symlink")
        ));
        assert!(store
            .read_resource(&owner.grant_id, "standards", "escape")
            .is_err());
        assert_eq!(
            fs::read_to_string(&outside_file).expect("read outside"),
            "DO_NOT_READ_OR_OVERWRITE\n"
        );

        fs::remove_file(standards_dir.join("escape.md")).expect("remove shelf symlink");
        let lessons_dir = vault_dir.join("lessons");
        fs::create_dir_all(&lessons_dir).expect("create lessons dir");
        std::os::unix::fs::symlink(&outside_file, lessons_dir.join("escape.md"))
            .expect("plant lessons symlink");
        let lessons_err = store.sync_vault_dir(&vault_dir);
        assert!(matches!(
            lessons_err,
            Err(StoreError::InvalidInput(ref msg)) if msg.contains("symlink")
        ));
        assert_eq!(
            fs::read_to_string(&outside_file).expect("read outside"),
            "DO_NOT_READ_OR_OVERWRITE\n"
        );
    }
}
