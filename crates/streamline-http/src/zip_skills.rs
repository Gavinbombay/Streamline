use std::path::Path;

use serde::Serialize;
use streamline_safe::sha256_hex;

const EMBEDDED_SKILLS: [(&str, &str, &str, &str); 4] = [
    (
        "streamline-director",
        "director",
        "streamline-director/SKILL.md",
        include_str!("../../../skills/streamline-director/SKILL.md"),
    ),
    (
        "streamline-worker",
        "worker",
        "streamline-worker/SKILL.md",
        include_str!("../../../skills/streamline-worker/SKILL.md"),
    ),
    (
        "streamline-acceptor",
        "acceptor",
        "streamline-acceptor/SKILL.md",
        include_str!("../../../skills/streamline-acceptor/SKILL.md"),
    ),
    (
        "streamline-vault",
        "vault",
        "streamline-vault/SKILL.md",
        include_str!("../../../skills/streamline-vault/SKILL.md"),
    ),
];

/// Portable skill document metadata and markdown body.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BundledSkill {
    pub name: String,
    pub role: String,
    pub entry_path: String,
    pub description: String,
    pub sha256: String,
    pub body_md: String,
}

fn parse_frontmatter_description(markdown: &str) -> String {
    let normalized = markdown.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return String::new();
    };
    let Some((frontmatter, _)) = rest.split_once("\n---\n") else {
        return String::new();
    };
    for line in frontmatter.lines() {
        if let Some((field_name, field_val)) = line.split_once(':') {
            if field_name.trim() == "description" {
                return field_val
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
            }
        }
    }
    String::new()
}

/// Loads all four portable Streamline skills strictly from the compile-time
/// `EMBEDDED_SKILLS` bundle without touching the filesystem.
/// (`_skills_dir` is retained only for signature compatibility.)
pub fn load_bundled_skills(_skills_dir: &Path) -> Vec<BundledSkill> {
    let mut skills = Vec::with_capacity(EMBEDDED_SKILLS.len());
    for (name, role, entry_path, embedded_body) in EMBEDDED_SKILLS {
        let body_md = embedded_body.to_string();
        let description = parse_frontmatter_description(&body_md);
        let sha256 = sha256_hex(body_md.as_bytes());
        skills.push(BundledSkill {
            name: name.to_string(),
            role: role.to_string(),
            entry_path: entry_path.to_string(),
            description,
            sha256,
            body_md,
        });
    }
    skills
}

/// Standard IEEE 802.3 CRC-32 (polynomial 0xEDB88320).
pub fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

struct ZipEntryRecord {
    name_bytes: Vec<u8>,
    crc: u32,
    size: u32,
    local_offset: u32,
}

/// Builds a valid uncompressed PKZIP (`method = 0`) archive containing all 4 portable skills
/// served strictly from compile-time `EMBEDDED_SKILLS` without reading the filesystem.
/// (`_skills_dir` is retained only for signature compatibility.)
pub fn build_skills_zip(_skills_dir: &Path) -> Vec<u8> {
    let skills = load_bundled_skills(_skills_dir);
    let mut archive = Vec::new();
    let mut records = Vec::with_capacity(skills.len());

    // 2026-01-01 00:00:00 in MS-DOS date/time encoding.
    let dos_time: u16 = 0x0000;
    let dos_date: u16 = 0x5C21;

    for skill in &skills {
        let name_bytes = skill.entry_path.as_bytes().to_vec();
        let data = skill.body_md.as_bytes();
        let crc = crc32_ieee(data);
        let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
        let name_len = u16::try_from(name_bytes.len()).unwrap_or(u16::MAX);
        let local_offset = u32::try_from(archive.len()).unwrap_or(u32::MAX);

        // Local file header (30 bytes + filename).
        archive.extend_from_slice(b"PK\x03\x04");
        archive.extend_from_slice(&20u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&dos_time.to_le_bytes());
        archive.extend_from_slice(&dos_date.to_le_bytes());
        archive.extend_from_slice(&crc.to_le_bytes());
        archive.extend_from_slice(&size.to_le_bytes());
        archive.extend_from_slice(&size.to_le_bytes());
        archive.extend_from_slice(&name_len.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&name_bytes);
        archive.extend_from_slice(data);

        records.push(ZipEntryRecord {
            name_bytes,
            crc,
            size,
            local_offset,
        });
    }

    let cd_offset = u32::try_from(archive.len()).unwrap_or(u32::MAX);
    for rec in &records {
        let name_len = u16::try_from(rec.name_bytes.len()).unwrap_or(u16::MAX);
        // Central directory file header (46 bytes + filename).
        archive.extend_from_slice(b"PK\x01\x02");
        archive.extend_from_slice(&0x0314u16.to_le_bytes());
        archive.extend_from_slice(&20u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&dos_time.to_le_bytes());
        archive.extend_from_slice(&dos_date.to_le_bytes());
        archive.extend_from_slice(&rec.crc.to_le_bytes());
        archive.extend_from_slice(&rec.size.to_le_bytes());
        archive.extend_from_slice(&rec.size.to_le_bytes());
        archive.extend_from_slice(&name_len.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&(0o100_644u32 << 16).to_le_bytes());
        archive.extend_from_slice(&rec.local_offset.to_le_bytes());
        archive.extend_from_slice(&rec.name_bytes);
    }

    let cd_size = u32::try_from(archive.len())
        .unwrap_or(u32::MAX)
        .saturating_sub(cd_offset);
    let entry_count = u16::try_from(records.len()).unwrap_or(u16::MAX);

    // End of central directory record (22 bytes).
    archive.extend_from_slice(b"PK\x05\x06");
    archive.extend_from_slice(&0u16.to_le_bytes());
    archive.extend_from_slice(&0u16.to_le_bytes());
    archive.extend_from_slice(&entry_count.to_le_bytes());
    archive.extend_from_slice(&entry_count.to_le_bytes());
    archive.extend_from_slice(&cd_size.to_le_bytes());
    archive.extend_from_slice(&cd_offset.to_le_bytes());
    archive.extend_from_slice(&0u16.to_le_bytes());

    archive
}
