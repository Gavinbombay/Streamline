use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::types::{
    CoreError, OwnDecision, PairedSyncRow, PrincipalRole, ReqStage, RequirementRow, SyncVerdict,
    TrioProfile, WorkKind,
};

/// Rubric scores and shape flags used to select a Trio+ profile.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AssessmentInput {
    pub d1_scope: u8,
    pub d2_interfaces: u8,
    pub d3_uncertainty: u8,
    pub d4_ui: u8,
    pub d5_novelty: u8,
    pub r1_exposure: u8,
    pub r2_irreversibility: u8,
    pub r3_blast_radius: u8,
    pub r4_evidence: u8,
    pub r5_custody: u8,
    #[serde(default)]
    pub work_kind: WorkKind,
    #[serde(default)]
    pub has_ui: bool,
    #[serde(default)]
    pub is_game: bool,
    #[serde(default)]
    pub under_accepted_design: bool,
}

/// Output of rubric scoring and profile planning.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AssessmentOutcome {
    pub difficulty: u8,
    pub risk: u8,
    pub profile: TrioProfile,
    pub expected_roles: Vec<String>,
}

fn score_dimension(rows: [u8; 5]) -> u8 {
    let clamped: [u8; 5] = rows.map(|value| value.min(4));
    let max_score = clamped.into_iter().max().unwrap_or(0);
    let threes_count = clamped.into_iter().filter(|&value| value == 3).count();
    if threes_count >= 2 {
        (max_score + 1).min(4)
    } else {
        max_score
    }
}

/// Computes difficulty (`0..=4`), risk (`0..=4`), Trio+ profile, and base expected roles.
pub fn assess_work(input: &AssessmentInput) -> AssessmentOutcome {
    assess_work_with_modules(input, &[])
}

/// Computes difficulty, risk, Trio+ profile, and expected roles including explicit opt-in modules.
pub fn assess_work_with_modules(
    input: &AssessmentInput,
    opt_in_modules: &[String],
) -> AssessmentOutcome {
    let difficulty = score_dimension([
        input.d1_scope,
        input.d2_interfaces,
        input.d3_uncertainty,
        input.d4_ui,
        input.d5_novelty,
    ]);
    let risk = score_dimension([
        input.r1_exposure,
        input.r2_irreversibility,
        input.r3_blast_radius,
        input.r4_evidence,
        input.r5_custody,
    ]);

    let profile = if input.under_accepted_design && difficulty <= 1 {
        TrioProfile::P0
    } else if difficulty >= 3 {
        TrioProfile::P4
    } else if input.is_game {
        TrioProfile::P3
    } else if input.has_ui {
        TrioProfile::P2
    } else {
        TrioProfile::P1
    };

    let roles = expected_roles(&profile, &input.work_kind, opt_in_modules);
    AssessmentOutcome {
        difficulty,
        risk,
        profile,
        expected_roles: roles,
    }
}

/// Computes the minimal complete document role set for a Trio+ profile and opt-in modules.
/// Never auto-forces `budget` for any work kind (including `Council`) or profile.
pub fn expected_roles(
    profile: &TrioProfile,
    _work_kind: &WorkKind,
    opt_in_modules: &[String],
) -> Vec<String> {
    let mut roles: Vec<String> = match profile {
        TrioProfile::P0 => vec!["fix_record".to_string()],
        TrioProfile::P1 => vec![
            "blueprint".to_string(),
            "runbook".to_string(),
            "as_built".to_string(),
        ],
        TrioProfile::P2 => vec![
            "blueprint".to_string(),
            "runbook".to_string(),
            "as_built".to_string(),
            "ui_ux_qol".to_string(),
        ],
        TrioProfile::P3 => vec![
            "blueprint".to_string(),
            "runbook".to_string(),
            "as_built".to_string(),
            "game_mechanics".to_string(),
        ],
        TrioProfile::P4 => vec![
            "blueprint".to_string(),
            "runbook".to_string(),
            "as_built".to_string(),
            "critique".to_string(),
            "requirements".to_string(),
            "acceptance_evidence".to_string(),
        ],
    };

    let mut seen: BTreeSet<String> = roles.iter().cloned().collect();
    for module_name in opt_in_modules {
        let normalized = module_name.trim().to_ascii_lowercase();
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            roles.push(normalized);
        }
    }
    roles
}

/// Input parameters for validating a `D I T A L` requirement stage transition.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StageTransitionRequest {
    pub current_stage: ReqStage,
    pub target_stage: ReqStage,
    pub risk: u8,
    pub actor_principal: String,
    pub actor_role: PrincipalRole,
    #[serde(default)]
    pub actor_is_custodian: bool,
    #[serde(default)]
    pub builder_principal: Option<String>,
    #[serde(default)]
    pub builder_family: Option<String>,
    #[serde(default)]
    pub acceptor_family: Option<String>,
    #[serde(default)]
    pub evidence_ref: String,
    #[serde(default)]
    pub how_it_works_approved: bool,
}

/// Enforces `D I T A L` stage transition rules, cross-family acceptance for R2+, and owner/custodian L sign-off.
pub fn validate_stage_transition(req: &StageTransitionRequest) -> Result<(), CoreError> {
    let actor = req.actor_principal.trim();
    if actor.is_empty() {
        return Err(CoreError::InvalidInput(
            "actor_principal cannot be empty".to_string(),
        ));
    }

    if req.actor_role == PrincipalRole::Guest {
        return Err(CoreError::ForbiddenStageRole(PrincipalRole::Guest));
    }

    match req.target_stage {
        ReqStage::Unknown | ReqStage::D => Ok(()),
        ReqStage::I | ReqStage::T => {
            if req.evidence_ref.trim().is_empty() {
                return Err(CoreError::EvidenceRequired(req.target_stage));
            }
            Ok(())
        }
        ReqStage::A => {
            match req.current_stage {
                ReqStage::I | ReqStage::T | ReqStage::A => {}
                other => {
                    return Err(CoreError::InvalidStageTransition {
                        from: other,
                        to: ReqStage::A,
                        reason: "requirement must reach I or T before acceptance".to_string(),
                    });
                }
            }

            if req.evidence_ref.trim().is_empty() {
                return Err(CoreError::EvidenceRequired(ReqStage::A));
            }

            let builder = req
                .builder_principal
                .as_deref()
                .map(str::trim)
                .filter(|val| !val.is_empty())
                .ok_or_else(|| CoreError::InvalidStageTransition {
                    from: req.current_stage,
                    to: ReqStage::A,
                    reason: "builder_principal must be recorded before independent acceptance"
                        .to_string(),
                })?;

            if builder.eq_ignore_ascii_case(actor) {
                return Err(CoreError::SelfAcceptanceForbidden {
                    principal: actor.to_string(),
                });
            }

            if req.risk >= 2 {
                let builder_family = req
                    .builder_family
                    .as_deref()
                    .map(str::trim)
                    .filter(|val| !val.is_empty())
                    .ok_or(CoreError::CrossFamilyMetadataMissing)?;
                let acceptor_family = req
                    .acceptor_family
                    .as_deref()
                    .map(str::trim)
                    .filter(|val| !val.is_empty())
                    .ok_or(CoreError::CrossFamilyMetadataMissing)?;

                if builder_family.eq_ignore_ascii_case(acceptor_family) {
                    return Err(CoreError::SameFamilyForbidden {
                        builder_family: builder_family.to_string(),
                        acceptor_family: acceptor_family.to_string(),
                    });
                }
            }

            Ok(())
        }
        ReqStage::L => {
            let can_mark_live = req.actor_role == PrincipalRole::Owner
                || (req.actor_role == PrincipalRole::Member && req.actor_is_custodian);
            if !can_mark_live {
                return Err(CoreError::ForbiddenStageRole(req.actor_role));
            }

            match req.current_stage {
                ReqStage::A | ReqStage::L => {}
                other => {
                    return Err(CoreError::InvalidStageTransition {
                        from: other,
                        to: ReqStage::L,
                        reason: "requirement must be Accepted (A) before marking Live (L)"
                            .to_string(),
                    });
                }
            }

            if !req.how_it_works_approved {
                return Err(CoreError::HowItWorksNotApproved);
            }

            if req.evidence_ref.trim().is_empty() {
                return Err(CoreError::EvidenceRequired(ReqStage::L));
            }

            Ok(())
        }
    }
}

/// Observed implementation claim parsed from an AS-BUILT markdown document.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AsBuiltClaim {
    pub claim_id: String,
    pub observed: String,
    pub stage: Option<ReqStage>,
    pub evidence_ref: Option<String>,
}

/// Deterministic summary emitted by `check_sync`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SyncCheckSummary {
    pub rows: Vec<PairedSyncRow>,
    pub verdict: SyncVerdict,
    pub unmatched_claims: Vec<AsBuiltClaim>,
}

fn clean_id_fragment(raw: &str) -> String {
    raw.trim()
        .trim_matches(|ch: char| matches!(ch, '`' | '*' | '[' | ']' | '(' | ')'))
        .trim()
        .to_ascii_uppercase()
}

fn is_trace_id(candidate: &str) -> bool {
    let upper = clean_id_fragment(candidate);
    (upper.starts_with("REQ-") || upper.starts_with("OWN-")) && upper.len() > 4
}

fn parse_stage_cell(raw: &str) -> Option<ReqStage> {
    let cleaned = raw
        .trim()
        .trim_matches(|ch: char| matches!(ch, '`' | '*' | '[' | ']' | '(' | ')'))
        .trim();
    cleaned.parse::<ReqStage>().ok()
}

fn is_table_separator(cells: &[&str]) -> bool {
    !cells.is_empty()
        && cells
            .iter()
            .all(|cell| !cell.is_empty() && cell.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

/// Parses requirement and owner-decision claims from markdown tables or bullet lines.
pub fn parse_as_built_markdown(markdown: &str) -> Vec<AsBuiltClaim> {
    let mut claims = Vec::new();
    let mut in_fence = false;

    for raw_line in markdown.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('|') {
            let cells: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            if cells.is_empty() || is_table_separator(&cells) {
                continue;
            }
            if is_trace_id(cells[0]) {
                let claim_id = clean_id_fragment(cells[0]);
                let mut stage: Option<ReqStage> = None;
                let mut text_cells: Vec<String> = Vec::new();
                for cell in cells.iter().skip(1) {
                    if let Some(parsed_stage) = parse_stage_cell(cell) {
                        stage = Some(parsed_stage);
                    } else if !cell.is_empty() {
                        text_cells.push((*cell).to_string());
                    }
                }
                let observed = text_cells.first().cloned().unwrap_or_default();
                let evidence_ref = if text_cells.len() > 1 {
                    text_cells.last().cloned()
                } else {
                    None
                };
                claims.push(AsBuiltClaim {
                    claim_id,
                    observed,
                    stage,
                    evidence_ref,
                });
            }
            continue;
        }

        let stripped = trimmed
            .trim_start_matches('#')
            .trim_start_matches('-')
            .trim_start_matches('*')
            .trim();
        let without_num = if let Some((prefix, rest)) = stripped.split_once(". ") {
            if !prefix.is_empty() && prefix.chars().all(|ch| ch.is_ascii_digit()) {
                rest.trim()
            } else {
                stripped
            }
        } else {
            stripped
        };

        let split_pos = without_num.find([':', ' ']).unwrap_or(without_num.len());
        let first_part = &without_num[..split_pos];
        if is_trace_id(first_part) {
            let claim_id = clean_id_fragment(first_part);
            let mut rest = without_num[split_pos..]
                .trim_start_matches(':')
                .trim_start_matches('-')
                .trim()
                .to_string();
            let mut stage: Option<ReqStage> = None;
            for bracket_open in ['[', '('] {
                let bracket_close = if bracket_open == '[' { ']' } else { ')' };
                if let Some(start_idx) = rest.rfind(bracket_open) {
                    if let Some(end_rel) = rest[start_idx + 1..].find(bracket_close) {
                        let inner = &rest[start_idx + 1..start_idx + 1 + end_rel];
                        if let Some(parsed_stage) = parse_stage_cell(inner) {
                            stage = Some(parsed_stage);
                            let before = rest[..start_idx].trim_end();
                            let after = rest[start_idx + 1 + end_rel + 1..].trim_start();
                            rest = if after.is_empty() {
                                before.to_string()
                            } else {
                                format!("{before} {after}")
                            };
                            break;
                        }
                    }
                }
            }
            claims.push(AsBuiltClaim {
                claim_id,
                observed: rest.trim().to_string(),
                stage,
                evidence_ref: None,
            });
        }
    }

    claims
}

fn has_negative_claim_marker(observed: &str) -> bool {
    let lower = observed.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return true;
    }
    matches!(
        lower.as_str(),
        "missing" | "none" | "todo" | "tbd" | "pending" | "n/a" | "unimplemented" | "skipped"
    ) || lower.contains("not implemented")
        || lower.contains("unimplemented")
        || lower.starts_with("missing:")
        || lower.starts_with("todo:")
}

/// Deterministically pairs `RequirementRow` and `OwnDecision` entries against parsed `AS-BUILT` markdown claims.
pub fn check_sync(
    requirements: &[RequirementRow],
    own_decisions: &[OwnDecision],
    as_built_markdown: &str,
) -> SyncCheckSummary {
    let parsed_claims = parse_as_built_markdown(as_built_markdown);
    check_sync_claims(requirements, own_decisions, &parsed_claims)
}

/// Deterministically pairs `RequirementRow` and `OwnDecision` entries against explicit `AsBuiltClaim` entries.
pub fn check_sync_claims(
    requirements: &[RequirementRow],
    own_decisions: &[OwnDecision],
    claims: &[AsBuiltClaim],
) -> SyncCheckSummary {
    let mut claim_map: BTreeMap<String, AsBuiltClaim> = BTreeMap::new();
    for claim in claims {
        claim_map.insert(clean_id_fragment(&claim.claim_id), claim.clone());
    }

    let own_by_id: BTreeMap<String, &OwnDecision> = own_decisions
        .iter()
        .map(|decision| (clean_id_fragment(&decision.own_id), decision))
        .collect();

    let mut sorted_reqs: Vec<&RequirementRow> = requirements.iter().collect();
    sorted_reqs.sort_by(|left, right| left.req_id.cmp(&right.req_id));

    let mut used_claims: BTreeSet<String> = BTreeSet::new();
    let mut covered_own_ids: BTreeSet<String> = BTreeSet::new();
    let mut rows: Vec<PairedSyncRow> = Vec::new();

    for req in sorted_reqs {
        let req_lookup = clean_id_fragment(&req.req_id);
        let source_lookup = clean_id_fragment(&req.source_id);
        if !source_lookup.is_empty() {
            covered_own_ids.insert(source_lookup.clone());
        }

        let planned = if let Some(own) = own_by_id.get(&source_lookup) {
            format!("[{}] {}", own.own_id, req.statement)
        } else {
            req.statement.clone()
        };

        if let Some(claim) = claim_map.get(&req_lookup) {
            used_claims.insert(req_lookup);
            let stage_ok = req.stage != ReqStage::Unknown
                && match claim.stage {
                    Some(claimed_stage) => {
                        claimed_stage != ReqStage::Unknown && claimed_stage == req.stage
                    }
                    None => true,
                };
            let matched = stage_ok && !has_negative_claim_marker(&claim.observed);
            rows.push(PairedSyncRow {
                req_id: req.req_id.clone(),
                planned,
                observed: claim.observed.clone(),
                stage: req.stage,
                matched,
            });
        } else {
            rows.push(PairedSyncRow {
                req_id: req.req_id.clone(),
                planned,
                observed: "MISSING".to_string(),
                stage: req.stage,
                matched: false,
            });
        }
    }

    let mut sorted_owns: Vec<&OwnDecision> = own_decisions.iter().collect();
    sorted_owns.sort_by(|left, right| left.own_id.cmp(&right.own_id));

    for own in sorted_owns {
        let own_lookup = clean_id_fragment(&own.own_id);
        if covered_own_ids.contains(&own_lookup) {
            continue;
        }

        if let Some(claim) = claim_map.get(&own_lookup) {
            used_claims.insert(own_lookup);
            let effective_stage = claim.stage.unwrap_or(ReqStage::D);
            let matched =
                effective_stage != ReqStage::Unknown && !has_negative_claim_marker(&claim.observed);
            rows.push(PairedSyncRow {
                req_id: own.own_id.clone(),
                planned: own.statement.clone(),
                observed: claim.observed.clone(),
                stage: effective_stage,
                matched,
            });
        } else {
            rows.push(PairedSyncRow {
                req_id: own.own_id.clone(),
                planned: own.statement.clone(),
                observed: "MISSING".to_string(),
                stage: ReqStage::D,
                matched: false,
            });
        }
    }

    let unmatched_claims: Vec<AsBuiltClaim> = claim_map
        .into_iter()
        .filter(|(claim_name, _)| !used_claims.contains(claim_name))
        .map(|(_, claim)| claim)
        .collect();

    let verdict = if rows.is_empty() {
        SyncVerdict::Fail
    } else if rows.iter().all(|row| row.matched) && unmatched_claims.is_empty() {
        SyncVerdict::Pass
    } else if rows.iter().any(|row| row.matched) {
        SyncVerdict::Partial
    } else {
        SyncVerdict::Fail
    };

    SyncCheckSummary {
        rows,
        verdict,
        unmatched_claims,
    }
}

struct MarkdownSection {
    heading_lower: String,
    body: String,
}

fn split_markdown_sections(markdown: &str) -> Vec<MarkdownSection> {
    let mut sections: Vec<MarkdownSection> = Vec::new();
    let mut current_heading = String::new();
    let mut current_body: Vec<&str> = Vec::new();
    let mut in_fence = false;

    for raw_line in markdown.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            current_body.push(raw_line);
            continue;
        }
        if !in_fence && trimmed.starts_with('#') {
            let heading_text = trimmed.trim_start_matches('#').trim();
            if !heading_text.is_empty() {
                if !current_heading.is_empty() || !current_body.is_empty() {
                    sections.push(MarkdownSection {
                        heading_lower: current_heading.clone(),
                        body: current_body.join("\n"),
                    });
                    current_body.clear();
                }
                current_heading = heading_text.to_ascii_lowercase();
                continue;
            }
        }
        current_body.push(raw_line);
    }

    if !current_heading.is_empty() || !current_body.is_empty() {
        sections.push(MarkdownSection {
            heading_lower: current_heading,
            body: current_body.join("\n"),
        });
    }

    sections
}

fn has_meaningful_body(body: &str) -> bool {
    let stripped: String = body
        .chars()
        .map(|ch| {
            if matches!(ch, '-' | '*' | '#' | '`' | '>' | '|') {
                ' '
            } else {
                ch
            }
        })
        .collect();
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "todo" | "tbd" | "none" | "n/a" | "coming soon" | "pending"
    ) {
        return false;
    }
    trimmed.split_whitespace().count() >= 3
}

fn section_matches(sections: &[MarkdownSection], patterns: &[&str]) -> bool {
    sections.iter().any(|section| {
        patterns
            .iter()
            .any(|pattern| section.heading_lower.contains(pattern))
            && has_meaningful_body(&section.body)
    })
}

fn measure_how_it_works_readability(markdown: &str) -> (usize, usize) {
    let mut in_fence = false;
    let mut current_fence_lines: usize = 0;
    let mut max_fence_lines: usize = 0;
    let mut prose_lines: Vec<String> = Vec::new();

    for raw_line in markdown.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with("```") {
            if in_fence {
                max_fence_lines = max_fence_lines.max(current_fence_lines);
                current_fence_lines = 0;
                in_fence = false;
            } else {
                in_fence = true;
                current_fence_lines = 0;
            }
            continue;
        }
        if in_fence {
            current_fence_lines += 1;
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        let cleaned: String = trimmed
            .chars()
            .map(|ch| {
                if matches!(ch, '-' | '*' | '#' | '`' | '>' | '|') {
                    ' '
                } else {
                    ch
                }
            })
            .collect();
        let words: Vec<&str> = cleaned.split_whitespace().collect();
        if !words.is_empty() {
            prose_lines.push(words.join(" "));
        }
    }

    if in_fence {
        max_fence_lines = max_fence_lines.max(current_fence_lines);
    }

    let prose_chars = prose_lines.join(" ").chars().count();
    (max_fence_lines, prose_chars)
}

/// Validates that a plain-English `HOW-IT-WORKS` teaching guide contains non-empty content
/// covering overview/purpose, runtime location, step-by-step flow, health/logs, and failure modes,
/// while rejecting giant code dumps (> 15 lines in a fence) and brief stubs (< 200 chars of prose).
pub fn validate_how_it_works(markdown: &str) -> Result<(), Vec<String>> {
    if markdown.trim().is_empty() {
        return Err(vec!["HOW-IT-WORKS document is empty".to_string()]);
    }

    let sections = split_markdown_sections(markdown);
    let mut errors = Vec::new();

    let has_overview = section_matches(
        &sections,
        &[
            "overview",
            "purpose",
            "summary",
            "introduction",
            "what it does",
            "about",
            "how it works",
        ],
    );
    if !has_overview {
        errors.push("missing non-empty overview/purpose section".to_string());
    }

    let has_runtime = section_matches(
        &sections,
        &[
            "runtime",
            "location",
            "where it runs",
            "deployment",
            "install",
            "service",
            "host",
            "environment",
            "ports",
            "files",
        ],
    );
    if !has_runtime {
        errors.push("missing non-empty runtime location section".to_string());
    }

    let has_flow = section_matches(
        &sections,
        &[
            "step-by-step",
            "step by step",
            "flow",
            "workflow",
            "steps",
            "walkthrough",
            "how it runs",
            "sequence",
            "lifecycle",
        ],
    );
    if !has_flow {
        errors.push("missing non-empty step-by-step flow section".to_string());
    }

    let has_health = section_matches(
        &sections,
        &[
            "health",
            "logs",
            "logging",
            "monitoring",
            "observability",
            "diagnostics",
            "status",
            "metrics",
        ],
    );
    if !has_health {
        errors.push("missing non-empty health/logs section".to_string());
    }

    let has_failures = section_matches(
        &sections,
        &[
            "failure",
            "failures",
            "troubleshooting",
            "error",
            "recovery",
            "rollback",
            "impasse",
            "what can go wrong",
        ],
    );
    if !has_failures {
        errors.push("missing non-empty failure modes section".to_string());
    }

    let (max_fence_lines, prose_chars) = measure_how_it_works_readability(markdown);
    if max_fence_lines > 15 {
        errors.push(format!(
            "contains a fenced code block with {max_fence_lines} lines (maximum 15 lines allowed so HOW-IT-WORKS stays plain-English)"
        ));
    }
    if prose_chars < 200 {
        errors.push(format!(
            "plain-English prose is too brief ({prose_chars} characters; minimum 200 characters required)"
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StreamlineConfig;

    fn base_input() -> AssessmentInput {
        AssessmentInput {
            d1_scope: 1,
            d2_interfaces: 1,
            d3_uncertainty: 0,
            d4_ui: 0,
            d5_novelty: 0,
            r1_exposure: 1,
            r2_irreversibility: 0,
            r3_blast_radius: 1,
            r4_evidence: 1,
            r5_custody: 0,
            work_kind: WorkKind::Software,
            has_ui: false,
            is_game: false,
            under_accepted_design: false,
        }
    }

    #[test]
    fn rubric_profiles_p0_through_p4() {
        let mut p0_in = base_input();
        p0_in.under_accepted_design = true;
        let p0 = assess_work(&p0_in);
        assert_eq!(p0.difficulty, 1);
        assert_eq!(p0.risk, 1);
        assert_eq!(p0.profile, TrioProfile::P0);
        assert_eq!(p0.expected_roles, vec!["fix_record"]);

        let p1 = assess_work(&base_input());
        assert_eq!(p1.profile, TrioProfile::P1);
        assert_eq!(p1.expected_roles, vec!["blueprint", "runbook", "as_built"]);

        let mut p2_in = base_input();
        p2_in.has_ui = true;
        p2_in.d4_ui = 2;
        let p2 = assess_work(&p2_in);
        assert_eq!(p2.difficulty, 2);
        assert_eq!(p2.profile, TrioProfile::P2);
        assert!(p2.expected_roles.contains(&"ui_ux_qol".to_string()));

        let mut p3_in = base_input();
        p3_in.is_game = true;
        p3_in.d1_scope = 2;
        let p3 = assess_work(&p3_in);
        assert_eq!(p3.profile, TrioProfile::P3);
        assert!(p3.expected_roles.contains(&"game_mechanics".to_string()));

        let mut p4_single_three = base_input();
        p4_single_three.d1_scope = 3;
        let p4_a = assess_work(&p4_single_three);
        assert_eq!(p4_a.difficulty, 3);
        assert_eq!(p4_a.profile, TrioProfile::P4);
        assert!(p4_a.expected_roles.contains(&"critique".to_string()));
        assert!(p4_a.expected_roles.contains(&"requirements".to_string()));
        assert!(p4_a
            .expected_roles
            .contains(&"acceptance_evidence".to_string()));

        let mut p4_two_threes = base_input();
        p4_two_threes.d1_scope = 3;
        p4_two_threes.d2_interfaces = 3;
        p4_two_threes.r1_exposure = 3;
        p4_two_threes.r5_custody = 3;
        let p4_b = assess_work(&p4_two_threes);
        assert_eq!(p4_b.difficulty, 4);
        assert_eq!(p4_b.risk, 4);
        assert_eq!(p4_b.profile, TrioProfile::P4);
    }

    #[test]
    fn council_never_auto_forces_budget() {
        let council_roles = expected_roles(&TrioProfile::P4, &WorkKind::Council, &[]);
        assert!(!council_roles.contains(&"budget".to_string()));

        let council_with_schedule = expected_roles(
            &TrioProfile::P1,
            &WorkKind::Council,
            &["schedule".to_string()],
        );
        assert!(council_with_schedule.contains(&"schedule".to_string()));
        assert!(!council_with_schedule.contains(&"budget".to_string()));

        let council_opt_budget = expected_roles(
            &TrioProfile::P1,
            &WorkKind::Council,
            &["budget".to_string()],
        );
        assert!(council_opt_budget.contains(&"budget".to_string()));
    }

    #[test]
    fn dital_transitions_and_cross_family_guard() {
        let missing_evidence = StageTransitionRequest {
            current_stage: ReqStage::D,
            target_stage: ReqStage::I,
            risk: 2,
            actor_principal: "worker-a".to_string(),
            actor_role: PrincipalRole::Agent,
            actor_is_custodian: false,
            builder_principal: None,
            builder_family: None,
            acceptor_family: None,
            evidence_ref: "   ".to_string(),
            how_it_works_approved: false,
        };
        assert_eq!(
            validate_stage_transition(&missing_evidence),
            Err(CoreError::EvidenceRequired(ReqStage::I))
        );

        let self_accept = StageTransitionRequest {
            current_stage: ReqStage::T,
            target_stage: ReqStage::A,
            risk: 1,
            actor_principal: "worker-a".to_string(),
            actor_role: PrincipalRole::Agent,
            actor_is_custodian: false,
            builder_principal: Some("worker-a".to_string()),
            builder_family: Some("openai-codex".to_string()),
            acceptor_family: Some("google-gemini".to_string()),
            evidence_ref: "test:ok".to_string(),
            how_it_works_approved: false,
        };
        assert_eq!(
            validate_stage_transition(&self_accept),
            Err(CoreError::SelfAcceptanceForbidden {
                principal: "worker-a".to_string(),
            })
        );

        let same_family_r2 = StageTransitionRequest {
            current_stage: ReqStage::T,
            target_stage: ReqStage::A,
            risk: 2,
            actor_principal: "acceptor-b".to_string(),
            actor_role: PrincipalRole::Agent,
            actor_is_custodian: false,
            builder_principal: Some("worker-a".to_string()),
            builder_family: Some("openai-codex".to_string()),
            acceptor_family: Some("openai-codex".to_string()),
            evidence_ref: "test:ok".to_string(),
            how_it_works_approved: false,
        };
        assert_eq!(
            validate_stage_transition(&same_family_r2),
            Err(CoreError::SameFamilyForbidden {
                builder_family: "openai-codex".to_string(),
                acceptor_family: "openai-codex".to_string(),
            })
        );

        let cross_family_r2 = StageTransitionRequest {
            acceptor_family: Some("google-gemini".to_string()),
            ..same_family_r2
        };
        assert!(validate_stage_transition(&cross_family_r2).is_ok());

        let live_without_how = StageTransitionRequest {
            current_stage: ReqStage::A,
            target_stage: ReqStage::L,
            risk: 2,
            actor_principal: "owner-1".to_string(),
            actor_role: PrincipalRole::Owner,
            actor_is_custodian: true,
            builder_principal: Some("worker-a".to_string()),
            builder_family: Some("openai-codex".to_string()),
            acceptor_family: Some("google-gemini".to_string()),
            evidence_ref: "live:127.0.0.1:5880/ready".to_string(),
            how_it_works_approved: false,
        };
        assert_eq!(
            validate_stage_transition(&live_without_how),
            Err(CoreError::HowItWorksNotApproved)
        );

        let live_agent_forbidden = StageTransitionRequest {
            actor_role: PrincipalRole::Agent,
            how_it_works_approved: true,
            ..live_without_how.clone()
        };
        assert_eq!(
            validate_stage_transition(&live_agent_forbidden),
            Err(CoreError::ForbiddenStageRole(PrincipalRole::Agent))
        );

        let live_custodian_ok = StageTransitionRequest {
            actor_role: PrincipalRole::Member,
            actor_is_custodian: true,
            how_it_works_approved: true,
            ..live_without_how
        };
        assert!(validate_stage_transition(&live_custodian_ok).is_ok());
    }

    #[test]
    fn check_sync_pairs_requirements_and_detects_drift() {
        let owns = vec![OwnDecision {
            project_id: "demo".to_string(),
            own_id: "OWN-001".to_string(),
            statement: "Run loopback HTTP server on 127.0.0.1:5880".to_string(),
            locked_at: Some("2026-11-18T10:00:00Z".to_string()),
            approved_by: Some("owner".to_string()),
        }];
        let reqs = vec![
            RequirementRow {
                project_id: "demo".to_string(),
                req_id: "REQ-01".to_string(),
                source_id: "OWN-001".to_string(),
                statement: "Serve /ready on 127.0.0.1:5880".to_string(),
                stage: ReqStage::T,
                evidence_ref: "cargo test ready".to_string(),
                builder_principal: Some("worker-a".to_string()),
                builder_family: Some("openai-codex".to_string()),
                acceptor_principal: None,
                acceptor_family: None,
                updated_at: "2026-11-18T10:05:00Z".to_string(),
            },
            RequirementRow {
                project_id: "demo".to_string(),
                req_id: "REQ-02".to_string(),
                source_id: "OWN-001".to_string(),
                statement: "Enforce ack_rules gate".to_string(),
                stage: ReqStage::T,
                evidence_ref: "cargo test ack".to_string(),
                builder_principal: Some("worker-a".to_string()),
                builder_family: Some("openai-codex".to_string()),
                acceptor_principal: None,
                acceptor_family: None,
                updated_at: "2026-11-18T10:06:00Z".to_string(),
            },
        ];

        let synced_md = "\
| ID | Observed | Stage | Evidence |
| :--- | :--- | :-: | :--- |
| REQ-01 | Loopback /ready returns 200 OK | T | test_ready |
| REQ-02 | Rule gate returns 412 until ack | T | test_ack |
";
        let pass_summary = check_sync(&reqs, &owns, synced_md);
        assert_eq!(pass_summary.verdict, SyncVerdict::Pass);
        assert_eq!(pass_summary.rows.len(), 2);
        assert!(pass_summary.rows.iter().all(|row| row.matched));

        let partial_md = "\
| ID | Observed | Stage |
| :--- | :--- | :-: |
| REQ-01 | Loopback /ready returns 200 OK | T |
";
        let partial_summary = check_sync(&reqs, &owns, partial_md);
        assert_eq!(partial_summary.verdict, SyncVerdict::Partial);
        assert!(partial_summary.rows[0].matched);
        assert!(!partial_summary.rows[1].matched);
        assert_eq!(partial_summary.rows[1].observed, "MISSING");
    }

    #[test]
    fn validate_how_it_works_checks_all_required_sections() {
        let incomplete = "\
## Overview
Streamline coordinates governance rules, project plans, revision history, and release gates for local operators in plain English so every automated change stays auditable and safe.

## Runtime Location
The service runs on loopback address 127.0.0.1:5880 and stores its SQLite database under /var/lib/streamline.
";
        let missing_errs = validate_how_it_works(incomplete).unwrap_err();
        assert_eq!(missing_errs.len(), 3);

        let complete = "\
## Overview and Purpose
Streamline serves governance documents, project plans, and verification records from one SQLite store.

## Runtime Location
The service binds loopback port 5880 and writes its database under /var/lib/streamline.

## Step-by-Step Flow
Clients call bootstrap to fetch the active rule digest, acknowledge rules, and then submit project mutations.

## Health and Logs
Call GET /ready to inspect server status and read structured tracing output from journald.

## Failure Modes and Recovery
If a write lacks rule acknowledgement the server returns 412; restore the latest SQLite backup if disk corruption occurs.
";
        assert!(validate_how_it_works(complete).is_ok());
    }

    #[test]
    fn validate_how_it_works_rejects_giant_code_fences_and_brief_prose() {
        let brief = "\
## Overview
Runs the service.

## Runtime Location
Local port 5880.

## Step-by-Step Flow
Start then listen.

## Health and Logs
Check ready endpoint.

## Failure Modes
Restart on error.
";
        let brief_errs = validate_how_it_works(brief).unwrap_err();
        assert!(brief_errs.iter().any(|err| err.contains("200 characters")));

        let giant_fence = (1..=16)
            .map(|idx| format!("let line_{idx} = {idx};"))
            .collect::<Vec<_>>()
            .join("\n");
        let with_giant_fence = format!(
            "\
## Overview and Purpose
Streamline serves governance documents, project plans, and verification records from one SQLite store.

## Runtime Location
The service binds loopback port 5880 and writes its database under /var/lib/streamline.

## Step-by-Step Flow
Clients call bootstrap to fetch the active rule digest, acknowledge rules, and then submit project mutations.
```rust
{giant_fence}
```

## Health and Logs
Call GET /ready to inspect server status and read structured tracing output from journald.

## Failure Modes and Recovery
If a write lacks rule acknowledgement the server returns 412; restore the latest SQLite backup if disk corruption occurs.
"
        );
        let fence_errs = validate_how_it_works(&with_giant_fence).unwrap_err();
        assert!(fence_errs.iter().any(|err| err.contains("16 lines")));

        let small_fence = (1..=15)
            .map(|idx| format!("step {idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        let with_small_fence = with_giant_fence.replace(&giant_fence, &small_fence);
        assert!(validate_how_it_works(&with_small_fence).is_ok());
    }

    #[test]
    fn config_parses_schema_toml() {
        let raw = r#"
bind_http = "127.0.0.1:5880"
bind_admin = "127.0.0.1:5881"
db_path = "/var/lib/streamline/streamline.sqlite3"
vault_sync_dir = "/var/lib/streamline/vault"
runs_dir = "/var/lib/streamline/runs"
public_safe_mode = true
presence_plugin_dir = ""
memory_sync_url = ""
worker_lease_url = ""

[[harnesses]]
id = "codex-main"
family = "openai-codex"
command = ["codex", "exec"]
max_concurrency = 1
"#;
        let cfg = StreamlineConfig::from_toml_str(raw).expect("valid toml");
        assert_eq!(cfg.bind_http, "127.0.0.1:5880");
        assert!(cfg.presence_plugin_path().is_none());
        assert_eq!(cfg.harnesses.len(), 1);
        assert_eq!(cfg.harnesses[0].family, "openai-codex");
    }
}
