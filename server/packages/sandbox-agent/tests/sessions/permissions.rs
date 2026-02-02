// Permission flow snapshots compare every agent to the mock baseline.
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
async fn permission_flow_snapshots() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");

    for config in &configs {
        let app = TestApp::new();
        let capabilities = fetch_capabilities(&app.app).await;
        let caps = capabilities
            .get(config.agent.as_str())
            .expect("capabilities missing");
        if !(caps.plan_mode && caps.permissions) {
            continue;
        }

        let _guard = apply_credentials(&config.credentials);
        install_agent(&app.app, config.agent).await;

        let permission_session = format!("perm-{}", config.agent.as_str());
        create_session(&app.app, config.agent, &permission_session, "plan").await;
        let status = send_status(
            &app.app,
            Method::POST,
            &format!("/v1/sessions/{permission_session}/messages"),
            Some(json!({ "message": PERMISSION_PROMPT })),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "send permission prompt");

        let permission_events = poll_events_until_match(
            &app.app,
            &permission_session,
            Duration::from_secs(120),
            |events| find_permission_id(events).is_some() || should_stop(events),
        )
        .await;
        let permission_events = truncate_permission_events(&permission_events);
        assert_session_snapshot("permission_events", normalize_events(&permission_events));

        if let Some(permission_id) = find_permission_id(&permission_events) {
            let status = send_status(
                &app.app,
                Method::POST,
                &format!("/v1/sessions/{permission_session}/permissions/{permission_id}/reply"),
                Some(json!({ "reply": "once" })),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT, "reply permission");
            assert_session_snapshot("permission_reply", snapshot_status(status));
        } else {
            let (status, payload) = send_json(
                &app.app,
                Method::POST,
                &format!("/v1/sessions/{permission_session}/permissions/missing-permission/reply"),
                Some(json!({ "reply": "once" })),
            )
            .await;
            assert!(!status.is_success(), "missing permission id should error");
            assert_session_snapshot(
                "permission_reply_missing",
                json!({
                    "status": status.as_u16(),
                    "payload": payload,
                }),
            );
        }
    }
}
