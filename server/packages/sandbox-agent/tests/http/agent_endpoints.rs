// Agent-specific HTTP endpoints live here; session-related snapshots are in tests/sessions/.
include!("../common/http.rs");

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auth_snapshots() {
    let token = "test-token";
    let app = TestApp::new_with_auth(AuthConfig::with_token(token.to_string()));

    let (status, payload) = send_json(&app.app, Method::GET, "/v1/health", None).await;
    assert_eq!(status, StatusCode::OK, "health should be public");
    insta::with_settings!({
        snapshot_suffix => snapshot_name("auth_health_public", None),
    }, {
        insta::assert_yaml_snapshot!(json!({
            "status": status.as_u16(),
            "payload": normalize_health(&payload),
        }));
    });

    let (status, payload) = send_json(&app.app, Method::GET, "/v1/agents", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "missing token should 401");
    insta::with_settings!({
        snapshot_suffix => snapshot_name("auth_missing_token", None),
    }, {
        insta::assert_yaml_snapshot!(json!({
            "status": status.as_u16(),
            "payload": payload,
        }));
    });

    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/agents")
        .header(header::AUTHORIZATION, "Bearer wrong-token")
        .body(Body::empty())
        .expect("auth invalid request");
    let (status, _headers, payload) = send_json_request(&app.app, request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "invalid token should 401");
    insta::with_settings!({
        snapshot_suffix => snapshot_name("auth_invalid_token", None),
    }, {
        insta::assert_yaml_snapshot!(json!({
            "status": status.as_u16(),
            "payload": payload,
        }));
    });

    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/agents")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("auth valid request");
    let (status, _headers, payload) = send_json_request(&app.app, request).await;
    assert_eq!(status, StatusCode::OK, "valid token should succeed");
    insta::with_settings!({
        snapshot_suffix => snapshot_name("auth_valid_token", None),
    }, {
        insta::assert_yaml_snapshot!(json!({
            "status": status.as_u16(),
            "payload": normalize_agent_list(&payload),
        }));
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cors_snapshots() {
    let cors = CorsLayer::new()
        .allow_origin("http://example.com".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
    let app = TestApp::new_with_auth_and_cors(AuthConfig::disabled(), Some(cors));

    let preflight = Request::builder()
        .method(Method::OPTIONS)
        .uri("/v1/agents")
        .header(header::ORIGIN, "http://example.com")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .header(
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "authorization,content-type",
        )
        .body(Body::empty())
        .expect("cors preflight request");
    let (status, headers, _payload) = send_request(&app.app, preflight).await;
    insta::with_settings!({
        snapshot_suffix => snapshot_name("cors_preflight", None),
    }, {
        insta::assert_yaml_snapshot!(snapshot_cors(status, &headers));
    });

    let actual = Request::builder()
        .method(Method::GET)
        .uri("/v1/health")
        .header(header::ORIGIN, "http://example.com")
        .body(Body::empty())
        .expect("cors actual request");
    let (status, headers, payload) = send_json_request(&app.app, actual).await;
    assert_eq!(status, StatusCode::OK, "cors actual request should succeed");
    insta::with_settings!({
        snapshot_suffix => snapshot_name("cors_actual", None),
    }, {
        insta::assert_yaml_snapshot!(json!({
            "cors": snapshot_cors(status, &headers),
            "payload": normalize_health(&payload),
        }));
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_endpoints_snapshots() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");
    let app = TestApp::new();

    let (status, health) = send_json(&app.app, Method::GET, "/v1/health", None).await;
    assert_eq!(status, StatusCode::OK, "health status");
    insta::with_settings!({
        snapshot_suffix => snapshot_name("health", None),
    }, {
        insta::assert_yaml_snapshot!(normalize_health(&health));
    });

    // List agents (verify IDs only; install state is environment-dependent).
    let (status, agents) = send_json(&app.app, Method::GET, "/v1/agents", None).await;
    assert_eq!(status, StatusCode::OK, "agents list");
    insta::with_settings!({
        snapshot_suffix => snapshot_name("agents_list", None),
    }, {
        insta::assert_yaml_snapshot!(normalize_agent_list(&agents));
    });

    for config in &configs {
        let _guard = apply_credentials(&config.credentials);
        let status = send_status(
            &app.app,
            Method::POST,
            &format!("/v1/agents/{}/install", config.agent.as_str()),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "install agent");
        insta::with_settings!({
            snapshot_suffix => snapshot_name("agent_install", Some(config.agent)),
        }, {
            insta::assert_yaml_snapshot!(snapshot_status(status));
        });
    }

    for config in &configs {
        let _guard = apply_credentials(&config.credentials);
        let (status, modes) = send_json(
            &app.app,
            Method::GET,
            &format!("/v1/agents/{}/modes", config.agent.as_str()),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "agent modes");
        insta::with_settings!({
            snapshot_suffix => snapshot_name("agent_modes", Some(config.agent)),
        }, {
            insta::assert_yaml_snapshot!(normalize_agent_modes(&modes));
        });
    }

    for config in &configs {
        let _guard = apply_credentials(&config.credentials);
        let (status, models) = send_json(
            &app.app,
            Method::GET,
            &format!("/v1/agents/{}/models", config.agent.as_str()),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "agent models");
        let model_count = models
            .get("models")
            .and_then(|value| value.as_array())
            .map(|models| models.len())
            .unwrap_or_default();
        assert!(model_count > 0, "agent models should not be empty");
        insta::with_settings!({
            snapshot_suffix => snapshot_name("agent_models", Some(config.agent)),
        }, {
            insta::assert_yaml_snapshot!(normalize_agent_models(&models, config.agent));
        });
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_session_with_skill_sources() {
    let app = TestApp::new();

    // Create a temp skill directory with SKILL.md
    let skill_dir = tempfile::tempdir().expect("create skill dir");
    let skill_path = skill_dir.path().join("my-test-skill");
    std::fs::create_dir_all(&skill_path).expect("create skill subdir");
    std::fs::write(skill_path.join("SKILL.md"), "# Test Skill\nA test skill.")
        .expect("write SKILL.md");

    // Create session with local skill source
    let (status, payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/sessions/skill-test-session",
        Some(json!({
            "agent": "mock",
            "skills": {
                "sources": [
                    {
                        "type": "local",
                        "source": skill_dir.path().to_string_lossy()
                    }
                ]
            }
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "create session with skills: {payload}"
    );
    assert!(
        payload
            .get("healthy")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "session should be healthy"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_session_with_skill_sources_filter() {
    let app = TestApp::new();

    // Create a temp directory with two skills
    let skill_dir = tempfile::tempdir().expect("create skill dir");
    let wanted = skill_dir.path().join("wanted-skill");
    let unwanted = skill_dir.path().join("unwanted-skill");
    std::fs::create_dir_all(&wanted).expect("create wanted dir");
    std::fs::create_dir_all(&unwanted).expect("create unwanted dir");
    std::fs::write(wanted.join("SKILL.md"), "# Wanted").expect("write wanted SKILL.md");
    std::fs::write(unwanted.join("SKILL.md"), "# Unwanted").expect("write unwanted SKILL.md");

    // Create session with filter
    let (status, payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/sessions/skill-filter-session",
        Some(json!({
            "agent": "mock",
            "skills": {
                "sources": [
                    {
                        "type": "local",
                        "source": skill_dir.path().to_string_lossy(),
                        "skills": ["wanted-skill"]
                    }
                ]
            }
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "create session with skill filter: {payload}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_session_with_invalid_skill_source() {
    let app = TestApp::new();

    // Use a non-existent path
    let (status, _payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/sessions/skill-invalid-session",
        Some(json!({
            "agent": "mock",
            "skills": {
                "sources": [
                    {
                        "type": "local",
                        "source": "/nonexistent/path/to/skills"
                    }
                ]
            }
        })),
    )
    .await;
    // Should fail with a 4xx or 5xx error
    assert_ne!(
        status,
        StatusCode::OK,
        "session with invalid skill source should fail"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_session_with_skill_filter_no_match() {
    let app = TestApp::new();

    let skill_dir = tempfile::tempdir().expect("create skill dir");
    let skill_path = skill_dir.path().join("alpha");
    std::fs::create_dir_all(&skill_path).expect("create alpha dir");
    std::fs::write(skill_path.join("SKILL.md"), "# Alpha").expect("write SKILL.md");

    // Filter for a skill that doesn't exist
    let (status, _payload) = send_json(
        &app.app,
        Method::POST,
        "/v1/sessions/skill-nomatch-session",
        Some(json!({
            "agent": "mock",
            "skills": {
                "sources": [
                    {
                        "type": "local",
                        "source": skill_dir.path().to_string_lossy(),
                        "skills": ["nonexistent"]
                    }
                ]
            }
        })),
    )
    .await;
    assert_ne!(
        status,
        StatusCode::OK,
        "session with no matching skills should fail"
    );
}
