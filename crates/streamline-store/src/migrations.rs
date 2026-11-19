use rusqlite::{params, Connection};

use crate::error::StoreError;
use crate::grants::{now_rfc3339, sha256_hex};

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS principals (
    id TEXT PRIMARY KEY,
    handle TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS access_grants (
    grant_id TEXT PRIMARY KEY,
    principal_id TEXT NOT NULL REFERENCES principals(id),
    credential_hash TEXT NOT NULL UNIQUE,
    granted_tags_json TEXT NOT NULL,
    can_write INTEGER NOT NULL,
    can_dispatch INTEGER NOT NULL,
    acked_rules_sha256 TEXT,
    expires_at TEXT,
    revoked_at TEXT,
    model_family TEXT
);

CREATE TABLE IF NOT EXISTS shelves (
    slug TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    required_tag TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS resources (
    id TEXT PRIMARY KEY,
    shelf_slug TEXT NOT NULL REFERENCES shelves(slug),
    slug TEXT NOT NULL,
    project_id TEXT,
    trio_role TEXT,
    lifecycle_tier TEXT NOT NULL,
    triage_state TEXT NOT NULL,
    title TEXT NOT NULL,
    current_rev INTEGER NOT NULL,
    current_sha256 TEXT NOT NULL,
    deleted_at TEXT,
    updated_at TEXT NOT NULL,
    UNIQUE(shelf_slug, slug)
);

CREATE TABLE IF NOT EXISTS resource_revisions (
    resource_id TEXT NOT NULL REFERENCES resources(id),
    rev INTEGER NOT NULL,
    body_md TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    author_principal TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (resource_id, rev)
);

CREATE TABLE IF NOT EXISTS source_passports (
    resource_id TEXT PRIMARY KEY REFERENCES resources(id),
    steward TEXT NOT NULL,
    last_verified_at TEXT NOT NULL,
    freshness_state TEXT NOT NULL,
    lineage_ref TEXT NOT NULL,
    access_scope TEXT NOT NULL,
    next_action TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS trio_projects (
    project_id TEXT PRIMARY KEY,
    create_id TEXT UNIQUE,
    recipe_sha256 TEXT NOT NULL,
    title TEXT NOT NULL,
    work_kind TEXT NOT NULL,
    difficulty INTEGER NOT NULL,
    risk INTEGER NOT NULL,
    profile TEXT NOT NULL,
    expected_roles_json TEXT NOT NULL,
    plan_locked_rev INTEGER,
    state TEXT NOT NULL,
    how_it_works_resource_id TEXT,
    cleanup_verified INTEGER NOT NULL DEFAULT 0,
    lessons_satisfied INTEGER NOT NULL DEFAULT 0,
    github_release INTEGER NOT NULL DEFAULT 0,
    audit_passed_commit TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS own_decisions (
    project_id TEXT NOT NULL REFERENCES trio_projects(project_id),
    own_id TEXT NOT NULL,
    statement TEXT NOT NULL,
    locked_at TEXT,
    approved_by TEXT,
    PRIMARY KEY (project_id, own_id)
);

CREATE TABLE IF NOT EXISTS requirement_rows (
    project_id TEXT NOT NULL REFERENCES trio_projects(project_id),
    req_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    statement TEXT NOT NULL,
    stage TEXT NOT NULL,
    evidence_ref TEXT NOT NULL,
    builder_principal TEXT,
    builder_family TEXT,
    acceptor_principal TEXT,
    acceptor_family TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (project_id, req_id)
);

CREATE TABLE IF NOT EXISTS divergences (
    project_id TEXT NOT NULL REFERENCES trio_projects(project_id),
    div_id TEXT NOT NULL,
    designed TEXT NOT NULL,
    observed TEXT NOT NULL,
    resolution TEXT NOT NULL,
    status TEXT NOT NULL,
    author_principal TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (project_id, div_id)
);

CREATE TABLE IF NOT EXISTS sync_reports (
    report_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES trio_projects(project_id),
    plan_rev INTEGER NOT NULL,
    asbuilt_rev INTEGER NOT NULL,
    paired_rows_json TEXT NOT NULL,
    verdict TEXT NOT NULL,
    acceptor_family TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS conveyor_runs (
    run_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES trio_projects(project_id),
    phase TEXT NOT NULL,
    harness_id TEXT NOT NULL,
    family TEXT NOT NULL,
    input_manifest_sha256 TEXT NOT NULL,
    output_manifest_sha256 TEXT NOT NULL,
    cleanup_clean INTEGER NOT NULL,
    scan_clean INTEGER NOT NULL,
    state TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT
);

CREATE TABLE IF NOT EXISTS audit_events (
    event_id TEXT PRIMARY KEY,
    principal_id TEXT NOT NULL,
    action TEXT NOT NULL,
    target_id TEXT NOT NULL,
    outcome TEXT NOT NULL,
    detail_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS lessons_learned (
    lesson_id TEXT PRIMARY KEY,
    project_id TEXT,
    tags_json TEXT NOT NULL,
    observed_fact TEXT NOT NULL,
    cause_kind TEXT NOT NULL,
    cause_note TEXT NOT NULL,
    forward_rule TEXT NOT NULL,
    validation_state TEXT NOT NULL,
    recorded_by TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_resources_shelf_tier
    ON resources(shelf_slug, lifecycle_tier, triage_state);

CREATE INDEX IF NOT EXISTS idx_resources_project
    ON resources(project_id);

CREATE INDEX IF NOT EXISTS idx_audit_events_created
    ON audit_events(created_at);

CREATE INDEX IF NOT EXISTS idx_lessons_created
    ON lessons_learned(created_at);

CREATE INDEX IF NOT EXISTS idx_lessons_project
    ON lessons_learned(project_id);

CREATE TRIGGER IF NOT EXISTS trg_resource_revisions_no_update
BEFORE UPDATE ON resource_revisions
BEGIN
    SELECT RAISE(ABORT, 'resource_revisions is append-only');
END;

CREATE TRIGGER IF NOT EXISTS trg_resource_revisions_no_delete
BEFORE DELETE ON resource_revisions
BEGIN
    SELECT RAISE(ABORT, 'resource_revisions is append-only');
END;

CREATE TRIGGER IF NOT EXISTS trg_own_decisions_locked_update
BEFORE UPDATE ON own_decisions
WHEN OLD.locked_at IS NOT NULL AND NEW.locked_at IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'OWN_LOCKED');
END;

CREATE TRIGGER IF NOT EXISTS trg_own_decisions_locked_delete
BEFORE DELETE ON own_decisions
WHEN OLD.locked_at IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'OWN_LOCKED');
END;
"#;

const DEFAULT_SHELVES: [(&str, &str, &str); 7] = [
    ("standards", "Standards", "standards"),
    ("runbooks", "Runbooks", "runbooks"),
    ("architecture", "Architecture", "architecture"),
    ("projects", "Projects", "projects"),
    ("skills", "Skills", "skills"),
    ("inbox", "Inbox", "inbox"),
    ("archive", "Archive", "archive"),
];

const DEFAULT_RULES_BODY: &str = "\
# Streamline Governance Rules

1. Acknowledge the active rules digest before executing writes or conveyor dispatches.
2. Keep locked OWN project choices immutable; record divergences instead of editing locked statements.
3. Never persist sensitive credentials, private hostnames, or private LAN addresses in repository or vault documents.
4. Require independent cross-family acceptance for R2+ work and owner approval of HOW-IT-WORKS before release.
";

fn ensure_trio_closeout_columns(conn: &Connection) -> Result<(), StoreError> {
    let mut existing = std::collections::BTreeSet::new();
    let mut stmt = conn.prepare("PRAGMA table_info(trio_projects)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for item in rows {
        existing.insert(item?.to_ascii_lowercase());
    }
    if !existing.contains("cleanup_verified") {
        conn.execute(
            "ALTER TABLE trio_projects ADD COLUMN cleanup_verified INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !existing.contains("lessons_satisfied") {
        conn.execute(
            "ALTER TABLE trio_projects ADD COLUMN lessons_satisfied INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !existing.contains("github_release") {
        conn.execute(
            "ALTER TABLE trio_projects ADD COLUMN github_release INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !existing.contains("audit_passed_commit") {
        conn.execute(
            "ALTER TABLE trio_projects ADD COLUMN audit_passed_commit TEXT",
            [],
        )?;
    }
    Ok(())
}

fn ensure_grant_columns(conn: &Connection) -> Result<(), StoreError> {
    let mut existing = std::collections::BTreeSet::new();
    let mut stmt = conn.prepare("PRAGMA table_info(\"access_grants\")")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for item in rows {
        existing.insert(item?.to_ascii_lowercase());
    }
    if !existing.contains("model_family") {
        conn.execute("ALTER TABLE access_grants ADD COLUMN model_family TEXT", [])?;
    }
    Ok(())
}

/// Runs idempotent schema migrations, triggers, default shelves, and initial rules seeding.
pub(crate) fn run_migrations(conn: &mut Connection) -> Result<(), StoreError> {
    conn.execute_batch(SCHEMA_SQL)?;
    ensure_trio_closeout_columns(conn)?;
    ensure_grant_columns(conn)?;

    let tx = conn.transaction()?;
    for (slug, title, required_tag) in DEFAULT_SHELVES {
        tx.execute(
            "INSERT OR IGNORE INTO shelves (slug, title, required_tag) VALUES (?1, ?2, ?3)",
            params![slug, title, required_tag],
        )?;
    }

    let resource_count: i64 =
        tx.query_row("SELECT COUNT(*) FROM resources", [], |row| row.get(0))?;
    if resource_count == 0 {
        let now = now_rfc3339();
        let rules_sha = sha256_hex(DEFAULT_RULES_BODY.as_bytes());
        let rules_id = "res-standards-rules";

        tx.execute(
            "INSERT INTO resources (
                id, shelf_slug, slug, project_id, trio_role, lifecycle_tier,
                triage_state, title, current_rev, current_sha256, deleted_at, updated_at
            ) VALUES (?1, 'standards', 'rules', NULL, NULL, 'active', 'reviewed', ?2, 1, ?3, NULL, ?4)",
            params![rules_id, "Streamline Governance Rules", rules_sha, now],
        )?;

        tx.execute(
            "INSERT INTO resource_revisions (
                resource_id, rev, body_md, sha256, author_principal, summary, created_at
            ) VALUES (?1, 1, ?2, ?3, 'system', 'Seed default governance rules', ?4)",
            params![rules_id, DEFAULT_RULES_BODY, rules_sha, now],
        )?;

        tx.execute(
            "INSERT INTO source_passports (
                resource_id, steward, last_verified_at, freshness_state,
                lineage_ref, access_scope, next_action
            ) VALUES (?1, 'owner', ?2, 'verified', 'seed:v1', 'standards', 'none')",
            params![rules_id, now],
        )?;
    }

    tx.commit()?;
    Ok(())
}
