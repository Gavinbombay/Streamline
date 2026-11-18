use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use streamline_conveyor::{
    append_trajectory_jsonl, execute_conveyor_blocking, ConveyorError, DispatchOutcome,
    DispatchRequest, TrajectoryExportInput,
};
use streamline_core::{
    assess_work_with_modules, check_sync, AssessmentInput, DivergenceStatus, FreshnessState,
    LifecycleTier, PrincipalRole, SourcePassport, TriageState, WorkKind,
};
use streamline_safe::inspect_text;
use streamline_store::{
    compare_texts, AccessGrant, CreateTrioProjectRequest, OwnDecisionInput,
    RecordDivergenceRequest, RecordLessonRequest, RequirementRowInput, ResourceFilter,
    SaveResourceRequest, StoreError, TrioProjectBundle, UpdateRequirementStageRequest,
};

use crate::error::ApiError;
use crate::redaction::{
    check_optional_safe_segment, check_optional_safe_text, check_safe_segment, check_safe_text,
};
use crate::state::AppState;
use crate::zip_skills::load_bundled_skills;

/// JSON Schema descriptor emitted by `tools/list`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDescriptor {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// Health payload shared between `GET /ready` and the `ready` MCP/REST tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadyStatus {
    pub ok: bool,
    pub status: String,
    pub version: String,
    pub public_safe_mode: bool,
    pub current_rules_sha256: String,
}

pub const KNOWN_TOOLS: [&str; 42] = [
    "ready",
    "bootstrap",
    "ack_rules",
    "get_workspace",
    "list_shelves",
    "list_resources",
    "read_resource",
    "search_resources",
    "resource_history",
    "compare_resources",
    "get_source_passport",
    "update_source_passport",
    "set_lifecycle_tier",
    "list_skills",
    "select_context",
    "assess_trio_work",
    "create_trio_project",
    "list_trio_projects",
    "get_project_bundle",
    "save_resource",
    "save_project_documents",
    "restore_resource",
    "lock_project_plan",
    "upsert_own_decision",
    "record_divergence",
    "upsert_requirement_row",
    "update_requirement_stage",
    "check_sync",
    "save_sync_report",
    "get_sync_report",
    "dispatch_run",
    "get_run_status",
    "list_conveyor_runs",
    "mark_project_cleanup_verified",
    "record_lesson",
    "record_no_new_lessons",
    "query_lessons",
    "submit_how_it_works",
    "approve_release",
    "export_trajectory",
    "sync_vault",
    "export_workspace",
];

/// Returns `true` when `tool_name` is a registered Streamline MCP/REST tool.
pub fn is_known_tool(tool_name: &str) -> bool {
    KNOWN_TOOLS.contains(&tool_name)
}

/// Computes the unauthenticated `/ready` health status from the store and config.
pub fn compute_ready_status(state: &AppState) -> Result<ReadyStatus, ApiError> {
    let current_rules_sha256 = state.store.current_rules_sha256()?;
    Ok(ReadyStatus {
        ok: true,
        status: "ready".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        public_safe_mode: state.config.public_safe_mode,
        current_rules_sha256,
    })
}

/// Returns MCP `tools/list` metadata and JSON Schemas for all 42 tools.
pub fn tools_list() -> Vec<McpToolDescriptor> {
    vec![
        McpToolDescriptor {
            name: "ready",
            description: "Return server readiness, version, public_safe_mode, and current_rules_sha256.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        McpToolDescriptor {
            name: "bootstrap",
            description: "Return active governance rules digest, onboarding slugs, active resources, active projects, and forward lessons.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        McpToolDescriptor {
            name: "ack_rules",
            description: "Acknowledge the active standards/rules SHA-256 digest to unlock write tools for this grant.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "rules_sha256": { "type": "string", "description": "Active SHA-256 digest of standards/rules." }
                },
                "required": ["rules_sha256"]
            }),
        },
        McpToolDescriptor {
            name: "get_workspace",
            description: "Return caller principal permissions, active shelves, projects, and rule-gate status.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        McpToolDescriptor {
            name: "list_shelves",
            description: "List authorized shelves with item counts and Source Passport freshness counts.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        McpToolDescriptor {
            name: "list_resources",
            description: "List resources filtered by shelf, lifecycle tier, triage state, project_id, or freshness state.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shelf_slug": { "type": "string" },
                    "lifecycle_tier": { "type": "string", "enum": ["active", "dormant", "legacy", "retired"] },
                    "triage_state": { "type": "string", "enum": ["reviewed", "needs_triage"] },
                    "project_id": { "type": "string" },
                    "freshness_state": { "type": "string", "enum": ["verified", "stale", "drift", "unknown"] },
                    "include_deleted": { "type": "boolean" }
                }
            }),
        },
        McpToolDescriptor {
            name: "read_resource",
            description: "Read a single resource by resource_id or (shelf_slug, slug) including body and Source Passport.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "resource_id": { "type": "string" },
                    "shelf_slug": { "type": "string" },
                    "slug": { "type": "string" }
                }
            }),
        },
        McpToolDescriptor {
            name: "search_resources",
            description: "Full-text search across authorized shelves with optional ResourceFilter fields.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "shelf_slug": { "type": "string" },
                    "lifecycle_tier": { "type": "string" },
                    "triage_state": { "type": "string" },
                    "project_id": { "type": "string" },
                    "freshness_state": { "type": "string" }
                },
                "required": ["query"]
            }),
        },
        McpToolDescriptor {
            name: "resource_history",
            description: "Return append-only revision history for a resource identified by (shelf_slug, slug).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shelf_slug": { "type": "string" },
                    "slug": { "type": "string" }
                },
                "required": ["shelf_slug", "slug"]
            }),
        },
        McpToolDescriptor {
            name: "compare_resources",
            description: "Compute a structured line and word diff between two revisions of one resource or between two distinct resources.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shelf_slug": { "type": "string" },
                    "slug": { "type": "string" },
                    "left_rev": { "type": "integer" },
                    "right_rev": { "type": "integer" },
                    "left_shelf_slug": { "type": "string" },
                    "left_slug": { "type": "string" },
                    "right_shelf_slug": { "type": "string" },
                    "right_slug": { "type": "string" },
                    "left_resource_id": { "type": "string" },
                    "right_resource_id": { "type": "string" }
                }
            }),
        },
        McpToolDescriptor {
            name: "get_source_passport",
            description: "Read the Source Passport for a resource by resource_id or (shelf_slug, slug).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "resource_id": { "type": "string" },
                    "shelf_slug": { "type": "string" },
                    "slug": { "type": "string" }
                }
            }),
        },
        McpToolDescriptor {
            name: "update_source_passport",
            description: "Create or update Source Passport verification metadata for a resource.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "resource_id": { "type": "string" },
                    "shelf_slug": { "type": "string" },
                    "slug": { "type": "string" },
                    "steward": { "type": "string" },
                    "last_verified_at": { "type": "string" },
                    "freshness_state": { "type": "string", "enum": ["verified", "stale", "drift", "unknown"] },
                    "lineage_ref": { "type": "string" },
                    "access_scope": { "type": "string" },
                    "next_action": { "type": "string" }
                },
                "required": ["steward", "freshness_state", "lineage_ref", "next_action"]
            }),
        },
        McpToolDescriptor {
            name: "set_lifecycle_tier",
            description: "Update lifecycle_tier, triage_state, or linked project_id on a resource.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shelf_slug": { "type": "string" },
                    "slug": { "type": "string" },
                    "lifecycle_tier": { "type": "string", "enum": ["active", "dormant", "legacy", "retired"] },
                    "triage_state": { "type": "string", "enum": ["reviewed", "needs_triage"] },
                    "target_project_id": { "type": "string" }
                },
                "required": ["shelf_slug", "slug", "lifecycle_tier"]
            }),
        },
        McpToolDescriptor {
            name: "list_skills",
            description: "List bundled portable Streamline skills and authorized resources in the skills shelf.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        McpToolDescriptor {
            name: "select_context",
            description: "Select profile, unmerged role skills, active standards, and relevant forward lessons for a task.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "role": { "type": "string" },
                    "work_kind": { "type": "string", "enum": ["software", "council", "procedure", "general"] },
                    "project_id": { "type": "string" },
                    "tag_filter": { "type": "string" },
                    "assessment": { "type": "object" },
                    "opt_in_modules": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },
        McpToolDescriptor {
            name: "assess_trio_work",
            description: "Compute D0..D4 difficulty, R0..R4 risk, Trio+ profile P0..P4, and expected document roles.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "assessment": { "type": "object" },
                    "opt_in_modules": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },
        McpToolDescriptor {
            name: "create_trio_project",
            description: "Idempotently create a P0..P4 Trio+ project bundle with initial OWN decisions and requirements.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "create_id": { "type": "string" },
                    "request_id": { "type": "string" },
                    "title": { "type": "string" },
                    "assessment": { "type": "object" },
                    "opt_in_modules": { "type": "array", "items": { "type": "string" } },
                    "initial_own_decisions": { "type": "array", "items": { "type": "object" } },
                    "initial_requirements": { "type": "array", "items": { "type": "object" } }
                },
                "required": ["project_id", "title", "assessment"]
            }),
        },
        McpToolDescriptor {
            name: "list_trio_projects",
            description: "List Trio+ projects visible to the caller's granted tags.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        McpToolDescriptor {
            name: "get_project_bundle",
            description: "Load full Trio+ project bundle (project, OWN decisions, requirements, divergences, resources, sync reports).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" }
                },
                "required": ["project_id"]
            }),
        },
        McpToolDescriptor {
            name: "save_resource",
            description: "Save a resource as an append-only revision after redaction scanning, rule-gate check, and tag check.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shelf_slug": { "type": "string" },
                    "slug": { "type": "string" },
                    "title": { "type": "string" },
                    "body_md": { "type": "string" },
                    "project_id": { "type": "string" },
                    "trio_role": { "type": "string" },
                    "lifecycle_tier": { "type": "string", "enum": ["active", "dormant", "legacy", "retired"] },
                    "triage_state": { "type": "string", "enum": ["reviewed", "needs_triage"] },
                    "summary": { "type": "string" },
                    "expected_rev": { "type": "integer" }
                },
                "required": ["shelf_slug", "slug", "title", "body_md"]
            }),
        },
        McpToolDescriptor {
            name: "save_project_documents",
            description: "Save one or more Trio+ project role documents while enforcing redaction and OWN lock guards.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "documents": { "type": "array", "items": { "type": "object" } },
                    "own_decisions": { "type": "array", "items": { "type": "object" } }
                },
                "required": ["project_id"]
            }),
        },
        McpToolDescriptor {
            name: "restore_resource",
            description: "Restore a prior revision of a resource by appending a new revision with that content.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shelf_slug": { "type": "string" },
                    "slug": { "type": "string" },
                    "target_rev": { "type": "integer" },
                    "summary": { "type": "string" }
                },
                "required": ["shelf_slug", "slug", "target_rev"]
            }),
        },
        McpToolDescriptor {
            name: "lock_project_plan",
            description: "Freeze plan_locked_rev on a Trio+ project and lock all OWN decision rows.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "plan_rev": { "type": "integer" }
                },
                "required": ["project_id"]
            }),
        },
        McpToolDescriptor {
            name: "upsert_own_decision",
            description: "Insert or update an OWN decision prior to Plan Lock (returns 409 OWN_LOCKED once locked).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "own_id": { "type": "string" },
                    "statement": { "type": "string" },
                    "approved_by": { "type": "string" }
                },
                "required": ["project_id", "own_id", "statement"]
            }),
        },
        McpToolDescriptor {
            name: "record_divergence",
            description: "Append a DIV-nnn divergence entry without modifying locked OWN decisions.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "div_id": { "type": "string" },
                    "designed": { "type": "string" },
                    "observed": { "type": "string" },
                    "resolution": { "type": "string" },
                    "status": { "type": "string", "enum": ["open", "accepted", "fixed"] }
                },
                "required": ["project_id", "designed", "observed", "resolution"]
            }),
        },
        McpToolDescriptor {
            name: "upsert_requirement_row",
            description: "Create or update a Trio+ requirement definition row (starting at stage D).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "req_id": { "type": "string" },
                    "source_id": { "type": "string" },
                    "statement": { "type": "string" }
                },
                "required": ["project_id", "req_id", "source_id", "statement"]
            }),
        },
        McpToolDescriptor {
            name: "update_requirement_stage",
            description: "Advance a requirement row through D -> I -> T -> A -> L with evidence and cross-family R2+ checks.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "req_id": { "type": "string" },
                    "target_stage": { "type": "string", "enum": ["D", "I", "T", "A", "L", "UNKNOWN"] },
                    "evidence_ref": { "type": "string" },
                    "actor_family": { "type": "string" },
                    "actor_is_custodian": { "type": "boolean" }
                },
                "required": ["project_id", "req_id", "target_stage", "evidence_ref"]
            }),
        },
        McpToolDescriptor {
            name: "check_sync",
            description: "Deterministically pair locked OWN/REQ rows against AS-BUILT markdown claims.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "as_built_markdown": { "type": "string" },
                    "as_built_slug": { "type": "string" }
                },
                "required": ["project_id"]
            }),
        },
        McpToolDescriptor {
            name: "save_sync_report",
            description: "Compute and persist a server-verified cross-model check_sync verdict and paired row report.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "asbuilt_slug": { "type": "string" }
                },
                "required": ["project_id"]
            }),
        },
        McpToolDescriptor {
            name: "get_sync_report",
            description: "Read a specific sync report by report_id or the latest sync report for a project.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "report_id": { "type": "string" }
                },
                "required": ["project_id"]
            }),
        },
        McpToolDescriptor {
            name: "dispatch_run",
            description: "Stage inputs and execute a hands-off conveyor build (plus cross-family accept for R2+) on configured harnesses.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "input_paths": { "type": "array", "items": { "type": "string" } },
                    "preferred_builder_harness": { "type": "string" },
                    "preferred_acceptor_harness": { "type": "string" },
                    "risk_band": { "type": "integer", "minimum": 0, "maximum": 4 }
                },
                "required": ["project_id"]
            }),
        },
        McpToolDescriptor {
            name: "get_run_status",
            description: "Return phase, manifests, cleanup/scan booleans, and state for a conveyor run.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" }
                },
                "required": ["run_id"]
            }),
        },
        McpToolDescriptor {
            name: "list_conveyor_runs",
            description: "List recorded conveyor build and acceptance runs for a Trio+ project.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" }
                },
                "required": ["project_id"]
            }),
        },
        McpToolDescriptor {
            name: "mark_project_cleanup_verified",
            description: "Part 1 of closeout gate: record verified cleanup of temporary, build, and scratch files for a project.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "detail": { "type": "string" }
                },
                "required": ["project_id"]
            }),
        },
        McpToolDescriptor {
            name: "record_lesson",
            description: "Part 2 of closeout gate: persist a forward lesson into the central lessons_learned database.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "lesson_id": { "type": "string" },
                    "project_id": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "observed_fact": { "type": "string" },
                    "cause_kind": { "type": "string", "enum": ["demonstrated", "hypothesis"] },
                    "cause_note": { "type": "string" },
                    "forward_rule": { "type": "string" },
                    "validation_state": { "type": "string", "enum": ["worked_once", "verified_reusable"] }
                },
                "required": ["observed_fact", "cause_kind", "forward_rule"]
            }),
        },
        McpToolDescriptor {
            name: "record_no_new_lessons",
            description: "Explicitly waive new central lessons for a project with a recorded reason.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "required": ["project_id", "reason"]
            }),
        },
        McpToolDescriptor {
            name: "query_lessons",
            description: "Query central forward lessons filtered by tag or project_id.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "tag_filter": { "type": "string" },
                    "project_filter": { "type": "string" },
                    "limit": { "type": "integer" }
                }
            }),
        },
        McpToolDescriptor {
            name: "submit_how_it_works",
            description: "Part 3 of closeout gate: save plain-English HOW-IT-WORKS.md and transition project to DOC_PENDING_OWNER.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "markdown": { "type": "string" },
                    "title": { "type": "string" }
                },
                "required": ["project_id", "markdown"]
            }),
        },
        McpToolDescriptor {
            name: "approve_release",
            description: "Owner-only release approval: transitions DOC_PENDING_OWNER -> DONE and promotes A rows to L.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" }
                },
                "required": ["project_id"]
            }),
        },
        McpToolDescriptor {
            name: "export_trajectory",
            description: "Owner-only action: append a redacted JSONL trajectory summary for a Trio+ project to trajectories.jsonl.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" }
                },
                "required": ["project_id"]
            }),
        },
        McpToolDescriptor {
            name: "sync_vault",
            description: "Execute two-way Obsidian markdown sync on the configured vault_sync_dir.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        McpToolDescriptor {
            name: "export_workspace",
            description: "Export a redacted, tag-scoped portable JSON archive of shelves, projects, and lessons.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "include_bodies": { "type": "boolean" }
                }
            }),
        },
    ]
}

fn normalize_args(raw: Value) -> Value {
    if raw.is_null() {
        json!({})
    } else {
        raw
    }
}

fn parse_args<T: for<'de> Deserialize<'de>>(raw: Value) -> Result<T, ApiError> {
    serde_json::from_value(normalize_args(raw)).map_err(|err| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "UNPROCESSABLE",
            format!("invalid tool arguments: {err}"),
        )
    })
}

fn require_grant(grant: Option<&AccessGrant>) -> Result<&AccessGrant, ApiError> {
    grant.ok_or_else(|| {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "missing authenticated access grant",
        )
    })
}

/// Executes a named Streamline tool after enforcing auth, redaction, and store rules.
pub fn dispatch_tool(
    state: &AppState,
    grant: Option<&AccessGrant>,
    tool_name: &str,
    raw_args: Value,
) -> Result<Value, ApiError> {
    match tool_name {
        "ready" => Ok(serde_json::to_value(compute_ready_status(state)?)?),
        "bootstrap" => {
            let active_grant = require_grant(grant)?;
            let view = state.store.bootstrap_view(&active_grant.grant_id)?;
            Ok(serde_json::to_value(view)?)
        }
        "ack_rules" => {
            let active_grant = require_grant(grant)?;
            let args: AckRulesArgs = parse_args(raw_args)?;
            let updated = state
                .store
                .ack_rules(&active_grant.grant_id, &args.rules_sha256)?;
            Ok(serde_json::to_value(updated)?)
        }
        "get_workspace" => {
            let active_grant = require_grant(grant)?;
            let rules_sha256 = state.store.current_rules_sha256()?;
            let rules_acked =
                active_grant.acked_rules_sha256.as_deref() == Some(rules_sha256.as_str());
            let shelves = state.store.list_shelves(&active_grant.grant_id)?;
            let projects = state.store.list_trio_projects(&active_grant.grant_id)?;
            Ok(json!({
                "principal_id": active_grant.principal_id,
                "principal_handle": active_grant.principal_handle,
                "principal_role": active_grant.principal_role,
                "granted_tags": active_grant.granted_tags,
                "can_write": active_grant.can_write,
                "can_dispatch": active_grant.can_dispatch,
                "rules_sha256": rules_sha256,
                "rules_acked": rules_acked,
                "public_safe_mode": state.config.public_safe_mode,
                "shelves": shelves,
                "projects": projects,
                "skills_download_path": "/downloads/streamline-skills.zip",
            }))
        }
        "list_shelves" => {
            let active_grant = require_grant(grant)?;
            let shelves = state.store.list_shelves(&active_grant.grant_id)?;
            Ok(serde_json::to_value(shelves)?)
        }
        "list_resources" => {
            let active_grant = require_grant(grant)?;
            let filter: ResourceFilter = parse_args(raw_args)?;
            check_optional_safe_segment("shelf_slug", filter.shelf_slug.as_deref())?;
            check_optional_safe_segment("project_id", filter.project_id.as_deref())?;
            let rows = state
                .store
                .list_resources(&active_grant.grant_id, &filter)?;
            Ok(serde_json::to_value(rows)?)
        }
        "read_resource" => {
            let active_grant = require_grant(grant)?;
            let args: ReadResourceArgs = parse_args(raw_args)?;
            let res = if let Some(ref resource_id) = args.resource_id {
                check_safe_segment("resource_id", resource_id)?;
                state
                    .store
                    .read_resource_by_id(&active_grant.grant_id, resource_id)?
            } else if let (Some(ref shelf_slug), Some(ref slug)) = (&args.shelf_slug, &args.slug) {
                check_safe_segment("shelf_slug", shelf_slug)?;
                check_safe_segment("slug", slug)?;
                state
                    .store
                    .read_resource(&active_grant.grant_id, shelf_slug, slug)?
            } else {
                return Err(ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "UNPROCESSABLE",
                    "provide either resource_id or both shelf_slug and slug",
                ));
            };
            Ok(serde_json::to_value(res)?)
        }
        "search_resources" => {
            let active_grant = require_grant(grant)?;
            let args: SearchResourcesArgs = parse_args(raw_args)?;
            check_optional_safe_segment("shelf_slug", args.filter.shelf_slug.as_deref())?;
            check_optional_safe_segment("project_id", args.filter.project_id.as_deref())?;
            let hits =
                state
                    .store
                    .search_resources(&active_grant.grant_id, &args.query, &args.filter)?;
            Ok(serde_json::to_value(hits)?)
        }
        "resource_history" => {
            let active_grant = require_grant(grant)?;
            let args: ResourceCoordArgs = parse_args(raw_args)?;
            check_safe_segment("shelf_slug", &args.shelf_slug)?;
            check_safe_segment("slug", &args.slug)?;
            let history = state.store.resource_history(
                &active_grant.grant_id,
                &args.shelf_slug,
                &args.slug,
            )?;
            Ok(serde_json::to_value(history)?)
        }
        "compare_resources" => {
            let active_grant = require_grant(grant)?;
            let args: CompareResourcesArgs = parse_args(raw_args)?;
            let diff = handle_compare_resources(state, &active_grant.grant_id, args)?;
            Ok(serde_json::to_value(diff)?)
        }
        "get_source_passport" => {
            let active_grant = require_grant(grant)?;
            let args: ReadResourceArgs = parse_args(raw_args)?;
            let passport = if let Some(ref resource_id) = args.resource_id {
                check_safe_segment("resource_id", resource_id)?;
                state
                    .store
                    .get_source_passport(&active_grant.grant_id, resource_id)?
            } else if let (Some(ref shelf_slug), Some(ref slug)) = (&args.shelf_slug, &args.slug) {
                check_safe_segment("shelf_slug", shelf_slug)?;
                check_safe_segment("slug", slug)?;
                state
                    .store
                    .read_resource(&active_grant.grant_id, shelf_slug, slug)?
                    .passport
            } else {
                return Err(ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "UNPROCESSABLE",
                    "provide either resource_id or both shelf_slug and slug",
                ));
            };
            Ok(serde_json::to_value(passport)?)
        }
        "update_source_passport" => {
            let active_grant = require_grant(grant)?;
            let args: UpdateSourcePassportArgs = parse_args(raw_args)?;
            let (resource_id, fallback_scope) = if let Some(ref rid) = args.resource_id {
                check_safe_segment("resource_id", rid)?;
                (rid.clone(), String::new())
            } else if let (Some(ref shelf), Some(ref slug)) = (&args.shelf_slug, &args.slug) {
                check_safe_segment("shelf_slug", shelf)?;
                check_safe_segment("slug", slug)?;
                let loaded = state
                    .store
                    .read_resource(&active_grant.grant_id, shelf, slug)?;
                (loaded.resource.id, loaded.resource.shelf_slug)
            } else {
                return Err(ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "UNPROCESSABLE",
                    "provide either resource_id or both shelf_slug and slug",
                ));
            };

            let verified_at = args
                .last_verified_at
                .unwrap_or_else(|| Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
            let access_scope = args.access_scope.unwrap_or_else(|| {
                if fallback_scope.is_empty() {
                    "authorized".to_string()
                } else {
                    fallback_scope
                }
            });

            check_safe_text("steward", &args.steward, &state.redaction_policy)?;
            check_safe_text("last_verified_at", &verified_at, &state.redaction_policy)?;
            check_safe_text("lineage_ref", &args.lineage_ref, &state.redaction_policy)?;
            check_safe_text("access_scope", &access_scope, &state.redaction_policy)?;
            check_safe_text("next_action", &args.next_action, &state.redaction_policy)?;

            let saved = state.store.upsert_source_passport(
                &active_grant.grant_id,
                SourcePassport {
                    resource_id,
                    steward: args.steward,
                    last_verified_at: verified_at,
                    freshness_state: args.freshness_state,
                    lineage_ref: args.lineage_ref,
                    access_scope,
                    next_action: args.next_action,
                },
            )?;
            Ok(serde_json::to_value(saved)?)
        }
        "set_lifecycle_tier" => {
            let active_grant = require_grant(grant)?;
            let args: SetLifecycleTierArgs = parse_args(raw_args)?;
            check_safe_segment("shelf_slug", &args.shelf_slug)?;
            check_safe_segment("slug", &args.slug)?;
            check_optional_safe_segment("target_project_id", args.target_project_id.as_deref())?;
            let updated = state.store.set_lifecycle_tier(
                &active_grant.grant_id,
                &args.shelf_slug,
                &args.slug,
                args.lifecycle_tier,
                args.triage_state,
                args.target_project_id,
            )?;
            Ok(serde_json::to_value(updated)?)
        }
        "list_skills" => {
            let active_grant = require_grant(grant)?;
            let bundled_skills = load_bundled_skills(&state.skills_dir);
            let shelf_skills = state
                .store
                .list_resources(
                    &active_grant.grant_id,
                    &ResourceFilter {
                        shelf_slug: Some("skills".to_string()),
                        ..Default::default()
                    },
                )
                .unwrap_or_default();
            Ok(json!({
                "download_path": "/downloads/streamline-skills.zip",
                "bundled_skills": bundled_skills,
                "shelf_skills": shelf_skills,
            }))
        }
        "select_context" => {
            let active_grant = require_grant(grant)?;
            let args: SelectContextArgs = parse_args(raw_args)?;
            handle_select_context(state, active_grant, args)
        }
        "assess_trio_work" => {
            let _active_grant = require_grant(grant)?;
            let normalized = normalize_args(raw_args);
            let (assessment, opt_in_modules) = if normalized.get("assessment").is_some() {
                let nested: NestedAssessArgs = parse_args(normalized)?;
                (nested.assessment, nested.opt_in_modules)
            } else {
                let flat: FlatAssessArgs = parse_args(normalized)?;
                (flat.assessment, flat.opt_in_modules)
            };
            let outcome = assess_work_with_modules(&assessment, &opt_in_modules);
            Ok(serde_json::to_value(outcome)?)
        }
        "create_trio_project" => {
            let active_grant = require_grant(grant)?;
            let args: CreateTrioProjectArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            let create_id = args.create_id.filter(|val| !val.trim().is_empty());
            check_optional_safe_text("create_id", create_id.as_deref(), &state.redaction_policy)?;
            check_safe_text("title", &args.title, &state.redaction_policy)?;
            for module_name in &args.opt_in_modules {
                check_safe_segment("opt_in_modules", module_name)?;
            }
            for own in &args.initial_own_decisions {
                check_safe_text("own_id", &own.own_id, &state.redaction_policy)?;
                check_safe_text("own_statement", &own.statement, &state.redaction_policy)?;
            }
            for row in &args.initial_requirements {
                check_safe_text("req_id", &row.req_id, &state.redaction_policy)?;
                check_safe_text("source_id", &row.source_id, &state.redaction_policy)?;
                check_safe_text("req_statement", &row.statement, &state.redaction_policy)?;
            }

            let req = CreateTrioProjectRequest {
                project_id: args.project_id,
                create_id,
                title: args.title,
                assessment: args.assessment,
                opt_in_modules: args.opt_in_modules,
                initial_own_decisions: args.initial_own_decisions,
                initial_requirements: args.initial_requirements,
            };
            let bundle = state
                .store
                .create_trio_project(&active_grant.grant_id, req)?;
            Ok(serde_json::to_value(bundle)?)
        }
        "list_trio_projects" => {
            let active_grant = require_grant(grant)?;
            let projects = state.store.list_trio_projects(&active_grant.grant_id)?;
            Ok(serde_json::to_value(projects)?)
        }
        "get_project_bundle" => {
            let active_grant = require_grant(grant)?;
            let args: ProjectIdArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            let bundle = state
                .store
                .get_project_bundle(&active_grant.grant_id, &args.project_id)?;
            Ok(serde_json::to_value(bundle)?)
        }
        "save_resource" => {
            let active_grant = require_grant(grant)?;
            let args: SaveResourceArgs = parse_args(raw_args)?;
            check_safe_segment("shelf_slug", &args.shelf_slug)?;
            check_safe_segment("slug", &args.slug)?;
            check_optional_safe_segment("project_id", args.project_id.as_deref())?;
            check_optional_safe_segment("trio_role", args.trio_role.as_deref())?;
            check_safe_text("title", &args.title, &state.redaction_policy)?;
            check_safe_text("body_md", &args.body_md, &state.redaction_policy)?;
            check_safe_text("summary", &args.summary, &state.redaction_policy)?;

            let req = SaveResourceRequest {
                shelf_slug: args.shelf_slug,
                slug: args.slug,
                title: args.title,
                body_md: args.body_md,
                project_id: args.project_id,
                trio_role: args.trio_role,
                lifecycle_tier: args.lifecycle_tier,
                triage_state: args.triage_state,
                summary: args.summary,
                expected_rev: args.expected_rev,
            };
            let saved = state.store.save_resource(&active_grant.grant_id, req)?;
            Ok(serde_json::to_value(saved)?)
        }
        "save_project_documents" => {
            let active_grant = require_grant(grant)?;
            let args: SaveProjectDocumentsArgs = parse_args(raw_args)?;
            handle_save_project_documents(state, &active_grant.grant_id, args)
        }
        "restore_resource" => {
            let active_grant = require_grant(grant)?;
            let args: RestoreResourceArgs = parse_args(raw_args)?;
            check_safe_segment("shelf_slug", &args.shelf_slug)?;
            check_safe_segment("slug", &args.slug)?;
            check_optional_safe_text("summary", args.summary.as_deref(), &state.redaction_policy)?;
            let restored = state.store.restore_resource(
                &active_grant.grant_id,
                &args.shelf_slug,
                &args.slug,
                args.target_rev,
                args.summary,
            )?;
            Ok(serde_json::to_value(restored)?)
        }
        "lock_project_plan" => {
            let active_grant = require_grant(grant)?;
            let args: LockProjectPlanArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            let locked = state.store.lock_project_plan(
                &active_grant.grant_id,
                &args.project_id,
                args.plan_rev,
            )?;
            Ok(serde_json::to_value(locked)?)
        }
        "upsert_own_decision" => {
            let active_grant = require_grant(grant)?;
            let args: UpsertOwnDecisionArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            check_safe_segment("own_id", &args.own_id)?;
            check_safe_text("statement", &args.statement, &state.redaction_policy)?;
            check_optional_safe_text(
                "approved_by",
                args.approved_by.as_deref(),
                &state.redaction_policy,
            )?;
            let saved = state.store.upsert_own_decision(
                &active_grant.grant_id,
                &args.project_id,
                &args.own_id,
                &args.statement,
                args.approved_by,
            )?;
            Ok(serde_json::to_value(saved)?)
        }
        "record_divergence" => {
            let active_grant = require_grant(grant)?;
            let args: RecordDivergenceArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            let div_id = args.div_id.filter(|val| !val.trim().is_empty());
            check_optional_safe_segment("div_id", div_id.as_deref())?;
            check_safe_text("designed", &args.designed, &state.redaction_policy)?;
            check_safe_text("observed", &args.observed, &state.redaction_policy)?;
            check_safe_text("resolution", &args.resolution, &state.redaction_policy)?;
            let div = state.store.record_divergence(
                &active_grant.grant_id,
                RecordDivergenceRequest {
                    project_id: args.project_id,
                    div_id,
                    designed: args.designed,
                    observed: args.observed,
                    resolution: args.resolution,
                    status: args.status,
                },
            )?;
            Ok(serde_json::to_value(div)?)
        }
        "upsert_requirement_row" => {
            let active_grant = require_grant(grant)?;
            let args: UpsertRequirementArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            check_safe_segment("req_id", &args.req_id)?;
            check_safe_text("source_id", &args.source_id, &state.redaction_policy)?;
            check_safe_text("statement", &args.statement, &state.redaction_policy)?;
            let saved = state.store.upsert_requirement_row(
                &active_grant.grant_id,
                &args.project_id,
                &args.req_id,
                &args.source_id,
                &args.statement,
            )?;
            Ok(serde_json::to_value(saved)?)
        }
        "update_requirement_stage" => {
            let active_grant = require_grant(grant)?;
            let req: UpdateRequirementStageRequest = parse_args(raw_args)?;
            check_safe_segment("project_id", &req.project_id)?;
            check_safe_segment("req_id", &req.req_id)?;
            check_safe_text("evidence_ref", &req.evidence_ref, &state.redaction_policy)?;
            check_optional_safe_text(
                "actor_family",
                req.actor_family.as_deref(),
                &state.redaction_policy,
            )?;
            let updated = state
                .store
                .update_requirement_stage(&active_grant.grant_id, req)?;
            Ok(serde_json::to_value(updated)?)
        }
        "check_sync" => {
            let active_grant = require_grant(grant)?;
            let args: CheckSyncArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            let bundle = state
                .store
                .get_project_bundle(&active_grant.grant_id, &args.project_id)?;
            let as_built_md = if let Some(md) = args.as_built_markdown {
                md
            } else if let Some(slug) = args.as_built_slug {
                check_safe_segment("as_built_slug", &slug)?;
                state
                    .store
                    .read_resource(&active_grant.grant_id, "projects", &slug)?
                    .body_md
            } else {
                let target_res = bundle
                    .resources
                    .iter()
                    .find(|res| {
                        matches!(
                            res.trio_role.as_deref(),
                            Some("as_built") | Some("fix_record")
                        )
                    })
                    .ok_or_else(|| {
                        ApiError::new(
                            StatusCode::NOT_FOUND,
                            "NOT_FOUND",
                            "no as_built_markdown provided and no as_built resource found in project",
                        )
                    })?;
                state
                    .store
                    .read_resource_by_id(&active_grant.grant_id, &target_res.id)?
                    .body_md
            };
            let summary = check_sync(&bundle.requirements, &bundle.own_decisions, &as_built_md);
            Ok(serde_json::to_value(summary)?)
        }
        "save_sync_report" => {
            let active_grant = require_grant(grant)?;
            let args: SaveSyncReportArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            check_optional_safe_segment("asbuilt_slug", args.asbuilt_slug.as_deref())?;
            let saved = state.store.compute_and_save_sync_report(
                &active_grant.grant_id,
                &args.project_id,
                args.asbuilt_slug.as_deref(),
            )?;
            Ok(serde_json::to_value(saved)?)
        }
        "get_sync_report" => {
            let active_grant = require_grant(grant)?;
            let args: GetSyncReportArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            check_optional_safe_segment("report_id", args.report_id.as_deref())?;
            let report = state.store.get_sync_report(
                &active_grant.grant_id,
                &args.project_id,
                args.report_id.as_deref(),
            )?;
            Ok(serde_json::to_value(report)?)
        }
        "dispatch_run" => {
            let active_grant = require_grant(grant)?;
            let args: DispatchRunArgs = parse_args(raw_args)?;
            handle_dispatch_run(state, active_grant, args)
        }
        "get_run_status" => {
            let active_grant = require_grant(grant)?;
            let args: RunIdArgs = parse_args(raw_args)?;
            check_safe_segment("run_id", &args.run_id)?;
            let run = state.store.get_conveyor_run(&args.run_id)?;
            let _bundle = state
                .store
                .get_project_bundle(&active_grant.grant_id, &run.project_id)?;
            Ok(serde_json::to_value(run)?)
        }
        "list_conveyor_runs" => {
            let active_grant = require_grant(grant)?;
            let args: ProjectIdArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            let _bundle = state
                .store
                .get_project_bundle(&active_grant.grant_id, &args.project_id)?;
            let runs = state.store.list_conveyor_runs(&args.project_id)?;
            Ok(serde_json::to_value(runs)?)
        }
        "mark_project_cleanup_verified" => {
            let active_grant = require_grant(grant)?;
            let args: MarkCleanupArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            let detail = args
                .detail
                .unwrap_or_else(|| "temporary and build artifacts verified clean".to_string());
            check_safe_text("detail", &detail, &state.redaction_policy)?;
            let proj = state.store.mark_project_cleanup_verified(
                &active_grant.grant_id,
                &args.project_id,
                &detail,
            )?;
            Ok(serde_json::to_value(proj)?)
        }
        "record_lesson" => {
            let active_grant = require_grant(grant)?;
            let args: RecordLessonArgs = parse_args(raw_args)?;
            let lesson_id = args.lesson_id.filter(|val| !val.trim().is_empty());
            let project_id = args.project_id.filter(|val| !val.trim().is_empty());
            check_optional_safe_segment("lesson_id", lesson_id.as_deref())?;
            check_optional_safe_segment("project_id", project_id.as_deref())?;
            for tag_item in &args.tags {
                check_safe_text("tags", tag_item, &state.redaction_policy)?;
            }
            check_safe_text(
                "observed_fact",
                &args.observed_fact,
                &state.redaction_policy,
            )?;
            check_safe_text("cause_kind", &args.cause_kind, &state.redaction_policy)?;
            check_safe_text("cause_note", &args.cause_note, &state.redaction_policy)?;
            check_safe_text("forward_rule", &args.forward_rule, &state.redaction_policy)?;
            check_optional_safe_text(
                "validation_state",
                args.validation_state.as_deref(),
                &state.redaction_policy,
            )?;

            let req = RecordLessonRequest {
                lesson_id,
                project_id,
                tags: args.tags,
                observed_fact: args.observed_fact,
                cause_kind: args.cause_kind,
                cause_note: args.cause_note,
                forward_rule: args.forward_rule,
                validation_state: args.validation_state,
            };
            let saved = state.store.record_lesson(&active_grant.grant_id, req)?;
            Ok(serde_json::to_value(saved)?)
        }
        "record_no_new_lessons" => {
            let active_grant = require_grant(grant)?;
            let args: NoNewLessonsArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            check_safe_text("reason", &args.reason, &state.redaction_policy)?;
            let proj = state.store.record_no_new_lessons(
                &active_grant.grant_id,
                &args.project_id,
                &args.reason,
            )?;
            Ok(serde_json::to_value(proj)?)
        }
        "query_lessons" => {
            let active_grant = require_grant(grant)?;
            let args: QueryLessonsArgs = parse_args(raw_args)?;
            check_optional_safe_segment("project_filter", args.project_filter.as_deref())?;
            let limit = args.limit.unwrap_or(50).min(500);
            let lessons = state.store.query_lessons(
                &active_grant.grant_id,
                args.tag_filter.as_deref(),
                args.project_filter.as_deref(),
                limit,
            )?;
            Ok(serde_json::to_value(lessons)?)
        }
        "submit_how_it_works" => {
            let active_grant = require_grant(grant)?;
            let args: SubmitHowItWorksArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            check_safe_text("markdown", &args.markdown, &state.redaction_policy)?;
            check_optional_safe_text("title", args.title.as_deref(), &state.redaction_policy)?;
            let proj = state.store.submit_how_it_works(
                &active_grant.grant_id,
                &args.project_id,
                &args.markdown,
                args.title,
            )?;
            Ok(serde_json::to_value(proj)?)
        }
        "approve_release" => {
            let active_grant = require_grant(grant)?;
            let args: ProjectIdArgs = parse_args(raw_args)?;
            check_safe_segment("project_id", &args.project_id)?;
            let bundle = state
                .store
                .approve_release(&active_grant.grant_id, &args.project_id)?;
            let vault_dir = state.config.vault_sync_dir.trim();
            if !vault_dir.is_empty() {
                if let Err(err) = state.store.sync_vault_dir(Path::new(vault_dir)) {
                    tracing::warn!(error = %err, "best-effort vault sync after approve_release skipped");
                }
            }
            let runs_dir = state.config.runs_dir.trim();
            if !runs_dir.is_empty() {
                if let Err(err) = export_bundle_trajectory(
                    state,
                    &active_grant.grant_id,
                    &bundle,
                    Path::new(runs_dir),
                ) {
                    tracing::warn!(error = %err.message, "best-effort trajectory export after approve_release skipped");
                }
            }
            Ok(serde_json::to_value(bundle)?)
        }
        "export_trajectory" => {
            let active_grant = require_grant(grant)?;
            let args: ProjectIdArgs = parse_args(raw_args)?;
            handle_export_trajectory(state, active_grant, args)
        }
        "sync_vault" => {
            let active_grant = require_grant(grant)?;
            let checked = state.store.require_rules_acked(&active_grant.grant_id)?;
            if !checked.can_write {
                return Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "FORBIDDEN",
                    "forbidden: grant lacks write permission",
                ));
            }
            let vault_dir = state.config.vault_sync_dir.trim();
            if vault_dir.is_empty() {
                return Err(ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "UNPROCESSABLE",
                    "vault_sync_dir is not configured",
                ));
            }
            let summary = state.store.sync_vault_dir(Path::new(vault_dir))?;
            Ok(serde_json::to_value(summary)?)
        }
        "export_workspace" => {
            let active_grant = require_grant(grant)?;
            let args: ExportWorkspaceArgs = parse_args(raw_args)?;
            handle_export_workspace(state, active_grant, args.include_bodies.unwrap_or(true))
        }
        other => Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("unknown tool '{other}'"),
        )),
    }
}

fn handle_compare_resources(
    state: &AppState,
    grant_id: &str,
    args: CompareResourcesArgs,
) -> Result<streamline_store::RevisionDiffSummary, ApiError> {
    if let (Some(shelf), Some(slug), Some(left_rev), Some(right_rev)) =
        (&args.shelf_slug, &args.slug, args.left_rev, args.right_rev)
    {
        check_safe_segment("shelf_slug", shelf)?;
        check_safe_segment("slug", slug)?;
        return state
            .store
            .compare_revisions(grant_id, shelf, slug, left_rev, right_rev)
            .map_err(ApiError::from);
    }

    if let (Some(left_id), Some(right_id)) = (&args.left_resource_id, &args.right_resource_id) {
        check_safe_segment("left_resource_id", left_id)?;
        check_safe_segment("right_resource_id", right_id)?;
        let left = state.store.read_resource_by_id(grant_id, left_id)?;
        let right = state.store.read_resource_by_id(grant_id, right_id)?;
        return Ok(compare_texts(
            left.resource.current_rev,
            right.resource.current_rev,
            &left.body_md,
            &right.body_md,
        ));
    }

    if let (Some(ls), Some(lslug), Some(rs), Some(rslug)) = (
        &args.left_shelf_slug,
        &args.left_slug,
        &args.right_shelf_slug,
        &args.right_slug,
    ) {
        check_safe_segment("left_shelf_slug", ls)?;
        check_safe_segment("left_slug", lslug)?;
        check_safe_segment("right_shelf_slug", rs)?;
        check_safe_segment("right_slug", rslug)?;
        let left = state.store.read_resource(grant_id, ls, lslug)?;
        let right = state.store.read_resource(grant_id, rs, rslug)?;
        return Ok(compare_texts(
            left.resource.current_rev,
            right.resource.current_rev,
            &left.body_md,
            &right.body_md,
        ));
    }

    Err(ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "UNPROCESSABLE",
        "provide (shelf_slug, slug, left_rev, right_rev), (left_resource_id, right_resource_id), or (left_shelf_slug, left_slug, right_shelf_slug, right_slug)",
    ))
}

fn handle_select_context(
    state: &AppState,
    grant: &AccessGrant,
    args: SelectContextArgs,
) -> Result<Value, ApiError> {
    check_optional_safe_segment("project_id", args.project_id.as_deref())?;

    let rules_sha256 = state.store.current_rules_sha256()?;
    let rules_acked = grant.acked_rules_sha256.as_deref() == Some(rules_sha256.as_str());

    let all_skills = load_bundled_skills(&state.skills_dir);
    let requested_role = args
        .role
        .as_deref()
        .map(|val| val.trim().to_ascii_lowercase());
    // Keep each role skill distinct and unmerged so voice boundaries stay intact.
    let selected_skills: Vec<_> = all_skills
        .into_iter()
        .filter(|skill| match requested_role.as_deref() {
            Some(role_name) if !role_name.is_empty() => {
                skill.role == role_name || skill.name == role_name
            }
            _ => true,
        })
        .collect();

    let profile_assessment = args.assessment.as_ref().map(|input| {
        let mut adjusted = input.clone();
        if let Some(kind) = args.work_kind {
            adjusted.work_kind = kind;
        }
        assess_work_with_modules(&adjusted, &args.opt_in_modules)
    });

    let active_standards = state
        .store
        .list_resources(
            &grant.grant_id,
            &ResourceFilter {
                shelf_slug: Some("standards".to_string()),
                lifecycle_tier: Some(LifecycleTier::Active),
                ..Default::default()
            },
        )
        .unwrap_or_default();

    let project_bundle = match args.project_id.as_deref() {
        Some(pid) => Some(state.store.get_project_bundle(&grant.grant_id, pid)?),
        None => None,
    };

    let forward_lessons = state.store.query_lessons(
        &grant.grant_id,
        args.tag_filter.as_deref(),
        args.project_id.as_deref(),
        25,
    )?;

    Ok(json!({
        "rules_sha256": rules_sha256,
        "rules_acked": rules_acked,
        "voice_isolation": "unmerged",
        "selected_skills": selected_skills,
        "profile_assessment": profile_assessment,
        "active_standards": active_standards,
        "project_bundle": project_bundle,
        "forward_lessons": forward_lessons,
    }))
}

fn handle_save_project_documents(
    state: &AppState,
    grant_id: &str,
    args: SaveProjectDocumentsArgs,
) -> Result<Value, ApiError> {
    check_safe_segment("project_id", &args.project_id)?;
    if args.documents.is_empty() && args.own_decisions.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "UNPROCESSABLE",
            "provide at least one document or own_decision entry",
        ));
    }

    let mut prepared_docs = Vec::with_capacity(args.documents.len());
    for doc in args.documents {
        check_safe_segment("shelf_slug", &doc.shelf_slug)?;
        check_optional_safe_segment("trio_role", doc.trio_role.as_deref())?;
        let resolved_slug = match doc.slug {
            Some(explicit) if !explicit.trim().is_empty() => explicit.trim().to_string(),
            _ => match doc.trio_role.as_deref() {
                Some(role) if !role.trim().is_empty() => {
                    format!("{}-{}", args.project_id, role.trim())
                }
                _ => {
                    return Err(ApiError::new(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "UNPROCESSABLE",
                        "each project document requires either slug or trio_role",
                    ));
                }
            },
        };
        check_safe_segment("slug", &resolved_slug)?;
        check_safe_text("title", &doc.title, &state.redaction_policy)?;
        check_safe_text("body_md", &doc.body_md, &state.redaction_policy)?;
        check_safe_text("summary", &doc.summary, &state.redaction_policy)?;

        prepared_docs.push(SaveResourceRequest {
            shelf_slug: doc.shelf_slug,
            slug: resolved_slug,
            title: doc.title,
            body_md: doc.body_md,
            project_id: Some(args.project_id.clone()),
            trio_role: doc.trio_role,
            lifecycle_tier: doc.lifecycle_tier,
            triage_state: doc.triage_state,
            summary: doc.summary,
            expected_rev: doc.expected_rev,
        });
    }

    for own in &args.own_decisions {
        check_safe_segment("own_id", &own.own_id)?;
        check_safe_text("statement", &own.statement, &state.redaction_policy)?;
    }

    // Verify project exists and caller has project access before applying mutations.
    let _bundle = state.store.get_project_bundle(grant_id, &args.project_id)?;

    let mut saved_own = Vec::with_capacity(args.own_decisions.len());
    for own in args.own_decisions {
        let saved = state.store.upsert_own_decision(
            grant_id,
            &args.project_id,
            &own.own_id,
            &own.statement,
            None,
        )?;
        saved_own.push(saved);
    }

    let mut saved_resources = Vec::with_capacity(prepared_docs.len());
    for req in prepared_docs {
        saved_resources.push(state.store.save_resource(grant_id, req)?);
    }

    Ok(json!({
        "project_id": args.project_id,
        "saved_own_decisions": saved_own,
        "saved_resources": saved_resources,
    }))
}

fn handle_export_workspace(
    state: &AppState,
    grant: &AccessGrant,
    include_bodies: bool,
) -> Result<Value, ApiError> {
    let shelves = state.store.list_shelves(&grant.grant_id)?;
    let resources = state
        .store
        .list_resources(&grant.grant_id, &ResourceFilter::default())?;

    let mut exported_resources = Vec::with_capacity(resources.len());
    for res in resources {
        if include_bodies {
            let full = state.store.read_resource_by_id(&grant.grant_id, &res.id)?;
            let report = inspect_text(&full.body_md, &state.redaction_policy);
            let (body_md, redacted) = if report.ok {
                (Some(full.body_md), false)
            } else {
                (Some("[REDACTED BY PUBLIC SAFE POLICY]".to_string()), true)
            };
            exported_resources.push(json!({
                "resource": full.resource,
                "passport": full.passport,
                "body_md": body_md,
                "redacted": redacted,
            }));
        } else {
            exported_resources.push(json!({
                "resource": res,
            }));
        }
    }

    let projects = state.store.list_trio_projects(&grant.grant_id)?;
    let mut exported_bundles = Vec::with_capacity(projects.len());
    for proj in &projects {
        exported_bundles.push(
            state
                .store
                .get_project_bundle(&grant.grant_id, &proj.project_id)?,
        );
    }

    let lessons = state
        .store
        .query_lessons(&grant.grant_id, None, None, 500)?;

    Ok(json!({
        "exported_at": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "principal_id": grant.principal_id,
        "granted_tags": grant.granted_tags,
        "public_safe_mode": state.config.public_safe_mode,
        "shelves": shelves,
        "resources": exported_resources,
        "projects": exported_bundles,
        "lessons": lessons,
    }))
}

fn map_conveyor_error(err: ConveyorError) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "CONVEYOR_ERROR",
        err.to_string(),
    )
}

fn handle_dispatch_run(
    state: &AppState,
    active_grant: &AccessGrant,
    args: DispatchRunArgs,
) -> Result<Value, ApiError> {
    check_safe_segment("project_id", &args.project_id)?;
    check_optional_safe_segment(
        "preferred_builder_harness",
        args.preferred_builder_harness.as_deref(),
    )?;
    check_optional_safe_segment(
        "preferred_acceptor_harness",
        args.preferred_acceptor_harness.as_deref(),
    )?;
    for path_item in &args.input_paths {
        check_safe_text("input_paths", path_item, &state.redaction_policy)?;
    }

    let checked_grant = state.store.require_rules_acked(&active_grant.grant_id)?;
    if !checked_grant.can_dispatch {
        return Err(ApiError::from(StoreError::ForbiddenDispatch));
    }

    let bundle = state
        .store
        .get_project_bundle(&checked_grant.grant_id, &args.project_id)?;
    let risk_band = args.risk_band.unwrap_or(bundle.project.risk);

    let req = DispatchRequest {
        project_id: args.project_id,
        input_paths: args.input_paths.into_iter().map(PathBuf::from).collect(),
        preferred_builder_harness: args.preferred_builder_harness,
        preferred_acceptor_harness: args.preferred_acceptor_harness,
        risk_band,
    };
    let outcome: DispatchOutcome =
        execute_conveyor_blocking(&state.config, &req).map_err(map_conveyor_error)?;

    state.store.upsert_conveyor_run(outcome.build_run.clone())?;
    if let Some(ref accept_run) = outcome.accept_run {
        state.store.upsert_conveyor_run(accept_run.clone())?;
    }

    Ok(serde_json::to_value(outcome)?)
}

fn export_bundle_trajectory(
    state: &AppState,
    grant_id: &str,
    bundle: &TrioProjectBundle,
    runs_dir: &Path,
) -> Result<PathBuf, ApiError> {
    let lessons =
        state
            .store
            .query_lessons(grant_id, None, Some(&bundle.project.project_id), 50)?;
    let own_statements = bundle
        .own_decisions
        .iter()
        .map(|item| format!("{}: {}", item.own_id, item.statement))
        .collect();
    let div_summaries = bundle
        .divergences
        .iter()
        .map(|item| {
            format!(
                "{} [{}]: {} -> {} ({})",
                item.div_id, item.status, item.designed, item.observed, item.resolution
            )
        })
        .collect();
    let lesson_rules = lessons
        .iter()
        .map(|item| item.forward_rule.clone())
        .collect();
    let sync_verdict = bundle.sync_reports.first().map(|report| report.verdict);
    let how_it_works_sha256 = bundle
        .project
        .how_it_works_resource_id
        .as_ref()
        .and_then(|res_id| bundle.resources.iter().find(|res| &res.id == res_id))
        .or_else(|| {
            bundle
                .resources
                .iter()
                .find(|res| res.trio_role.as_deref() == Some("how_it_works"))
        })
        .map(|res| res.current_sha256.clone());

    let input = TrajectoryExportInput {
        project_id: bundle.project.project_id.clone(),
        title: bundle.project.title.clone(),
        profile: bundle.project.profile,
        own_statements,
        div_summaries,
        lesson_rules,
        sync_verdict,
        how_it_works_sha256,
    };

    append_trajectory_jsonl(runs_dir, &state.redaction_policy, &input).map_err(map_conveyor_error)
}

fn handle_export_trajectory(
    state: &AppState,
    active_grant: &AccessGrant,
    args: ProjectIdArgs,
) -> Result<Value, ApiError> {
    check_safe_segment("project_id", &args.project_id)?;
    let checked_grant = state.store.require_rules_acked(&active_grant.grant_id)?;
    let principal = state.store.get_principal(&checked_grant.principal_id)?;
    if principal.role != PrincipalRole::Owner {
        return Err(ApiError::from(StoreError::ForbiddenRole(
            principal.role.to_string(),
        )));
    }
    let runs_dir = state.config.runs_dir.trim();
    if runs_dir.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "UNPROCESSABLE",
            "runs_dir is not configured",
        ));
    }
    let bundle = state
        .store
        .get_project_bundle(&checked_grant.grant_id, &args.project_id)?;
    let trajectory_file =
        export_bundle_trajectory(state, &checked_grant.grant_id, &bundle, Path::new(runs_dir))?;
    Ok(json!({
        "project_id": args.project_id,
        "trajectory_file": trajectory_file,
    }))
}

#[derive(Deserialize)]
struct AckRulesArgs {
    #[serde(alias = "sha256", alias = "digest")]
    rules_sha256: String,
}

#[derive(Deserialize)]
struct ReadResourceArgs {
    resource_id: Option<String>,
    shelf_slug: Option<String>,
    slug: Option<String>,
}

#[derive(Deserialize)]
struct SearchResourcesArgs {
    query: String,
    #[serde(flatten)]
    filter: ResourceFilter,
}

#[derive(Deserialize)]
struct ResourceCoordArgs {
    shelf_slug: String,
    slug: String,
}

#[derive(Deserialize)]
struct CompareResourcesArgs {
    shelf_slug: Option<String>,
    slug: Option<String>,
    left_rev: Option<i64>,
    right_rev: Option<i64>,
    left_shelf_slug: Option<String>,
    left_slug: Option<String>,
    right_shelf_slug: Option<String>,
    right_slug: Option<String>,
    left_resource_id: Option<String>,
    right_resource_id: Option<String>,
}

#[derive(Deserialize)]
struct UpdateSourcePassportArgs {
    resource_id: Option<String>,
    shelf_slug: Option<String>,
    slug: Option<String>,
    steward: String,
    last_verified_at: Option<String>,
    freshness_state: FreshnessState,
    lineage_ref: String,
    access_scope: Option<String>,
    next_action: String,
}

#[derive(Deserialize)]
struct SetLifecycleTierArgs {
    shelf_slug: String,
    slug: String,
    lifecycle_tier: LifecycleTier,
    triage_state: Option<TriageState>,
    target_project_id: Option<String>,
}

#[derive(Deserialize, Default)]
struct SelectContextArgs {
    role: Option<String>,
    work_kind: Option<WorkKind>,
    project_id: Option<String>,
    tag_filter: Option<String>,
    assessment: Option<AssessmentInput>,
    #[serde(default)]
    opt_in_modules: Vec<String>,
}

#[derive(Deserialize)]
struct NestedAssessArgs {
    assessment: AssessmentInput,
    #[serde(default)]
    opt_in_modules: Vec<String>,
}

#[derive(Deserialize)]
struct FlatAssessArgs {
    #[serde(flatten)]
    assessment: AssessmentInput,
    #[serde(default)]
    opt_in_modules: Vec<String>,
}

#[derive(Deserialize)]
struct CreateTrioProjectArgs {
    project_id: String,
    #[serde(alias = "request_id")]
    create_id: Option<String>,
    title: String,
    assessment: AssessmentInput,
    #[serde(default)]
    opt_in_modules: Vec<String>,
    #[serde(default)]
    initial_own_decisions: Vec<OwnDecisionInput>,
    #[serde(default)]
    initial_requirements: Vec<RequirementRowInput>,
}

#[derive(Deserialize)]
struct ProjectIdArgs {
    project_id: String,
}

fn default_save_summary() -> String {
    "Save resource".to_string()
}

#[derive(Deserialize)]
struct SaveResourceArgs {
    shelf_slug: String,
    slug: String,
    title: String,
    body_md: String,
    project_id: Option<String>,
    trio_role: Option<String>,
    lifecycle_tier: Option<LifecycleTier>,
    triage_state: Option<TriageState>,
    #[serde(default = "default_save_summary")]
    summary: String,
    expected_rev: Option<i64>,
}

fn default_projects_shelf() -> String {
    "projects".to_string()
}

#[derive(Deserialize)]
struct ProjectDocumentInput {
    #[serde(default = "default_projects_shelf")]
    shelf_slug: String,
    slug: Option<String>,
    trio_role: Option<String>,
    title: String,
    body_md: String,
    #[serde(default = "default_save_summary")]
    summary: String,
    expected_rev: Option<i64>,
    lifecycle_tier: Option<LifecycleTier>,
    triage_state: Option<TriageState>,
}

#[derive(Deserialize)]
struct SaveProjectDocumentsArgs {
    project_id: String,
    #[serde(default)]
    documents: Vec<ProjectDocumentInput>,
    #[serde(default)]
    own_decisions: Vec<OwnDecisionInput>,
}

#[derive(Deserialize)]
struct RestoreResourceArgs {
    shelf_slug: String,
    slug: String,
    target_rev: i64,
    summary: Option<String>,
}

#[derive(Deserialize)]
struct LockProjectPlanArgs {
    project_id: String,
    plan_rev: Option<i64>,
}

#[derive(Deserialize)]
struct UpsertOwnDecisionArgs {
    project_id: String,
    own_id: String,
    statement: String,
    approved_by: Option<String>,
}

fn default_div_status() -> DivergenceStatus {
    DivergenceStatus::Open
}

#[derive(Deserialize)]
struct RecordDivergenceArgs {
    project_id: String,
    div_id: Option<String>,
    designed: String,
    observed: String,
    resolution: String,
    #[serde(default = "default_div_status")]
    status: DivergenceStatus,
}

#[derive(Deserialize)]
struct UpsertRequirementArgs {
    project_id: String,
    req_id: String,
    source_id: String,
    statement: String,
}

#[derive(Deserialize)]
struct CheckSyncArgs {
    project_id: String,
    as_built_markdown: Option<String>,
    as_built_slug: Option<String>,
}

#[derive(Deserialize)]
struct SaveSyncReportArgs {
    project_id: String,
    #[serde(alias = "as_built_slug")]
    asbuilt_slug: Option<String>,
}

#[derive(Deserialize)]
struct GetSyncReportArgs {
    project_id: String,
    report_id: Option<String>,
}

#[derive(Deserialize)]
struct MarkCleanupArgs {
    project_id: String,
    detail: Option<String>,
}

#[derive(Deserialize)]
struct RecordLessonArgs {
    lesson_id: Option<String>,
    project_id: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    observed_fact: String,
    cause_kind: String,
    #[serde(default)]
    cause_note: String,
    forward_rule: String,
    validation_state: Option<String>,
}

#[derive(Deserialize)]
struct NoNewLessonsArgs {
    project_id: String,
    reason: String,
}

#[derive(Deserialize, Default)]
struct QueryLessonsArgs {
    #[serde(alias = "tag")]
    tag_filter: Option<String>,
    #[serde(alias = "project_id")]
    project_filter: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct SubmitHowItWorksArgs {
    project_id: String,
    #[serde(alias = "body_md", alias = "content")]
    markdown: String,
    title: Option<String>,
}

#[derive(Deserialize, Default)]
struct ExportWorkspaceArgs {
    include_bodies: Option<bool>,
}

#[derive(Deserialize)]
struct DispatchRunArgs {
    project_id: String,
    #[serde(default)]
    input_paths: Vec<String>,
    preferred_builder_harness: Option<String>,
    preferred_acceptor_harness: Option<String>,
    risk_band: Option<u8>,
}

#[derive(Deserialize)]
struct RunIdArgs {
    run_id: String,
}
