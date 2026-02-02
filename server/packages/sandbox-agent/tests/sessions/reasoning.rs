// Reasoning capability checks are isolated from baseline snapshots.
include!("../common/http.rs");

fn reasoning_prompt(_agent: AgentId) -> &'static str {
    "Answer briefly and include your reasoning."
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reasoning_events_present() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");

    for config in &configs {
        let app = TestApp::new();
        let capabilities = fetch_capabilities(&app.app).await;
        let caps = capabilities
            .get(config.agent.as_str())
            .expect("capabilities missing");
        if !caps.reasoning {
            continue;
        }

        let _guard = apply_credentials(&config.credentials);
        install_agent(&app.app, config.agent).await;

        let session_id = format!("reasoning-{}", config.agent.as_str());
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
            Some(json!({ "message": reasoning_prompt(config.agent) })),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "send reasoning prompt");

        let events =
            poll_events_until_match(&app.app, &session_id, Duration::from_secs(120), |events| {
                events_have_content_type(events, "reasoning") || events.iter().any(is_error_event)
            })
            .await;
        assert!(
            events_have_content_type(&events, "reasoning"),
            "expected reasoning content for {}",
            config.agent
        );
    }
}
