#[path = "../common/mod.rs"]
mod common;

use axum::http::Method;
use common::*;
use sandbox_agent_agent_management::agents::AgentId;
use sandbox_agent_agent_management::testing::test_agents_from_env;
use serde_json::Value;
use std::fs;
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_file_edit_flow() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");
    let app = TestApp::new();
    let capabilities = fetch_capabilities(&app.app).await;

    for config in &configs {
        let caps = capabilities
            .get(config.agent.as_str())
            .expect("capabilities missing");
        if !caps.file_changes {
            continue;
        }
        if config.agent == AgentId::Mock {
            // Mock agent only emits synthetic file change events.
            continue;
        }

        let _guard = apply_credentials(&config.credentials);
        install_agent(&app.app, config.agent).await;

        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let file_path = temp_dir.path().join("edit.txt");
        fs::write(&file_path, "before\n").expect("write seed file");

        let session_id = format!("file-edit-{}", config.agent.as_str());
        create_session(
            &app.app,
            config.agent,
            &session_id,
            test_permission_mode(config.agent),
        )
        .await;
        let prompt = format!(
            "Edit the file at {} so its entire contents are exactly 'updated' (no quotes). \
Do not change any other files. Reply only with DONE after editing.",
            file_path.display()
        );
        send_message(&app.app, &session_id, &prompt).await;

        let start = Instant::now();
        let mut offset = 0u64;
        let mut events = Vec::new();
        let mut replied = false;
        let mut updated = false;
        while start.elapsed() < Duration::from_secs(180) {
            let path = format!("/v1/sessions/{session_id}/events?offset={offset}&limit=200");
            let (status, payload) = send_json(&app.app, Method::GET, &path, None).await;
            assert_eq!(status, axum::http::StatusCode::OK, "poll events");
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
                if !replied {
                    if let Some(permission_id) = find_permission_id(&events) {
                        let _ = send_status(
                            &app.app,
                            Method::POST,
                            &format!("/v1/sessions/{session_id}/permissions/{permission_id}/reply"),
                            Some(serde_json::json!({ "reply": "once" })),
                        )
                        .await;
                        replied = true;
                    }
                }
            }

            let contents = fs::read_to_string(&file_path).unwrap_or_default();
            let trimmed = contents.trim_end_matches(&['\r', '\n'][..]);
            if trimmed == "updated" {
                updated = true;
                break;
            }

            tokio::time::sleep(Duration::from_millis(800)).await;
        }

        assert!(
            updated,
            "file edit did not complete for {}",
            config.agent.as_str()
        );
    }
}
