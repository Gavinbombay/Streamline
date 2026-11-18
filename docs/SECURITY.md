# Streamline Security Notes

Streamline is built from the ground up for secure, self-hosted governance and workflow coordination. This document outlines key security design principles and operational safeguards.

## Loopback by Default
By default, Streamline binds its HTTP and Model Context Protocol servers strictly to loopback (`127.0.0.1:5880`), preventing accidental exposure on external network interfaces. Operators deploying behind a trusted reverse proxy (such as Beacon) should configure appropriate upstream TLS and header forwarding.

## Hashed Grant Credentials
Raw credentials, tokens, and bearer keys are never stored or logged in plain text. Streamline stores only SHA-256 cryptographic hashes of grant credentials (`grant_digest`) in the database. When a client authenticates, the supplied credential is hashed and compared against stored records.

## Mechanical Precondition Gate (`412 RULES_ACK_REQUIRED`)
Sessions are locked out of all write and execution capabilities until they explicitly acknowledge the current governance rules digest via `ack_rules`. If rules are updated, the digest changes and active sessions must re-acknowledge before performing mutations.

## Tag-Scoped Shelf and Project Isolation (`granted_tags`)
Access grants are scoped to specific tag filters (`granted_tags`). Clients can only read authorized shelves and projects matching their granted scope, returning strict isolation barriers and zero capacity leaks for unauthorized areas.

## High-Entropy Redaction Gate (`streamline-safe`)
The `streamline-safe` crate inspects all documents, resources, and commit inputs for secret-shaped tokens, high-entropy credentials, private LAN IP literals, and banned identifiers before persistence. Any violation triggers an immediate `422 Unprocessable Entity` response with zero bytes persisted.

## Read-Only Input Staging (`streamline-conveyor`)
When executing automated runs, `streamline-conveyor` stages all input files with strict operating system read-only permissions (`0444`/`0555`) to prevent accidental modification or tampering during worker execution.

## Cross-Family Quality Control (`builder_family != acceptor_family`)
For higher-risk Trio+ workflows (`R2` and above), Streamline mechanically enforces that the quality control acceptance agent belongs to a different model family than the builder agent (`builder_family != acceptor_family`), preventing self-grading bias.

## Optional Plugin Boundary (`presence_plugin_dir`)
The signature visual canvas and brand assets remain completely private. Streamline ships with a clean pluggable status mark slot in the open-source distribution and loads custom presence plugins only when an operator explicitly configures a local private asset directory.

## Dependency Hygiene and Audits
Continuous dependency audits are maintained using `cargo audit` and `cargo deny` with standard network access during release pipelines.
