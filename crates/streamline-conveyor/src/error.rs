use thiserror::Error;

/// Errors returned by the Streamline conveyor staging, execution, and export layers.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ConveyorError {
    #[error("builder family '{0}' cannot act as its own acceptor for R2+ work")]
    SameFamilyForbidden(String),
    #[error("cleanup failed: stray artifact found at '{0}'")]
    DirtyScratch(String),
    #[error("staging failed: {0}")]
    StageFailure(String),
    #[error("missing input path: '{0}'")]
    MissingInput(String),
    #[error("invalid conveyor configuration: {0}")]
    InvalidConfig(String),
    #[error("runner execution failed: {0}")]
    RunnerFailure(String),
    #[error("redaction policy blocked output: {0}")]
    RedactionBlocked(String),
}
