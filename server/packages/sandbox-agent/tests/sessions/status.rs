// Status capability checks are isolated from baseline snapshots.
include!("../common/http.rs");

fn status_prompt(_agent: AgentId) -> &'static str {
    "Provide a short status update."
}

fn events_have_status(events: &[Value]) -> bool {
    events.iter().any(|event| event_is_status_item(event))
        || events_have_content_type(events, "status")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_events_present() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");

    for config in &configs {
        let app = TestApp::new();
        let capabilities = fetch_capabilities(&app.app).await;
        let caps = capabilities
            .get(config.agent.as_str())
            .expect("capabilities missing");
        if !caps.status {
            continue;
        }

        let _guard = apply_credentials(&config.credentials);
        install_agent(&app.app, config.agent).await;

        let session_id = format!("status-{}", config.agent.as_str());
        create_session(
            &app.app,
            config.agent,
            &session_id,
            test_permission_mode(config.agent),
        )
        .await;
        let status = send_status(
            &app.app,
            Method::POST,
            &format!("/v1/sessions/{session_id}/messages"),
            Some(json!({ "message": status_prompt(config.agent) })),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "send status prompt");

        let events =
            poll_events_until_match(&app.app, &session_id, Duration::from_secs(120), |events| {
                events_have_status(events) || events.iter().any(is_error_event)
            })
            .await;
        assert!(
            events_have_status(&events),
            "expected status events for {}",
            config.agent
        );
    }
}
