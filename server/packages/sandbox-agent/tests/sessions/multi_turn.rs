// Multi-turn session snapshots use the mock baseline as the single source of truth.
include!("../common/http.rs");

const FIRST_PROMPT: &str = "Reply with exactly the word FIRST.";
const SECOND_PROMPT: &str = "Reply with exactly the word SECOND.";

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

async fn send_message_with_text(app: &Router, session_id: &str, text: &str) {
    let status = send_status(
        app,
        Method::POST,
        &format!("/v1/sessions/{session_id}/messages"),
        Some(json!({ "message": text })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "send message");
}

async fn poll_events_until_from(
    app: &Router,
    session_id: &str,
    offset: u64,
    timeout: Duration,
) -> (Vec<Value>, u64) {
    let start = Instant::now();
    let mut offset = offset;
    let mut events = Vec::new();
    while start.elapsed() < timeout {
        let path = format!("/v1/sessions/{session_id}/events?offset={offset}&limit=200");
        let (status, payload) = send_json(app, Method::GET, &path, None).await;
        assert_eq!(status, StatusCode::OK, "poll events");
        let new_events = payload
            .get("events")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !new_events.is_empty() {
            if let Some(last) = new_events
                .last()
                .and_then(|event| event.get("sequence"))
                .and_then(Value::as_u64)
            {
                offset = last;
            }
            events.extend(new_events);
            if should_stop(&events) {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
    }
    (events, offset)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multi_turn_snapshots() {
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

        let session_id = format!("multi-turn-{}", config.agent.as_str());
        create_session(
            &app.app,
            config.agent,
            &session_id,
            test_permission_mode(config.agent),
        )
        .await;

        send_message_with_text(&app.app, &session_id, FIRST_PROMPT).await;
        let (first_events, offset) =
            poll_events_until_from(&app.app, &session_id, 0, Duration::from_secs(120)).await;
        let first_events = truncate_after_first_stop(&first_events);
        assert!(
            !first_events.is_empty(),
            "no events collected for first turn {}",
            config.agent
        );
        assert!(
            should_stop(&first_events),
            "timed out waiting for assistant/error event for first turn {}",
            config.agent
        );

        send_message_with_text(&app.app, &session_id, SECOND_PROMPT).await;
        let (second_events, _offset) =
            poll_events_until_from(&app.app, &session_id, offset, Duration::from_secs(120)).await;
        let second_events = truncate_after_first_stop(&second_events);
        assert!(
            !second_events.is_empty(),
            "no events collected for second turn {}",
            config.agent
        );
        assert!(
            should_stop(&second_events),
            "timed out waiting for assistant/error event for second turn {}",
            config.agent
        );

        let snapshot = json!({
            "first": normalize_events(&first_events),
            "second": normalize_events(&second_events),
        });
        assert_session_snapshot("multi_turn", snapshot);
    }
}
