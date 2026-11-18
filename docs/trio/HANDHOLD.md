# Streamline Handhold: Slices, Traceability, Custody, and Checklist

Version: `0.1.0-d1`

## 1. Vertical Slice Build Order (`L8`)

Each slice reaches a real end-to-end journey on the actual binary before moving to the next:

### Slice 1: Core Rust Server, SQLite + Vault Store, Redaction Gate, and Enforced MCP
- Workspace setup (`Cargo.toml`, `check-toolchain.sh`, `crates/streamline-{core,safe,store,http}`), SQLite migrations, and two-way Obsidian markdown sync.
- Full MCP + HTTP API for `ready`, `bootstrap`, `ack_rules` (mechanical precondition), shelves + Source Passports, profile + skill selection, scalable `P0`..`P4` Trio+ projects, `OWN` lock vs `DIV` log, `check_sync` / `save_sync_report`, `HOW-IT-WORKS` submission, and `/downloads/streamline-skills.zip` (the 4 portable skills).
- **Exit proof**: Real loopback + LAN runs from both local agent clients (`Hermes` and desktop IDE) completing a `P1` Trio+ cycle end to end.

### Slice 2: Hands-Off Conveyor (`streamline-conveyor`)
- Read-only input staging (`0444`/`0555` + `SHA256SUMS` computed by Rust, not prompt prose), configurable CLI harness runner + optional `worker_lease_url` webhook, post-run `CLEANUP` verifier, `streamline-safe` output scan, automatic cross-family Acceptor QC trigger, and JSONL trajectory export.
- **Exit proof**: One locked `P1` task dispatched hands-off -> built by Family A -> auto-checked by Family B -> lands in `DOC_PENDING_OWNER` with clean scratch space and verified hashes.

### Slice 3: 3-Pane Web UI, Pluggable Presence Slot, Captures, and OSS Audit
- Server-rendered / lightweight progressive 3-pane Web UI with locked Streamline CSS tokens, side-by-side diff highlights, `DOC_PENDING_OWNER` approval inbox, pluggable `presence_plugin_dir` mount, and scoped external token/OAuth grants.
- `Beacon`-style README, badges, committed `.txt` + `.svg` captures (`docs/tools/render-captures.py`), `HOW-IT-WORKS.md`, and full 6-phase security audit (`AUDIT.md`) prior to any GitHub push decision.

## 2. Requirements Trace Table (`L9`: `D I T A L`)

| ID | Source | Requirement | Verification | D | I | T | A | L |
| :--- | :--- | :--- | :--- | :-: | :-: | :-: | :-: | :-: |
| `REQ-01` | `OWN-001` | Rust 1.98 + Axum + SQLite workspace builds cleanly offline/locked with zero Python product services | `check-toolchain.sh` + `cargo test --workspace` (`43/0`) | X | X | X | X | X |
| `REQ-02` | `OWN-004` | Mechanical `bootstrap` + `ack_rules` blocks write/dispatch calls (`412`) until current rules digest is acknowledged | Integration test `ack_gate_test` + live `/mcp` | X | X | X | X | X |
| `REQ-03` | `OWN-004` | Tag-scoped grants enforce shelf/project read & write isolation (`403` on ungranted shelves) | Integration test `tag_isolation_test` | X | X | X | X | X |
| `REQ-04` | `OWN-004` | `streamline-safe` rejects high-entropy credentials, private LAN literals (in public-safe mode), and banned patterns (`422`) | Unit + MCP negative suite `redaction_gate_test` | X | X | X | X | X |
| `REQ-05` | `OWN-004` | Shelves and `SourcePassport` store steward, verified timestamp, freshness/drift state, and lineage | Store + HTTP round-trip test | X | X | X | X | X |
| `REQ-06` | `OWN-008` | Two-way Obsidian markdown sync imports clean edits as revisions and flags conflicts without data loss | Store sync test + `2,937`-file live vault sync | X | X | X | X | X |
| `REQ-07` | `OWN-004` | Profile and communication skills (`select_context`) serve active voice/rules without merging voices | MCP `select_context` contract test | X | X | X | X | X |
| `REQ-08` | `OWN-004` | `create_trio_project` scales records from `P0` (1 fix record) to `P4` with idempotent retry (`request_id`) | Trio+ planner & retry suite | X | X | X | X | X |
| `REQ-09` | `OWN-004` | `lock_project_plan` freezes `OWN` decisions (`409` on edit) while permitting `record_divergence` (`DIV`) | Mutation guard test `own_lock_test` | X | X | X | X | X |
| `REQ-10` | `OWN-004` | `check_sync` pairs plan vs `AS-BUILT` rows and `save_sync_report` validates evidence references | Sync pairing & validation suite | X | X | X | X | X |
| `REQ-11` | `OWN-004` | `streamline-conveyor` stages read-only inputs, runs builder, verifies `CLEANUP` + scan, and enforces `builder_family != acceptor_family` | `12/12` conveyor e2e harness tests | X | X | X | X | X |
| `REQ-12` | `OWN-007` | Release stays blocked at `DOC_PENDING_OWNER` until `HOW-IT-WORKS.md` passes section validation and owner calls `approve_release` | Release gate e2e + owner `HOW-IT-WORKS` sign-off | X | X | X | X | X |
| `REQ-13` | `OWN-004` | `/downloads/streamline-skills.zip` serves validated `director`, `worker`, `acceptor`, and `vault` skills | Archive byte & PKZIP CRC check | X | X | X | X | X |
| `REQ-14` | `OWN-005` | 3-pane Web UI preserves exact Streamline dark palette, `16px`/`14px` floors, `44px` targets, and expanded word diffs | DOM/CSS assertion suite | X | X | X | X | X |
| `REQ-15` | `OWN-006` | Open-source UI renders default status mark while loading private Presence canvas only when `presence_plugin_dir` is configured | Plugin mount isolation test | X | X | X | X | X |
| `REQ-16` | `OWN-002/003` | Repo ships `Apache-2.0`, `NOTICE`, `Beacon`-style README/captures/badges, zero private strings, optional `scripts/pre-push` release-hygiene hook, and `AUDIT.md` PASS | Hygiene scan `0/0/0`; optional local `scripts/pre-push` hook (`HOLD_PUSH`); Git host protections configured separately | X | X | X | | |
| `REQ-17` | `OWN-011` | Closeout enforces verified temp cleanup (`mark_project_cleanup_verified`), forward lessons in the central `lessons_learned` DB (`record_lesson` / `query_lessons`), and plain-English non-technical `HOW-IT-WORKS.md` approval | Store + MCP closeout gate + `3` live central lessons | X | X | X | X | X |

## 3. Four-Gate Independent Acceptance (`L10`)

1. **Construction Quality**: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace --all-targets`, zero compiler warnings.
2. **Source & Reference Fidelity**: CSS token parity with locked Streamline palette; `Beacon` house-style parity on root docs, `LICENSE`, `NOTICE`, and captures.
3. **Usability**: Real MCP connection from both `Hermes` and desktop `Antigravity` executing `bootstrap -> ack_rules -> create_trio_project -> lock -> sync -> HOW-IT-WORKS` without manual file hacking.
4. **Operational Readiness**: Runs under a dedicated unprivileged systemd unit on the local Linux host (`127.0.0.1` / LAN allowlist), clean SQLite backup/restore verified, rollback tested, and `CLEANUP` leaves zero stray build/temp artifacts outside managed dirs.

## 4. Custody, `HOLD_PUSH`, and Cleanup (`L12`, `L13`)

- **Local Git Custody**: Initialized in the local repository (`git config user.name "[owner]"`, `git config user.email "owner@example.com"`).
- **Optional `HOLD_PUSH` Release-Hygiene Hook (`scripts/pre-push`)**: No GitHub remote is configured during build. Maintainers may enable the committed `scripts/pre-push` hook locally (`git config core.hooksPath scripts`), which exits nonzero (`HOLD_PUSH active`) until `AUDIT.md` exists at the repository root and `STREAMLINE_ALLOW_PUSH=1` is set explicitly. This local hook is an optional maintainer release-hygiene convenience, NOT a runtime security boundary or server-enforced control; repository hosting branch and tag protections must be configured independently on the Git host.
- **Per-Slice `CLEANUP`**: Scratch probes and temporary test DBs run under ephemeral temp dirs removed on exit; `cargo clean` or shared target pruning keeps disk usage well below the `60%` yellow mark.

## 5. Ordered Execution Checklist (`L15`)

| # | Phase | Role | Action | Observable / Evidence | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `C-01` | Preflight | Director | Verify `rustc 1.98.1`, free disk, and initialize the local repository with `[owner]` git identity + `scripts/pre-push` `HOLD_PUSH` release-hygiene hook | Commit `b853154d0b60` + hook exit check | PASS |
| `C-02` | Slice 1 | Worker | Implement `streamline-{core,safe,store,http}` + 4 skills zip + unit/integration tests | `cargo test --workspace` `30/30` rc `0` + Clippy `0` | PASS |
| `C-03` | Slice 1 | Acceptor | Connect live MCP; execute real `P1` Trio+ lifecycle, `2,933`-item legacy import, `bootstrap` fencing, central lessons, and skills zip verification | Live loopback MCP e2e (`200`, `0` temp files left) | PASS |
| `C-04` | Slice 2 | Worker | Implement `streamline-conveyor` (read-only stage, runner, cleanup/scan gate, cross-family QC, JSONL export) | `12/12` conveyor e2e tests (`conveyor-closeout.svg`) | PASS |
| `C-05` | Slice 3 | Worker | Implement 3-pane Web UI + `presence_plugin_dir` hook + `Beacon`-style README, captures, and `HOW-IT-WORKS.md` | `10/10` HTTP/UI tests (`mcp-policy.svg`), `streamline.service` active, `cargo clean` `2.6 GiB` freed | PASS |
| `C-06` | Audit | Acceptor | Run 6-phase `cloudflare/security-audit-skill` (`Codex Luna`) + zero-hit scan right before GitHub push; configure Git host branch/tag protections independently of local `scripts/pre-push` | Parked until owner triggers GitHub push | PARKED |
| `C-07` | Closeout | Owner | Review plain-English `HOW-IT-WORKS.md` (`DOC-PENDING-OWNER`); later supply logo assets and decide on GitHub push | Owner approved `HOW-IT-WORKS.md` (`2026-11-18`) | PASS |
