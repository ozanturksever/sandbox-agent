// Terminal HTTP endpoint tests.
include!("../common/http.rs");

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_create_list_delete() {
    let app = TestApp::new();

    // List terminals — should be empty initially
    let (status, payload) = send_json(&app.app, Method::GET, "/v1/terminal", None).await;
    assert_eq!(status, StatusCode::OK, "list terminals");
    let terminals = payload.as_array().expect("terminals should be array");
    assert!(terminals.is_empty(), "should start with no terminals");

    // Create a terminal (default: /bin/bash or similar shell)
    let (status, payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/terminal",
        Some(json!({
            "command": "/bin/sh",
            "cols": 80,
            "rows": 24
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create terminal");
    let terminal_id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .expect("terminal should have id");
    assert_eq!(
        payload.get("command").and_then(|v| v.as_str()),
        Some("/bin/sh")
    );
    assert_eq!(payload.get("cols").and_then(|v| v.as_u64()), Some(80));
    assert_eq!(payload.get("rows").and_then(|v| v.as_u64()), Some(24));
    assert_eq!(payload.get("alive").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload.get("connected").and_then(|v| v.as_bool()),
        Some(false)
    );

    // List terminals — should have one
    let (status, payload) = send_json(&app.app, Method::GET, "/v1/terminal", None).await;
    assert_eq!(status, StatusCode::OK, "list terminals after create");
    let terminals = payload.as_array().expect("terminals array");
    assert_eq!(terminals.len(), 1, "should have one terminal");
    assert_eq!(
        terminals[0].get("id").and_then(|v| v.as_str()),
        Some(terminal_id)
    );

    // Get single terminal
    let (status, payload) = send_json(
        &app.app,
        Method::GET,
        &format!("/v1/terminal/{terminal_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get terminal");
    assert_eq!(
        payload.get("id").and_then(|v| v.as_str()),
        Some(terminal_id)
    );

    // Delete terminal
    let (status, _payload) = send_json(
        &app.app,
        Method::DELETE,
        &format!("/v1/terminal/{terminal_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "delete terminal");

    // List terminals — should be empty again
    let (status, payload) = send_json(&app.app, Method::GET, "/v1/terminal", None).await;
    assert_eq!(status, StatusCode::OK, "list terminals after delete");
    let terminals = payload.as_array().expect("terminals array");
    assert!(terminals.is_empty(), "should have no terminals after delete");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_get_not_found() {
    let app = TestApp::new();

    let (status, _payload) = send_json(
        &app.app,
        Method::GET,
        "/v1/terminal/nonexistent",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "get nonexistent terminal");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_delete_not_found() {
    let app = TestApp::new();

    let (status, _payload) = send_json(
        &app.app,
        Method::DELETE,
        "/v1/terminal/nonexistent",
        None,
    )
    .await;
    // Should return an error (the kill method returns InvalidRequest)
    assert_ne!(status, StatusCode::NO_CONTENT, "delete nonexistent should fail");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_resize() {
    let app = TestApp::new();

    // Create a terminal
    let (status, payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/terminal",
        Some(json!({
            "command": "/bin/sh",
            "cols": 80,
            "rows": 24
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create terminal for resize");
    let terminal_id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .expect("terminal id");

    // Resize the terminal
    let (status, _payload) = send_json(
        &app.app,
        Method::POST,
        &format!("/v1/terminal/{terminal_id}/resize"),
        Some(json!({ "cols": 120, "rows": 40 })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "resize terminal");

    // Verify resize by getting terminal info
    let (status, payload) = send_json(
        &app.app,
        Method::GET,
        &format!("/v1/terminal/{terminal_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get terminal after resize");
    assert_eq!(payload.get("cols").and_then(|v| v.as_u64()), Some(120));
    assert_eq!(payload.get("rows").and_then(|v| v.as_u64()), Some(40));

    // Cleanup
    let _ = send_json(
        &app.app,
        Method::DELETE,
        &format!("/v1/terminal/{terminal_id}"),
        None,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_resize_not_found() {
    let app = TestApp::new();

    let (status, _payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/terminal/nonexistent/resize",
        Some(json!({ "cols": 120, "rows": 40 })),
    )
    .await;
    assert_ne!(
        status,
        StatusCode::NO_CONTENT,
        "resize nonexistent should fail"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_create_with_defaults() {
    let app = TestApp::new();

    // Create with minimal/empty body — should use defaults
    let (status, payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/terminal",
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create terminal with defaults");
    assert_eq!(
        payload.get("command").and_then(|v| v.as_str()),
        Some("/bin/bash")
    );
    assert_eq!(payload.get("cols").and_then(|v| v.as_u64()), Some(80));
    assert_eq!(payload.get("rows").and_then(|v| v.as_u64()), Some(24));
    assert!(payload.get("id").and_then(|v| v.as_str()).is_some());

    let terminal_id = payload.get("id").and_then(|v| v.as_str()).unwrap();

    // Cleanup
    let _ = send_json(
        &app.app,
        Method::DELETE,
        &format!("/v1/terminal/{terminal_id}"),
        None,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_create_with_custom_command() {
    let app = TestApp::new();

    // Create a terminal with a custom command and args
    let (status, payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/terminal",
        Some(json!({
            "command": "/bin/sh",
            "args": ["-c", "echo hello"],
            "cols": 100,
            "rows": 30
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create terminal with custom command");
    assert_eq!(
        payload.get("command").and_then(|v| v.as_str()),
        Some("/bin/sh")
    );
    let args = payload
        .get("args")
        .and_then(|v| v.as_array())
        .expect("args array");
    assert_eq!(args.len(), 2);
    assert_eq!(args[0].as_str(), Some("-c"));
    assert_eq!(args[1].as_str(), Some("echo hello"));
    assert_eq!(payload.get("cols").and_then(|v| v.as_u64()), Some(100));
    assert_eq!(payload.get("rows").and_then(|v| v.as_u64()), Some(30));

    let terminal_id = payload.get("id").and_then(|v| v.as_str()).unwrap();

    // Cleanup
    let _ = send_json(
        &app.app,
        Method::DELETE,
        &format!("/v1/terminal/{terminal_id}"),
        None,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_multiple_sessions() {
    let app = TestApp::new();

    // Create multiple terminals
    let mut ids = Vec::new();
    for _ in 0..3 {
        let (status, payload) = send_json(
            &app.app,
            Method::POST,
            "/v1/terminal",
            Some(json!({ "command": "/bin/sh" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "create terminal");
        let id = payload
            .get("id")
            .and_then(|v| v.as_str())
            .expect("terminal id")
            .to_string();
        ids.push(id);
    }

    // List should show all 3
    let (status, payload) = send_json(&app.app, Method::GET, "/v1/terminal", None).await;
    assert_eq!(status, StatusCode::OK, "list terminals");
    let terminals = payload.as_array().expect("terminals array");
    assert_eq!(terminals.len(), 3, "should have 3 terminals");

    // Each id should be unique
    let mut unique_ids: Vec<String> = terminals
        .iter()
        .filter_map(|t| t.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();
    unique_ids.sort();
    unique_ids.dedup();
    assert_eq!(unique_ids.len(), 3, "all terminal ids should be unique");

    // Cleanup
    for id in &ids {
        let _ = send_json(
            &app.app,
            Method::DELETE,
            &format!("/v1/terminal/{id}"),
            None,
        )
        .await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_process_exit_detection() {
    let app = TestApp::new();

    // Create a terminal that exits immediately
    let (status, payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/terminal",
        Some(json!({
            "command": "/bin/sh",
            "args": ["-c", "exit 0"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create short-lived terminal");
    let terminal_id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .expect("terminal id");

    // Wait a moment for the process to exit
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Check that alive reports false
    let (status, payload) = send_json(
        &app.app,
        Method::GET,
        &format!("/v1/terminal/{terminal_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get terminal after exit");
    assert_eq!(
        payload.get("alive").and_then(|v| v.as_bool()),
        Some(false),
        "terminal should report not alive after process exits"
    );

    // Cleanup
    let _ = send_json(
        &app.app,
        Method::DELETE,
        &format!("/v1/terminal/{terminal_id}"),
        None,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_auth_required() {
    let token = "test-terminal-token";
    let app = TestApp::new_with_auth(AuthConfig::with_token(token.to_string()));

    // Without token — should fail
    let (status, _payload) = send_json(&app.app, Method::GET, "/v1/terminal", None).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "listing terminals without auth should fail"
    );

    let (status, _payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/terminal",
        Some(json!({ "command": "/bin/sh" })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "creating terminal without auth should fail"
    );

    // With token — should work
    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/terminal")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("auth list request");
    let (status, _headers, _payload) = send_json_request(&app.app, request).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "listing terminals with auth should succeed"
    );

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/terminal")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "command": "/bin/sh" }).to_string()))
        .expect("auth create request");
    let (status, _headers, payload) = send_json_request(&app.app, request).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "creating terminal with auth should succeed"
    );
    let terminal_id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .expect("terminal id");

    // Delete with token
    let request = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/v1/terminal/{terminal_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("auth delete request");
    let (status, _headers, _payload) = send_json_request(&app.app, request).await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "deleting terminal with auth should succeed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_uptime_increases() {
    let app = TestApp::new();

    let (status, payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/terminal",
        Some(json!({ "command": "/bin/sh" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create terminal");
    let terminal_id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .expect("terminal id");
    let uptime1 = payload.get("uptime_secs").and_then(|v| v.as_u64()).unwrap_or(0);

    // Wait a bit
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let (status, payload) = send_json(
        &app.app,
        Method::GET,
        &format!("/v1/terminal/{terminal_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get terminal for uptime");
    let uptime2 = payload.get("uptime_secs").and_then(|v| v.as_u64()).unwrap_or(0);
    assert!(
        uptime2 >= uptime1 + 1,
        "uptime should increase: {uptime1} -> {uptime2}"
    );

    // Cleanup
    let _ = send_json(
        &app.app,
        Method::DELETE,
        &format!("/v1/terminal/{terminal_id}"),
        None,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_env_and_cwd() {
    let app = TestApp::new();

    let temp = tempfile::tempdir().expect("create temp dir");
    let temp_path = temp.path().to_string_lossy().to_string();

    let (status, payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/terminal",
        Some(json!({
            "command": "/bin/sh",
            "cwd": temp_path,
            "env": { "MY_TEST_VAR": "hello123" }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create terminal with cwd and env");
    assert_eq!(
        payload.get("cwd").and_then(|v| v.as_str()),
        Some(temp_path.as_str()),
        "cwd should match requested cwd"
    );

    let terminal_id = payload.get("id").and_then(|v| v.as_str()).unwrap();

    // Cleanup
    let _ = send_json(
        &app.app,
        Method::DELETE,
        &format!("/v1/terminal/{terminal_id}"),
        None,
    )
    .await;
}
