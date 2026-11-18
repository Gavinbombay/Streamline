use streamline_core::CoreError;
use thiserror::Error;

/// Typed persistence, authorization, concurrency, and workflow errors.
#[derive(Error, Debug)]
pub enum StoreError {
    #[error("rules acknowledgement required (current_rules_sha256={current_rules_sha256})")]
    RulesAckRequired { current_rules_sha256: String },

    #[error("forbidden: missing required tag '{required_tag}'")]
    ForbiddenTag { required_tag: String },

    #[error("forbidden: grant lacks write permission")]
    ForbiddenWrite,

    #[error("forbidden: grant lacks dispatch permission")]
    ForbiddenDispatch,

    #[error("forbidden: role '{0}' cannot perform this operation")]
    ForbiddenRole(String),

    #[error("unauthorized: invalid grant or credential")]
    Unauthorized,

    #[error("grant has been revoked")]
    GrantRevoked,

    #[error("grant has expired")]
    GrantExpired,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("OWN decision is locked and cannot be modified; record a divergence instead")]
    OwnLocked,

    #[error("invalid state: {0}")]
    InvalidState(String),

    #[error("invalid HOW-IT-WORKS document: {0:?}")]
    InvalidHowItWorks(Vec<String>),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("domain validation failed: {0}")]
    Core(#[from] CoreError),

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("internal store lock poisoned")]
    LockPoisoned,
}
