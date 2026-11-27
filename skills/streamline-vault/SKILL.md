---
name: streamline-vault
description: Use when managing Streamline Vault conventions, bottom-up Obsidian markdown structures, Source Passports, central lessons database separation, or shelving tiers.
---

# Streamline Vault Skill

## Overview
As a Streamline Vault manager, your core responsibilities are organizing governance resources using bottom-up Obsidian markdown conventions, maintaining Source Passports, managing central lessons databases, and maintaining shelving tiers.

## Core Workflows

1. **Obsidian Markdown Conventions**
   - Structure markdown documents with clear headings, frontmatter metadata, and wiki-style or standard relative links.
   - Maintain clean directory layouts matching Kepano Obsidian vault standards.

2. **Source Passports and Central Lessons**
   - Manage Source Passports for every governance resource (`verified`, `stale`, `drift`, `unknown`).
   - Support central lessons management (`query_lessons` and `record_lesson`) ensuring insights are never trapped in local folders.

3. **Active Governance Shelves vs Triage Shelves**
   - Keep active governance shelves (`active`) clean, structured, and visible to agent bootstrapping.
   - Segregate historical designs, legacy product cards, and inbox ideas into fenced `dormant` or `legacy` triage shelves.
   - Ensure triage shelves remain browsable and revivable by the owner without polluting agent bootstrap memory.

4. **External Working Memory Separation**
   - Keep durable system rules, specifications, and plain-English guides inside Streamline database and vault stores, separate from ephemeral external agent working memory.

## Verification

Verify that changed governance documents have appropriate Source Passports, resolvable links, and the correct shelf. Confirm that central lessons are recorded centrally, triage material stays out of active bootstrap context, and ephemeral working memory remains separate.
