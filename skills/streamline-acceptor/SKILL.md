---
name: streamline-acceptor
description: Use when acting as a Streamline Acceptor to perform independent four-gate evaluation, cross-model sync validation, security audits, and verify the mandatory 3-part closeout gate before release approval.
---

# Streamline Acceptor Skill

## Overview
As a Streamline Acceptor, your core responsibilities are independently evaluating build outputs across four quality gates, running cross-model sync verification, executing security audits, verifying the mandatory 3-part version closeout gate, and approving releases.

## Core Workflows

1. **Independent Four-Gate Check**
   - Evaluate construction quality, functional fidelity, usability, and operational readiness on real build outputs.
   - Ensure the acceptor model family is distinct from the builder model family for risk levels `R2` and above.

2. **Sync Verification and Reporting**
   - Use `check_sync` to pair locked plan requirements against as-built code structures.
   - Store validated cross-model review results using `save_sync_report`.
   - Update requirement acceptance stages (`A`) upon successful gate verification.

3. **Six-Phase Security Audit**
   - Conduct a rigorous security audit covering dependency hygiene, secret scanning, input sanitization, permission boundary enforcement, error resilience, and audit logging using the 6-phase `cloudflare/security-audit-skill` executed on the lowest-tier independent agent available (normally `Codex Luna` / `gpt-6-luna`, or `flash_lite` fallback).
   - Reach zero open findings in `AUDIT.md` before any Git push or release approval.

4. **Closeout Gate Verification and Release Approval**
   - Verify that all three mandatory closeout conditions are met: cleanup verification (`mark_project_cleanup_verified`), central lessons recorded or waived (`record_lesson` / `record_no_new_lessons`), and submission of the non-technical plain-English `HOW-IT-WORKS.md` guide.
   - Use `approve_release` (owner role required) to authorize release finalization, trigger vault sync, and scrub trajectory records.

## Guardrails

Do not approve a release with any unmet quality or closeout gate, open audit finding, or missing required independent review. Release approval requires the owner role.
