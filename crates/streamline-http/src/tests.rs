use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use serde_json::{json, Value};
use streamline_core::{HarnessSpec, PrincipalRole, StreamlineConfig};
use streamline_store::{CreateGrantRequest, StreamlineStore};
use tower::ServiceExt;

use crate::zip_skills::crc32_ieee;
use crate::{build_router, handle_mcp_jsonrpc, tools, AppState};

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn make_test_state(config: StreamlineConfig) -> AppState {
    let store = StreamlineStore::open_in_memory().expect("open in-memory store");
    let skills_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("skills");
    AppState::new(Arc::new(store), config, skills_dir)
}

fn seed_grant_with_family(
    store: &StreamlineStore,
    id: &str,
    role: PrincipalRole,
    raw_cred: &str,
    tags: Vec<String>,
    model_family: Option<&str>,
) -> String {
    store
        .upsert_principal(id, id, role)
        .expect("upsert principal");
    store
        .create_grant(CreateGrantRequest {
            grant_id: Some(format!("grant-{id}")),
            principal_id: id.to_string(),
            raw_credential: raw_cred.to_string(),
            granted_tags: tags,
            can_write: true,
            can_dispatch: true,
            expires_at: None,
            model_family: model_family.map(str::to_string),
        })
        .expect("create grant")
        .grant_id
}

fn seed_grant(
    store: &StreamlineStore,
    id: &str,
    role: PrincipalRole,
    raw_cred: &str,
    tags: Vec<String>,
) -> String {
    seed_grant_with_family(store, id, role, raw_cred, tags, None)
}

async fn read_json_body(resp: axum::response::Response) -> (StatusCode, Value) {
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let parsed = serde_json::from_slice::<Value>(&bytes).expect("parse JSON body");
    (status, parsed)
}

async fn mcp_call(
    router: &axum::Router,
    cred: Option<&str>,
    req_id: i64,
    tool_name: &str,
    arguments: Value,
) -> Value {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(raw) = cred {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {raw}"));
    }
    let body = json!({
        "jsonrpc": "2.0",
        "id": req_id,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments,
        }
    });
    let req = builder
        .body(Body::from(body.to_string()))
        .expect("build mcp request");
    let resp = router
        .clone()
        .oneshot(req)
        .await
        .expect("execute mcp request");
    let (status, payload) = read_json_body(resp).await;
    assert_eq!(status, StatusCode::OK);
    payload
}

fn parse_mcp_tool_text(mcp_resp: &Value) -> (bool, Value) {
    let result = &mcp_resp["result"];
    let is_error = result["isError"].as_bool().unwrap_or(false);
    let text = result["content"][0]["text"]
        .as_str()
        .expect("mcp content[0].text");
    let parsed: Value = serde_json::from_str(text).expect("parse mcp text JSON");
    (is_error, parsed)
}

fn valid_how_it_works_doc() -> &'static str {
    "\
## Overview and Purpose
This service stores project governance records and verifies Trio+ stages cleanly.

## Runtime Location
The service listens on loopback 127.0.0.1:5880 and persists SQLite data on disk.

## Step-by-Step Flow
Callers read bootstrap rules, acknowledge the rules digest, verify cleanup, and submit guides.

## Health and Logs
Check GET /ready for health status and inspect structured server logs for diagnostics.

## Failure Modes and Recovery
Unacknowledged writes return 412, locked OWN updates return 409, and backups restore SQLite.
"
}

#[tokio::test]
async fn ready_endpoint_returns_200_with_rules_digest() {
    let state = make_test_state(StreamlineConfig::default());
    let expected_sha = state.store.current_rules_sha256().expect("rules sha");
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/ready")
                .body(Body::empty())
                .expect("build ready request"),
        )
        .await
        .expect("call /ready");

    let (status, json) = read_json_body(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert_eq!(json["public_safe_mode"], true);
    assert_eq!(json["current_rules_sha256"], expected_sha);
}

#[tokio::test]
async fn mcp_initialize_and_tools_list_return_all_registered_tools() {
    let state = make_test_state(StreamlineConfig::default());
    let app = build_router(state);

    let init_req = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {}
            })
            .to_string(),
        ))
        .expect("init request");
    let (init_status, init_json) =
        read_json_body(app.clone().oneshot(init_req).await.unwrap()).await;
    assert_eq!(init_status, StatusCode::OK);
    assert_eq!(init_json["result"]["serverInfo"]["name"], "streamline");

    let notif_req = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            })
            .to_string(),
        ))
        .expect("notif request");
    let notif_resp = app.clone().oneshot(notif_req).await.unwrap();
    assert_eq!(notif_resp.status(), StatusCode::ACCEPTED);

    let list_req = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {}
            })
            .to_string(),
        ))
        .expect("tools/list request");
    let (list_status, list_json) =
        read_json_body(app.clone().oneshot(list_req).await.unwrap()).await;
    assert_eq!(list_status, StatusCode::OK);

    let registered = list_json["result"]["tools"]
        .as_array()
        .expect("tools array");
    assert_eq!(registered.len(), tools::KNOWN_TOOLS.len());
    for expected_name in tools::KNOWN_TOOLS {
        assert!(
            registered
                .iter()
                .any(|entry| entry["name"].as_str() == Some(expected_name)),
            "missing tool {expected_name} in tools/list"
        );
    }

    let ready_mcp = mcp_call(&app, None, 3, "ready", json!({})).await;
    let (is_err, ready_data) = parse_mcp_tool_text(&ready_mcp);
    assert!(!is_err);
    assert_eq!(ready_data["ok"], true);
    assert!(ready_data["data"]["current_rules_sha256"]
        .as_str()
        .is_some());
}

#[tokio::test]
async fn save_resource_requires_ack_rules_412_and_succeeds_after_ack() {
    let state = make_test_state(StreamlineConfig::default());
    let rules_sha = state.store.current_rules_sha256().expect("rules sha");
    let owner_cred = "cred-http-owner-01";
    seed_grant(
        &state.store,
        "owner-1",
        PrincipalRole::Owner,
        owner_cred,
        vec!["*".to_string()],
    );
    let app = build_router(state);

    let save_payload = json!({
        "shelf_slug": "standards",
        "slug": "style-guide",
        "title": "Style Guide",
        "body_md": "Write plain-English prose without em dashes.",
        "summary": "Initial style guide"
    });

    let blocked_req = Request::builder()
        .method(Method::POST)
        .uri("/api/tools/save_resource")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {owner_cred}"))
        .body(Body::from(save_payload.to_string()))
        .expect("blocked save request");
    let (blocked_status, blocked_json) =
        read_json_body(app.clone().oneshot(blocked_req).await.unwrap()).await;
    assert_eq!(blocked_status, StatusCode::PRECONDITION_FAILED);
    assert_eq!(blocked_json["ok"], false);
    assert_eq!(blocked_json["error"]["code"], "RULES_ACK_REQUIRED");
    assert_eq!(
        blocked_json["error"]["detail"]["current_rules_sha256"],
        rules_sha
    );

    let ack_req = Request::builder()
        .method(Method::POST)
        .uri("/api/tools/ack_rules")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Streamline-Credential", owner_cred)
        .body(Body::from(json!({ "rules_sha256": rules_sha }).to_string()))
        .expect("ack request");
    let (ack_status, ack_json) = read_json_body(app.clone().oneshot(ack_req).await.unwrap()).await;
    assert_eq!(ack_status, StatusCode::OK);
    assert_eq!(ack_json["ok"], true);

    let ok_req = Request::builder()
        .method(Method::POST)
        .uri("/api/tools/save_resource")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {owner_cred}"))
        .body(Body::from(save_payload.to_string()))
        .expect("save request after ack");
    let (ok_status, ok_json) = read_json_body(app.clone().oneshot(ok_req).await.unwrap()).await;
    assert_eq!(ok_status, StatusCode::OK);
    assert_eq!(ok_json["ok"], true);
    assert_eq!(ok_json["data"]["resource"]["slug"], "style-guide");
}

#[tokio::test]
async fn streamline_safe_blocks_credential_write_with_422_and_zero_persistence() {
    let state = make_test_state(StreamlineConfig::default());
    let rules_sha = state.store.current_rules_sha256().expect("rules sha");
    let owner_cred = "cred-safe-owner-02";
    let grant_id = seed_grant(
        &state.store,
        "owner-safe",
        PrincipalRole::Owner,
        owner_cred,
        vec!["*".to_string()],
    );
    state
        .store
        .ack_rules(&grant_id, &rules_sha)
        .expect("ack owner");
    let app = build_router(state.clone());

    let sensitive_sample = ["ghp_", "AbCdEf123456GhIj"].concat();
    let bad_body = format!("Do not persist this credential: {sensitive_sample}");

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/tools/save_resource")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Streamline-Credential", owner_cred)
        .body(Body::from(
            json!({
                "shelf_slug": "runbooks",
                "slug": "leaky-doc",
                "title": "Leaky Doc",
                "body_md": bad_body,
                "summary": "Attempted leak"
            })
            .to_string(),
        ))
        .expect("leaky save request");

    let (status, json_body) = read_json_body(app.oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json_body["ok"], false);
    assert_eq!(json_body["error"]["code"], "REDACTION_BLOCKED");

    let raw_serialized = json_body.to_string();
    assert!(
        !raw_serialized.contains(&sensitive_sample),
        "error response must never repeat sensitive credential substring"
    );

    let read_back = state
        .store
        .read_resource(&grant_id, "runbooks", "leaky-doc");
    assert!(read_back.is_err(), "blocked write must not persist to DB");
}

#[tokio::test]
async fn full_three_part_closeout_gate_over_mcp() {
    let state = make_test_state(StreamlineConfig::default());
    let rules_sha = state.store.current_rules_sha256().expect("rules sha");
    let owner_cred = "cred-closeout-owner";
    let worker_cred = "cred-closeout-worker";
    let acceptor_cred = "cred-closeout-acceptor";

    let owner_grant = seed_grant(
        &state.store,
        "owner-closeout",
        PrincipalRole::Owner,
        owner_cred,
        vec!["*".to_string()],
    );
    let worker_grant = seed_grant_with_family(
        &state.store,
        "worker-closeout",
        PrincipalRole::Agent,
        worker_cred,
        vec!["*".to_string()],
        Some("openai-codex"),
    );
    let acceptor_grant = seed_grant_with_family(
        &state.store,
        "acceptor-closeout",
        PrincipalRole::Agent,
        acceptor_cred,
        vec!["*".to_string()],
        Some("google-gemini"),
    );

    state.store.ack_rules(&owner_grant, &rules_sha).unwrap();
    state.store.ack_rules(&worker_grant, &rules_sha).unwrap();
    state.store.ack_rules(&acceptor_grant, &rules_sha).unwrap();

    let app = build_router(state);

    let create_resp = mcp_call(
        &app,
        Some(worker_cred),
        10,
        "create_trio_project",
        json!({
            "project_id": "closeout-demo",
            "request_id": "req-closeout-001",
            "title": "Closeout Gate Demo",
            "assessment": {
                "d1_scope": 2,
                "d2_interfaces": 1,
                "d3_uncertainty": 1,
                "d4_ui": 0,
                "d5_novelty": 1,
                "r1_exposure": 2,
                "r2_irreversibility": 1,
                "r3_blast_radius": 1,
                "r4_evidence": 1,
                "r5_custody": 1,
                "work_kind": "software"
            },
            "initial_own_decisions": [
                { "own_id": "OWN-001", "statement": "Keep loopback bind 127.0.0.1:5880" }
            ],
            "initial_requirements": [
                { "req_id": "REQ-01", "source_id": "OWN-001", "statement": "Enforce 3-part closeout gate" }
            ]
        }),
    )
    .await;
    let (create_err, create_data) = parse_mcp_tool_text(&create_resp);
    assert!(!create_err, "create failed: {create_data}");
    assert_eq!(create_data["data"]["project"]["profile"], "P1");

    let lock_resp = mcp_call(
        &app,
        Some(owner_cred),
        11,
        "lock_project_plan",
        json!({ "project_id": "closeout-demo" }),
    )
    .await;
    let (lock_err, _) = parse_mcp_tool_text(&lock_resp);
    assert!(!lock_err);

    // Locked OWN edit must fail with OWN_LOCKED.
    let own_edit_resp = mcp_call(
        &app,
        Some(worker_cred),
        12,
        "upsert_own_decision",
        json!({
            "project_id": "closeout-demo",
            "own_id": "OWN-001",
            "statement": "Change to 0.0.0.0"
        }),
    )
    .await;
    let (own_edit_err, own_edit_data) = parse_mcp_tool_text(&own_edit_resp);
    assert!(own_edit_err);
    assert_eq!(own_edit_data["error"]["code"], "OWN_LOCKED");

    // Advance REQ-01 through T (worker) and A (cross-family acceptor).
    let req_t = mcp_call(
        &app,
        Some(worker_cred),
        13,
        "update_requirement_stage",
        json!({
            "project_id": "closeout-demo",
            "req_id": "REQ-01",
            "target_stage": "T",
            "evidence_ref": "integration test passed",
            "actor_family": "openai-codex"
        }),
    )
    .await;
    assert!(!parse_mcp_tool_text(&req_t).0);

    let req_a = mcp_call(
        &app,
        Some(acceptor_cred),
        14,
        "update_requirement_stage",
        json!({
            "project_id": "closeout-demo",
            "req_id": "REQ-01",
            "target_stage": "A",
            "evidence_ref": "independent cross-family qc pass",
            "actor_family": "google-gemini"
        }),
    )
    .await;
    assert!(!parse_mcp_tool_text(&req_a).0);

    // Part 3 alone must be rejected before cleanup and lessons are recorded.
    let early_how = mcp_call(
        &app,
        Some(worker_cred),
        15,
        "submit_how_it_works",
        json!({
            "project_id": "closeout-demo",
            "markdown": valid_how_it_works_doc()
        }),
    )
    .await;
    let (early_err, _) = parse_mcp_tool_text(&early_how);
    assert!(
        early_err,
        "submit_how_it_works must fail before cleanup + lessons"
    );

    // Part 1: verify temp cleanup.
    let cleanup_resp = mcp_call(
        &app,
        Some(worker_cred),
        16,
        "mark_project_cleanup_verified",
        json!({
            "project_id": "closeout-demo",
            "detail": "removed scratch and target artifacts"
        }),
    )
    .await;
    assert!(!parse_mcp_tool_text(&cleanup_resp).0);

    // Still rejected until Part 2 (lessons) is satisfied.
    let mid_how = mcp_call(
        &app,
        Some(worker_cred),
        17,
        "submit_how_it_works",
        json!({
            "project_id": "closeout-demo",
            "markdown": valid_how_it_works_doc()
        }),
    )
    .await;
    let (mid_err, _) = parse_mcp_tool_text(&mid_how);
    assert!(
        mid_err,
        "submit_how_it_works must fail before central lesson"
    );

    // Part 2: record a central forward lesson.
    let lesson_resp = mcp_call(
        &app,
        Some(worker_cred),
        18,
        "record_lesson",
        json!({
            "project_id": "closeout-demo",
            "tags": ["rust", "closeout"],
            "observed_fact": "HTTP closeout handlers must enforce redaction before store mutations.",
            "cause_kind": "demonstrated",
            "cause_note": "Verified by integration test suite.",
            "forward_rule": "Validate every write field through streamline-safe prior to calling store."
        }),
    )
    .await;
    let (lesson_err, lesson_data) = parse_mcp_tool_text(&lesson_resp);
    assert!(!lesson_err, "record_lesson failed: {lesson_data}");

    // Part 3 now succeeds and transitions to DOC_PENDING_OWNER.
    let how_resp = mcp_call(
        &app,
        Some(worker_cred),
        19,
        "submit_how_it_works",
        json!({
            "project_id": "closeout-demo",
            "markdown": valid_how_it_works_doc()
        }),
    )
    .await;
    let (how_err, how_data) = parse_mcp_tool_text(&how_resp);
    assert!(!how_err, "submit_how_it_works failed: {how_data}");
    assert_eq!(how_data["data"]["state"], "DOC_PENDING_OWNER");

    // query_lessons returns the recorded central lesson.
    let query_resp = mcp_call(
        &app,
        Some(worker_cred),
        20,
        "query_lessons",
        json!({
            "tag_filter": "closeout",
            "limit": 10
        }),
    )
    .await;
    let (query_err, query_data) = parse_mcp_tool_text(&query_resp);
    assert!(!query_err);
    let lessons = query_data["data"].as_array().expect("lessons array");
    assert!(lessons.iter().any(|item| {
        item["forward_rule"]
            .as_str()
            .unwrap_or_default()
            .contains("streamline-safe")
    }));

    // Non-owner cannot approve release.
    let worker_approve = mcp_call(
        &app,
        Some(worker_cred),
        21,
        "approve_release",
        json!({ "project_id": "closeout-demo" }),
    )
    .await;
    assert!(parse_mcp_tool_text(&worker_approve).0);

    // Owner approves release -> DONE + stage L.
    let owner_approve = mcp_call(
        &app,
        Some(owner_cred),
        22,
        "approve_release",
        json!({ "project_id": "closeout-demo" }),
    )
    .await;
    let (approve_err, approve_data) = parse_mcp_tool_text(&owner_approve);
    assert!(!approve_err, "approve_release failed: {approve_data}");
    assert_eq!(approve_data["data"]["project"]["state"], "DONE");
    assert_eq!(approve_data["data"]["requirements"][0]["stage"], "L");
}

#[tokio::test]
async fn skills_zip_endpoint_serves_valid_pkzip_with_all_four_skills() {
    assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);

    let state = make_test_state(StreamlineConfig::default());
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/downloads/streamline-skills.zip")
                .body(Body::empty())
                .expect("build zip request"),
        )
        .await
        .expect("call zip endpoint");

    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert_eq!(content_type, "application/zip");

    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read zip bytes");
    assert!(bytes.starts_with(b"PK\x03\x04"), "missing PK local header");
    assert!(
        bytes.windows(4).any(|w| w == b"PK\x05\x06"),
        "missing PK EOCD record"
    );

    let as_lossy = String::from_utf8_lossy(&bytes);
    for entry in [
        "streamline-director/SKILL.md",
        "streamline-worker/SKILL.md",
        "streamline-acceptor/SKILL.md",
        "streamline-vault/SKILL.md",
    ] {
        assert!(as_lossy.contains(entry), "missing zip entry {entry}");
    }
}

#[tokio::test]
async fn ui_palette_tag_isolation_and_presence_plugin_boundary() {
    // 1. Default config without presence_plugin_dir.
    let state = make_test_state(StreamlineConfig::default());
    let scoped_cred = "cred-scoped-ui-test";
    seed_grant(
        &state.store,
        "scoped-agent",
        PrincipalRole::Agent,
        scoped_cred,
        vec!["standards".to_string()],
    );
    let app = build_router(state);

    let ui_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ui_resp.status(), StatusCode::OK);
    let html_bytes = to_bytes(ui_resp.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(html_bytes.to_vec()).expect("utf8 html");
    for required_fragment in [
        "#050817",
        "#101a35",
        "#3969f8",
        "#74d8ff",
        "#94ead1",
        "16px",
        "14px",
        "44px",
        "streamline-presence-slot",
    ] {
        assert!(
            html.contains(required_fragment),
            "UI missing required fragment {required_fragment}"
        );
    }
    assert!(!html.contains('\u{2014}'), "UI must not contain em dash");
    assert!(!html.contains('\u{2013}'), "UI must not contain en dash");
    assert!(
        !html.contains("/_streamline/presence-plugin/presence.js"),
        "presence plugin script must be absent when presence_plugin_dir is empty"
    );

    let missing_plugin = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/_streamline/presence-plugin/presence.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_plugin.status(), StatusCode::NOT_FOUND);

    // Scoped agent receives 403 FORBIDDEN on ungranted shelf.
    let forbidden_req = Request::builder()
        .method(Method::POST)
        .uri("/api/tools/read_resource")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Streamline-Credential", scoped_cred)
        .body(Body::from(
            json!({ "shelf_slug": "runbooks", "slug": "deploy" }).to_string(),
        ))
        .unwrap();
    let (forbidden_status, forbidden_json) =
        read_json_body(app.oneshot(forbidden_req).await.unwrap()).await;
    assert_eq!(forbidden_status, StatusCode::FORBIDDEN);
    assert_eq!(forbidden_json["error"]["code"], "FORBIDDEN");

    // 2. Configured presence_plugin_dir serves plugin JS cleanly.
    let plugin_dir = TempDirGuard::new("streamline-presence-plugin");
    fs::write(
        plugin_dir.path.join("presence.js"),
        "export function mountPresence() { return 'ok'; }\n",
    )
    .expect("write presence.js");
    let plugin_cfg = StreamlineConfig {
        presence_plugin_dir: plugin_dir.path.display().to_string(),
        ..StreamlineConfig::default()
    };
    let plugin_app = build_router(make_test_state(plugin_cfg));

    let plugin_ui = plugin_app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let plugin_html = String::from_utf8(
        to_bytes(plugin_ui.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(plugin_html.contains("/_streamline/presence-plugin/presence.js"));

    let plugin_asset = plugin_app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/_streamline/presence-plugin/presence.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plugin_asset.status(), StatusCode::OK);
}

#[test]
fn stdio_mcp_dispatcher_supports_param_credential_and_notifications() {
    let state = make_test_state(StreamlineConfig::default());
    let cred = "cred-stdio-test";
    seed_grant(
        &state.store,
        "stdio-user",
        PrincipalRole::Owner,
        cred,
        vec!["*".to_string()],
    );

    let notif = handle_mcp_jsonrpc(
        &state,
        None,
        true,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    );
    assert!(notif.is_none());

    let boot_resp = handle_mcp_jsonrpc(
        &state,
        None,
        true,
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "bootstrap",
                "credential": cred,
                "arguments": {}
            }
        }),
    )
    .expect("bootstrap response");
    let (is_err, parsed) = parse_mcp_tool_text(&boot_resp);
    assert!(!is_err);
    assert_eq!(parsed["ok"], true);
    assert!(parsed["data"]["rules_sha256"].as_str().is_some());
}

#[tokio::test]
async fn conveyor_dispatch_and_trajectory_export_over_mcp() {
    let temp = TempDirGuard::new("streamline-conveyor-mcp");
    let runs_dir = temp.path.join("runs");
    let vault_dir = temp.path.join("vault");
    let input_dir = temp.path.join("inputs");
    fs::create_dir_all(&runs_dir).expect("create runs dir");
    fs::create_dir_all(&vault_dir).expect("create vault dir");
    fs::create_dir_all(&input_dir).expect("create input dir");
    let input_file = input_dir.join("spec.md");
    fs::write(
        &input_file,
        "# Conveyor Spec\n\nLoopback bind 127.0.0.1:5880.\n",
    )
    .expect("write input spec");

    let config = StreamlineConfig {
        runs_dir: runs_dir.display().to_string(),
        vault_sync_dir: vault_dir.display().to_string(),
        allowed_input_roots: vec![input_dir.display().to_string()],
        harnesses: vec![
            HarnessSpec {
                id: "codex-sh".to_string(),
                family: "openai-codex".to_string(),
                command: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "printf 'builder-ok\\n'".to_string(),
                ],
                max_concurrency: 1,
            },
            HarnessSpec {
                id: "gemini-sh".to_string(),
                family: "google-gemini".to_string(),
                command: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "printf 'acceptor-ok\\n'".to_string(),
                ],
                max_concurrency: 1,
            },
        ],
        ..StreamlineConfig::default()
    };

    let state = make_test_state(config);
    let rules_sha = state.store.current_rules_sha256().expect("rules sha");

    let owner_cred = "cred-conveyor-owner";
    let worker_cred = "cred-conveyor-worker";
    let acceptor_cred = "cred-conveyor-acceptor";
    let no_dispatch_cred = "cred-conveyor-nodispatch";

    let owner_grant = seed_grant(
        &state.store,
        "owner-conveyor",
        PrincipalRole::Owner,
        owner_cred,
        vec!["*".to_string()],
    );
    let worker_grant = seed_grant_with_family(
        &state.store,
        "worker-conveyor",
        PrincipalRole::Agent,
        worker_cred,
        vec!["*".to_string()],
        Some("openai-codex"),
    );
    let acceptor_grant = seed_grant_with_family(
        &state.store,
        "acceptor-conveyor",
        PrincipalRole::Agent,
        acceptor_cred,
        vec!["*".to_string()],
        Some("google-gemini"),
    );

    state
        .store
        .upsert_principal("nodispatch-agent", "nodispatch-agent", PrincipalRole::Agent)
        .expect("upsert nodispatch principal");
    let no_dispatch_grant = state
        .store
        .create_grant(CreateGrantRequest {
            grant_id: Some("grant-nodispatch".to_string()),
            principal_id: "nodispatch-agent".to_string(),
            raw_credential: no_dispatch_cred.to_string(),
            granted_tags: vec!["*".to_string()],
            can_write: true,
            can_dispatch: false,
            expires_at: None,
            model_family: None,
        })
        .expect("create no_dispatch grant")
        .grant_id;

    state.store.ack_rules(&owner_grant, &rules_sha).unwrap();
    state.store.ack_rules(&worker_grant, &rules_sha).unwrap();
    state.store.ack_rules(&acceptor_grant, &rules_sha).unwrap();
    state
        .store
        .ack_rules(&no_dispatch_grant, &rules_sha)
        .unwrap();

    let app = build_router(state);

    let create_resp = mcp_call(
        &app,
        Some(worker_cred),
        101,
        "create_trio_project",
        json!({
            "project_id": "conveyor-demo",
            "request_id": "req-conveyor-001",
            "title": "Conveyor Dispatch Demo",
            "assessment": {
                "d1_scope": 2,
                "d2_interfaces": 1,
                "d3_uncertainty": 1,
                "d4_ui": 0,
                "d5_novelty": 1,
                "r1_exposure": 2,
                "r2_irreversibility": 1,
                "r3_blast_radius": 1,
                "r4_evidence": 1,
                "r5_custody": 1,
                "work_kind": "software"
            },
            "initial_own_decisions": [
                { "own_id": "OWN-001", "statement": "Bind loopback 127.0.0.1:5880" }
            ],
            "initial_requirements": [
                { "req_id": "REQ-01", "source_id": "OWN-001", "statement": "Run build and cross-family accept cleanly" }
            ]
        }),
    )
    .await;
    let (create_err, create_data) = parse_mcp_tool_text(&create_resp);
    assert!(!create_err, "create failed: {create_data}");
    assert_eq!(create_data["data"]["project"]["risk"], 2);

    // Grant with can_dispatch = false must receive 403 FORBIDDEN on dispatch_run.
    let forbidden_dispatch_req = Request::builder()
        .method(Method::POST)
        .uri("/api/tools/dispatch_run")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Streamline-Credential", no_dispatch_cred)
        .body(Body::from(
            json!({
                "project_id": "conveyor-demo",
                "input_paths": [input_file.display().to_string()]
            })
            .to_string(),
        ))
        .expect("build forbidden dispatch request");
    let (forbidden_status, forbidden_body) =
        read_json_body(app.clone().oneshot(forbidden_dispatch_req).await.unwrap()).await;
    assert_eq!(forbidden_status, StatusCode::FORBIDDEN);
    assert_eq!(forbidden_body["error"]["code"], "FORBIDDEN");

    // Dispatch risk = 2 build + auto cross-family accept via MCP dispatch_run.
    let dispatch_resp = mcp_call(
        &app,
        Some(worker_cred),
        102,
        "dispatch_run",
        json!({
            "project_id": "conveyor-demo",
            "input_paths": [input_file.display().to_string()],
            "preferred_builder_harness": "codex-sh"
        }),
    )
    .await;
    let (dispatch_err, dispatch_data) = parse_mcp_tool_text(&dispatch_resp);
    assert!(!dispatch_err, "dispatch_run failed: {dispatch_data}");

    let build_run = &dispatch_data["data"]["build_run"];
    let build_run_id = build_run["run_id"]
        .as_str()
        .expect("build_run.run_id")
        .to_string();
    assert_eq!(build_run["project_id"], "conveyor-demo");
    assert_eq!(build_run["phase"], "build");
    assert_eq!(build_run["family"], "openai-codex");
    assert_eq!(build_run["cleanup_clean"], true);
    assert_eq!(build_run["scan_clean"], true);

    let accept_run = &dispatch_data["data"]["accept_run"];
    assert!(
        accept_run.is_object(),
        "risk = 2 dispatch must auto-trigger cross-family accept_run"
    );
    let accept_run_id = accept_run["run_id"]
        .as_str()
        .expect("accept_run.run_id")
        .to_string();
    assert_eq!(accept_run["phase"], "accept");
    assert_eq!(accept_run["family"], "google-gemini");
    assert_eq!(accept_run["cleanup_clean"], true);
    assert_eq!(accept_run["scan_clean"], true);

    // Verify get_run_status for build and accept runs.
    let status_build_resp = mcp_call(
        &app,
        Some(worker_cred),
        103,
        "get_run_status",
        json!({ "run_id": build_run_id }),
    )
    .await;
    let (status_build_err, status_build_data) = parse_mcp_tool_text(&status_build_resp);
    assert!(
        !status_build_err,
        "get_run_status build failed: {status_build_data}"
    );
    assert_eq!(status_build_data["data"]["run_id"], build_run_id);
    assert_eq!(status_build_data["data"]["phase"], "build");

    let status_accept_resp = mcp_call(
        &app,
        Some(worker_cred),
        104,
        "get_run_status",
        json!({ "run_id": accept_run_id }),
    )
    .await;
    let (status_accept_err, status_accept_data) = parse_mcp_tool_text(&status_accept_resp);
    assert!(
        !status_accept_err,
        "get_run_status accept failed: {status_accept_data}"
    );
    assert_eq!(status_accept_data["data"]["run_id"], accept_run_id);
    assert_eq!(status_accept_data["data"]["phase"], "accept");

    // Verify list_conveyor_runs returns both persisted runs for the project.
    let list_runs_resp = mcp_call(
        &app,
        Some(worker_cred),
        105,
        "list_conveyor_runs",
        json!({ "project_id": "conveyor-demo" }),
    )
    .await;
    let (list_runs_err, list_runs_data) = parse_mcp_tool_text(&list_runs_resp);
    assert!(
        !list_runs_err,
        "list_conveyor_runs failed: {list_runs_data}"
    );
    let runs_arr = list_runs_data["data"].as_array().expect("runs array");
    assert_eq!(runs_arr.len(), 2);
    assert!(runs_arr.iter().any(|r| r["run_id"] == build_run_id));
    assert!(runs_arr.iter().any(|r| r["run_id"] == accept_run_id));

    // Advance REQ-01 through T and cross-family A, then complete 3-part closeout.
    let req_t = mcp_call(
        &app,
        Some(worker_cred),
        106,
        "update_requirement_stage",
        json!({
            "project_id": "conveyor-demo",
            "req_id": "REQ-01",
            "target_stage": "T",
            "evidence_ref": "conveyor build clean",
            "actor_family": "openai-codex"
        }),
    )
    .await;
    assert!(!parse_mcp_tool_text(&req_t).0);

    let req_a = mcp_call(
        &app,
        Some(acceptor_cred),
        107,
        "update_requirement_stage",
        json!({
            "project_id": "conveyor-demo",
            "req_id": "REQ-01",
            "target_stage": "A",
            "evidence_ref": "cross-family gemini accept clean",
            "actor_family": "google-gemini"
        }),
    )
    .await;
    assert!(!parse_mcp_tool_text(&req_a).0);

    let cleanup_resp = mcp_call(
        &app,
        Some(worker_cred),
        108,
        "mark_project_cleanup_verified",
        json!({
            "project_id": "conveyor-demo",
            "detail": "temporary staging artifacts verified clean"
        }),
    )
    .await;
    assert!(!parse_mcp_tool_text(&cleanup_resp).0);

    let lesson_resp = mcp_call(
        &app,
        Some(worker_cred),
        109,
        "record_lesson",
        json!({
            "project_id": "conveyor-demo",
            "tags": ["conveyor", "cross-family"],
            "observed_fact": "Cross-family R2 acceptance enforces distinct harness families cleanly.",
            "cause_kind": "demonstrated",
            "cause_note": "Verified over MCP dispatch_run.",
            "forward_rule": "Always pair R2+ builds with an independent acceptor family."
        }),
    )
    .await;
    assert!(!parse_mcp_tool_text(&lesson_resp).0);

    let how_resp = mcp_call(
        &app,
        Some(worker_cred),
        110,
        "submit_how_it_works",
        json!({
            "project_id": "conveyor-demo",
            "markdown": valid_how_it_works_doc()
        }),
    )
    .await;
    let (how_err, how_data) = parse_mcp_tool_text(&how_resp);
    assert!(!how_err, "submit_how_it_works failed: {how_data}");
    assert_eq!(how_data["data"]["state"], "DOC_PENDING_OWNER");

    let approve_resp = mcp_call(
        &app,
        Some(owner_cred),
        111,
        "approve_release",
        json!({ "project_id": "conveyor-demo" }),
    )
    .await;
    let (approve_err, approve_data) = parse_mcp_tool_text(&approve_resp);
    assert!(!approve_err, "approve_release failed: {approve_data}");
    assert_eq!(approve_data["data"]["project"]["state"], "DONE");
    assert_eq!(approve_data["data"]["requirements"][0]["stage"], "L");

    // Verify trajectories.jsonl was written cleanly on approve_release.
    let traj_path = runs_dir.join("trajectories.jsonl");
    assert!(
        traj_path.exists(),
        "approve_release must append trajectory to trajectories.jsonl"
    );
    let traj_text = fs::read_to_string(&traj_path).expect("read trajectories.jsonl");
    let lines: Vec<&str> = traj_text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(!lines.is_empty(), "trajectories.jsonl must contain a line");
    let first_entry: Value = serde_json::from_str(lines[0]).expect("parse trajectory JSONL entry");
    assert_eq!(first_entry["project_id"], "conveyor-demo");

    // Non-owner cannot call export_trajectory; owner can.
    let worker_export = mcp_call(
        &app,
        Some(worker_cred),
        112,
        "export_trajectory",
        json!({ "project_id": "conveyor-demo" }),
    )
    .await;
    assert!(parse_mcp_tool_text(&worker_export).0);

    let owner_export = mcp_call(
        &app,
        Some(owner_cred),
        113,
        "export_trajectory",
        json!({ "project_id": "conveyor-demo" }),
    )
    .await;
    let (owner_export_err, owner_export_data) = parse_mcp_tool_text(&owner_export);
    assert!(
        !owner_export_err,
        "export_trajectory failed: {owner_export_data}"
    );
    assert_eq!(owner_export_data["data"]["project_id"], "conveyor-demo");
    assert!(owner_export_data["data"]["trajectory_file"]
        .as_str()
        .unwrap_or_default()
        .ends_with("trajectories.jsonl"));
}

#[tokio::test]
async fn ui_renders_interactive_three_pane_tabs_and_triage_controls() {
    let state = make_test_state(StreamlineConfig::default());
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/?panel=passport")
                .body(Body::empty())
                .expect("build UI request"),
        )
        .await
        .expect("call UI handler");

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read UI HTML bytes");
    let html = String::from_utf8(bytes.to_vec()).expect("valid UTF-8 HTML");

    // Verify all 6 sidebar navigation tabs.
    for nav_tab in [
        "data-view=\"overview\"",
        "data-view=\"projects\"",
        "data-view=\"shelves\"",
        "data-view=\"inbox\"",
        "data-view=\"lessons\"",
        "data-view=\"apps\"",
    ] {
        assert!(
            html.contains(nav_tab),
            "UI missing navigation tab {nav_tab}"
        );
    }

    // Verify DOC-PENDING-OWNER approval inbox, Dormant/Legacy triage controls, and Lock Plan beside Compare.
    for expected in [
        "DOC-PENDING-OWNER",
        "Approval Inbox (DOC-PENDING-OWNER)",
        "Dormant",
        "Legacy",
        "Needs Triage",
        "Keep Dormant",
        "Promote to Active",
        "Archive Retired",
        "/api/tools/set_lifecycle_tier",
        "Lock Plan Version",
        "id=\"lock-plan-btn\"",
        "id=\"compare-plan-btn\"",
        "Compare",
        "History &amp; Restore",
        "Source Passport",
        "optional-modules-details",
    ] {
        assert!(
            html.contains(expected),
            "UI missing interactive fragment {expected}"
        );
    }

    // Verify budget optional module is unchecked by default.
    let budget_line = html
        .lines()
        .find(|line| line.contains("value=\"budget\""))
        .expect("budget checkbox line");
    assert!(
        !budget_line.contains("checked"),
        "budget module checkbox must not be checked by default"
    );

    // Verify zero external CDN script/link leaks and zero em/en dashes.
    assert!(
        !html.contains("http://"),
        "UI must not reference http:// URLs"
    );
    assert!(
        !html.contains("https://"),
        "UI must not reference https:// CDN URLs"
    );
    assert!(!html.contains('\u{2014}'), "UI must not contain em dash");
    assert!(!html.contains('\u{2013}'), "UI must not contain en dash");
}

#[tokio::test]
async fn http_and_mcp_security_regressions_enforced() {
    let state = make_test_state(StreamlineConfig::default());
    let rules_sha = state.store.current_rules_sha256().expect("rules sha");

    let owner_cred = "cred-sec-owner";
    let demo_cred = "cred-sec-demo";
    let codex_worker_cred = "cred-sec-codex-w";
    let codex_spoof_cred = "cred-sec-codex-s";
    let gemini_acceptor_cred = "cred-sec-gemini-a";

    let owner_grant = seed_grant(
        &state.store,
        "sec-owner",
        PrincipalRole::Owner,
        owner_cred,
        vec!["*".to_string()],
    );
    let demo_grant = seed_grant(
        &state.store,
        "sec-demo",
        PrincipalRole::Agent,
        demo_cred,
        vec!["project:demo".to_string()],
    );
    let codex_worker_grant = seed_grant_with_family(
        &state.store,
        "sec-codex-worker",
        PrincipalRole::Agent,
        codex_worker_cred,
        vec!["*".to_string()],
        Some("openai-codex"),
    );
    let codex_spoof_grant = seed_grant_with_family(
        &state.store,
        "sec-codex-spoof",
        PrincipalRole::Agent,
        codex_spoof_cred,
        vec!["*".to_string()],
        Some("openai-codex"),
    );
    let gemini_acceptor_grant = seed_grant_with_family(
        &state.store,
        "sec-gemini-acceptor",
        PrincipalRole::Agent,
        gemini_acceptor_cred,
        vec!["*".to_string()],
        Some("google-gemini"),
    );

    for gid in [
        &owner_grant,
        &demo_grant,
        &codex_worker_grant,
        &codex_spoof_grant,
        &gemini_acceptor_grant,
    ] {
        state.store.ack_rules(gid, &rules_sha).unwrap();
    }

    let app = build_router(state);

    // Seed a project:other resource via REST.
    let seed_other_req = Request::builder()
        .method(Method::POST)
        .uri("/api/tools/save_resource")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {owner_cred}"))
        .body(Body::from(
            json!({
                "shelf_slug": "projects",
                "slug": "other-bounded",
                "title": "Other Project Doc",
                "body_md": "Original other content.",
                "project_id": "other",
                "summary": "Seed other doc"
            })
            .to_string(),
        ))
        .unwrap();
    let (seed_status, _) = read_json_body(app.clone().oneshot(seed_other_req).await.unwrap()).await;
    assert_eq!(seed_status, StatusCode::OK);

    // 1. project:demo agent overwriting project:other resource must receive 403 FORBIDDEN.
    let hijack_req = Request::builder()
        .method(Method::POST)
        .uri("/api/tools/save_resource")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {demo_cred}"))
        .body(Body::from(
            json!({
                "shelf_slug": "projects",
                "slug": "other-bounded",
                "title": "Hijacked Title",
                "body_md": "Hijacked body.",
                "project_id": "demo",
                "summary": "Cross-project overwrite"
            })
            .to_string(),
        ))
        .unwrap();
    let (hijack_status, hijack_body) =
        read_json_body(app.clone().oneshot(hijack_req).await.unwrap()).await;
    assert_eq!(hijack_status, StatusCode::FORBIDDEN);
    assert_eq!(hijack_body["error"]["code"], "FORBIDDEN");

    // Create R2+ project over MCP.
    let create_resp = mcp_call(
        &app,
        Some(owner_cred),
        201,
        "create_trio_project",
        json!({
            "project_id": "sec-proj",
            "request_id": "req-sec-001",
            "title": "Security Regression Demo",
            "assessment": {
                "d1_scope": 2,
                "d2_interfaces": 1,
                "d3_uncertainty": 1,
                "d4_ui": 0,
                "d5_novelty": 1,
                "r1_exposure": 2,
                "r2_irreversibility": 1,
                "r3_blast_radius": 1,
                "r4_evidence": 1,
                "r5_custody": 1,
                "work_kind": "software"
            },
            "initial_own_decisions": [
                { "own_id": "OWN-001", "statement": "Use loopback 127.0.0.1" }
            ],
            "initial_requirements": [
                { "req_id": "REQ-01", "source_id": "OWN-001", "statement": "Bind loopback only" }
            ]
        }),
    )
    .await;
    assert!(!parse_mcp_tool_text(&create_resp).0);

    // 2. lock_project_plan with nonexistent revision 999 must be rejected without locking OWN.
    let bad_lock = mcp_call(
        &app,
        Some(owner_cred),
        202,
        "lock_project_plan",
        json!({ "project_id": "sec-proj", "plan_rev": 999 }),
    )
    .await;
    assert!(parse_mcp_tool_text(&bad_lock).0);

    let edit_own_after_bad_lock = mcp_call(
        &app,
        Some(owner_cred),
        203,
        "upsert_own_decision",
        json!({
            "project_id": "sec-proj",
            "own_id": "OWN-001",
            "statement": "Use loopback 127.0.0.1:5880"
        }),
    )
    .await;
    assert!(!parse_mcp_tool_text(&edit_own_after_bad_lock).0);

    let good_lock = mcp_call(
        &app,
        Some(owner_cred),
        204,
        "lock_project_plan",
        json!({ "project_id": "sec-proj" }),
    )
    .await;
    assert!(!parse_mcp_tool_text(&good_lock).0);

    // 3. Advance REQ-01 to T (openai-codex), then reject spoofed actor_family on A.
    let req_t = mcp_call(
        &app,
        Some(codex_worker_cred),
        205,
        "update_requirement_stage",
        json!({
            "project_id": "sec-proj",
            "req_id": "REQ-01",
            "target_stage": "T",
            "evidence_ref": "tests passed"
        }),
    )
    .await;
    let (req_t_err, req_t_data) = parse_mcp_tool_text(&req_t);
    assert!(!req_t_err, "req T failed: {req_t_data}");
    assert_eq!(req_t_data["data"]["builder_family"], "openai-codex");

    let spoof_a = mcp_call(
        &app,
        Some(codex_spoof_cred),
        206,
        "update_requirement_stage",
        json!({
            "project_id": "sec-proj",
            "req_id": "REQ-01",
            "target_stage": "A",
            "evidence_ref": "spoofed qc",
            "actor_family": "google-gemini"
        }),
    )
    .await;
    assert!(parse_mcp_tool_text(&spoof_a).0);

    let real_a = mcp_call(
        &app,
        Some(gemini_acceptor_cred),
        207,
        "update_requirement_stage",
        json!({
            "project_id": "sec-proj",
            "req_id": "REQ-01",
            "target_stage": "A",
            "evidence_ref": "cross-family verified"
        }),
    )
    .await;
    let (real_a_err, real_a_data) = parse_mcp_tool_text(&real_a);
    assert!(!real_a_err, "real A failed: {real_a_data}");
    assert_eq!(real_a_data["data"]["stage"], "A");
    assert_eq!(real_a_data["data"]["acceptor_family"], "google-gemini");

    // 4. save_sync_report computes Fail before as_built documents REQ-01, ignoring forged pass.
    let forged_sync = mcp_call(
        &app,
        Some(gemini_acceptor_cred),
        208,
        "save_sync_report",
        json!({
            "project_id": "sec-proj",
            "plan_rev": 1,
            "asbuilt_rev": 1,
            "verdict": "pass",
            "acceptor_family": "google-gemini",
            "paired_rows": [
                { "req_id": "REQ-01", "planned": "x", "observed": "y", "stage": "A", "matched": true }
            ]
        }),
    )
    .await;
    let (forged_sync_err, forged_sync_data) = parse_mcp_tool_text(&forged_sync);
    assert!(
        !forged_sync_err,
        "save_sync_report failed: {forged_sync_data}"
    );
    assert_eq!(forged_sync_data["data"]["verdict"], "fail");
    assert_eq!(forged_sync_data["data"]["paired_rows"][0]["matched"], false);

    // Document REQ-01 in as_built, then save_sync_report computes real Pass.
    let save_asbuilt = mcp_call(
        &app,
        Some(codex_worker_cred),
        209,
        "save_resource",
        json!({
            "shelf_slug": "projects",
            "slug": "sec-proj-as_built",
            "title": "Security Regression Demo: as_built",
            "body_md": "# As Built\n\n| REQ-01 | A | Bound 127.0.0.1:5880 | test:ok |\n",
            "project_id": "sec-proj",
            "trio_role": "as_built",
            "summary": "Document REQ-01",
            "expected_rev": 1
        }),
    )
    .await;
    assert!(!parse_mcp_tool_text(&save_asbuilt).0);

    let real_sync = mcp_call(
        &app,
        Some(gemini_acceptor_cred),
        210,
        "save_sync_report",
        json!({ "project_id": "sec-proj" }),
    )
    .await;
    let (real_sync_err, real_sync_data) = parse_mcp_tool_text(&real_sync);
    assert!(
        !real_sync_err,
        "real save_sync_report failed: {real_sync_data}"
    );
    assert_eq!(real_sync_data["data"]["verdict"], "pass");
    assert_eq!(real_sync_data["data"]["asbuilt_rev"], 2);
    assert_eq!(real_sync_data["data"]["paired_rows"][0]["matched"], true);

    // 5. Updating requirement statement via upsert_requirement_row resets stage A -> D.
    let reset_req = mcp_call(
        &app,
        Some(owner_cred),
        211,
        "upsert_requirement_row",
        json!({
            "project_id": "sec-proj",
            "req_id": "REQ-01",
            "source_id": "OWN-001",
            "statement": "Bind loopback 127.0.0.1:5880 with TLS"
        }),
    )
    .await;
    let (reset_err, reset_data) = parse_mcp_tool_text(&reset_req);
    assert!(!reset_err, "upsert_requirement_row failed: {reset_data}");
    assert_eq!(reset_data["data"]["stage"], "D");
    assert!(reset_data["data"]["acceptor_family"].is_null());
    assert!(reset_data["data"]["builder_family"].is_null());
}

#[cfg(unix)]
#[tokio::test]
async fn skills_zip_and_presence_plugin_reject_symlinks_and_body_cap_enforced() {
    let temp = TempDirGuard::new("streamline-http-symlinks");
    let outside_file = temp.path.join("outside.txt");
    fs::write(&outside_file, "OUTSIDE_SENTINEL_CONTENT\n").expect("write outside");

    // 1. Symlinked skill file must be ignored and replaced by embedded fallback.
    let skills_root = temp.path.join("skills");
    let dir_skill = skills_root.join("streamline-director");
    fs::create_dir_all(&dir_skill).expect("create director skill dir");
    std::os::unix::fs::symlink(&outside_file, dir_skill.join("SKILL.md"))
        .expect("plant skill symlink");

    let zip_bytes = crate::zip_skills::build_skills_zip(&skills_root);
    let zip_text = String::from_utf8_lossy(&zip_bytes);
    assert!(
        !zip_text.contains("OUTSIDE_SENTINEL_CONTENT"),
        "build_skills_zip must never read outside symlink target"
    );
    assert!(zip_text.contains("streamline-director/SKILL.md"));

    // 2. Symlinked presence_plugin_dir must not be served and oversized body must hit 413.
    let real_plugin_target = temp.path.join("real-plugin");
    fs::create_dir_all(&real_plugin_target).expect("create real plugin target");
    fs::write(
        real_plugin_target.join("presence.js"),
        "export function mountPresence() { return 'escape'; }\n",
    )
    .expect("write plugin js");
    let symlink_plugin = temp.path.join("plugin-link");
    std::os::unix::fs::symlink(&real_plugin_target, &symlink_plugin)
        .expect("plant plugin dir symlink");

    let store = StreamlineStore::open_in_memory().expect("open store");
    let config = StreamlineConfig {
        presence_plugin_dir: symlink_plugin.display().to_string(),
        ..StreamlineConfig::default()
    };
    let app = build_router(AppState::new(Arc::new(store), config, skills_root.clone()));

    let plugin_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/_streamline/presence-plugin/presence.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plugin_resp.status(), StatusCode::NOT_FOUND);

    // Nested symlink inside a real presence_plugin_dir must also return 404 NOT_FOUND.
    std::os::unix::fs::symlink(&outside_file, real_plugin_target.join("escape.js"))
        .expect("plant nested plugin symlink");
    let real_plugin_app = build_router(AppState::new(
        Arc::new(StreamlineStore::open_in_memory().expect("open store 2")),
        StreamlineConfig {
            presence_plugin_dir: real_plugin_target.display().to_string(),
            ..StreamlineConfig::default()
        },
        skills_root.clone(),
    ));
    let nested_symlink_resp = real_plugin_app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/_streamline/presence-plugin/escape.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(nested_symlink_resp.status(), StatusCode::NOT_FOUND);

    let zip_http = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/downloads/streamline-skills.zip")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(zip_http.status(), StatusCode::OK);
    let http_zip_bytes = to_bytes(zip_http.into_body(), usize::MAX).await.unwrap();
    assert!(!String::from_utf8_lossy(&http_zip_bytes).contains("OUTSIDE_SENTINEL_CONTENT"));

    let oversized = vec![b'a'; 2 * 1024 * 1024 + 4096];
    let limit_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(oversized))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(limit_resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
