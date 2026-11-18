# Streamline Schema, Contracts, and State Machines

Version: `0.1.0-d1`

## 1. Crate and Single-Writer Ownership Map (`L4`)

| Crate / Target | Single Responsibility | Writes To |
| :--- | :--- | :--- |
| `src/main.rs` | Binary entrypoint, config loading, startup migrations, listener binding | Process lifecycle only |
| `crates/streamline-core` | Domain types, `P0` to `P4` Trio+ planner, `OWN` lock rules, `D I T A L` state transitions, `check_sync` row pairing | In-memory domain structs |
| `crates/streamline-safe` | High-entropy credential detector, private-literal filter, path-segment sanitizer, HTML/Markdown safety | Pure validation verdicts |
| `crates/streamline-store` | Single writer for SQLite (`rusqlite`), append-only revisions, two-way Obsidian markdown sync, optional memory webhook | SQLite file + configured vault directory |
| `crates/streamline-conveyor` | Read-only OS staging, CLI/webhook worker runner, post-run `CLEANUP` check, cross-family QC dispatch, JSONL trajectory writer | Isolated run staging/output directories |
| `crates/streamline-http` | Axum HTTP API, Streamable HTTP + stdio MCP handler (`/mcp`), auth/tag guards, `/downloads/streamline-skills.zip`, 3-pane Web UI | Network responses (delegates mutations to `store` and `conveyor`) |

## 2. Configuration Contract (`StreamlineConfig`)

Loaded from `STREAMLINE_CONFIG_PATH` (TOML) or `STREAMLINE_CONFIG_JSON` (neutral field names only):

```toml
bind_http = "127.0.0.1:5880"
bind_admin = "127.0.0.1:5881"
db_path = "/var/lib/streamline/streamline.sqlite3"
vault_sync_dir = "/var/lib/streamline/vault"
runs_dir = "/var/lib/streamline/runs"
public_safe_mode = true

# Optional local directory for private brand/Presence assets (not required in OSS)
presence_plugin_dir = ""

# Optional external webhooks (empty in standalone mode)
memory_sync_url = ""
worker_lease_url = ""

[[harnesses]]
id = "codex-main"
family = "openai-codex"
command = ["codex", "exec"]
max_concurrency = 1

[[harnesses]]
id = "agy-main"
family = "google-gemini"
command = ["agy", "run"]
max_concurrency = 2
```

## 3. SQLite Data Model (`L5`)

1. **`principals`**: `id`, `handle`, `role` (`owner | member | agent | guest`), `created_at`.
2. **`access_grants`**: `grant_id`, `principal_id`, `credential_hash`, `granted_tags_json`, `can_write`, `can_dispatch`, `acked_rules_sha256`, `expires_at`, `revoked_at`.
3. **`shelves`**: `slug` (`standards | runbooks | architecture | projects | skills | inbox | archive`), `title`, `required_tag`.
4. **`resources`**: `id`, `shelf_slug`, `slug`, `project_id`, `trio_role`, `lifecycle_tier` (`active | dormant | legacy | retired`), `triage_state` (`reviewed | needs_triage`), `title`, `current_rev`, `current_sha256`, `deleted_at`, `updated_at` (`bootstrap` surfaces `lifecycle_tier = 'active'` only; `dormant` and `legacy` remain searchable and revivable in triage views without polluting agent bootstrap).
5. **`resource_revisions`**: `resource_id`, `rev`, `body_md`, `sha256`, `author_principal`, `summary`, `created_at` (append-only).
6. **`source_passports`**: `resource_id`, `steward`, `last_verified_at`, `freshness_state` (`verified | stale | drift | unknown`), `lineage_ref`, `access_scope`, `next_action`.
7. **`trio_projects`**: `project_id`, `title`, `work_kind` (`software | council | procedure | general`), `difficulty` (`0..4`), `risk` (`0..4`), `profile` (`P0 | P1 | P2 | P3 | P4`), `expected_roles_json`, `plan_locked_rev`, `state`, `how_it_works_resource_id`, `created_at`.
8. **`own_decisions`**: `project_id`, `own_id` (`OWN-001`..), `statement`, `locked_at`, `approved_by` (immutable while locked unless owner unlocks/bumps minor version).
9. **`requirement_rows`**: `project_id`, `req_id` (`REQ-01`..), `source_id`, `statement`, `stage` (`D | I | T | A | L | UNKNOWN`), `evidence_ref`, `updated_at`.
10. **`divergences`**: `project_id`, `div_id` (`DIV-001`..), `designed`, `observed`, `resolution`, `status` (`open | accepted | fixed`), `author_principal`, `created_at`.
11. **`sync_reports`**: `report_id`, `project_id`, `plan_rev`, `asbuilt_rev`, `paired_rows_json`, `verdict` (`pass | fail | partial`), `acceptor_family`, `created_at`.
12. **`conveyor_runs`**: `run_id`, `project_id`, `phase` (`build | accept`), `harness_id`, `family`, `input_manifest_sha256`, `output_manifest_sha256`, `cleanup_clean`, `scan_clean`, `state`, `started_at`, `finished_at`.
13. **`audit_events`**: `event_id`, `principal_id`, `action`, `target_id`, `outcome`, `detail_json`, `created_at`.
14. **`lessons_learned`** (Central Learning Database): `lesson_id`, `project_id`, `tags_json`, `observed_fact`, `cause_kind` (`demonstrated | hypothesis`), `cause_note`, `forward_rule`, `validation_state` (`worked_once | verified_reusable`), `recorded_by`, `created_at` (synced to `<vault>/lessons/` and surfaced automatically in `bootstrap` / `query_lessons`; `trio_projects` tracks `cleanup_verified` and `lessons_satisfied` so `submit_how_it_works` blocks closeout until temp cleanup and forward central lessons are both recorded).

## 4. MCP and HTTP Tool Contract (`L5`)

All tools return typed JSON (`{"ok": bool, "data": ..., "error": {"code", "message"}}`).

### Always Unlocked (Discovery & Rule Gate)
1. `ready`: Health, server version, `public_safe_mode`, active `rules_sha256`.
2. `bootstrap`: Active governance summary, `rules_sha256`, caller `granted_tags`, and required onboarding slugs.
3. `ack_rules`: Records the caller session's acknowledgement of `rules_sha256`, unlocking write and dispatch tools for that grant/session.

### Context, Profile, and Shelves (Tag-Checked)
4. `get_workspace`: Owner profile summary, active projects, shelves, and live permissions (`can_write`, `can_dispatch`, `rules_acked`).
5. `list_shelves`: Authorized shelves with item counts and freshness counts.
6. `list_resources`: Filter by shelf, tag, project, or freshness state.
7. `read_resource`: Returns body, revision, `sha256`, and `SourcePassport`.
8. `search_resources`: Full-text search across authorized shelves.
9. `resource_history`: Revision log for one resource.
10. `compare_resources`: Structured line/word diff between two revisions or two resources.
11. `get_source_passport` / `update_source_passport`: Read or refresh live verification metadata (`verified | stale | drift | unknown`).
12. `list_skills` / `select_context` / `explain_selection`: Select profile + applicable voice/technical skills for the current task without merging voices.

### Scalable Trio+ Lifecycle (Requires `ack_rules` for Writes)
13. `assess_trio_work`: Computes `D0..D4`, `R0..R4`, and recommended `P0..P4` profile from rubric inputs.
14. `create_trio_project`: Idempotent creation (`request_id`) of a `P0`..`P4` project bundle (minimum 3 core records for `P1+`, 1 fix record for `P0`, optional schedule/budget/UX/game modules).
15. `list_trio_projects` / `get_project_bundle`: Full bundle with `OWN` decisions, `REQ` `D I T A L` rows, `DIV`s, and checklist.
16. `save_resource` / `save_project_documents`: Guarded through `streamline-safe` (`422` on redaction hit, `409` on revision conflict or locked `OWN` mutation).
17. `restore_resource`: Appends a new revision restoring a prior revision's content.
18. `lock_project_plan`: Freezes reference revision and locks `OWN` rows.
19. `record_divergence`: Appends a `DIV-nnn` entry without altering `OWN` requirements.
20. `update_requirement_stage`: Advances `I` or `T` (worker), `A` (independent acceptor only for `R2+`), or `L` (custodian/owner after live read-back) with mandatory `evidence_ref`.
21. `check_sync` / `save_sync_report` / `get_sync_report`: Pairs locked plan rows against `AS-BUILT` rows and stores validated cross-model sync reviews.

### Hands-Off Conveyor & Release Gate
22. `dispatch_run`: Stages read-only inputs, launches `build` on a configured harness (or requests a worker lease via `worker_lease_url`), runs `CLEANUP` + `streamline-safe` scan, and auto-queues cross-family `accept`.
23. `get_run_status`: Returns phase, manifests, cleanup/scan booleans, and escalation details if at impasse.
24. `submit_how_it_works`: Saves the plain-English `HOW-IT-WORKS` guide (validated for required teaching sections and zero redaction hits) and sets project state to `DOC_PENDING_OWNER`.
25. `approve_release`: Owner-only action; approves `HOW-IT-WORKS`, unlocks `L` (Live) transition, triggers vault sync, and appends a scrubbed JSONL trajectory record.
26. `export_workspace`: Redacted portable archive of authorized shelves and Trio+ bundles.

## 5. State Machines (`L6`)

### 5.1 Session Rule Gate
`UNACKNOWLEDGED` -> (`ack_rules(current_sha256)`) -> `ACKNOWLEDGED`
If active `RULES` resource updates to a new `sha256`, grant reverts to `UNACKNOWLEDGED` until re-acknowledged.

### 5.2 Requirement Trace (`D I T A L`)
`D (Designed)` -> `I (Implemented)` -> `T (Tested)` -> `A (Accepted by non-builder / cross-family for R2+)` -> `L (Live verified)`
`UNKNOWN` is explicit whenever live state is unverified.

### 5.3 Conveyor Run Lifecycle
`STAGED` -> `BUILDING` -> `CLEANING` -> `SCANNING` -> `ACCEPTING` -> `DOC_PENDING_OWNER` -> `DONE`
Failure/stop branches:
- `IMPASSE`: 2-strike failure, unlisted missing input, or stall watchdog (`ESCALATION.md` recorded).
- `QUARANTINED`: Post-run cleanup could not be proven clean; harness slot held until cleared.

## 6. As-Built Ledger (`L14`)

*Status at `0.1.0` (`LIVE / CLOSED` for v0.1): Slices 1, 2, and 3 implemented, tested (`43 passed, 0 failed`), `HOW-IT-WORKS.md` approved by owner on `2026-11-18`, and running live as `streamline.service` (`127.0.0.1:5880`) with `2,933` imported legacy records, `3` forward lessons in `lessons_learned`, and synced vault markdown files.*

| Component | Target Location | State | Evidence |
| :--- | :--- | :--- | :--- |
| `streamline-core` | `crates/streamline-core` | `L` | `7/7` unit tests + Clippy `0` warnings |
| `streamline-safe` | `crates/streamline-safe` | `L` | `8/8` redaction/IP/segment tests + `422` live MCP block |
| `streamline-store` | `crates/streamline-store` | `L` | `6/6` SQLite/vault/legacy/lessons tests + `2,937`-file live vault sync |
| `streamline-conveyor` | `crates/streamline-conveyor` | `L` | `12/12` read-only stage, quarantine, 2-strike impasse, cross-family QC, and JSONL trajectory tests |
| `streamline-http` | `crates/streamline-http` | `L` | `10/10` HTTP/MCP/3-pane UI/conveyor tests + live `streamline.service` `/ready` `200` |
