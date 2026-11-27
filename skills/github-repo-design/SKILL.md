---
name: github-repo-design
description: Use when preparing, polishing, or auditing public Gavinbombay GitHub repositories for consistent layout, README structure, visual identity, self-hosted badges, documentation, and pre-push hygiene/audit gates (matching Beacon, Streamline, and Diffuser).
---

# GitHub Repo Design Skill

Use this skill whenever preparing, polishing, or auditing a public GitHub repository so every repo shares the same clean, readable, operator-first presentation established by **Beacon**, **Streamline**, and **Diffuser**.

## 1. Standard Top-Level Repository Layout

Keep the repository root clean and predictable (roughly 10 to 16 top-level entries). Never leave internal run logs (`RUN*_REPORT.md`, `SHA256SUMS.run*`, scratch dumps, or archived `kit/` directories) in a public repo root.

Required root files:
- `README.md`: Showcase entry point (8 KB to 25 KB, crisp and scannable, never an internal dev dump).
- `HOW-IT-WORKS.md`: Plain-English operator guide (overview, step-by-step flow, runtime locations, health/logs, failure modes).
- `DESIGN.md`: Concise architectural boundaries and invariants (1 page).
- `AUDIT.md`: Summary of the latest Cloudflare Security Audit (`CLEAN PASS`, counts, coverage, closed leads, validator exit codes).
- `LICENSE`: Apache-2.0 (or explicit approved OSS license).
- `NOTICE`: Copyright, license pointer, and third-party font/asset attributions.
- `.gitignore` and `.gitattributes`.

Standard `docs/` layout:
- `docs/SECURITY.md`: Supported versions, private vulnerability reporting, explicit operator trust boundaries.
- `docs/assets/<repo>-mark.png`: Square icon/mark rendered at `width="88"` at the top of `README.md`.
- `docs/assets/<repo>-hero.jpg`: Widescreen `16:9` editorial hero banner (dark slate/obsidian + warm amber-gold/teal palette, zero text, zero letters, zero watermarks).
- `docs/badges/license.svg`, `docs/badges/build.svg`, `docs/badges/security-audit.svg`: Self-hosted local SVG badges (avoid external `img.shields.io` URLs).
- `docs/captures/`: Optional real UI screenshots (for repos with web UIs), documented by `docs/CAPTURES.md`.

## 2. Exact `README.md` Blueprint (Top to Bottom)

Follow this exact order so every repo feels part of one cohesive family:

1. **Centered 88px mark**:
   ```html
   <p align="center"><img src="docs/assets/<repo>-mark.png" alt="<Repo> mark" width="88"></p>
   ```
2. **Title & one-line tagline**:
   ```markdown
   # <Repo>

   ### <Three to six word tagline.>
   ```
   Examples: `Your sites. One front door.` (Beacon), `Your context. One enforced workflow.` (Streamline), `One request. The right worker.` (Diffuser).
3. **Self-hosted local SVG badges** (3 badges max):
   - License (`docs/badges/license.svg` -> `LICENSE`)
   - Verified build/test count (`docs/badges/build.svg` -> `#build` or `#quickstart`)
   - Security audit (`docs/badges/security-audit.svg` -> `AUDIT.md`)
4. **Widescreen `16:9` hero artwork**:
   ```markdown
   ![<Descriptive alt text>](docs/assets/<repo>-hero.jpg)
   ```
5. **One-sentence bold summary + inline navigation bar**:
   ```markdown
   **<One crisp sentence explaining what the program is and what problem it solves.>**

   [For AI agents](#for-ai-agents) · [Architecture](#architecture) · [Quickstart](#quickstart) · [Security notes](docs/SECURITY.md) · [Security audit](AUDIT.md)
   ```
6. **Core `##` sections in order**:
   - `## Features`: 5 to 7 crisp bullets focusing on concrete guarantees.
   - `## For AI agents`: Exact auth handshake, base endpoints/MCP/CLI contract, and a compact reference table so agents can drive the tool without guessing.
   - `## Architecture`: Clean `mermaid` `flowchart LR` diagram (quote labels with parentheses/brackets; never use internal-only hostnames or legacy prefixes).
   - `## Screenshots` (if applicable): Real verified captures.
   - `## Quickstart`: Copy-pasteable build, config, and health check commands.
   - `## Capability status`: Honest table separating proven features from staged/optional/experimental ones.
   - `## Security notes` / `## Security audit`: Honest trust boundaries and link to `AUDIT.md` and `docs/SECURITY.md`.
   - `## License`: Apache-2.0 (or chosen license) + `NOTICE` link.

## 3. Style & Copy Rules

- **Zero em dashes and zero en dashes** anywhere in `README.md`, `DESIGN.md`, `AUDIT.md`, `HOW-IT-WORKS.md`, `docs/`, comments, or commit messages.
- **Hardware-agnostic & personal-detail-free**: No private LAN IPs (`10.0.0.x`), no workstation usernames or `/home/<user>` paths, no personal emails, no internal codenames (`uai-`, box names) in public branding.
- **Honest boundaries**: Never overclaim security or capability; state loopback/proxy trust boundaries and manual release steps plainly.

## 4. Public Mirror & Pre-Push Release Gate

1. **Curated public mirror**: If the working repo contains internal run logs, private plugins, or archived `kit/` trees, maintain a clean public mirror (`<repo>-public`) and sync only audited source files. **Never overwrite a curated public `README.md` with an internal dev `README.md`.**
2. **Hygiene scan (`gen85`)**: Run `crew-scan.py` over the entire public tree and require `0` raw hits, `0` unruled hits, and `0` namegrep hits before committing.
3. **High-effort Security Audit**: Run the Cloudflare Security Audit Skill via `Codex Luna` (`gpt-6-luna`, `-c model_reasoning_effort="high"`, `NO STOPS`) and loop `Sol Fix -> Luna Re-Audit` until `CLEAN_PASS` (`0` confirmed, `0` open `needs_validation`, both Node validators exit `0`).
4. **Commit authorship**: Author all public commits as `[owner] <owner@example.com>`.
5. **Owner approval**: Never push publicly to GitHub without explicit owner approval.
6. **GitHub repository metadata**: Set the repo About description and topics via `gh repo edit <owner>/<repo> --description "..." --add-topic ...` so pinned profile cards match sibling repos.

## Verification

Verify that the public tree follows the layout and README order, local badges and documentation links resolve, and required hygiene and audit gates have recorded passing results before release. Do not push without owner approval.
