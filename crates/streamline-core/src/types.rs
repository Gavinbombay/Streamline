use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Role assigned to an authenticated principal.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalRole {
    Owner,
    Member,
    Agent,
    Guest,
}

impl PrincipalRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Member => "member",
            Self::Agent => "agent",
            Self::Guest => "guest",
        }
    }
}

impl fmt::Display for PrincipalRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PrincipalRole {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "owner" => Ok(Self::Owner),
            "member" => Ok(Self::Member),
            "agent" => Ok(Self::Agent),
            "guest" => Ok(Self::Guest),
            _ => Err(CoreError::InvalidEnum {
                kind: "PrincipalRole",
                value: s.to_string(),
            }),
        }
    }
}

/// Lifecycle tier controlling bootstrap visibility and archival triage.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleTier {
    Active,
    Dormant,
    Legacy,
    Retired,
}

impl LifecycleTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Dormant => "dormant",
            Self::Legacy => "legacy",
            Self::Retired => "retired",
        }
    }
}

impl fmt::Display for LifecycleTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LifecycleTier {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "dormant" => Ok(Self::Dormant),
            "legacy" => Ok(Self::Legacy),
            "retired" => Ok(Self::Retired),
            _ => Err(CoreError::InvalidEnum {
                kind: "LifecycleTier",
                value: s.to_string(),
            }),
        }
    }
}

/// Triage review status for governance and imported resources.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TriageState {
    Reviewed,
    NeedsTriage,
}

impl TriageState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reviewed => "reviewed",
            Self::NeedsTriage => "needs_triage",
        }
    }
}

impl fmt::Display for TriageState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TriageState {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "reviewed" => Ok(Self::Reviewed),
            "needs_triage" | "needs-triage" => Ok(Self::NeedsTriage),
            _ => Err(CoreError::InvalidEnum {
                kind: "TriageState",
                value: s.to_string(),
            }),
        }
    }
}

/// Verification freshness recorded on a resource Source Passport.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessState {
    Verified,
    Stale,
    Drift,
    Unknown,
}

impl FreshnessState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Stale => "stale",
            Self::Drift => "drift",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for FreshnessState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FreshnessState {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "verified" => Ok(Self::Verified),
            "stale" => Ok(Self::Stale),
            "drift" => Ok(Self::Drift),
            "unknown" => Ok(Self::Unknown),
            _ => Err(CoreError::InvalidEnum {
                kind: "FreshnessState",
                value: s.to_string(),
            }),
        }
    }
}

/// Broad classification of a Trio+ work unit.
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum WorkKind {
    Software,
    Council,
    Procedure,
    #[default]
    General,
}

impl WorkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Software => "software",
            Self::Council => "council",
            Self::Procedure => "procedure",
            Self::General => "general",
        }
    }
}

impl fmt::Display for WorkKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for WorkKind {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "software" => Ok(Self::Software),
            "council" => Ok(Self::Council),
            "procedure" => Ok(Self::Procedure),
            "general" => Ok(Self::General),
            _ => Err(CoreError::InvalidEnum {
                kind: "WorkKind",
                value: s.to_string(),
            }),
        }
    }
}

/// Scalable Trio+ project profile tier.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrioProfile {
    P0,
    P1,
    P2,
    P3,
    P4,
}

impl TrioProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::P0 => "P0",
            Self::P1 => "P1",
            Self::P2 => "P2",
            Self::P3 => "P3",
            Self::P4 => "P4",
        }
    }
}

impl fmt::Display for TrioProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TrioProfile {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "P0" => Ok(Self::P0),
            "P1" => Ok(Self::P1),
            "P2" => Ok(Self::P2),
            "P3" => Ok(Self::P3),
            "P4" => Ok(Self::P4),
            _ => Err(CoreError::InvalidEnum {
                kind: "TrioProfile",
                value: s.to_string(),
            }),
        }
    }
}

/// Requirement verification stage (`D -> I -> T -> A -> L`, or explicit `UNKNOWN`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReqStage {
    D,
    I,
    T,
    A,
    L,
    #[serde(rename = "UNKNOWN")]
    Unknown,
}

impl ReqStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::D => "D",
            Self::I => "I",
            Self::T => "T",
            Self::A => "A",
            Self::L => "L",
            Self::Unknown => "UNKNOWN",
        }
    }
}

impl fmt::Display for ReqStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReqStage {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "D" => Ok(Self::D),
            "I" => Ok(Self::I),
            "T" => Ok(Self::T),
            "A" => Ok(Self::A),
            "L" => Ok(Self::L),
            "UNKNOWN" => Ok(Self::Unknown),
            _ => Err(CoreError::InvalidEnum {
                kind: "ReqStage",
                value: s.to_string(),
            }),
        }
    }
}

/// Conveyor execution phase.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Build,
    Accept,
    #[serde(rename = "SECURITY_AUDIT", alias = "security_audit")]
    SecurityAudit,
}

impl RunPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Accept => "accept",
            Self::SecurityAudit => "SECURITY_AUDIT",
        }
    }
}

impl fmt::Display for RunPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RunPhase {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "build" => Ok(Self::Build),
            "accept" => Ok(Self::Accept),
            "security_audit" => Ok(Self::SecurityAudit),
            _ => Err(CoreError::InvalidEnum {
                kind: "RunPhase",
                value: s.to_string(),
            }),
        }
    }
}

/// Conveyor and project lifecycle state.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunState {
    Staged,
    Building,
    Cleaning,
    Scanning,
    Accepting,
    DocPendingOwner,
    AuditPending,
    Done,
    Impasse,
    Quarantined,
}

impl RunState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Staged => "STAGED",
            Self::Building => "BUILDING",
            Self::Cleaning => "CLEANING",
            Self::Scanning => "SCANNING",
            Self::Accepting => "ACCEPTING",
            Self::DocPendingOwner => "DOC_PENDING_OWNER",
            Self::AuditPending => "AUDIT_PENDING",
            Self::Done => "DONE",
            Self::Impasse => "IMPASSE",
            Self::Quarantined => "QUARANTINED",
        }
    }
}

impl fmt::Display for RunState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RunState {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_ascii_uppercase().replace('-', "_");
        match normalized.as_str() {
            "STAGED" => Ok(Self::Staged),
            "BUILDING" => Ok(Self::Building),
            "CLEANING" => Ok(Self::Cleaning),
            "SCANNING" => Ok(Self::Scanning),
            "ACCEPTING" => Ok(Self::Accepting),
            "DOC_PENDING_OWNER" => Ok(Self::DocPendingOwner),
            "AUDIT_PENDING" => Ok(Self::AuditPending),
            "DONE" => Ok(Self::Done),
            "IMPASSE" => Ok(Self::Impasse),
            "QUARANTINED" => Ok(Self::Quarantined),
            _ => Err(CoreError::InvalidEnum {
                kind: "RunState",
                value: s.to_string(),
            }),
        }
    }
}

/// Resolution status for an appended divergence entry.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DivergenceStatus {
    Open,
    Accepted,
    Fixed,
}

impl DivergenceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Accepted => "accepted",
            Self::Fixed => "fixed",
        }
    }
}

impl fmt::Display for DivergenceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DivergenceStatus {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "open" => Ok(Self::Open),
            "accepted" => Ok(Self::Accepted),
            "fixed" => Ok(Self::Fixed),
            _ => Err(CoreError::InvalidEnum {
                kind: "DivergenceStatus",
                value: s.to_string(),
            }),
        }
    }
}

/// Verdict produced by check_sync and persisted in sync_reports.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SyncVerdict {
    Pass,
    Fail,
    Partial,
}

impl SyncVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Partial => "partial",
        }
    }
}

impl fmt::Display for SyncVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SyncVerdict {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pass" => Ok(Self::Pass),
            "fail" => Ok(Self::Fail),
            "partial" => Ok(Self::Partial),
            _ => Err(CoreError::InvalidEnum {
                kind: "SyncVerdict",
                value: s.to_string(),
            }),
        }
    }
}

/// Owner decision entry belonging to a Trio+ project.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnDecision {
    pub project_id: String,
    pub own_id: String,
    pub statement: String,
    pub locked_at: Option<String>,
    pub approved_by: Option<String>,
}

/// Requirement row tracked through the D I T A L lifecycle.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RequirementRow {
    pub project_id: String,
    pub req_id: String,
    pub source_id: String,
    pub statement: String,
    pub stage: ReqStage,
    pub evidence_ref: String,
    #[serde(default)]
    pub builder_principal: Option<String>,
    #[serde(default)]
    pub builder_family: Option<String>,
    #[serde(default)]
    pub acceptor_principal: Option<String>,
    #[serde(default)]
    pub acceptor_family: Option<String>,
    pub updated_at: String,
}

/// Logged divergence from a locked OWN decision or plan baseline.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    pub project_id: String,
    pub div_id: String,
    pub designed: String,
    pub observed: String,
    pub resolution: String,
    pub status: DivergenceStatus,
    pub author_principal: String,
    pub created_at: String,
}

/// Paired row emitted by check_sync comparing planned requirements with observed claims.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PairedSyncRow {
    pub req_id: String,
    pub planned: String,
    pub observed: String,
    pub stage: ReqStage,
    pub matched: bool,
}

/// Source Passport capturing custody, freshness, and lineage for a resource.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SourcePassport {
    pub resource_id: String,
    pub steward: String,
    pub last_verified_at: String,
    pub freshness_state: FreshnessState,
    pub lineage_ref: String,
    pub access_scope: String,
    pub next_action: String,
}

/// Conveyor build or acceptance run record.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConveyorRun {
    pub run_id: String,
    pub project_id: String,
    pub phase: RunPhase,
    pub harness_id: String,
    pub family: String,
    pub input_manifest_sha256: String,
    pub output_manifest_sha256: String,
    pub cleanup_clean: bool,
    pub scan_clean: bool,
    pub state: RunState,
    pub started_at: String,
    pub finished_at: Option<String>,
}

/// Persistent audit event entry.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AuditEvent {
    pub event_id: String,
    pub principal_id: String,
    pub action: String,
    pub target_id: String,
    pub outcome: String,
    pub detail_json: String,
    pub created_at: String,
}

/// Domain validation and configuration errors.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid enum value '{value}' for {kind}")]
    InvalidEnum { kind: &'static str, value: String },
    #[error("invalid stage transition from {from} to {to}: {reason}")]
    InvalidStageTransition {
        from: ReqStage,
        to: ReqStage,
        reason: String,
    },
    #[error("evidence_ref is required to advance requirement stage to {0}")]
    EvidenceRequired(ReqStage),
    #[error("self-acceptance forbidden: acceptor '{principal}' cannot equal builder")]
    SelfAcceptanceForbidden { principal: String },
    #[error("cross-family acceptance required for risk >= 2 (builder family '{builder_family}', acceptor family '{acceptor_family}')")]
    SameFamilyForbidden {
        builder_family: String,
        acceptor_family: String,
    },
    #[error("cross-family metadata missing for risk >= 2 acceptance")]
    CrossFamilyMetadataMissing,
    #[error("role '{0}' cannot perform this stage transition")]
    ForbiddenStageRole(PrincipalRole),
    #[error("approved HOW-IT-WORKS document is required before setting stage L")]
    HowItWorksNotApproved,
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_state_and_phase_round_trip() {
        assert_eq!(RunState::AuditPending.as_str(), "AUDIT_PENDING");
        assert_eq!(RunState::AuditPending.to_string(), "AUDIT_PENDING");
        assert_eq!(
            "AUDIT_PENDING".parse::<RunState>().unwrap(),
            RunState::AuditPending
        );
        assert_eq!(
            "audit-pending".parse::<RunState>().unwrap(),
            RunState::AuditPending
        );
        let state_json = serde_json::to_string(&RunState::AuditPending).unwrap();
        assert_eq!(state_json, "\"AUDIT_PENDING\"");
        assert_eq!(
            serde_json::from_str::<RunState>(&state_json).unwrap(),
            RunState::AuditPending
        );

        assert_eq!(RunPhase::SecurityAudit.as_str(), "SECURITY_AUDIT");
        assert_eq!(RunPhase::SecurityAudit.to_string(), "SECURITY_AUDIT");
        assert_eq!(
            "SECURITY_AUDIT".parse::<RunPhase>().unwrap(),
            RunPhase::SecurityAudit
        );
        assert_eq!(
            "security_audit".parse::<RunPhase>().unwrap(),
            RunPhase::SecurityAudit
        );
        let phase_json = serde_json::to_string(&RunPhase::SecurityAudit).unwrap();
        assert_eq!(phase_json, "\"SECURITY_AUDIT\"");
        assert_eq!(
            serde_json::from_str::<RunPhase>(&phase_json).unwrap(),
            RunPhase::SecurityAudit
        );
    }
}
