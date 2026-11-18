# Streamline Security Notes

Streamline is built from the ground up for secure, self-hosted governance and workflow coordination. This document outlines key security design principles and operational safeguards.

## Loopback by Default and Bounded Request Bodies
By default, Streamline binds its HTTP and Model Context Protocol servers strictly to loopback (`127.0.0.1:5880`), preventing accidental exposure on external network interfaces, and enforces a strict `2 MiB` request body ceiling (`DefaultBodyLimit`) across JSON routes. Operators deploying behind a trusted reverse proxy (such as Beacon) should configure appropriate upstream TLS and header forwarding.

## Hashed Grant Credentials and Server-Bound Model Families
Raw grant credentials are never stored or logged in plain text. Streamline stores only SHA-256 cryptographic hashes of grant credentials (`grant_digest`) alongside each grant's server-assigned `model_family` and `granted_tags` in SQLite. When a client authenticates, the supplied credential is hashed and compared against stored records, and builder or acceptor identity is derived from the authenticated grant record rather than caller-supplied request fields.

## Mechanical Precondition Gate (`412 RULES_ACK_REQUIRED`)
Sessions are locked out of all write and execution capabilities until they explicitly acknowledge the current governance rules digest via `ack_rules`. If rules are updated, the digest changes and active sessions must re-acknowledge before performing mutations.

## Tag-Scoped Shelf and Project Isolation (`granted_tags`)
Access grants are scoped to specific tag filters (`granted_tags`). Clients can only read or overwrite authorized shelves and projects matching both existing and destination project scopes, returning strict isolation barriers and zero capacity leaks for unauthorized areas.

## High-Entropy Redaction Gate (`streamline-safe`)
The `streamline-safe` crate inspects all documents, resources, and commit inputs for credential-shaped literals, high-entropy strings, private LAN IP literals, and banned identifiers before persistence. Any violation triggers an immediate `422 Unprocessable Entity` response with zero bytes persisted.

## Scoped, No-Symlink File Boundaries (`streamline-conveyor`, Vault Sync, and Skills Zip)
`streamline-conveyor` restricts `dispatch_run` `input_paths` to configured `allowed_input_roots`, canonicalizes every source path inside those roots, rejects symbolic links across input staging, trajectory exports, vault exports, and `/downloads/streamline-skills.zip`, and stages run inputs with strict operating system read-only permissions (`0444`/`0555`).

## Server-Computed Sync Verdicts and Cross-Family Quality Control
Plan locks validate target revisions against stored `resource_revisions`, requirement definition edits reset prior acceptance state, `save_sync_report` computes `check_sync` verdicts server-side, and higher-risk Trio+ workflows (`R2` and above) mechanically enforce that the quality control acceptance grant belongs to a different `model_family` than the builder grant.

## Optional Plugin Boundary (`presence_plugin_dir`)
The signature visual canvas and brand assets remain completely private. Streamline ships with a clean pluggable status mark slot in the open-source distribution and loads custom presence plugins only when an operator explicitly configures a local private asset directory.

## Dependency Hygiene and Audits
Continuous dependency audits are maintained using `cargo audit` and `cargo deny` with standard network access during release pipelines.
