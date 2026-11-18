//! Safety validation, redaction scanning, and path-segment sanitization for Streamline.
//!
//! All verdicts are pure and deterministic, and violation reports never expose
//! matched sensitive substrings.

pub mod pinned_fs;

pub use pinned_fs::{kill_process_group, PinnedDir};

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use thiserror::Error;

const MIN_HIGH_ENTROPY_LEN: usize = 32;
const MIN_SHANNON_ENTROPY: f64 = 4.2;
const MAX_SEGMENT_LEN: usize = 128;

const ALLOWED_BEARER_PLACEHOLDERS: [&str; 3] = ["<CREDENTIAL>", "[R]", "<REDACTED>"];
const BEARER_PROSE_STOPWORDS: [&str; 18] = [
    "of", "is", "are", "was", "to", "in", "for", "and", "or", "the", "with", "from", "by", "as",
    "at", "on", "that", "this",
];

const PATH_EXTENSIONS: [&str; 24] = [
    ".rs", ".md", ".toml", ".json", ".yaml", ".yml", ".txt", ".lock", ".sh", ".py", ".js", ".ts",
    ".tsx", ".jsx", ".html", ".css", ".svg", ".png", ".jpg", ".zip", ".gz", ".tar", ".log",
    ".sqlite3",
];

/// Policy controlling safety inspection behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionPolicy {
    /// When true, RFC-1918 private IPv4 literals are rejected while loopback
    /// (`127.0.0.1`) and RFC-5737 documentation ranges remain allowed.
    pub public_safe_mode: bool,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            public_safe_mode: true,
        }
    }
}

/// Classification of a detected safety or style violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    PemPrivateBlock,
    CredentialPrefix,
    BearerCredential,
    HighEntropyLiteral,
    PrivateNetworkAddress,
    DisallowedDash,
}

/// Single violation record. Never contains matched sensitive substrings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("line {line} ({kind:?}): {reason}")]
pub struct SafetyViolation {
    pub kind: ViolationKind,
    pub line: usize,
    pub reason: String,
}

/// Aggregate result of inspecting a text payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyReport {
    pub ok: bool,
    pub violations: Vec<SafetyViolation>,
}

struct CompiledPatterns {
    pem_block_re: Regex,
    prefix_re: Regex,
    auth_bearer_re: Regex,
    standalone_bearer_re: Regex,
    ipv4_re: Regex,
    markdown_link_re: Regex,
    url_re: Regex,
}

fn patterns() -> &'static CompiledPatterns {
    static PATTERNS: OnceLock<CompiledPatterns> = OnceLock::new();
    PATTERNS.get_or_init(|| CompiledPatterns {
        pem_block_re: Regex::new(
            r"(?i:-----\s*BEGIN\s+(?:[A-Z0-9_-]+\s+)*PRIVATE\b)|\bBEGIN\s+(?:[A-Z0-9_-]+\s+)*PRIVATE\b",
        )
        .expect("valid PEM block regex"),
        prefix_re: Regex::new(
            r"(?:^|[^A-Za-z0-9_])(?:(?:ghp_|gho_|github_pat_|uai_pat_|xai-)[A-Za-z0-9_-]{6,}|sk-[A-Za-z0-9_-]{8,}|AKIA[A-Za-z0-9]{8,})",
        )
        .expect("valid credential prefix regex"),
        auth_bearer_re: Regex::new(r#"(?i)\bauthorization\s*[:=]\s*["']?bearer\s+([^\s"';,`]+)"#)
            .expect("valid authorization bearer regex"),
        standalone_bearer_re: Regex::new(r#"\bBearer\s+([^\s"';,`]+)"#)
            .expect("valid standalone bearer regex"),
        ipv4_re: Regex::new(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b")
            .expect("valid IPv4 regex"),
        markdown_link_re: Regex::new(r"\]\([^)\s]+\)")
            .expect("valid markdown link regex"),
        url_re: Regex::new(r#"(?i)\b(?:https?|wss?|ftp|ssh|git|file)://[^\s"'<>)\]]+"#)
            .expect("valid URL regex"),
    })
}

/// Inspects `input` line-by-line under `policy` and returns a [`SafetyReport`].
///
/// Violations never expose the matched sensitive substring.
pub fn inspect_text(input: &str, policy: &RedactionPolicy) -> SafetyReport {
    let compiled = patterns();
    let mut violations = Vec::new();

    for (idx, raw_line) in input.lines().enumerate() {
        let line_num = idx + 1;

        if compiled.pem_block_re.is_match(raw_line) {
            violations.push(SafetyViolation {
                kind: ViolationKind::PemPrivateBlock,
                line: line_num,
                reason: "PEM private block header detected".to_string(),
            });
        }

        if compiled.prefix_re.is_match(raw_line) {
            violations.push(SafetyViolation {
                kind: ViolationKind::CredentialPrefix,
                line: line_num,
                reason: "disallowed credential prefix detected".to_string(),
            });
        }

        let mut bearer_flagged = false;
        for caps in compiled.auth_bearer_re.captures_iter(raw_line) {
            if let Some(val_match) = caps.get(1) {
                let candidate = trim_bearer_value(val_match.as_str());
                if !is_allowed_bearer_placeholder(candidate) {
                    bearer_flagged = true;
                    violations.push(SafetyViolation {
                        kind: ViolationKind::BearerCredential,
                        line: line_num,
                        reason: "Bearer authorization literal with non-placeholder value detected"
                            .to_string(),
                    });
                    break;
                }
            }
        }

        if !bearer_flagged {
            for caps in compiled.standalone_bearer_re.captures_iter(raw_line) {
                if let Some(val_match) = caps.get(1) {
                    let candidate = trim_bearer_value(val_match.as_str());
                    if !is_allowed_bearer_placeholder(candidate) && !is_bearer_prose_word(candidate)
                    {
                        violations.push(SafetyViolation {
                            kind: ViolationKind::BearerCredential,
                            line: line_num,
                            reason:
                                "Bearer authorization literal with non-placeholder value detected"
                                    .to_string(),
                        });
                        break;
                    }
                }
            }
        }

        if policy.public_safe_mode && line_has_private_ipv4(raw_line, &compiled.ipv4_re) {
            violations.push(SafetyViolation {
                kind: ViolationKind::PrivateNetworkAddress,
                line: line_num,
                reason: "RFC-1918 private IPv4 address literal is disallowed in public_safe_mode"
                    .to_string(),
            });
        }

        if line_has_high_entropy_literal(raw_line, compiled) {
            violations.push(SafetyViolation {
                kind: ViolationKind::HighEntropyLiteral,
                line: line_num,
                reason: "high-entropy credential-like literal detected".to_string(),
            });
        }
    }

    SafetyReport {
        ok: violations.is_empty(),
        violations,
    }
}

/// Enforces `inspect_text`, returning `Err(violations)` when any violation is found.
pub fn require_safe_text(
    input: &str,
    policy: &RedactionPolicy,
) -> Result<(), Vec<SafetyViolation>> {
    let report = inspect_text(input, policy);
    if report.ok {
        Ok(())
    } else {
        Err(report.violations)
    }
}

/// Inspects `input` for em dashes (`\u{2014}`) and en dashes (`\u{2013}`).
pub fn inspect_style_no_dashes(input: &str) -> SafetyReport {
    let mut violations = Vec::new();
    for (idx, raw_line) in input.lines().enumerate() {
        let line_num = idx + 1;
        let has_em = raw_line.contains('\u{2014}');
        let has_en = raw_line.contains('\u{2013}');
        if has_em || has_en {
            let reason = match (has_em, has_en) {
                (true, true) => "em dash (U+2014) and en dash (U+2013) are disallowed",
                (true, false) => "em dash (U+2014) is disallowed",
                (false, true) => "en dash (U+2013) is disallowed",
                (false, false) => unreachable!(),
            };
            violations.push(SafetyViolation {
                kind: ViolationKind::DisallowedDash,
                line: line_num,
                reason: reason.to_string(),
            });
        }
    }

    SafetyReport {
        ok: violations.is_empty(),
        violations,
    }
}

/// Validates a single safe path or slug segment (`[a-zA-Z0-9._-]+`, non-empty,
/// not `.` or `..`, no `/` or `\`, max 128 characters).
pub fn validate_safe_segment(segment: &str) -> Result<(), String> {
    if segment.is_empty() {
        return Err("segment must not be empty".to_string());
    }
    if segment.len() > MAX_SEGMENT_LEN {
        return Err(format!(
            "segment exceeds maximum length of {MAX_SEGMENT_LEN} characters"
        ));
    }
    if segment == "." || segment == ".." {
        return Err("segment must not be '.' or '..'".to_string());
    }
    if segment.contains('/') || segment.contains('\\') {
        return Err("segment must not contain path separators".to_string());
    }
    if !segment
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(
            "segment may only contain ASCII letters, digits, '.', '_', and '-'".to_string(),
        );
    }
    Ok(())
}

/// Computes the lowercase hexadecimal SHA-256 digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn trim_bearer_value(raw: &str) -> &str {
    raw.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | ',' | ';' | ')' | '}'))
}

fn is_allowed_bearer_placeholder(candidate: &str) -> bool {
    ALLOWED_BEARER_PLACEHOLDERS.contains(&candidate)
}

fn is_bearer_prose_word(candidate: &str) -> bool {
    let lower = candidate.to_ascii_lowercase();
    BEARER_PROSE_STOPWORDS.contains(&lower.as_str())
}

fn line_has_private_ipv4(raw_line: &str, ipv4_re: &Regex) -> bool {
    let bytes = raw_line.as_bytes();
    for caps in ipv4_re.captures_iter(raw_line) {
        let Some(full) = caps.get(0) else {
            continue;
        };
        let before_bad = full
            .start()
            .checked_sub(1)
            .map(|i| bytes[i] == b'.' || bytes[i].is_ascii_digit())
            .unwrap_or(false);
        let after_bad = bytes
            .get(full.end())
            .map(|&b| b == b'.' || b.is_ascii_digit())
            .unwrap_or(false);
        if before_bad || after_bad {
            continue;
        }

        let Ok(a) = caps[1].parse::<u8>() else {
            continue;
        };
        let Ok(b) = caps[2].parse::<u8>() else {
            continue;
        };
        let Ok(c) = caps[3].parse::<u8>() else {
            continue;
        };
        let Ok(_d) = caps[4].parse::<u8>() else {
            continue;
        };

        if is_loopback_ipv4(a) || is_rfc5737_doc_ipv4(a, b, c) {
            continue;
        }
        if is_rfc1918_ipv4(a, b) {
            return true;
        }
    }
    false
}

fn is_loopback_ipv4(a: u8) -> bool {
    a == 127
}

fn is_rfc5737_doc_ipv4(a: u8, b: u8, c: u8) -> bool {
    (a == 192 && b == 0 && c == 2)
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
}

fn is_rfc1918_ipv4(a: u8, b: u8) -> bool {
    a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168)
}

fn line_has_high_entropy_literal(raw_line: &str, compiled: &CompiledPatterns) -> bool {
    // Strip markdown link targets and URLs so links do not trigger entropy false positives.
    let without_md_targets = compiled.markdown_link_re.replace_all(raw_line, "]()");
    let without_urls = compiled.url_re.replace_all(&without_md_targets, " ");

    for ws_chunk in without_urls.split_whitespace() {
        for delim_part in ws_chunk.split(|c: char| {
            matches!(
                c,
                '"' | '\'' | '`' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>'
            )
        }) {
            if delim_part.is_empty() {
                continue;
            }
            if check_candidate_entropy(delim_part) {
                return true;
            }
            // Also inspect RHS of `NAME=VALUE` or `name:value` assignments.
            if let Some((lhs, rhs)) = split_assignment_pair(delim_part) {
                if !lhs.is_empty() && check_candidate_entropy(rhs) {
                    return true;
                }
            }
        }
    }
    false
}

fn split_assignment_pair(chunk: &str) -> Option<(&str, &str)> {
    if let Some((lhs, rhs)) = chunk.split_once('=') {
        let valid_lhs = !lhs.is_empty()
            && lhs
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'));
        if valid_lhs && !rhs.is_empty() {
            return Some((lhs, rhs));
        }
    }
    if let Some((lhs, rhs)) = chunk.split_once(':') {
        let valid_lhs = !lhs.is_empty()
            && !rhs.starts_with(':')
            && lhs
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'));
        if valid_lhs && !rhs.is_empty() {
            return Some((lhs, rhs));
        }
    }
    None
}

fn check_candidate_entropy(raw_candidate: &str) -> bool {
    let candidate = raw_candidate.trim_end_matches('.');
    if candidate.len() < MIN_HIGH_ENTROPY_LEN {
        return false;
    }
    if is_sha256_hex(candidate) || is_file_path_like(candidate) {
        return false;
    }
    if !has_mixed_char_classes(candidate) {
        return false;
    }
    shannon_entropy(candidate) >= MIN_SHANNON_ENTROPY
}

fn is_sha256_hex(candidate: &str) -> bool {
    candidate.len() == 64 && candidate.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_file_path_like(candidate: &str) -> bool {
    if candidate.starts_with('/')
        || candidate.starts_with("./")
        || candidate.starts_with("../")
        || candidate.starts_with("~/")
        || candidate.contains('\\')
    {
        return true;
    }

    let bytes = candidate.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/' {
        return true;
    }

    let lower = candidate.to_ascii_lowercase();
    if PATH_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
        return true;
    }

    if candidate.contains('/') && !candidate.contains('+') && !candidate.contains('=') {
        let segments: Vec<&str> = candidate.split('/').collect();
        if segments.len() >= 2
            && segments.iter().all(|seg| {
                !seg.is_empty()
                    && seg.bytes().all(|b| {
                        b.is_ascii_lowercase()
                            || b.is_ascii_digit()
                            || matches!(b, b'.' | b'_' | b'-')
                    })
            })
        {
            return true;
        }
    }

    false
}

fn has_mixed_char_classes(candidate: &str) -> bool {
    let mut has_lower = false;
    let mut has_upper = false;
    let mut has_digit = false;
    let mut has_symbol = false;

    for b in candidate.bytes() {
        if b.is_ascii_lowercase() {
            has_lower = true;
        } else if b.is_ascii_uppercase() {
            has_upper = true;
        } else if b.is_ascii_digit() {
            has_digit = true;
        } else if b.is_ascii_graphic() {
            has_symbol = true;
        }
    }

    let class_count = usize::from(has_lower)
        + usize::from(has_upper)
        + usize::from(has_digit)
        + usize::from(has_symbol);

    class_count >= 3 || (has_digit && (has_lower || has_upper))
}

fn shannon_entropy(input: &str) -> f64 {
    if input.is_empty() {
        return 0.0;
    }
    let mut counts = [0usize; 256];
    let bytes = input.as_bytes();
    for &b in bytes {
        counts[b as usize] += 1;
    }
    let total = bytes.len() as f64;
    let mut entropy = 0.0;
    for &count in &counts {
        if count > 0 {
            let prob = count as f64 / total;
            entropy -= prob * prob.log2();
        }
    }
    entropy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_no_leak(violations: &[SafetyViolation], sensitive: &str) {
        assert!(!violations.is_empty(), "expected at least one violation");
        for item in violations {
            assert!(
                !item.reason.contains(sensitive),
                "violation reason leaked sensitive substring"
            );
        }
    }

    #[test]
    fn rejects_pem_headers_without_leaking() {
        let policy = RedactionPolicy {
            public_safe_mode: true,
        };
        let header_a = ["-----BEGIN RSA ", "PRIVATE ", "KEY-----"].concat();
        let header_b = ["BEGIN OPENSSH ", "PRIVATE ", "BLOCK"].concat();
        let payload = format!("safe line\n{header_a}\n{header_b}\n");

        let report = inspect_text(&payload, &policy);
        assert!(!report.ok);
        assert_eq!(report.violations.len(), 2);
        assert_eq!(report.violations[0].kind, ViolationKind::PemPrivateBlock);
        assert_eq!(report.violations[0].line, 2);
        assert_eq!(report.violations[1].kind, ViolationKind::PemPrivateBlock);
        assert_eq!(report.violations[1].line, 3);
        assert_no_leak(&report.violations, &header_a);
        assert_no_leak(&report.violations, &header_b);
    }

    #[test]
    fn rejects_api_prefixes_without_leaking() {
        let policy = RedactionPolicy {
            public_safe_mode: true,
        };
        let samples = [
            ["ghp_", "AbCdEf123456GhIj"].concat(),
            ["gho_", "XyZ987654321MnOp"].concat(),
            ["github_pat_", "11AABBCCDD_123456"].concat(),
            ["sk-", "live998877665544"].concat(),
            ["xai-", "demo112233445566"].concat(),
            ["AKIA", "IOSFODNN7EXAMPLE"].concat(),
            ["uai_pat_", "0011223344556677"].concat(),
        ];

        for sample in &samples {
            let err = require_safe_text(sample, &policy).expect_err("expected prefix rejection");
            assert!(err
                .iter()
                .any(|v| v.kind == ViolationKind::CredentialPrefix));
            assert_no_leak(&err, sample);
        }
    }

    #[test]
    fn bearer_allows_placeholders_and_rejects_live_values() {
        let policy = RedactionPolicy {
            public_safe_mode: true,
        };
        let safe_doc = "\
            Authorization: Bearer <CREDENTIAL>\n\
            Authorization: Bearer [R]\n\
            Authorization: Bearer <REDACTED>\n\
            The bearer of good news arrived.\n";
        assert!(require_safe_text(safe_doc, &policy).is_ok());

        let unsafe_val = ["auth_grant_", "99887766aabbccdd"].concat();
        let unsafe_doc = format!("Authorization: Bearer {unsafe_val}");
        let err = require_safe_text(&unsafe_doc, &policy).expect_err("expected bearer rejection");
        assert!(err
            .iter()
            .any(|v| v.kind == ViolationKind::BearerCredential));
        assert_no_leak(&err, &unsafe_val);
    }

    #[test]
    fn high_entropy_catches_opaque_literals_and_allows_sha256_and_links() {
        let policy = RedactionPolicy {
            public_safe_mode: true,
        };
        let digest = sha256_hex(b"streamline-safe-test-payload");
        assert_eq!(digest.len(), 64);

        let safe_doc = format!(
            "{digest}  streamline-linux-x86_64.tar.gz\n\
             [Architecture Guide](https://example.com/docs/v1/aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3z)\n\
             [Local Module](crates/streamline-conveyor/src/staging_manifest_builder.rs)\n\
             Look at VeryLongCamelCaseRustTypeIdentifierNameExample in source.\n"
        );
        assert!(require_safe_text(&safe_doc, &policy).is_ok());

        let opaque_grant = "K9mP2qX8vN3wZ5rT8yU4iO9pL1jH6gF3dS";
        let unsafe_doc = format!("GRANT={opaque_grant}\n");
        let err = require_safe_text(&unsafe_doc, &policy).expect_err("expected entropy rejection");
        assert!(err
            .iter()
            .any(|v| v.kind == ViolationKind::HighEntropyLiteral));
        assert_no_leak(&err, opaque_grant);
    }

    #[test]
    fn enforces_public_safe_mode_ipv4_rules() {
        let public_policy = RedactionPolicy {
            public_safe_mode: true,
        };
        let private_policy = RedactionPolicy {
            public_safe_mode: false,
        };

        let allowed_doc = "\
            bind_http = \"127.0.0.1:5880\"\n\
            peer_a = \"192.0.2.10\"\n\
            peer_b = \"198.51.100.25\"\n\
            peer_c = \"203.0.113.42\"\n";
        assert!(require_safe_text(allowed_doc, &public_policy).is_ok());

        let lan_ip = format!("{}.{}.{}.{}", 192, 168, 1, 50);
        let rfc1918_a = format!("{}.{}.{}.{}", 10, 0, 0, 5);
        let rfc1918_b = format!("{}.{}.{}.{}", 172, 16, 4, 20);

        for addr in [&lan_ip, &rfc1918_a, &rfc1918_b] {
            let line = format!("host = \"{addr}:8080\"");
            let err =
                require_safe_text(&line, &public_policy).expect_err("expected RFC-1918 rejection");
            assert_eq!(err.len(), 1);
            assert_eq!(err[0].kind, ViolationKind::PrivateNetworkAddress);
            assert_no_leak(&err, addr);

            assert!(require_safe_text(&line, &private_policy).is_ok());
        }
    }

    #[test]
    fn style_helper_detects_em_and_en_dashes() {
        let clean = inspect_style_no_dashes("plain-ascii - hyphen only");
        assert!(clean.ok);
        assert!(clean.violations.is_empty());

        let with_em = format!("first{}second", '\u{2014}');
        let em_report = inspect_style_no_dashes(&with_em);
        assert!(!em_report.ok);
        assert_eq!(em_report.violations[0].kind, ViolationKind::DisallowedDash);
        assert_eq!(em_report.violations[0].line, 1);

        let with_en = format!("line one\nrange 1{}10", '\u{2013}');
        let en_report = inspect_style_no_dashes(&with_en);
        assert!(!en_report.ok);
        assert_eq!(en_report.violations[0].kind, ViolationKind::DisallowedDash);
        assert_eq!(en_report.violations[0].line, 2);
    }

    #[test]
    fn validates_safe_segments() {
        assert!(validate_safe_segment("streamline-core_01.md").is_ok());
        assert!(validate_safe_segment("v1.0.0").is_ok());
        assert!(validate_safe_segment("../etc").is_err());
        assert!(validate_safe_segment("a/b").is_err());
        assert!(validate_safe_segment("a\\b").is_err());
        assert!(validate_safe_segment("").is_err());
        assert!(validate_safe_segment(".").is_err());
        assert!(validate_safe_segment("..").is_err());
        assert!(validate_safe_segment("bad segment").is_err());
        let too_long = "a".repeat(129);
        assert!(validate_safe_segment(&too_long).is_err());
        let max_len = "a".repeat(128);
        assert!(validate_safe_segment(&max_len).is_ok());
    }

    #[test]
    fn sha256_and_serde_roundtrip() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        let report = inspect_text("safe text", &RedactionPolicy::default());
        let encoded = serde_json::to_string(&report).expect("serialize report");
        let decoded: SafetyReport = serde_json::from_str(&encoded).expect("deserialize report");
        assert_eq!(report, decoded);
    }
}
