pub mod config;
pub mod trio;
pub mod types;

pub use config::{HarnessSpec, StreamlineConfig};
pub use trio::{
    assess_work, assess_work_with_modules, check_sync, check_sync_claims, expected_roles,
    parse_as_built_markdown, validate_how_it_works, validate_stage_transition, AsBuiltClaim,
    AssessmentInput, AssessmentOutcome, StageTransitionRequest, SyncCheckSummary,
};
pub use types::{
    AuditEvent, ConveyorRun, CoreError, Divergence, DivergenceStatus, FreshnessState,
    LifecycleTier, OwnDecision, PairedSyncRow, PrincipalRole, ReqStage, RequirementRow, RunPhase,
    RunState, SourcePassport, SyncVerdict, TriageState, TrioProfile, WorkKind,
};
