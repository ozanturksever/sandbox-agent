// Question flow snapshots compare every agent to the mock baseline.
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
async fn question_flow_snapshots() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");

    for config in &configs {
        let app = TestApp::new();
        let capabilities = fetch_capabilities(&app.app).await;
        let caps = capabilities
            .get(config.agent.as_str())
            .expect("capabilities missing");
        if !caps.questions {
            continue;
        }

        let _guard = apply_credentials(&config.credentials);
        install_agent(&app.app, config.agent).await;

        let question_reply_session = format!("question-reply-{}", config.agent.as_str());
        create_session(&app.app, config.agent, &question_reply_session, "plan").await;
        let status = send_status(
            &app.app,
            Method::POST,
            &format!("/v1/sessions/{question_reply_session}/messages"),
            Some(json!({ "message": QUESTION_PROMPT })),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "send question prompt");

        let question_events = poll_events_until_match(
            &app.app,
            &question_reply_session,
            Duration::from_secs(120),
            |events| find_question_id_and_answers(events).is_some() || should_stop(events),
        )
        .await;
        let question_events = truncate_question_events(&question_events);
        assert_session_snapshot("question_reply_events", normalize_events(&question_events));

        if let Some((question_id, answers)) = find_question_id_and_answers(&question_events) {
            let status = send_status(
                &app.app,
                Method::POST,
                &format!("/v1/sessions/{question_reply_session}/questions/{question_id}/reply"),
                Some(json!({ "answers": answers })),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT, "reply question");
            assert_session_snapshot("question_reply", snapshot_status(status));
        } else {
            let (status, payload) = send_json(
                &app.app,
                Method::POST,
                &format!("/v1/sessions/{question_reply_session}/questions/missing-question/reply"),
                Some(json!({ "answers": [] })),
            )
            .await;
            assert!(!status.is_success(), "missing question id should error");
            assert_session_snapshot(
                "question_reply_missing",
                json!({
                    "status": status.as_u16(),
                    "payload": payload,
                }),
            );
        }

        let question_reject_session = format!("question-reject-{}", config.agent.as_str());
        create_session(&app.app, config.agent, &question_reject_session, "plan").await;
        let status = send_status(
            &app.app,
            Method::POST,
            &format!("/v1/sessions/{question_reject_session}/messages"),
            Some(json!({ "message": QUESTION_PROMPT })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NO_CONTENT,
            "send question prompt reject"
        );

        let reject_events = poll_events_until_match(
            &app.app,
            &question_reject_session,
            Duration::from_secs(120),
            |events| find_question_id_and_answers(events).is_some() || should_stop(events),
        )
        .await;
        let reject_events = truncate_question_events(&reject_events);
        assert_session_snapshot("question_reject_events", normalize_events(&reject_events));

        if let Some((question_id, _)) = find_question_id_and_answers(&reject_events) {
            let status = send_status(
                &app.app,
                Method::POST,
                &format!("/v1/sessions/{question_reject_session}/questions/{question_id}/reject"),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT, "reject question");
            assert_session_snapshot("question_reject", snapshot_status(status));
        } else {
            let (status, payload) = send_json(
                &app.app,
                Method::POST,
                &format!(
                    "/v1/sessions/{question_reject_session}/questions/missing-question/reject"
                ),
                None,
            )
            .await;
            assert!(
                !status.is_success(),
                "missing question id reject should error"
            );
            assert_session_snapshot(
                "question_reject_missing",
                json!({
                    "status": status.as_u16(),
                    "payload": payload,
                }),
            );
        }
    }
}
