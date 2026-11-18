# How Streamline Works

A plain-English owner guide to every part of Streamline: what it does, where it runs, how a job moves from idea to approved release, how to check its health, and what to do when something stops.

## What Streamline Replaces

Before Streamline, rules, project plans, and lessons lived in scattered places: sticky notes, ad-hoc markdown folders, chat transcripts, and old database tables. Each AI assistant started fresh, could quietly skip rules, could overwrite an owner choice while trying to be helpful, or could mark its own work "done" without an independent check.

Streamline replaces that guesswork with one enforced system:

- **One durable rulebook and library**: Your active standards, runbooks, architecture notes, skills, and project records live in one place instead of across disconnected folders.
- **One fenced home for older ideas**: Past designs and wish-list cards move into `dormant` or `legacy` shelves (categories) so you can still search and revive them anytime without cluttering active AI sessions.
- **One mechanical referee**: AI assistants cannot write changes or launch builds until they prove they read the current rules, cannot alter your locked decisions, and cannot grade their own high-risk work.
- **One central memory for lessons**: Things learned on one job are saved into a shared learning table and automatically shown to future jobs with matching tags (labels), instead of sitting forgotten inside one old folder.

## Overview and Purpose

Streamline is a **single-binary** (a self-contained program file with no extra runtime packages to install) written in **Rust** (a programming language built for speed and memory safety). Inside that single program are three connected parts:

1. **Governance Catalog and Vault**: Organizes documents on named **shelves** (folder-like categories such as `standards`, `runbooks`, `architecture`, `projects`, `skills`, `inbox`, and `archive`). Every document carries a **Source Passport** (a cover card showing who stewards it, when it was last verified, whether it is `verified`, `stale`, `drift`, or `unknown`, and what action is needed next).
2. **Trio+ Project Referee**: Sizes each piece of work from `P0` (a small quick fix) up to `P4` (a full large commission), locks owner choices so workers cannot erase them, tracks every requirement through five stages (**D**esigned, **I**mplemented, **T**ested, **A**ccepted, **L**ive), and checks that the final build matches the locked plan.
3. **Hands-Off Conveyor Helpers and Release Gate**: Prepares clean read-only inputs, checks that temporary build folders are deleted, enforces cross-family review on riskier changes, and holds every completed version at `DOC_PENDING_OWNER` (waiting for the owner to read and approve `HOW-IT-WORKS.md`) before closing out.

Everything is stored in **SQLite** (a single-file database kept right on disk, with no separate database server process to maintain) and can sync two ways with an **Obsidian markdown vault** (a plain folder of `.md` text notes you can view and edit in Obsidian or any text editor).

## Runtime Location

Streamline runs as one program (`streamline`) on your local computer or private server. Here is what lives where:

- **Web and tool address (`bind_http`)**: By default, the built-in **Axum** (web server toolkit) listener binds to **loopback** (`127.0.0.1:5880`, meaning "this computer only") and never exposes itself directly to the public internet unless you place a **reverse proxy** (a front-door traffic router such as Beacon) in front of it.
- **Database file (`db_path`)**: A single SQLite file on disk (for example, `/var/lib/streamline/streamline.sqlite3`). All document revisions are **append-only** (new edits add a new numbered version instead of overwriting history).
- **Markdown sync folder (`vault_sync_dir`)**: An optional folder on disk holding plain `.md` files organized by shelf and a `lessons/` subfolder.
- **Run staging folder (`runs_dir`)**: A workspace folder where automated build and check runs stage read-only copies of inputs and verify cleanup afterward.
- **Configuration file**: Loaded from a **TOML** (simple `setting = value` text file) or **JSON** (structured text format) path set in `STREAMLINE_CONFIG_PATH` or `STREAMLINE_CONFIG_JSON`, or passed with `--config`.

A minimal configuration looks like this:

```toml
bind_http = "127.0.0.1:5880"
db_path = "/var/lib/streamline/streamline.sqlite3"
vault_sync_dir = "/var/lib/streamline/vault"
runs_dir = "/var/lib/streamline/runs"
public_safe_mode = true
```

Streamline listens to both human browsers and AI assistants at the same address:

- **Browser workspace (`/`)**: A dark-theme three-pane screen (left navigation, center work area, right reader/history/related/passport panel) with a pluggable status mark slot.
- **MCP (Model Context Protocol, a standard plug letting different AI tools use the same server) endpoint (`/mcp`)**: Accepts **JSON-RPC** (structured request/response messages) over web requests, or over **stdio** (a direct text pipe when a desktop app launches `streamline mcp-stdio`).
- **Tool API (`/api/tools/:tool_name`)** and **Skill bundle (`/downloads/streamline-skills.zip`)**: Direct HTTP endpoints for calling tools and downloading the four portable AI role guides (`streamline-director`, `streamline-worker`, `streamline-acceptor`, `streamline-vault`).

## Step-by-Step Flow

Every piece of work in Streamline moves through four clear stages from first connection to owner sign-off.

### 1. Starting a Session and Acknowledging Active Rules

Every person or AI assistant connects using a **grant** (a permission pass created by the owner) and a **credential** (the secret pass phrase that proves who holds that grant). Streamline stores only a **SHA-256 hash / digest** (a one-way digital fingerprint) of the credential, never the raw phrase.

1. The caller connects (sending `Authorization: Bearer <CREDENTIAL>` or `X-Streamline-Credential: <CREDENTIAL>` over HTTP, or `STREAMLINE_CREDENTIAL` in stdio mode) and calls `bootstrap`.
2. `bootstrap` returns the active rules digest (`rules_sha256`), the caller's allowed **tags** (access labels like `standards` or `project:demo`), active projects, onboarding document slugs, and relevant forward lessons.
3. Until the caller proves it saw the rules, every write or dispatch tool returns HTTP `412` (`RULES_ACK_REQUIRED`).
4. The caller calls `ack_rules` with the exact `rules_sha256` digest. That unlocks write tools matching the caller's grant. If the owner later updates the active rules, the digest changes and every caller automatically locks again until they acknowledge the new digest.

```text
connect -> bootstrap (get rules_sha256)
        -> write attempt before ack -> 412 RULES_ACK_REQUIRED
        -> ack_rules(rules_sha256) -> write tools unlocked
```

### 2. Browsing Active Shelves vs Fenced `dormant` and `legacy` Ideas

When Streamline imports older catalogs (using `streamline import-legacy --from <path>`), it sorts items cleanly so old clutter never hijacks new AI work:

- **Active tier (`active`)**: Current rules, standards, runbooks, and ongoing projects. `bootstrap` shows only `active` items, and only to callers whose grant includes the shelf's required tag.
- **Fenced triage tiers (`dormant` and `legacy`)**: Older design notes, product cards, and "maybe someday" ideas live here. They are excluded from `bootstrap` so AI sessions stay focused, while the owner can still search, browse, review (`triage_state`), and promote any worthy idea back to `active` with `set_lifecycle_tier`.
- **Retired tier (`retired`)**: Kept in revision history for auditability without cluttering daily work.

Every write passes through the **redaction gate** (`streamline-safe`), an automatic safety scanner that blocks private key blocks, API credential prefixes, raw bearer strings, high-entropy secret-looking strings, disallowed em/en dashes, and (when `public_safe_mode = true`) private local network addresses (`422 REDACTION_BLOCKED`) before a single byte is saved.

### 3. Sizing a Trio+ Project and Locking Owner Choices

When starting a piece of work, `assess_trio_work` scores two dimensions from `0` to `4`: **Difficulty** (`D0` to `D4`) and **Risk** (`R0` to `R4`). Those scores pick the smallest complete **profile** (`P0` to `P4`):

- **`P0` (Quick Fix)**: A single compact fix record for trivial, low-risk changes.
- **`P1` to `P2` (Standard Build)**: Core Trio+ records (`ASSESSMENT`, `V1`, `SCHEMA`, `HANDHOLD`, plus `UI-UX-QOL` when there is a screen) without bloated paperwork.
- **`P3` to `P4` (Expanded Commission)**: Adds optional modules (such as schedule, budget, or full system specs) only when complexity or risk actually demands them.

Inside a project, decisions are clearly separated by who owns them:

- **`OWN` decisions (`upsert_own_decision`)**: Binding owner choices (for example, "use SQLite" or "keep public-safe mode on"). Once the owner runs `lock_project_plan`, all `OWN` rows are frozen at that revision (`plan_locked_rev`). Any later attempt by a worker to edit a locked `OWN` row is rejected (`409 OWN_LOCKED`).
- **`DIV` divergences (`record_divergence`)**: When a worker hits a real-world surprise while building, the worker appends a `DIV-001` note recording what was designed, what was observed, and the proposed resolution. `OWN` remains untouched for the owner to review.

Every requirement row (`REQ-01`, `REQ-02`, and so on) moves step by step through **D -> I -> T -> A -> L**, and must carry an `evidence_ref` (a link to a test output, revision, or check):

- **`D` (Designed)**: Agreed in the locked plan.
- **`I` (Implemented)**: Built by the worker.
- **`T` (Tested)**: Verified by automated tests or checks.
- **`A` (Accepted)**: Reviewed by an independent checker (and for `R2+` work, must come from a different AI **model family** than the builder).
- **`L` (Live)**: Confirmed working in real life after release approval. Any unverified live claim stays marked `UNKNOWN` instead of guessing.

### 4. Hands-Off Building, Cleanup, Cross-Family Checking, Lessons, and `DOC_PENDING_OWNER`

To close out a Trio+ project cleanly, Streamline coordinates the **conveyor** (automated build-and-check handoff) and enforces a strict three-part release gate:

1. **Staged Build and Cleanup Verification**: Inputs are fingerprinted with a deterministic SHA-256 manifest, work runs in an isolated folder, and `verify_cleanup_clean` checks that stray temporary build folders (`target`, `.venv`, `venv`, `node_modules`, `__pycache__`) are gone before recording `mark_project_cleanup_verified`.
2. **Cross-Family Checking and Sync Report**: `check_sync` pairs locked plan requirements (`OWN` and `REQ` rows) against as-built claims. For `R2` and higher risk, `verify_cross_family` forbids the same model family from grading its own build. `save_sync_report` records the independent verdict.
3. **Central Lessons Learned (`lessons_learned`)**: The worker records reusable findings with `record_lesson` (tagged so `bootstrap` and `query_lessons` automatically surface them to future matching jobs), or explicitly records why none applied (`record_no_new_lessons`).
4. **Plain-English Guide and `DOC_PENDING_OWNER`**: `submit_how_it_works` checks that `HOW-IT-WORKS.md` covers all required sections, contains real plain-English prose (at least 200 characters, no code dumps over 15 lines), passes the redaction gate, and confirms both cleanup and lessons are satisfied. The project then pauses at `DOC_PENDING_OWNER` until the owner calls `approve_release`, which unlocks `L` (Live) status and syncs the vault.

*(Note for early `v0.1` releases: the `streamline-conveyor` crate provides the cross-family guard, manifest hashing, and cleanup checks today, while automated subprocess/webhook dispatch runs in the next build slice.)*

## Health and Logs

You can check Streamline's health at any time without logging in:

- **HTTP health check (`GET /ready`)**: Returns JSON showing `"ok": true`, `"status": "ready"`, the server version, whether `public_safe_mode` is active, and the current `current_rules_sha256` fingerprint.
- **MCP / tool health check (`ready`)**: Returns the exact same status payload inside an AI session.
- **Browser status screen (`GET /`)**: Shows active rule digest, safe mode status, and workspace state in the left sidebar and main view.
- **Server logs**: Streamline logs structured operational events using standard terminal logging controlled by `RUST_LOG` (default `info`). In `streamline serve`, logs go to standard output; in `streamline mcp-stdio`, logs go strictly to `stderr` so they never corrupt MCP JSON messages on `stdout`. Credential contents are never logged.
- **Audit trail (`audit_events`)**: Important actions (grant checks, rule acknowledgements, locks, sync reports, and releases) are recorded in SQLite with caller ID, target ID, and outcome.

## Failure Modes and Recovery

Here are the normal safety stops you may see, why they happen, and how to recover cleanly:

- **`401 UNAUTHORIZED` (Missing, Expired, or Revoked Pass)**: The caller sent no credential, a mistyped credential, or an expired/revoked grant. Fix: verify the environment variable or header holds the right credential, or issue a new grant with `streamline init-grant`.
- **`403 FORBIDDEN` (Tag, Write, or Role Limit)**: The caller's grant lacks the required shelf tag, `can_write`, or owner role (for example, a worker trying to call `approve_release`). Fix: keep the caller scoped to its job, or update the grant if genuinely needed.
- **`412 PRECONDITION_FAILED` / `RULES_ACK_REQUIRED`**: The caller has not acknowledged the active rules digest, or the owner updated `standards/rules` since the last session. Fix: call `bootstrap` to read the new `rules_sha256` and call `ack_rules`.
- **`409 OWN_LOCKED` or `CONFLICT`**: Either a worker tried to overwrite a locked `OWN` decision (fix: use `record_divergence` to log a `DIV` note instead) or two edits raced on the same document revision (fix: read the latest revision and re-apply or merge cleanly).
- **`422 REDACTION_BLOCKED`**: `streamline-safe` detected a secret-looking token, private key header, private LAN address (in public-safe mode), or disallowed dash. Zero bytes were saved. Fix: inspect the reported field and line number, remove the sensitive or disallowed text, and retry.
- **`SAME_FAMILY_FORBIDDEN` or `DirtyScratch` (`conveyor`)**: Either an `R2+` acceptance check was assigned to the same AI model family that built the code (fix: assign a distinct family), or leftover temporary directories like `target` or `node_modules` remained in the workspace (fix: delete stray artifacts and re-run cleanup verification).
- **Two-way Vault Sync Conflict**: If both the SQLite database and a `.md` file in `vault_sync_dir` changed independently, Streamline compares SHA-256 hashes rather than silently overwriting either side. Review the revision history (`resource_history`) and save or restore (`restore_resource`) the version you want.
