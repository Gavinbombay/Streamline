# Streamline security audit

The final security audit completed on **2026-11-18** (America/St_Johns), against commit `4446b50c6a640544c4bc66cde51e227323d8db98`. It was a **full audit, all phases, standard profile**, with a complete run status, a passing final clean critic, and **zero open findings**. No confirmed, parked, or needs-validation items remained in the final Pass 11 (`r11`) findings record (`46` active coverage units covered, `0` candidate, `1` removed hook unit not applicable).

## Method and provenance

Independent lane reviewers (`gpt-6-luna`) used [cloudflare/security-audit-skill](https://github.com/cloudflare/security-audit-skill), vendored at commit `c1c8a8c1471069fb0e188eeaff69b8e8db6564a8` under the MIT license. The result describes the use of that open-source method by the independent reviewers. It does not imply that the skill's publisher performed or endorsed this audit.

The full review series covered the Rust workspace (`streamline-core`, `streamline-safe`, `streamline-store`, `streamline-conveyor`, `streamline-http`, and `src/main.rs`), bundled skills, capture generator tooling, and repository documentation. Compatible prior coverage was carried forward with its recorded evidence while every changed source unit received fresh hunting, verification, and critic review. All six phases were executed:

1. Reconnaissance mapped architecture, trust boundaries, entry points, and coverage units.
2. Coverage-led hunting reviewed application authorization, SQLite and Obsidian vault persistence, Unix descriptor-pinned filesystem paths, child-process conveyor execution, HTTP and Model Context Protocol routes, user-interface scripts, and documentation. Post-wave critics checked coverage completeness.
3. Candidate validation independently re-verified prior findings and newly surfaced leads against current source.
4. Structured output validated `findings.json` and `coverage-ledger.json` against the unmodified schemas and scripts.
5. Independent record verification audited claim-to-evidence alignment before an independent final clean critic passed with zero discrepancies.
6. Target-neutral reporting assembled `REPORT.md`, `FINDINGS-DETAIL.md`, `NEEDS-VALIDATION.md`, and `run-metadata.json`.

The retained lane packages (`secaudit-v010-r1` through `secaudit-v010-r11`) preserve reconnaissance, per-agent hunting transcripts, `coverage-ledger.json`, `findings.json`, `record-verification.md`, `REPORT.md`, and `run-metadata.json`. Unmodified `validate-findings.cjs`, `validate-coverage-ledger.cjs`, and their 65 unit self-tests exited `0`. Those internal artifacts are retained privately outside this repository; this document is their public summary.

## How the result changed across runs

| Run | Result and subsequent work |
| --- | --- |
| Pass 1 (`r1`, `1abbd2c517fb`) | **Initial baseline (`6` confirmed, `4` needs-validation).** Surfaced existing-row tag authorization on `save_resource`, plan-lock revision existence checks, caller-claimed `actor_family` on `R2+` acceptance, unverified caller sync verdicts, requirement-edit acceptance resets, `render-captures.py` symlink overwrite, and pathname symlink/scope leads across conveyor inputs, trajectories, vault sync, and skills archive. |
| Pass 2 (`r2`, `d104e69c5cab`) | **All 4 medium Pass 1 findings verified closed (`2` low confirmed, `8` needs-validation).** Added pre-overwrite scope checks, revision table verification, grant-bound `model_family` storage, server-side `save_sync_report`, atomic SVG writes, `2 MiB` HTTP body cap, and initial symlink checks. Remaining items covered unbound `Owner` fallback on `R2+`, `Partial` sync verdicts on unmatched `AS-BUILT` claims, `Owner`-gating on `sync_vault`, project-scoped dispatch roots, and pathname check-to-use windows. |
| Pass 3 (`r3`, `ea308af64af7`) | **Both Pass 2 confirmed items closed (`1` low confirmed, `8` needs-validation).** Added Unix `PinnedDir` (`openat`, `fstatat`, `mkdirat`, `renameat`, `O_NOFOLLOW`), compile-time embedded skills zip (zero disk reads), `Owner`-only `sync_vault` and `export_workspace`, and `<allowed_root>/projects/<project_id>` dispatch scoping. Surfaced `read_resource` `resource_id` `403`/`404` existence oracle, `#[cfg(not(unix))]` fallback paths, ancestor resolution, `presence_plugin_dir` write modes, and conveyor output scan quotas. |
| Pass 4 (`r4`, `0838a6b34212`) | **Zero confirmed findings (`0` confirmed, `1` needs-validation).** Enforced Unix-only compilation (`compile_error!` on non-Unix), root-walked `PinnedDir` from `/` with UID and mode checks (`0o022 == 0` on `presence_plugin_dir`), uniform `404` on restricted `resource_id` and cross-project lookups, redacted `ForbiddenTag` HTTP responses, bounded output file/byte/finding caps, and `Beacon`-aligned publication docs. Only unbounded `readdir` entry materialization remained. |
| Passes 5 to 10 (`r5`..`r10`) | **Zero confirmed findings; iterative closure of 1 remaining boundary lead per pass.** Added `list_entry_names_bounded` and `32`-level tree-depth caps (`a56b272f5e10`); rejected non-UTF-8 `dirent` names and switched output scanning to lossy UTF-8 (`da8c07cf68dc`); restricted `Live` (`L`) promotion and post-lock requirement edits to `Owner` grants (`a87274ca5238`); scanned output entry names and hashed unsafe diagnostic/escalation paths in `runner.rs` (`38afabe85def`) and `staging.rs` / `map_conveyor_error` (`397ab2ed542b`); isolated child harnesses with `Stdio::null()`, `300s` runtime timeouts, and `max_concurrency` slots (`f9452f90e6fe`). |
| Pass 11 (`r11`, `4446b50c6a64`) | **Complete standard-profile PASS at the commit above.** `run_harness_command_with_timeout` terminates and drains the isolated Unix process group (`SIGKILL` + `ESRCH` poll) on normal exit as well as on timeout and error before output scanning runs. All `46` active units are covered, with **zero confirmed and zero needs-validation findings**. |

## Recorded verification

The Pass 11 verification matrix at commit `4446b50c6a640544c4bc66cde51e227323d8db98` recorded **75 passed, 0 failed** across the Cargo workspace (`26` in `streamline-conveyor`, `16` in `streamline-safe`, `14` in `streamline-store`, `12` in `streamline-http`, and `7` in `streamline-core`). Required gates passed with exit code `0`: pinned toolchain verification (`rustc 1.98.1`), `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` (`0` warnings), `cargo test --locked --workspace --all-targets`, `cargo build --locked --release`, deterministic SVG capture regeneration (`docs/tools/render-captures.py`), and repository hygiene scans (`0 / 0 / 0`).

## Scope and limits

This is a static source and bounded local test review, not a live-deployment penetration test or a guarantee that software has no vulnerabilities. No live host, upstream proxy, or external network service was probed during audit execution. Operators remain responsible for keeping Streamline bound to loopback or behind an authenticated reverse proxy, storing configured `vault_sync_dir`, `runs_dir`, `allowed_input_roots`, and optional `presence_plugin_dir` paths on trusted local filesystems protected from concurrent untrusted modification, and configuring trusted `StreamlineConfig.harnesses` binaries that do not detach into separate sessions.

`cargo-audit` and `cargo-deny` were not installed on the local build host during this offline review, so RustSec advisory database and crates.io yanked-package checks were not executed in this run. Maintainers with network access should run the dependency commands in [`docs/SECURITY.md`](docs/SECURITY.md) before publishing releases.

This post-audit documentation update adds `AUDIT.md` and updates static documentation badges only. It preserves the exact Pass 11 audited source at commit `4446b50c6a640544c4bc66cde51e227323d8db98`, and the build and security-audit badges refer to that recorded evidence rather than a live continuous integration service.
