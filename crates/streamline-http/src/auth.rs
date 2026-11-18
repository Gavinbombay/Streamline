use axum::http::{header, HeaderMap, StatusCode};
use streamline_store::AccessGrant;

use crate::error::ApiError;
use crate::state::AppState;

/// Extracts a raw credential from `Authorization: Bearer <credential>` or
/// `X-Streamline-Credential: <credential>` headers without logging its contents.
pub fn extract_http_credential(headers: &HeaderMap) -> Option<String> {
    if let Some(auth_val) = headers.get(header::AUTHORIZATION) {
        if let Ok(raw_auth) = auth_val.to_str() {
            if let Some((scheme, rest)) = raw_auth.trim().split_once(' ') {
                let trimmed = rest.trim();
                if scheme.eq_ignore_ascii_case("bearer") && !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    if let Some(cred_val) = headers.get("x-streamline-credential") {
        if let Ok(raw_cred) = cred_val.to_str() {
            let trimmed = raw_cred.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

/// Authenticates an HTTP caller against `StreamlineStore` access grants.
pub fn authenticate_http(state: &AppState, headers: &HeaderMap) -> Result<AccessGrant, ApiError> {
    let raw_cred = extract_http_credential(headers);
    authenticate_raw(state, raw_cred.as_deref())
}

/// Authenticates a raw credential string against `StreamlineStore` access grants.
pub fn authenticate_raw(state: &AppState, raw_cred: Option<&str>) -> Result<AccessGrant, ApiError> {
    let credential = raw_cred
        .map(str::trim)
        .filter(|val| !val.is_empty())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "missing or empty authentication credential",
            )
        })?;

    state
        .store
        .authenticate_grant(credential)
        .map_err(ApiError::from)
}
