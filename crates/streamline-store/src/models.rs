use serde::{Deserialize, Serialize};
use streamline_core::{
    AssessmentInput, Divergence, DivergenceStatus, FreshnessState, LifecycleTier, OwnDecision,
    PairedSyncRow, PrincipalRole, ReqStage, RequirementRow, RunState, SourcePassport, SyncVerdict,
    TriageState, TrioProfile, WorkKind,
};

/// Authenticated identity record.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Principal {
    pub id: String,
    pub handle: String,
    pub role: PrincipalRole,
    pub created_at: String,
}

/// Request payload for creating a tag-scoped access grant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CreateGrantRequest {
    pub grant_id: Option<String>,
    pub principal_id: String,
    pub raw_credential: String,
    pub granted_tags: Vec<String>,
    pub can_write: bool,
    pub can_dispatch: bool,
    pub expires_at: Option<String>,
    #[serde(default)]
    pub model_family: Option<String>,
}

/// Public representation of an access grant (never exposes raw credentials or hashes).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccessGrant {
    pub grant_id: String,
    pub principal_id: String,
    pub principal_handle: String,
    pub principal_role: PrincipalRole,
    pub granted_tags: Vec<String>,
    pub can_write: bool,
    pub can_dispatch: bool,
    pub acked_rules_sha256: Option<String>,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    #[serde(default)]
    pub model_family: Option<String>,
}

/// Authorized shelf summary with item and freshness counts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ShelfSummary {
    pub slug: String,
    pub title: String,
    pub required_tag: String,
    pub total_count: usize,
    pub active_count: usize,
    pub verified_count: usize,
    pub stale_count: usize,
    pub drift_count: usize,
    pub unknown_count: usize,
}

/// Resource metadata record stored in the `resources` table.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ResourceRecord {
    pub id: String,
    pub shelf_slug: String,
    pub slug: String,
    pub project_id: Option<String>,
    pub trio_role: Option<String>,
    pub lifecycle_tier: LifecycleTier,
    pub triage_state: TriageState,
    pub title: String,
    pub current_rev: i64,
    pub current_sha256: String,
    pub deleted_at: Option<String>,
    pub updated_at: String,
}

/// Append-only revision entry stored in `resource_revisions`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ResourceRevision {
    pub resource_id: String,
    pub rev: i64,
    pub body_md: String,
    pub sha256: String,
    pub author_principal: String,
    pub summary: String,
    pub created_at: String,
}

/// Full resource view combining metadata, markdown body, and optional Source Passport.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ResourceWithPassport {
    pub resource: ResourceRecord,
    pub body_md: String,
    pub passport: Option<SourcePassport>,
}

/// Filter parameters for listing or searching resources.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ResourceFilter {
    pub shelf_slug: Option<String>,
    pub lifecycle_tier: Option<LifecycleTier>,
    pub triage_state: Option<TriageState>,
    pub project_id: Option<String>,
    pub freshness_state: Option<FreshnessState>,
    #[serde(default)]
    pub include_deleted: bool,
}

/// Search match containing the resource, snippet, and optional Source Passport.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ResourceSearchHit {
    pub resource: ResourceRecord,
    pub snippet: String,
    pub passport: Option<SourcePassport>,
}

/// Request payload for creating or updating a resource as a new append-only revision.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SaveResourceRequest {
    pub shelf_slug: String,
    pub slug: String,
    pub title: String,
    pub body_md: String,
    pub project_id: Option<String>,
    pub trio_role: Option<String>,
    pub lifecycle_tier: Option<LifecycleTier>,
    pub triage_state: Option<TriageState>,
    pub summary: String,
    pub expected_rev: Option<i64>,
}

/// Central forward-learning record stored in `lessons_learned`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LessonRecord {
    pub lesson_id: String,
    pub project_id: Option<String>,
    pub tags: Vec<String>,
    pub observed_fact: String,
    pub cause_kind: String,
    pub cause_note: String,
    pub forward_rule: String,
    pub validation_state: String,
    pub recorded_by: String,
    pub created_at: String,
}

/// Request payload for recording a forward lesson into the central learning database.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecordLessonRequest {
    pub lesson_id: Option<String>,
    pub project_id: Option<String>,
    pub tags: Vec<String>,
    pub observed_fact: String,
    pub cause_kind: String,
    pub cause_note: String,
    pub forward_rule: String,
    pub validation_state: Option<String>,
}

/// Active-only bootstrap snapshot returned to connecting clients.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BootstrapView {
    pub rules_sha256: String,
    pub rules_acked: bool,
    pub granted_tags: Vec<String>,
    pub required_onboarding_slugs: Vec<String>,
    pub active_resources: Vec<ResourceRecord>,
    pub active_projects: Vec<TrioProject>,
    #[serde(default)]
    pub forward_lessons: Vec<LessonRecord>,
}

/// Single line inside a revision diff.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", content = "text", rename_all = "snake_case")]
pub enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}

/// Single word change inside a revision diff.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", content = "word", rename_all = "snake_case")]
pub enum WordChange {
    Context(String),
    Added(String),
    Removed(String),
}

/// Structured line and word comparison between two revisions or texts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RevisionDiffSummary {
    pub left_rev: i64,
    pub right_rev: i64,
    pub left_sha256: String,
    pub right_sha256: String,
    pub unchanged_lines: usize,
    pub added_lines: Vec<String>,
    pub removed_lines: Vec<String>,
    pub line_hunks: Vec<DiffLine>,
    pub changed_words: Vec<WordChange>,
    pub summary: String,
}

/// Initial OWN decision supplied when creating a Trio+ project.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnDecisionInput {
    pub own_id: String,
    pub statement: String,
}

/// Initial requirement trace row supplied when creating a Trio+ project.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RequirementRowInput {
    pub req_id: String,
    pub source_id: String,
    pub statement: String,
}

/// Idempotent Trio+ project creation request.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CreateTrioProjectRequest {
    pub project_id: String,
    pub create_id: Option<String>,
    pub title: String,
    pub assessment: AssessmentInput,
    #[serde(default)]
    pub opt_in_modules: Vec<String>,
    #[serde(default)]
    pub initial_own_decisions: Vec<OwnDecisionInput>,
    #[serde(default)]
    pub initial_requirements: Vec<RequirementRowInput>,
    #[serde(default)]
    pub github_release: bool,
}

/// Trio+ project header record.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TrioProject {
    pub project_id: String,
    pub create_id: Option<String>,
    pub title: String,
    pub work_kind: WorkKind,
    pub difficulty: u8,
    pub risk: u8,
    pub profile: TrioProfile,
    pub expected_roles: Vec<String>,
    pub plan_locked_rev: Option<i64>,
    pub state: RunState,
    pub how_it_works_resource_id: Option<String>,
    #[serde(default)]
    pub cleanup_verified: bool,
    #[serde(default)]
    pub lessons_satisfied: bool,
    #[serde(default)]
    pub github_release: bool,
    #[serde(default)]
    pub audit_passed_commit: Option<String>,
    pub created_at: String,
}

/// Persisted cross-model sync report.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SyncReport {
    pub report_id: String,
    pub project_id: String,
    pub plan_rev: i64,
    pub asbuilt_rev: i64,
    pub paired_rows: Vec<PairedSyncRow>,
    pub verdict: SyncVerdict,
    pub acceptor_family: String,
    pub created_at: String,
}

/// Complete Trio+ project bundle returned by create/get/approve flows.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TrioProjectBundle {
    pub project: TrioProject,
    pub own_decisions: Vec<OwnDecision>,
    pub requirements: Vec<RequirementRow>,
    pub divergences: Vec<Divergence>,
    pub resources: Vec<ResourceRecord>,
    pub sync_reports: Vec<SyncReport>,
}

/// Request to append a DIV entry to a Trio+ project.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecordDivergenceRequest {
    pub project_id: String,
    pub div_id: Option<String>,
    pub designed: String,
    pub observed: String,
    pub resolution: String,
    pub status: DivergenceStatus,
}

/// Request to advance a requirement row through `D I T A L`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UpdateRequirementStageRequest {
    pub project_id: String,
    pub req_id: String,
    pub target_stage: ReqStage,
    pub evidence_ref: String,
    pub actor_family: Option<String>,
    #[serde(default)]
    pub actor_is_custodian: bool,
}

/// Request to save a validated `check_sync` report.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SaveSyncReportRequest {
    pub project_id: String,
    pub plan_rev: i64,
    pub asbuilt_rev: i64,
    pub paired_rows: Vec<PairedSyncRow>,
    pub verdict: SyncVerdict,
    pub acceptor_family: String,
}

/// Explicit concurrent conflict recorded in `inbox` during two-way vault sync.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct VaultConflictRecord {
    pub shelf_slug: String,
    pub slug: String,
    pub conflict_resource_id: String,
    pub conflict_slug: String,
    pub db_rev: i64,
    pub db_sha256: String,
    pub disk_sha256: String,
}

/// Outcome summary of a two-way Obsidian vault sync run.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct VaultSyncSummary {
    pub exported: usize,
    pub imported: usize,
    pub unchanged: usize,
    pub conflicts: Vec<VaultConflictRecord>,
}

/// Outcome summary of a read-only legacy SQLite catalog import.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LegacyImportSummary {
    pub detected_tables: Vec<String>,
    pub imported_count: usize,
    pub skipped_noise_count: usize,
    pub skipped_duplicate_count: usize,
    pub skipped_tables: Vec<String>,
    pub imported_resource_ids: Vec<String>,
}
