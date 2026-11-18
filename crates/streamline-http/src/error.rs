use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use streamline_core::CoreError;
use streamline_safe::SafetyViolation;
use streamline_store::StoreError;

/// Structured error body returned in both REST responses and MCP tool errors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_rules_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

/// Typed JSON envelope matching SCHEMA.md section 4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiEnvelope<T> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiErrorBody>,
}

impl<T: Serialize> ApiEnvelope<T> {
    pub fn success(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }
}

/// HTTP and MCP error carrying HTTP status code and structured JSON error detail.
#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
    pub current_rules_sha256: Option<String>,
    pub detail: Option<Value>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
            current_rules_sha256: None,
            detail: None,
        }
    }

    pub fn with_detail(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
        detail: Value,
    ) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
            current_rules_sha256: None,
            detail: Some(detail),
        }
    }

    /// Constructs a `422 REDACTION_BLOCKED` error reporting line and reason without repeating
    /// the matched sensitive substring.
    pub fn redaction_blocked(field_name: &str, violations: &[SafetyViolation]) -> Self {
        let summary_parts: Vec<String> = violations
            .iter()
            .map(|item| format!("line {}: {}", item.line, item.reason))
            .collect();
        let message = if summary_parts.is_empty() {
            format!("redaction policy blocked field '{field_name}'")
        } else {
            format!(
                "redaction policy blocked field '{field_name}' ({})",
                summary_parts.join("; ")
            )
        };
        let detail = json!({
            "field": field_name,
            "violations": violations,
        });
        Self::with_detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "REDACTION_BLOCKED",
            message,
            detail,
        )
    }

    pub fn to_error_body(&self) -> ApiErrorBody {
        ApiErrorBody {
            code: self.code.clone(),
            message: self.message.clone(),
            current_rules_sha256: self.current_rules_sha256.clone(),
            detail: self.detail.clone(),
        }
    }

    pub fn to_envelope_value(&self) -> Value {
        json!({
            "ok": false,
            "error": self.to_error_body(),
        })
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status;
        let body = ApiEnvelope::<Value> {
            ok: false,
            data: None,
            error: Some(self.to_error_body()),
        };
        (status, Json(body)).into_response()
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "UNPROCESSABLE",
            format!("invalid JSON payload: {err}"),
        )
    }
}

impl From<StoreError> for ApiError {
    #[allow(unreachable_patterns)]
    fn from(err: StoreError) -> Self {
        match err {
            StoreError::Unauthorized | StoreError::GrantRevoked | StoreError::GrantExpired => {
                ApiError::new(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", err.to_string())
            }
            StoreError::ForbiddenTag { ref required_tag } => ApiError::with_detail(
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                err.to_string(),
                json!({ "required_tag": required_tag }),
            ),
            StoreError::ForbiddenWrite
            | StoreError::ForbiddenDispatch
            | StoreError::ForbiddenRole(_) => {
                ApiError::new(StatusCode::FORBIDDEN, "FORBIDDEN", err.to_string())
            }
            StoreError::NotFound(_) => {
                ApiError::new(StatusCode::NOT_FOUND, "NOT_FOUND", err.to_string())
            }
            StoreError::Conflict(_) => {
                ApiError::new(StatusCode::CONFLICT, "CONFLICT", err.to_string())
            }
            StoreError::OwnLocked => {
                ApiError::new(StatusCode::CONFLICT, "OWN_LOCKED", err.to_string())
            }
            StoreError::RulesAckRequired {
                current_rules_sha256,
            } => {
                let mut api_err = ApiError::with_detail(
                    StatusCode::PRECONDITION_FAILED,
                    "RULES_ACK_REQUIRED",
                    format!(
                        "rules acknowledgement required (current_rules_sha256={current_rules_sha256})"
                    ),
                    json!({ "current_rules_sha256": current_rules_sha256 }),
                );
                api_err.current_rules_sha256 = Some(current_rules_sha256);
                api_err
            }
            StoreError::InvalidHowItWorks(ref errors) => ApiError::with_detail(
                StatusCode::UNPROCESSABLE_ENTITY,
                "UNPROCESSABLE",
                err.to_string(),
                json!({ "validation_errors": errors }),
            ),
            StoreError::InvalidState(_) | StoreError::InvalidInput(_) => ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "UNPROCESSABLE",
                err.to_string(),
            ),
            StoreError::Core(ref core_err) => match core_err {
                CoreError::ForbiddenStageRole(_) => {
                    ApiError::new(StatusCode::FORBIDDEN, "FORBIDDEN", err.to_string())
                }
                _ => ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "UNPROCESSABLE",
                    err.to_string(),
                ),
            },
            StoreError::Sqlite(ref sql_err) => {
                let msg = sql_err.to_string();
                if msg.contains("OWN_LOCKED") {
                    ApiError::new(
                        StatusCode::CONFLICT,
                        "OWN_LOCKED",
                        "OWN decision is locked and cannot be modified; record a divergence instead",
                    )
                } else {
                    ApiError::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "INTERNAL_ERROR",
                        "internal database error",
                    )
                }
            }
            StoreError::Io(_) | StoreError::Json(_) | StoreError::LockPoisoned => ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "internal storage error",
            ),
            other => ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "UNPROCESSABLE",
                other.to_string(),
            ),
        }
    }
}
