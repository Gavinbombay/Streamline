# Streamline Security Notes

Streamline is built from the ground up for secure, self-hosted governance and workflow coordination. This document outlines core security design principles and operational safeguards.

## Unix-Only Build (`#[cfg(unix)]`)
Streamline compiles on Unix targets only (`compile_error!` on non-Unix) so `PinnedDir` (`openat`, `fstatat`, `mkdirat`, `renameat`, `O_NOFOLLOW`) has zero path-based fallback.

## Loopback by Default and Bounded Request Bodies
By default, Streamline binds its HTTP and Model Context Protocol servers strictly to loopback (`127.0.0.1:5880`), preventing accidental exposure on external network interfaces, and enforces a strict `2 MiB` request body ceiling (`DefaultBodyLimit`) across JSON routes. Operators deploying behind a trusted reverse proxy (such as Beacon) should configure appropriate upstream TLS and header forwarding.

## Hashed Grant Credentials and Server-Bound Model Families
Raw grant credentials are never stored or logged in plain text. Streamline stores only SHA-256 cryptographic hashes of grant credentials (`grant_digest`) alongside each grant's server-assigned `model_family` and `granted_tags` in SQLite. When a client authenticates, the supplied credential is hashed and compared against stored records, and builder or acceptor identity is derived strictly from the authenticated grant record rather than caller-supplied request fields. On `R2+` projects, all roles (including `Owner`) must hold a grant bound to a non-empty `model_family` for build/test/acceptance (`I`, `T`, `A`) transitions and sync reports, and any mismatched or unbound `actor_family` claim returns `403 Forbidden`.

## Mechanical Precondition Gate (`412 RULES_ACK_REQUIRED`)
Sessions are locked out of all write and execution capabilities until they explicitly acknowledge the current governance rules digest via `ack_rules`. If rules are updated, the digest changes and active sessions must re-acknowledge before performing mutations.

## Tag-Scoped Isolation, Uniform `404` Lookups, and Redacted Tag Errors
Access grants are scoped to specific tag filters (`granted_tags`). Clients can only read or overwrite authorized shelves and projects matching both existing and destination project scopes:
- `read_resource` (`resource_id`), `get_source_passport`, `update_source_passport` (`resource_id`), and cross-project `(shelf_slug, slug)` lookups return uniform `404 NOT_FOUND` without loading bodies or disclosing required tag names.
- Ungranted shelf-level `403 FORBIDDEN` responses return a generic scope message (`"forbidden: grant does not cover requested scope"`) without echoing `required_tag` or `detail`.
- Global vault and workspace operations (`sync_vault` and `export_workspace`, alongside `approve_release` and `export_trajectory`) require an `Owner` role grant (`403 Forbidden` for non-Owner grants), and non-wildcard `dispatch_run` grants (grants without `Owner` role or `*` tag) restrict declared `input_paths` to `<allowed_root>/projects/<project_id>`.

## High-Entropy Redaction Gate (`streamline-safe`)
The `streamline-safe` crate inspects all documents, resources, and commit inputs for credential-shaped literals, high-entropy strings, private LAN IP literals, and banned identifiers before persistence. Any violation triggers an immediate `422 Unprocessable Entity` response with zero bytes persisted.

## Root-Walked Descriptor Pinning, Ownership Checks, and Bounded Conveyor Scanning
`streamline_safe::PinnedDir` walks configured vault, run, input, and plugin directories component-by-component from `/` (or `.`) with `O_NOFOLLOW`, verifying every component is owned by `geteuid()` or `root` (`uid 0`) and rejecting world-writable paths (plus rejecting group-writable `0o022` paths on `presence_plugin_dir` and its assets). Operators must still keep configured data roots on trusted local filesystems protected from concurrent untrusted modification.

`streamline-conveyor` restricts `dispatch_run` `input_paths` to configured `allowed_input_roots` (and `<allowed_root>/projects/<project_id>` for non-wildcard grants), rejects symbolic links and non-UTF-8 directory entry names, and stages run inputs with strict operating system read-only permissions (`0444`/`0555`). Harness output scanning inspects both entry names and lossy-decoded file contents (replacing any path that fails redaction or character bounds with a hashed `redacted-path-*` placeholder in `ESCALATION.md`) and enforces explicit directory-entry (`512` across `readdir`), directory-depth (`32` levels), file-count (`512`), per-file (`4 MiB`), aggregate-byte (`16 MiB`), and finding-count (`64`) caps, stopping at `Impasse` (`OUTPUT_QUOTA_EXCEEDED`) when exceeded. `/downloads/streamline-skills.zip` and skill inspection tools serve the four `SKILL.md` documents exclusively from compile-time embedded bytes with zero runtime filesystem reads.

## Server-Computed Sync Verdicts and Cross-Family Quality Control
Plan locks validate target revisions against stored `resource_revisions`, post-lock requirement definition edits and `Live` (`L`) stage promotions or post-release mutations are restricted server-side to `Owner` grants (rejecting any caller-supplied `actor_is_custodian` claim), requirement definition edits reset prior acceptance state, `save_sync_report` persists `check_sync` verdicts directly from server-side analysis (so unmatched `AS-BUILT` claims produce `Partial` rather than `Pass`), and higher-risk Trio+ workflows (`R2` and above) mechanically enforce a grant-bound `model_family` for all roles (including `Owner`) and require `builder_family != acceptor_family`.

## Optional Plugin Boundary (`presence_plugin_dir`)
The signature visual canvas and brand assets remain completely private. Streamline ships with a clean pluggable status mark slot in the open-source distribution and loads custom presence plugins only when an operator explicitly configures a local private asset directory. `presence_plugin_dir`, any subdirectories, and `presence.js` (and served assets) must pass strict root-walked `PinnedDir` checks (`O_NOFOLLOW`, owned by `geteuid()` or `uid 0`, and zero group/world write bits `(st_mode & 0o022) == 0`); otherwise the UI omits the plugin `<script>` tag and the asset route returns `404 NOT_FOUND`.

## Maintainer Dependency Checks
Run dependency vulnerability, advisory, license, and source audits from the repository root:

```sh
cargo audit
cargo deny check advisories bans licenses sources
```

Repository publication and Git host branch/tag protections are managed separately by the repository owner after `AUDIT.md` passes.
