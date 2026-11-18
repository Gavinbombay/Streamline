# Assessment: Streamline (Open-Source Server, MCP Referee, and Trio+ Conveyor)

Version: `0.1.0-d1`
Profile: `P2` (`V1.md`, `SCHEMA.md`, `UI-UX-QOL.md`, `HANDHOLD.md`)

## 1. Difficulty and Risk Scoring

### Difficulty (`D3`)
- **D1 Scope breadth (`3`)**: Combines a curated governance catalog, owner profile and skill selector, scalable Trio+ project engine (`P0` to `P4`), hands-off worker/QC conveyor, MCP endpoint, and 3-pane web workspace.
- **D2 Interfaces and state (`3`)**: SQLite persistence, Obsidian-compatible markdown folder sync, session rule-acknowledgement gate, locked `OWN` vs `DIV` state machine, and pluggable worker/memory webhooks.
- **D3 Uncertainty (`1`)**: Core patterns already proven across prior Rust catalog crates and the Streamline `0.7.0` MCP/UI prototype; toolchain (`rustc 1.98.1`) verified on the build host.
- **D4 UI and interaction (`2`)**: Preserves the Streamline dark palette, accessible sizing (`16px` body, `14px` metadata floor, `44px` minimum targets), side-by-side plan comparison, 3-pane reader, and pluggable local Presence slot.
- **D5 Novelty (`1`)**: Unifies two existing first-party designs into one standalone open-source binary.

### Risk (`R2`)
- **R1 Live exposure (`2`)**: Runs loopback/LAN by default with optional reverse-proxy exposure; enforces tag-scoped bearer/OAuth grants and high-entropy redaction checks on every write.
- **R2 Irreversibility (`1`)**: Append-only revision history on resources; `HOLD_PUSH` enforced locally until security audit and owner sign-off pass.
- **R3 Blast radius (`2`)**: Open-source repository under `Gavinbombay/Streamline`; requires zero private hostnames, zero LAN addresses, and zero secret-shaped fixtures from commit 1.
- **R4 Evidence standing (`1`)**: Real loopback HTTP/MCP tests, SQLite round-trips, redaction negatives, and rendered terminal/UI captures required per slice.
- **R5 Authorization and custody (`2`)**: Server-enforced rule acknowledgement before writes/dispatches; owner sign-off (`DOC-PENDING-OWNER`) required before closing a release.

## 2. Why Profile `P2`

`Streamline` is both an AI-driven Rust/Axum service (`T-SVC`) and a human-facing 3-pane web workspace (`T-APP`). `P2` keeps the design tight in four focused files (`V1.md`, `SCHEMA.md`, `UI-UX-QOL.md`, `HANDHOLD.md`) without splitting into redundant sub-files, while giving the preserved visual tokens, Presence plugin boundary, and 3-pane journeys a dedicated `UI-UX-QOL.md` contract.
