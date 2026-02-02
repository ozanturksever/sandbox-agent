use std::sync::Arc;

use sandbox_agent::router::test_utils::{exit_status, spawn_sleep_process, TestHarness};
use sandbox_agent_agent_management::agents::AgentId;
use sandbox_agent_universal_agent_schema::SessionEndReason;
use tokio::time::{timeout, Duration};

async fn wait_for_exit(child: &Arc<std::sync::Mutex<Option<std::process::Child>>>) {
    for _ in 0..20 {
        let done = {
            let mut guard = child.lock().expect("child lock");
            match guard.as_mut() {
                Some(child) => child.try_wait().ok().flatten().is_some(),
                None => true,
            }
        };
        if done {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn register_and_unregister_sessions() {
    let harness = TestHarness::new().await;
    harness
        .register_session(AgentId::Codex, "sess-1", Some("thread-1"))
        .await;

    assert!(harness.has_session_mapping(AgentId::Codex, "sess-1").await);
    assert_eq!(
        harness
            .native_mapping(AgentId::Codex, "thread-1")
            .await
            .as_deref(),
        Some("sess-1")
    );

    harness
        .unregister_session(AgentId::Codex, "sess-1", Some("thread-1"))
        .await;

    assert!(!harness.has_session_mapping(AgentId::Codex, "sess-1").await);
    assert!(harness
        .native_mapping(AgentId::Codex, "thread-1")
        .await
        .is_none());
}

#[tokio::test]
async fn shutdown_marks_servers_stopped_and_kills_child() {
    let harness = TestHarness::new().await;
    let child = harness
        .insert_stdio_server(AgentId::Codex, Some(spawn_sleep_process()), 0)
        .await;

    harness.shutdown().await;

    assert!(matches!(
        harness.server_status(AgentId::Codex).await,
        Some(sandbox_agent::router::ServerStatus::Stopped)
    ));

    wait_for_exit(&child).await;
    let exited = {
        let mut guard = child.lock().expect("child lock");
        guard
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten())
            .is_some()
    };
    assert!(exited);
}

#[tokio::test]
async fn handle_process_exit_marks_error_and_ends_sessions() {
    let harness = TestHarness::new().await;
    harness
        .insert_session("sess-1", AgentId::Codex, Some("thread-1"))
        .await;
    harness
        .register_session(AgentId::Codex, "sess-1", Some("thread-1"))
        .await;
    harness.insert_stdio_server(AgentId::Codex, None, 1).await;

    harness
        .handle_process_exit(AgentId::Codex, 1, exit_status(7))
        .await;

    assert!(matches!(
        harness.server_status(AgentId::Codex).await,
        Some(sandbox_agent::router::ServerStatus::Error)
    ));
    assert!(harness
        .server_last_error(AgentId::Codex)
        .await
        .unwrap_or_default()
        .contains("exited"));
    assert!(harness.session_ended("sess-1").await);
    assert!(matches!(
        harness.session_end_reason("sess-1").await,
        Some(SessionEndReason::Error)
    ));
}

#[tokio::test]
async fn auto_restart_notifier_emits_signal() {
    let harness = TestHarness::new().await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    harness.set_restart_notifier(tx).await;
    harness.insert_http_server(AgentId::Mock, 2).await;

    harness
        .handle_process_exit(AgentId::Mock, 2, exit_status(2))
        .await;

    let received = timeout(Duration::from_millis(200), rx.recv())
        .await
        .expect("timeout");
    assert_eq!(received, Some(AgentId::Mock));
}
