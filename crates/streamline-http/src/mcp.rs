use serde_json::{json, Value};

use crate::auth;
use crate::error::ApiError;
use crate::state::AppState;
use crate::tools;

/// Maximum number of entries (including notifications) in one MCP batch.
pub(crate) const MAX_MCP_BATCH_ITEMS: usize = 16;

fn jsonrpc_ok(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn jsonrpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into(),
        }
    })
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {
                "listChanged": false
            }
        },
        "serverInfo": {
            "name": "streamline",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": "Call ready, bootstrap, and query_lessons first, then call ack_rules with rules_sha256 before any write.",
    })
}

fn tool_success_result(data: Value) -> Value {
    let envelope = json!({
        "ok": true,
        "data": data,
    });
    let text = serde_json::to_string(&envelope).unwrap_or_else(|_| "{\"ok\":true}".to_string());
    json!({
        "content": [
            {
                "type": "text",
                "text": text,
            }
        ],
        "structuredContent": envelope,
        "isError": false,
    })
}

fn tool_error_result(err: &ApiError) -> Value {
    let envelope = json!({
        "ok": false,
        "status": err.status.as_u16(),
        "error": err.to_error_body(),
    });
    let text = serde_json::to_string(&envelope).unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":{\"code\":\"INTERNAL_ERROR\",\"message\":\"serialization error\"}}"
            .to_string()
    });
    json!({
        "content": [
            {
                "type": "text",
                "text": text,
            }
        ],
        "structuredContent": envelope,
        "isError": true,
    })
}

fn extract_param_credential(params: &Value, arguments: &mut Value) -> Option<String> {
    if let Some(cred) = params
        .get("credential")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|val| !val.is_empty())
    {
        return Some(cred.to_string());
    }
    if let Some(obj) = arguments.as_object_mut() {
        if let Some(val) = obj.remove("credential") {
            if let Some(cred) = val.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                return Some(cred.to_string());
            }
        }
    }
    None
}

fn handle_single_request(
    state: &AppState,
    default_credential: Option<&str>,
    allow_param_credential: bool,
    payload: Value,
) -> Option<Value> {
    let Some(obj) = payload.as_object() else {
        return Some(jsonrpc_error(
            Value::Null,
            -32600,
            "Invalid Request: expected JSON object",
        ));
    };

    let id = obj.get("id").cloned();
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return Some(jsonrpc_error(
            id.unwrap_or(Value::Null),
            -32600,
            "Invalid Request: missing method",
        ));
    };
    let params = obj.get("params").cloned().unwrap_or(Value::Null);

    if method == "notifications/initialized"
        || (method.starts_with("notifications/") && id.is_none())
    {
        return id.map(|req_id| jsonrpc_ok(req_id, json!({})));
    }

    let req_id = id.unwrap_or(Value::Null);
    match method {
        "initialize" => Some(jsonrpc_ok(req_id, initialize_result())),
        "ping" => Some(jsonrpc_ok(req_id, json!({}))),
        "tools/list" => Some(jsonrpc_ok(
            req_id,
            json!({
                "tools": tools::tools_list(),
            }),
        )),
        "tools/call" => {
            let Some(tool_name) = params.get("name").and_then(Value::as_str) else {
                return Some(jsonrpc_error(
                    req_id,
                    -32602,
                    "Invalid params: missing tool name",
                ));
            };
            if !tools::is_known_tool(tool_name) {
                return Some(jsonrpc_error(
                    req_id,
                    -32601,
                    format!("unknown tool '{tool_name}'"),
                ));
            }

            let mut arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));

            let grant_opt = if tool_name == "ready" {
                None
            } else {
                let mut resolved_cred = default_credential
                    .map(str::trim)
                    .filter(|val| !val.is_empty())
                    .map(str::to_string);
                if resolved_cred.is_none() && allow_param_credential {
                    resolved_cred = extract_param_credential(&params, &mut arguments);
                }
                match auth::authenticate_raw(state, resolved_cred.as_deref()) {
                    Ok(grant) => Some(grant),
                    Err(api_err) => {
                        return Some(jsonrpc_ok(req_id, tool_error_result(&api_err)));
                    }
                }
            };

            match tools::dispatch_tool(state, grant_opt.as_ref(), tool_name, arguments) {
                Ok(data) => Some(jsonrpc_ok(req_id, tool_success_result(data))),
                Err(api_err) => Some(jsonrpc_ok(req_id, tool_error_result(&api_err))),
            }
        }
        other => Some(jsonrpc_error(
            req_id,
            -32601,
            format!("Method not found: {other}"),
        )),
    }
}

/// Reusable JSON-RPC 2.0 MCP handler shared by `POST /mcp` and `streamline mcp-stdio`.
///
/// Returns `None` for notifications that expect no JSON-RPC response line.
pub fn handle_mcp_jsonrpc(
    state: &AppState,
    default_credential: Option<&str>,
    allow_param_credential: bool,
    payload: Value,
) -> Option<Value> {
    if let Value::Array(items) = payload {
        if items.is_empty() {
            return Some(jsonrpc_error(
                Value::Null,
                -32600,
                "Invalid Request: empty batch",
            ));
        }
        // Reject the entire batch before dispatching any item, including writes.
        if items.len() > MAX_MCP_BATCH_ITEMS {
            return Some(jsonrpc_error(
                Value::Null,
                -32600,
                format!("Invalid Request: batch exceeds {MAX_MCP_BATCH_ITEMS} items"),
            ));
        }
        let mut responses = Vec::new();
        for item in items {
            if let Some(resp) =
                handle_single_request(state, default_credential, allow_param_credential, item)
            {
                responses.push(resp);
            }
        }
        if responses.is_empty() {
            None
        } else {
            Some(Value::Array(responses))
        }
    } else {
        handle_single_request(state, default_credential, allow_param_credential, payload)
    }
}
