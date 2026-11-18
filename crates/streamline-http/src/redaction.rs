use axum::http::StatusCode;
use streamline_safe::{require_safe_text, validate_safe_segment, RedactionPolicy};

use crate::error::ApiError;

/// Validates a path, slug, or project identifier segment via `streamline_safe::validate_safe_segment`.
pub fn check_safe_segment(field_name: &str, value: &str) -> Result<(), ApiError> {
    validate_safe_segment(value).map_err(|reason| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "UNPROCESSABLE",
            format!("invalid segment for '{field_name}': {reason}"),
        )
    })
}

/// Validates an optional segment when provided.
pub fn check_optional_safe_segment(field_name: &str, value: Option<&str>) -> Result<(), ApiError> {
    if let Some(seg) = value {
        check_safe_segment(field_name, seg)?;
    }
    Ok(())
}

/// Validates text content through `streamline_safe::require_safe_text`, returning
/// `422 REDACTION_BLOCKED` on policy hits without repeating sensitive values.
pub fn check_safe_text(
    field_name: &str,
    value: &str,
    policy: &RedactionPolicy,
) -> Result<(), ApiError> {
    if value.is_empty() {
        return Ok(());
    }
    require_safe_text(value, policy)
        .map_err(|violations| ApiError::redaction_blocked(field_name, &violations))
}

/// Validates an optional text field when provided.
pub fn check_optional_safe_text(
    field_name: &str,
    value: Option<&str>,
    policy: &RedactionPolicy,
) -> Result<(), ApiError> {
    if let Some(text) = value {
        check_safe_text(field_name, text, policy)?;
    }
    Ok(())
}
