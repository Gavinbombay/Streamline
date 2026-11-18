# Streamline Security Notes

Streamline is built from the ground up for secure, self-hosted governance and workflow coordination. This document outlines key security design principles and operational safeguards.

## Loopback by Default and Bounded Request Bodies
By default, Streamline binds its HTTP and Model Context Protocol servers strictly to loopback (`127.0.0.1:5880`), preventing accidental exposure on external network interfaces, and enforces a strict `2 MiB` request body ceiling (`DefaultBodyLimit`) across JSON routes. Operators deploying behind a trusted reverse proxy (such as Beacon) should configure appropriate upstream TLS and header forwarding.

## Hashed Grant Credentials and Server-Bound Model Families
Raw grant credentials are never stored or logged in plain text. Streamline stores only SHA-256 cryptographic hashes of grant credentials (`grant_digest`) alongside each grant's server-assigned `model_family` and `granted_tags` in SQLite. When a client authenticates, the supplied credential is hashed and compared against stored records, and builder or acceptor identity is derived strictly from the authenticated grant record rather than caller-supplied request fields. On `R2+` projects, all roles (including `Owner`) must hold a grant bound to a non-empty `model_family` for build/test/acceptance (`I`, `T`, `A`) transitions and sync reports, and any mismatched or unbound `actor_family` claim returns `403 Forbidden`.

## Mechanical Precondition Gate (`412 RULES_ACK_REQUIRED`)
Sessions are locked out of all write and execution capabilities until they explicitly acknowledge the current governance rules digest via `ack_rules`. If rules are updated, the digest changes and active sessions must re-acknowledge before performing mutations.

## Tag-Scoped Shelf, Project, and Owner Isolation (`granted_tags`)
Access grants are scoped to specific tag filters (`granted_tags`). Clients can only read or overwrite authorized shelves and projects matching both existing and destination project scopes, returning strict isolation barriers and zero capacity leaks for unauthorized areas. Global vault and workspace operations (`sync_vault` and `export_workspace`, alongside `approve_release` and `export_trajectory`) require an `Owner` role grant (`403 Forbidden` for non-Owner grants), and non-wildcard `dispatch_run` grants (grants without `Owner` role or `*` tag) restrict declared `input_paths` to `<allowed_root>/projects/<project_id>`.

## High-Entropy Redaction Gate (`streamline-safe`)
The `streamline-safe` crate inspects all documents, resources, and commit inputs for credential-shaped literals, high-entropy strings, private LAN IP literals, and banned identifiers before persistence. Any violation triggers an immediate `422 Unprocessable Entity` response with zero bytes persisted.

## Descriptor-Pinned, No-Symlink File Boundaries (`PinnedDir`, Conveyor, Vault Sync, and Skills Zip)
Streamline uses descriptor-pinned `openat`/`O_NOFOLLOW`/`renameat` operations (`streamline_safe::PinnedDir`) across `sync_vault_dir`, `streamline-conveyor` input staging, JSONL trajectory exports, and optional `presence_plugin_dir` asset serving to eliminate pathname check-to-use (TOCTOU) symlink escapes. `streamline-conveyor` restricts `dispatch_run` `input_paths` to configured `allowed_input_roots` (and `<allowed_root>/projects/<project_id>` for non-wildcard grants), rejects symbolic links, and stages run inputs with strict operating system read-only permissions (`0444`/`0555`). `/downloads/streamline-skills.zip` and skill inspection tools serve the four `SKILL.md` documents exclusively from compile-time embedded bytes with zero runtime filesystem reads.

## Server-Computed Sync Verdicts and Cross-Family Quality Control
Plan locks validate target revisions against stored `resource_revisions`, requirement definition edits reset prior acceptance state, `save_sync_report` persists `check_sync` verdicts directly from server-side analysis (so unmatched `AS-BUILT` claims produce `Partial` rather than `Pass`), and higher-risk Trio+ workflows (`R2` and above) mechanically enforce a grant-bound `model_family` for all roles (including `Owner`) and require `builder_family != acceptor_family`.

## Optional Plugin Boundary (`presence_plugin_dir`)
The signature visual canvas and brand assets remain completely private. Streamline ships with a clean pluggable status mark slot in the open-source distribution and loads custom presence plugins only when an operator explicitly configures a local private asset directory, serving verified non-symlink assets via `PinnedDir`.

## Dependency Hygiene, Audits, and Operational Safeguards
Continuous dependency audits are maintained using `cargo audit` and `cargo deny` with standard network access during release pipelines. The committed `scripts/pre-push` helper (which maintainers may enable locally via `git config core.hooksPath scripts` to check `AUDIT.md` and `STREAMLINE_ALLOW_PUSH=1`) is strictly a local maintainer workflow aid rather than a hosting security boundary; repository branch protections, tag rules, and push authorization must be configured independently on the Git host.
