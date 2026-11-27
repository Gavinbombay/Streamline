use std::path::PathBuf;
use std::sync::Arc;

use streamline_core::StreamlineConfig;
use streamline_safe::RedactionPolicy;
use streamline_store::StreamlineStore;
use tokio::sync::Semaphore;

/// Shared server state passed to Axum handlers and the MCP JSON-RPC dispatcher.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<StreamlineStore>,
    pub config: StreamlineConfig,
    pub redaction_policy: RedactionPolicy,
    pub skills_dir: PathBuf,
    // Shared by every clone serving MCP and REST requests.
    pub(crate) ecosystem_brief_slots: Arc<Semaphore>,
    #[cfg(test)]
    pub(crate) ecosystem_brief_override: Option<fn() -> serde_json::Value>,
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
            ecosystem_brief_slots: Arc::new(Semaphore::new(1)),
            #[cfg(test)]
            ecosystem_brief_override: None,
        }
    }
}
