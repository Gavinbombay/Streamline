<p align="center"><img src="docs/assets/streamline-mark.svg" alt="Streamline mark" width="88"></p>

# Streamline

### Your context. One enforced workflow.

[![License](docs/badges/license.svg)](LICENSE)
[![Build](docs/badges/build.svg)](#build)
[![Security Audit](docs/badges/security-audit.svg)](AUDIT.md)

**A Rust governance catalog, personal context store, Model Context Protocol referee, and hands-off Trio+ workflow server.**

Streamline stores durable rules, project plans, Source Passports, and reusable lessons in an embedded SQLite database with optional two-way sync to an Obsidian-compatible markdown vault, exposing one gated surface to humans and AI coding agents alike.

[For AI agents](#for-ai-agents) · [Architecture](#architecture) · [Quickstart](#quickstart) · [Deploying behind a proxy](#deploying-behind-a-proxy) · [Security notes](docs/SECURITY.md)

---

Streamline grew out of running multiple AI assistants across real engineering projects and hitting the same failure modes over and over: agents skipping project rules, silently overwriting owner choices, marking unverified work live, or burying hard-won lessons inside old job folders. It puts a single compiled Rust server (`rust-version = "1.98"`) in front of your knowledge base and workflow state so rules are checked by code rather than politeness.

**Early-release status (`v0.1`)**: Slice 1 is built and verified across the HTTP/MCP server, 38 tools, SQLite store, two-way vault sync, redaction gate, Trio+ plan locking, central `lessons_learned`, `DOC_PENDING_OWNER` closeout gate, and `streamline-conveyor` cross-family and cleanup guards. Automated subprocess/webhook worker dispatch in the conveyor runner is staged for Slice 2.

## Features

- **Mechanical rule gate (`bootstrap` -> `ack_rules`)**: Write and workflow tools return `412 Precondition Failed` (`RULES_ACK_REQUIRED`) until the caller acknowledges the current `rules_sha256` digest. Updating rules automatically re-locks active grants until re-acknowledged.
- **Tag-scoped grants and fenced legacy shelves**: Issue SHA-256-hashed credentials scoped by shelf tags, write permission, and dispatch permission. `bootstrap` surfaces `active` items only, keeping imported `dormant` and `legacy` records searchable and revivable without polluting agent startup context.
- **High-entropy redaction gate (`streamline-safe`)**: Blocks PEM private blocks, credential prefixes, raw bearer strings, high-entropy literals, disallowed dashes, and private LAN literals (in `public_safe_mode`) with `422 REDACTION_BLOCKED` before any bytes are persisted.
- **Scalable Trio+ engine (`P0` to `P4`)**: Scores Difficulty (`D0` to `D4`) and Risk (`R0` to `R4`) to select the right document set, freezes owner `OWN` decisions on `lock_project_plan` (`409 OWN_LOCKED` on worker mutation), tracks requirements across `D -> I -> T -> A -> L`, and pairs plan rows against as-built reports (`check_sync`).
- **Two-way Obsidian markdown sync and central lessons**: Keeps append-only SQLite revisions in step with plain `.md` files on disk (`vault_sync_dir`), and surfaces tagged `lessons_learned` records automatically in `bootstrap` and `query_lessons`.
- **Three-pane Web UI + portable skill bundle**: Serves a dark-palette three-pane workspace (`/`), an optional local `Presence` plugin slot (`/_streamline/presence-plugin/`), and `/downloads/streamline-skills.zip` (`streamline-director`, `streamline-worker`, `streamline-acceptor`, `streamline-vault`).

## For AI agents

Streamline exposes both a **Streamable HTTP MCP endpoint** (`POST /mcp`), a **stdio MCP loop** (`streamline mcp-stdio`), and a **typed REST tool API** (`POST /api/tools/:tool_name`) backed by the same 38-tool registry.

- **Authentication**: Pass a grant credential via `Authorization: Bearer <CREDENTIAL>` or `X-Streamline-Credential: <CREDENTIAL>` (or `STREAMLINE_CREDENTIAL` in `mcp-stdio` mode). Only `GET /ready` and `ready` are unauthenticated.
- **Mandatory rule handshake**: Call `bootstrap` first to read `current_rules_sha256` and your `granted_tags`, then call `ack_rules` with that exact digest. Any mutating tool called beforehand fails with HTTP `412` (`RULES_ACK_REQUIRED`).
- **Downloadable role skills**: Fetch `/downloads/streamline-skills.zip` for ready-to-use `SKILL.md` instructions covering director, worker, acceptor, and vault workflows.

| Surface / Tool Group | Endpoint or Tools | Auth / Gate | Purpose |
| :--- | :--- | :--- | :--- |
| Health & Discovery | `GET /ready`, `ready`, `bootstrap`, `ack_rules` | `ready` open; `bootstrap` / `ack_rules` require credential | Inspect `rules_sha256`, read active context, unlock writes |
| Shelves & Passports | `list_shelves`, `list_resources`, `read_resource`, `search_resources`, `resource_history`, `compare_resources`, `get_source_passport`, `update_source_passport`, `set_lifecycle_tier` | Credential + shelf tag; writes require `ack_rules` | Browse active or dormant shelves, inspect diffs and freshness |
| Profile & Skills | `get_workspace`, `list_skills`, `select_context` | Credential + tag check | Inspect caller permissions and select role context |
| Trio+ Planning & Sync | `assess_trio_work`, `create_trio_project`, `list_trio_projects`, `get_project_bundle`, `save_resource`, `save_project_documents`, `restore_resource`, `lock_project_plan`, `upsert_own_decision`, `record_divergence`, `upsert_requirement_row`, `update_requirement_stage`, `check_sync`, `save_sync_report`, `get_sync_report` | Credential + `ack_rules` + role checks | Size `P0`..`P4`, lock `OWN` rows, log `DIV`s, trace `D I T A L` |
| Closeout & Lessons | `mark_project_cleanup_verified`, `record_lesson`, `record_no_new_lessons`, `query_lessons`, `submit_how_it_works`, `approve_release`, `sync_vault`, `export_workspace` | `approve_release` requires `owner` role | Verify cleanup, save forward lessons, gate on `HOW-IT-WORKS` |
| Web UI & Bundles | `GET /`, `GET /downloads/streamline-skills.zip`, `POST /mcp`, `POST /api/tools/:tool_name` | UI open on local bind; tools enforce grant auth | 3-pane workspace, skill zip archive, MCP/REST dispatch |

Example handshake over the REST tool endpoint:

```sh
curl -sS -X POST http://127.0.0.1:5880/api/tools/bootstrap \
  -H "Authorization: Bearer <CREDENTIAL>"
curl -sS -X POST http://127.0.0.1:5880/api/tools/ack_rules \
  -H "Authorization: Bearer <CREDENTIAL>" \
  -H "Content-Type: application/json" \
  -d '{"rules_sha256":"<rules_sha256>"}'
```

## Architecture

Streamline is organized as a Cargo workspace with five focused crates under a single CLI binary:

```text
streamline (src/main.rs: serve | mcp-stdio | init-grant | import-legacy | sync-vault)
  ├── crates/streamline-core     Domain models, P0..P4 Trio+ planner, OWN lock rules, D I T A L checks
  ├── crates/streamline-safe     Redaction engine, private-IP filter, dash guard, path sanitizer
  ├── crates/streamline-store    Single SQLite writer (rusqlite), append-only revisions, Obsidian vault sync
  ├── crates/streamline-conveyor Read-only manifest hashing, stray-artifact cleanup checks, cross-family guard
  └── crates/streamline-http     Axum router, Streamable HTTP + stdio MCP handler, REST tools, 3-pane Web UI
```

Design documents live in [`DESIGN.md`](DESIGN.md) and [`docs/trio/`](docs/trio/V1.md). Plain-English operational walkthroughs live in [`HOW-IT-WORKS.md`](HOW-IT-WORKS.md).

## Screenshots

Captured views and plain-text terminal transcripts are documented in [`docs/CAPTURES.md`](docs/CAPTURES.md).

- **MCP policy and rule gate (`412 RULES_ACK_REQUIRED` -> `ack_rules` -> `422 REDACTION_BLOCKED`)**: [`docs/captures/mcp-policy.svg`](docs/captures/mcp-policy.svg)
- **Trio+ closeout gate (`check_sync` -> `lessons_learned` -> `DOC_PENDING_OWNER` -> `approve_release`)**: [`docs/captures/conveyor-closeout.svg`](docs/captures/conveyor-closeout.svg)

## Quickstart

1. Verify the pinned Rust toolchain (`1.98.1`):

```sh
sh ./check-toolchain.sh
```

2. Create a local configuration file (for example `/tmp/streamline.toml` or `/etc/streamline/streamline.toml`) using loopback or RFC-5737 documentation addresses:

```toml
bind_http = "127.0.0.1:5880"
bind_admin = "127.0.0.1:5881"
db_path = "./data/streamline.sqlite3"
vault_sync_dir = "./data/vault"
runs_dir = "./data/runs"
public_safe_mode = true
```

3. Issue an owner grant from an environment variable (never pass raw credentials as CLI arguments):

```sh
export STREAMLINE_BOOTSTRAP_CRED="<CREDENTIAL>"
cargo run --locked -- --config ./streamline.toml init-grant \
  --principal-id owner-1 --handle owner --role owner \
  --credential-env STREAMLINE_BOOTSTRAP_CRED \
  --tags standards,runbooks,architecture,projects,skills,inbox,archive \
  --write --dispatch
```

4. Start the server (or run `mcp-stdio` for desktop MCP clients):

```sh
cargo run --locked -- --config ./streamline.toml serve
```

5. Check readiness at `http://127.0.0.1:5880/ready` and open `http://127.0.0.1:5880/` in a browser.

## Deploying behind a proxy

Streamline binds to `127.0.0.1:5880` by default and expects TLS termination, public DNS, and external rate limiting to live at a reverse proxy such as `Beacon`.

When placing Streamline behind a reverse proxy on an edge host (for example `192.0.2.10`):

- Keep `bind_http = "127.0.0.1:5880"` so Streamline never listens directly on public interfaces.
- Forward `Authorization` and `X-Streamline-Credential` headers unchanged to `/mcp` and `/api/tools/*`.
- Disable response buffering on `/mcp` so JSON-RPC responses return immediately.
- Leave `public_safe_mode = true` whenever external clients or public-safe repositories share the instance, and keep `presence_plugin_dir` empty unless loading private local brand assets from disk.

## How the Trio+ referee and conveyor work

1. **Assess and create**: `assess_trio_work` computes Difficulty (`D0`..`D4`), Risk (`R0`..`R4`), and a `P0`..`P4` profile, and `create_trio_project` materializes the minimal required records.
2. **Lock owner choices**: `lock_project_plan` freezes `OWN` decisions at `plan_locked_rev`. Workers cannot edit locked `OWN` text (`409 OWN_LOCKED`) and instead log real-world findings with `record_divergence` (`DIV-001`).
3. **Trace and verify**: Every requirement (`REQ-01`..) moves through `D -> I -> T -> A -> L` with an explicit `evidence_ref`. For `R2+` work, `streamline-conveyor` rejects `builder_family == acceptor_family` (`SAME_FAMILY_FORBIDDEN`), checks deterministic SHA-256 input/output manifests, and confirms `target`, `.venv`, `node_modules`, and `__pycache__` are removed (`verify_cleanup_clean`).
4. **Closeout gate**: `submit_how_it_works` requires verified cleanup, central `lessons_learned` entries (or an explicit `record_no_new_lessons`), and a plain-English `HOW-IT-WORKS` guide (checked for required sections, readable prose, and zero giant code dumps). The project waits at `DOC_PENDING_OWNER` until the owner calls `approve_release`.

For a complete non-technical walkthrough, see [`HOW-IT-WORKS.md`](HOW-IT-WORKS.md).

## Build

Run the repeatable verification suite from the repository root:

```sh
sh ./check-toolchain.sh
cargo fmt --all -- --check
cargo test --locked --workspace --all-targets
cargo build --locked --release
```

## License

Licensed under the [Apache License, Version 2.0](LICENSE). See [`NOTICE`](NOTICE) for attribution details.
