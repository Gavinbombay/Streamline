use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::types::CoreError;

/// Execution harness definition for hands-off build and acceptance runs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HarnessSpec {
    pub id: String,
    pub family: String,
    pub command: Vec<String>,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: u32,
}

fn default_max_concurrency() -> u32 {
    1
}

/// Top-level runtime configuration matching SCHEMA.md section 2.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct StreamlineConfig {
    pub bind_http: String,
    pub bind_admin: String,
    pub db_path: String,
    pub vault_sync_dir: String,
    pub runs_dir: String,
    pub public_safe_mode: bool,
    pub presence_plugin_dir: String,
    pub memory_sync_url: String,
    pub worker_lease_url: String,
    pub harnesses: Vec<HarnessSpec>,
}

impl Default for StreamlineConfig {
    fn default() -> Self {
        Self {
            bind_http: "127.0.0.1:5880".to_string(),
            bind_admin: "127.0.0.1:5881".to_string(),
            db_path: "/var/lib/streamline/streamline.sqlite3".to_string(),
            vault_sync_dir: "/var/lib/streamline/vault".to_string(),
            runs_dir: "/var/lib/streamline/runs".to_string(),
            public_safe_mode: true,
            presence_plugin_dir: String::new(),
            memory_sync_url: String::new(),
            worker_lease_url: String::new(),
            harnesses: Vec::new(),
        }
    }
}

impl StreamlineConfig {
    /// Parses configuration from a TOML string.
    pub fn from_toml_str(raw: &str) -> Result<Self, CoreError> {
        toml::from_str::<Self>(raw).map_err(|err| CoreError::InvalidConfig(err.to_string()))
    }

    /// Parses configuration from a JSON string.
    pub fn from_json_str(raw: &str) -> Result<Self, CoreError> {
        serde_json::from_str::<Self>(raw).map_err(|err| CoreError::InvalidConfig(err.to_string()))
    }

    /// Loads configuration from `STREAMLINE_CONFIG_JSON` or `STREAMLINE_CONFIG_PATH`,
    /// falling back to defaults when neither environment variable is set.
    pub fn load_from_env() -> Result<Self, CoreError> {
        if let Ok(raw_json) = std::env::var("STREAMLINE_CONFIG_JSON") {
            if !raw_json.trim().is_empty() {
                return Self::from_json_str(&raw_json);
            }
        }
        if let Ok(config_path) = std::env::var("STREAMLINE_CONFIG_PATH") {
            let trimmed = config_path.trim();
            if !trimmed.is_empty() {
                let contents = std::fs::read_to_string(trimmed)
                    .map_err(|err| CoreError::InvalidConfig(err.to_string()))?;
                if trimmed.ends_with(".json") {
                    return Self::from_json_str(&contents);
                }
                return Self::from_toml_str(&contents);
            }
        }
        Ok(Self::default())
    }

    /// Returns the optional local Presence plugin directory when configured.
    pub fn presence_plugin_path(&self) -> Option<&Path> {
        let trimmed = self.presence_plugin_dir.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(Path::new(trimmed))
        }
    }

    /// Returns the optional external memory sync webhook URL when configured.
    pub fn memory_sync_endpoint(&self) -> Option<&str> {
        let trimmed = self.memory_sync_url.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    /// Returns the optional external worker lease webhook URL when configured.
    pub fn worker_lease_endpoint(&self) -> Option<&str> {
        let trimmed = self.worker_lease_url.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }
}
