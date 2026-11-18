# Streamline Architecture and Design Overview

Streamline is a single-binary Rust/Axum governance catalog, personal context store, Model Context Protocol (MCP) referee, and hands-off Trio+ workflow server backed by SQLite and an optional Obsidian-compatible markdown directory.

## Design Package (`P2`)

The complete maintained Trio+ design package lives in `docs/trio/`:

1. [`docs/trio/ASSESSMENT.md`](docs/trio/ASSESSMENT.md): Difficulty (`D3`), Risk (`R2`), and `P2` profile selection.
2. [`docs/trio/V1.md`](docs/trio/V1.md): Approved owner decisions (`OWN-001`..`OWN-008`), baseline evidence, scope, non-goals, critical journeys, and falsifiable critique.
3. [`docs/trio/SCHEMA.md`](docs/trio/SCHEMA.md): Crate ownership map, neutral TOML configuration, SQLite tables, 26-tool MCP/HTTP contract, state machines (`ack_rules`, `D I T A L`, conveyor lifecycle), and `AS-BUILT` ledger.
4. [`docs/trio/UI-UX-QOL.md`](docs/trio/UI-UX-QOL.md): Locked dark visual tokens, accessibility floors, pluggable private Presence slot, and 3-pane reader/workspace layout.
5. [`docs/trio/HANDHOLD.md`](docs/trio/HANDHOLD.md): 3-slice vertical build order, `REQ-01`..`REQ-16` trace table, 4-gate acceptance, `HOLD_PUSH` custody, and execution checklist.
