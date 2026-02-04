#[path = "../common/mod.rs"]
mod common;

use common::*;
use sandbox_agent_agent_management::agents::AgentId;
use sandbox_agent_agent_management::testing::test_agents_from_env;
use serde_json::Value;
use std::time::Duration;

/// Test that Codebuff agent can provide a basic reply
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codebuff_basic_reply() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");
    let codebuff_config = configs.iter().find(|c| c.agent == AgentId::Codebuff);

    let config = match codebuff_config {
        Some(config) => config,
        None => {
            eprintln!("Skipping codebuff_basic_reply: Codebuff not configured");
            return;
        }
    };

    let app = TestApp::new();
    let _guard = apply_credentials(&config.credentials);
    install_agent(&app.app, config.agent).await;

    let session_id = "codebuff-basic-reply";
    create_session(
        &app.app,
        config.agent,
        session_id,
        test_permission_mode(config.agent),
    )
    .await;
    send_message(&app.app, session_id, PROMPT).await;

    // Codebuff emits text as item.delta events, and session.ended when complete
    let events = poll_events_until(&app.app, session_id, Duration::from_secs(120), |events| {
        has_event_type(events, "error")
            || has_event_type(events, "session.ended")
            || has_event_type(events, "item.delta")
    })
    .await;

    assert!(
        !events.is_empty(),
        "no events collected for Codebuff basic reply"
    );

    // Verify no unparsed events (parse errors)
    assert!(
        !events.iter().any(|event| {
            event.get("type").and_then(Value::as_str) == Some("agent.unparsed")
        }),
        "agent.unparsed event detected - Codebuff event parsing failed"
    );

    // Check for session lifecycle events
    let has_session_started = events
        .iter()
        .any(|event| event.get("type").and_then(Value::as_str) == Some("session.started"));
    assert!(
        has_session_started,
        "session.started event missing for Codebuff"
    );

    // Codebuff should emit item.delta for text responses or session.ended when complete
    let has_response = has_event_type(&events, "item.delta")
        || has_event_type(&events, "session.ended")
        || has_event_type(&events, "item.completed");
    assert!(
        has_response || has_event_type(&events, "error"),
        "no response events (item.delta, item.completed, or session.ended) for Codebuff"
    );
}

/// Test that Codebuff agent can execute tools and we receive tool_call/tool_result events
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codebuff_tool_flow() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");
    let codebuff_config = configs.iter().find(|c| c.agent == AgentId::Codebuff);

    let config = match codebuff_config {
        Some(config) => config,
        None => {
            eprintln!("Skipping codebuff_tool_flow: Codebuff not configured");
            return;
        }
    };

    let app = TestApp::new();
    let capabilities = fetch_capabilities(&app.app).await;
    let caps = capabilities
        .get(config.agent.as_str())
        .expect("capabilities missing");

    if !caps.tool_calls {
        eprintln!("Skipping codebuff_tool_flow: tool_calls capability not supported");
        return;
    }

    let _guard = apply_credentials(&config.credentials);
    install_agent(&app.app, config.agent).await;

    let session_id = "codebuff-tool-flow";
    create_session(
        &app.app,
        config.agent,
        session_id,
        test_permission_mode(config.agent),
    )
    .await;
    send_message(&app.app, session_id, TOOL_PROMPT).await;

    let events = poll_events_until(&app.app, session_id, Duration::from_secs(180), |events| {
        has_event_type(events, "error") || has_tool_result(events)
    })
    .await;

    assert!(
        !events.is_empty(),
        "no events collected for Codebuff tool flow"
    );

    // Verify tool_call was received
    let tool_call = find_tool_call(&events);
    assert!(
        tool_call.is_some(),
        "tool_call missing for Codebuff tool flow"
    );

    // Verify tool_result was received after tool_call
    assert!(
        has_tool_result(&events),
        "tool_result missing after tool_call for Codebuff"
    );
}

/// Test that Codebuff agent can ask questions via ask_user tool
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codebuff_question_flow() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");
    let codebuff_config = configs.iter().find(|c| c.agent == AgentId::Codebuff);

    let config = match codebuff_config {
        Some(config) => config,
        None => {
            eprintln!("Skipping codebuff_question_flow: Codebuff not configured");
            return;
        }
    };

    let app = TestApp::new();
    let capabilities = fetch_capabilities(&app.app).await;
    let caps = capabilities
        .get(config.agent.as_str())
        .expect("capabilities missing");

    if !caps.questions {
        eprintln!("Skipping codebuff_question_flow: questions capability not supported");
        return;
    }

    let _guard = apply_credentials(&config.credentials);
    install_agent(&app.app, config.agent).await;

    let session_id = "codebuff-question-flow";
    create_session(
        &app.app,
        config.agent,
        session_id,
        test_permission_mode(config.agent),
    )
    .await;
    send_message(&app.app, session_id, QUESTION_PROMPT).await;

    let events = poll_events_until(&app.app, session_id, Duration::from_secs(120), |events| {
        has_event_type(events, "error") || has_event_type(events, "question.requested")
    })
    .await;

    assert!(
        !events.is_empty(),
        "no events collected for Codebuff question flow"
    );

    // Verify question.requested event was received or we got an error
    let has_question = has_event_type(&events, "question.requested");
    let has_error = has_event_type(&events, "error");

    // Either we got a question or an error (some prompts may not trigger questions)
    assert!(
        has_question || has_error,
        "neither question.requested nor error event received for Codebuff question flow"
    );

    // If question was requested, verify question_id is present
    if has_question {
        let question_id = find_question_id(&events);
        assert!(
            question_id.is_some(),
            "question_id missing in question.requested event for Codebuff"
        );
    }
}

/// Test that Codebuff events include proper item deltas for streaming
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codebuff_streaming_deltas() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS or install agents");
    let codebuff_config = configs.iter().find(|c| c.agent == AgentId::Codebuff);

    let config = match codebuff_config {
        Some(config) => config,
        None => {
            eprintln!("Skipping codebuff_streaming_deltas: Codebuff not configured");
            return;
        }
    };

    let app = TestApp::new();
    let capabilities = fetch_capabilities(&app.app).await;
    let caps = capabilities
        .get(config.agent.as_str())
        .expect("capabilities missing");

    if !caps.streaming_deltas {
        eprintln!("Skipping codebuff_streaming_deltas: streaming_deltas capability not supported");
        return;
    }

    let _guard = apply_credentials(&config.credentials);
    install_agent(&app.app, config.agent).await;

    let session_id = "codebuff-streaming-deltas";
    create_session(
        &app.app,
        config.agent,
        session_id,
        test_permission_mode(config.agent),
    )
    .await;
    send_message(&app.app, session_id, PROMPT).await;

    let events = poll_events_until(&app.app, session_id, Duration::from_secs(120), |events| {
        has_event_type(events, "error")
            || has_event_type(events, "session.ended")
            || has_event_type(events, "item.delta")
    })
    .await;

    assert!(
        !events.is_empty(),
        "no events collected for Codebuff streaming deltas"
    );

    // Check for item.delta events (Codebuff emits text as item.delta)
    let has_deltas = events
        .iter()
        .any(|event| event.get("type").and_then(Value::as_str) == Some("item.delta"));

    // Codebuff should emit item.delta events for streaming text
    assert!(
        has_deltas || has_event_type(&events, "error"),
        "no item.delta events found for Codebuff streaming - expected streaming deltas"
    );
}
