pub mod auth;
pub mod error;
pub mod mcp;
pub mod redaction;
pub mod state;
pub mod tools;
pub mod ui;
pub mod zip_skills;

#[cfg(test)]
mod tests;

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tower_http::services::ServeDir;

pub use error::{ApiEnvelope, ApiError, ApiErrorBody};
pub use mcp::handle_mcp_jsonrpc;
pub use state::AppState;
pub use tools::{tools_list, McpToolDescriptor, ReadyStatus};

/// Builds the complete Axum router for Streamline HTTP, MCP, skills archive, and Web UI.
pub fn build_router(state: AppState) -> Router {
    let mut router = Router::new()
        .route("/", get(ui::index_handler))
        .route("/ready", get(ready_handler))
        .route("/mcp", post(mcp_http_handler))
        .route("/api/tools/:tool_name", post(rest_tool_handler))
        .route("/downloads/streamline-skills.zip", get(skills_zip_handler));

    if let Some(plugin_dir) = state.config.presence_plugin_path() {
        router = router.nest_service("/_streamline/presence-plugin", ServeDir::new(plugin_dir));
    }

    router.with_state(state)
}

async fn ready_handler(State(state): State<AppState>) -> Response {
    match tools::compute_ready_status(&state) {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(err) => err.into_response(),
    }
}

async fn mcp_http_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let payload = match serde_json::from_str::<Value>(&body) {
        Ok(val) => val,
        Err(err) => {
            return (
                StatusCode::OK,
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {err}"),
                    }
                })),
            )
                .into_response();
        }
    };

    let header_cred = auth::extract_http_credential(&headers);
    let outcome = tokio::task::spawn_blocking(move || {
        mcp::handle_mcp_jsonrpc(&state, header_cred.as_deref(), false, payload)
    })
    .await;

    match outcome {
        Ok(Some(response_json)) => (StatusCode::OK, Json(response_json)).into_response(),
        Ok(None) => StatusCode::ACCEPTED.into_response(),
        Err(_) => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "MCP dispatch task failed",
        )
        .into_response(),
    }
}

async fn rest_tool_handler(
    State(state): State<AppState>,
    Path(tool_name): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if !tools::is_known_tool(&tool_name) {
        return ApiError::new(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("unknown tool '{tool_name}'"),
        )
        .into_response();
    }

    let args: Value = if body.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str::<Value>(&body) {
            Ok(parsed) => parsed,
            Err(err) => {
                return ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "UNPROCESSABLE",
                    format!("invalid JSON body: {err}"),
                )
                .into_response();
            }
        }
    };

    let header_cred = auth::extract_http_credential(&headers);
    let outcome = tokio::task::spawn_blocking(move || -> Result<Value, ApiError> {
        let grant = if tool_name == "ready" {
            None
        } else {
            Some(auth::authenticate_raw(&state, header_cred.as_deref())?)
        };
        tools::dispatch_tool(&state, grant.as_ref(), &tool_name, args)
    })
    .await;

    match outcome {
        Ok(Ok(data)) => (StatusCode::OK, Json(ApiEnvelope::success(data))).into_response(),
        Ok(Err(api_err)) => api_err.into_response(),
        Err(_) => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "tool execution task failed",
        )
        .into_response(),
    }
}

async fn skills_zip_handler(State(state): State<AppState>) -> Response {
    let bytes = zip_skills::build_skills_zip(&state.skills_dir);
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"streamline-skills.zip\"",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response()
}
