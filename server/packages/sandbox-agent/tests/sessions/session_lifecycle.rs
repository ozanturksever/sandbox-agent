// Session lifecycle and streaming snapshots use the mock baseline as the single source of truth.
include!("../common/http.rs");

fn session_snapshot_suffix(prefix: &str) -> String {
    snapshot_name(prefix, Some(AgentId::Mock))
}

fn assert_session_snapshot(prefix: &str, value: Value) {
    insta::with_settings!({
        snapshot_suffix => session_snapshot_suffix(prefix),
    }, {
        insta::assert_yaml_snapshot!(value);
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_endpoints_snapshots() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");

    for config in &configs {
        let app = TestApp::new();
        let capabilities = fetch_capabilities(&app.app).await;
        let caps = capabilities
            .get(config.agent.as_str())
            .expect("capabilities missing");
        if !caps.session_lifecycle {
            continue;
        }

        let _guard = apply_credentials(&config.credentials);
        install_agent(&app.app, config.agent).await;

        let session_id = format!("snapshot-{}", config.agent.as_str());
        let permission_mode = test_permission_mode(config.agent);
        let (status, created) = send_json(
            &app.app,
            Method::POST,
            &format!("/v1/sessions/{session_id}"),
            Some(json!({
                "agent": config.agent.as_str(),
                "permissionMode": permission_mode
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "create session");
        assert_session_snapshot("create_session", normalize_create_session(&created));

        let (status, sessions) = send_json(&app.app, Method::GET, "/v1/sessions", None).await;
        assert_eq!(status, StatusCode::OK, "list sessions");
        assert_session_snapshot("sessions_list", normalize_sessions(&sessions));

        let status = send_status(
            &app.app,
            Method::POST,
            &format!("/v1/sessions/{session_id}/messages"),
            Some(json!({ "message": PROMPT })),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "send message");
        assert_session_snapshot("send_message", snapshot_status(status));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_events_snapshots() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");

    for config in &configs {
        // OpenCode's embedded bun hangs when installing plugins, blocking event streaming.
        if config.agent == AgentId::Opencode {
            continue;
        }
        let app = TestApp::new();
        let capabilities = fetch_capabilities(&app.app).await;
        let caps = capabilities
            .get(config.agent.as_str())
            .expect("capabilities missing");
        if !caps.session_lifecycle {
            continue;
        }
        run_http_events_snapshot(&app.app, config).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accept_edits_noop_for_non_claude() {
    let app = TestApp::new();
    let session_id = "accept-edits-noop";

    let (status, _) = send_json(
        &app.app,
        Method::POST,
        &format!("/v1/sessions/{session_id}"),
        Some(json!({
            "agent": AgentId::Mock.as_str(),
            "permissionMode": "acceptEdits"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create session with acceptEdits");

    let (status, sessions) = send_json(&app.app, Method::GET, "/v1/sessions", None).await;
    assert_eq!(status, StatusCode::OK, "list sessions");

    let sessions = sessions
        .get("sessions")
        .and_then(Value::as_array)
        .expect("sessions list");
    let session = sessions
        .iter()
        .find(|entry| {
            entry
                .get("sessionId")
                .and_then(Value::as_str)
                .is_some_and(|id| id == session_id)
        })
        .expect("created session");
    let permission_mode = session
        .get("permissionMode")
        .and_then(Value::as_str)
        .expect("permissionMode");
    assert_eq!(permission_mode, "default");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sse_events_snapshots() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");

    for config in &configs {
        // OpenCode's embedded bun hangs when installing plugins, blocking SSE event streaming.
        if config.agent == AgentId::Opencode {
            continue;
        }
        let app = TestApp::new();
        let capabilities = fetch_capabilities(&app.app).await;
        let caps = capabilities
            .get(config.agent.as_str())
            .expect("capabilities missing");
        if !caps.session_lifecycle {
            continue;
        }
        run_sse_events_snapshot(&app.app, config).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrency_snapshots() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");

    for config in &configs {
        let app = TestApp::new();
        let capabilities = fetch_capabilities(&app.app).await;
        let caps = capabilities
            .get(config.agent.as_str())
            .expect("capabilities missing");
        if !caps.session_lifecycle {
            continue;
        }
        run_concurrency_snapshot(&app.app, config).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn turn_stream_route() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");

    for config in &configs {
        // OpenCode's embedded bun can hang while installing plugins, which blocks turn streaming.
        // OpenCode turn behavior is covered by the dedicated opencode-compat suite.
        if config.agent == AgentId::Opencode {
            continue;
        }
        let app = TestApp::new();
        let capabilities = fetch_capabilities(&app.app).await;
        let caps = capabilities
            .get(config.agent.as_str())
            .expect("capabilities missing");
        if !caps.session_lifecycle {
            continue;
        }
        run_turn_stream_check(&app.app, config).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn turn_stream_emits_turn_lifecycle_for_mock() {
    let app = TestApp::new();
    install_agent(&app.app, AgentId::Mock).await;

    let session_id = "turn-lifecycle-mock";
    create_session(
        &app.app,
        AgentId::Mock,
        session_id,
        test_permission_mode(AgentId::Mock),
    )
    .await;

    let events = read_turn_stream_events(&app.app, session_id, Duration::from_secs(30)).await;
    let started_count = events
        .iter()
        .filter(|event| event.get("type").and_then(Value::as_str) == Some("turn.started"))
        .count();
    let ended_count = events
        .iter()
        .filter(|event| event.get("type").and_then(Value::as_str) == Some("turn.ended"))
        .count();

    assert_eq!(started_count, 1, "expected exactly one turn.started event");
    assert_eq!(ended_count, 1, "expected exactly one turn.ended event");
}

async fn run_concurrency_snapshot(app: &Router, config: &TestAgentConfig) {
    let _guard = apply_credentials(&config.credentials);
    install_agent(app, config.agent).await;

    let session_a = format!("concurrent-a-{}", config.agent.as_str());
    let session_b = format!("concurrent-b-{}", config.agent.as_str());
    let perm_mode = test_permission_mode(config.agent);
    create_session(app, config.agent, &session_a, perm_mode).await;
    create_session(app, config.agent, &session_b, perm_mode).await;

    let app_a = app.clone();
    let app_b = app.clone();
    let send_a = send_message(&app_a, &session_a);
    let send_b = send_message(&app_b, &session_b);
    tokio::join!(send_a, send_b);

    let app_a = app.clone();
    let app_b = app.clone();
    let poll_a = poll_events_until(&app_a, &session_a, Duration::from_secs(120));
    let poll_b = poll_events_until(&app_b, &session_b, Duration::from_secs(120));
    let (events_a, events_b) = tokio::join!(poll_a, poll_b);
    let events_a = truncate_after_first_stop(&events_a);
    let events_b = truncate_after_first_stop(&events_b);

    assert!(
        !events_a.is_empty(),
        "no events collected for concurrent session a {}",
        config.agent
    );
    assert!(
        !events_b.is_empty(),
        "no events collected for concurrent session b {}",
        config.agent
    );
    assert!(
        should_stop(&events_a),
        "timed out waiting for assistant/error event for concurrent session a {}",
        config.agent
    );
    assert!(
        should_stop(&events_b),
        "timed out waiting for assistant/error event for concurrent session b {}",
        config.agent
    );

    let snapshot = json!({
        "session_a": normalize_events(&events_a),
        "session_b": normalize_events(&events_b),
    });
    assert_session_snapshot("concurrency_events", snapshot);
}
