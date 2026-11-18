use std::path::PathBuf;
use std::sync::Arc;

use streamline_core::StreamlineConfig;
use streamline_safe::RedactionPolicy;
use streamline_store::StreamlineStore;

/// Shared server state passed to Axum handlers and the MCP JSON-RPC dispatcher.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<StreamlineStore>,
    pub config: StreamlineConfig,
    pub redaction_policy: RedactionPolicy,
    pub skills_dir: PathBuf,
}

impl AppState {
    /// Builds `AppState` with a `RedactionPolicy` synchronized to `config.public_safe_mode`.
    pub fn new(store: Arc<StreamlineStore>, config: StreamlineConfig, skills_dir: PathBuf) -> Self {
        let redaction_policy = RedactionPolicy {
            public_safe_mode: config.public_safe_mode,
        };
        Self {
            store,
            config,
            redaction_policy,
            skills_dir,
        }
    }
}
