//! OpenCode-compatible API handlers mounted under `/opencode`.
//!
//! These endpoints implement the full OpenCode OpenAPI surface. Most routes are
//! stubbed responses with deterministic helpers for snapshot testing. A minimal
//! in-memory state tracks sessions/messages/ptys to keep behavior coherent.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive};
use axum::response::{IntoResponse, Sse};
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use futures::stream;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{broadcast, Mutex};
use tokio::time::interval;

use crate::router::{AppState, CreateSessionRequest};
use sandbox_agent_error::SandboxError;
use sandbox_agent_universal_agent_schema::{
    ContentPart, ItemDeltaData, ItemEventData, ItemKind, ItemRole, UniversalEvent, UniversalEventData,
    UniversalEventType, UniversalItem,
};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);
static MESSAGE_COUNTER: AtomicU64 = AtomicU64::new(1);
static PART_COUNTER: AtomicU64 = AtomicU64::new(1);
static PTY_COUNTER: AtomicU64 = AtomicU64::new(1);
static PROJECT_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
struct OpenCodeCompatConfig {
    fixed_time_ms: Option<i64>,
    fixed_directory: Option<String>,
    fixed_worktree: Option<String>,
    fixed_home: Option<String>,
    fixed_state: Option<String>,
    fixed_config: Option<String>,
    fixed_branch: Option<String>,
    fixed_agent: Option<String>,
}

impl OpenCodeCompatConfig {
    fn from_env() -> Self {
        Self {
            fixed_time_ms: std::env::var("OPENCODE_COMPAT_FIXED_TIME_MS")
                .ok()
                .and_then(|value| value.parse::<i64>().ok()),
            fixed_directory: std::env::var("OPENCODE_COMPAT_DIRECTORY").ok(),
            fixed_worktree: std::env::var("OPENCODE_COMPAT_WORKTREE").ok(),
            fixed_home: std::env::var("OPENCODE_COMPAT_HOME").ok(),
            fixed_state: std::env::var("OPENCODE_COMPAT_STATE").ok(),
            fixed_config: std::env::var("OPENCODE_COMPAT_CONFIG").ok(),
            fixed_branch: std::env::var("OPENCODE_COMPAT_BRANCH").ok(),
            fixed_agent: std::env::var("OPENCODE_COMPAT_AGENT").ok(),
        }
    }

    fn now_ms(&self) -> i64 {
        if let Some(value) = self.fixed_time_ms {
            return value;
        }
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug)]
struct OpenCodeSessionRecord {
    id: String,
    slug: String,
    project_id: String,
    directory: String,
    parent_id: Option<String>,
    title: String,
    version: String,
    created_at: i64,
    updated_at: i64,
    share_url: Option<String>,
}

impl OpenCodeSessionRecord {
    fn to_value(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("id".to_string(), json!(self.id));
        map.insert("slug".to_string(), json!(self.slug));
        map.insert("projectID".to_string(), json!(self.project_id));
        map.insert("directory".to_string(), json!(self.directory));
        map.insert("title".to_string(), json!(self.title));
        map.insert("version".to_string(), json!(self.version));
        map.insert(
            "time".to_string(),
            json!({
                "created": self.created_at,
                "updated": self.updated_at,
            }),
        );
        if let Some(parent_id) = &self.parent_id {
            map.insert("parentID".to_string(), json!(parent_id));
        }
        if let Some(url) = &self.share_url {
            map.insert("share".to_string(), json!({"url": url}));
        }
        Value::Object(map)
    }
}

#[derive(Clone, Debug)]
struct OpenCodeMessageRecord {
    info: Value,
    parts: Vec<Value>,
}

#[derive(Clone, Debug)]
struct OpenCodePtyRecord {
    id: String,
    title: String,
    command: String,
    args: Vec<String>,
    cwd: String,
    status: String,
    pid: i64,
}

impl OpenCodePtyRecord {
    fn to_value(&self) -> Value {
        json!({
            "id": self.id,
            "title": self.title,
            "command": self.command,
            "args": self.args,
            "cwd": self.cwd,
            "status": self.status,
            "pid": self.pid,
        })
    }
}

#[derive(Default, Clone)]
struct OpenCodeSessionRuntime {
    last_user_message_id: Option<String>,
    last_agent: Option<String>,
    last_model_provider: Option<String>,
    last_model_id: Option<String>,
    message_id_for_item: HashMap<String, String>,
    text_by_message: HashMap<String, String>,
    part_id_by_message: HashMap<String, String>,
    tool_part_by_call: HashMap<String, String>,
}

pub struct OpenCodeState {
    config: OpenCodeCompatConfig,
    default_project_id: String,
    sessions: Mutex<HashMap<String, OpenCodeSessionRecord>>,
    messages: Mutex<HashMap<String, Vec<OpenCodeMessageRecord>>>,
    ptys: Mutex<HashMap<String, OpenCodePtyRecord>>,
    session_runtime: Mutex<HashMap<String, OpenCodeSessionRuntime>>,
    session_streams: Mutex<HashMap<String, bool>>,
    event_broadcaster: broadcast::Sender<Value>,
}

impl OpenCodeState {
    pub fn new() -> Self {
        let (event_broadcaster, _) = broadcast::channel(256);
        let project_id = format!("proj_{}", PROJECT_COUNTER.fetch_add(1, Ordering::Relaxed));
        Self {
            config: OpenCodeCompatConfig::from_env(),
            default_project_id: project_id,
            sessions: Mutex::new(HashMap::new()),
            messages: Mutex::new(HashMap::new()),
            ptys: Mutex::new(HashMap::new()),
            session_runtime: Mutex::new(HashMap::new()),
            session_streams: Mutex::new(HashMap::new()),
            event_broadcaster,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.event_broadcaster.subscribe()
    }

    pub fn emit_event(&self, event: Value) {
        let _ = self.event_broadcaster.send(event);
    }

    fn now_ms(&self) -> i64 {
        self.config.now_ms()
    }

    fn directory_for(&self, headers: &HeaderMap, query: Option<&String>) -> String {
        if let Some(value) = query {
            return value.clone();
        }
        if let Some(value) = self
            .config
            .fixed_directory
            .as_ref()
            .cloned()
            .or_else(|| {
                headers
                    .get("x-opencode-directory")
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.to_string())
            })
        {
            return value;
        }
        std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(|v| v.to_string()))
            .unwrap_or_else(|| ".".to_string())
    }

    fn worktree_for(&self, directory: &str) -> String {
        self.config
            .fixed_worktree
            .clone()
            .unwrap_or_else(|| directory.to_string())
    }

    fn home_dir(&self) -> String {
        self.config
            .fixed_home
            .clone()
            .or_else(|| std::env::var("HOME").ok())
            .unwrap_or_else(|| "/".to_string())
    }

    fn state_dir(&self) -> String {
        self.config
            .fixed_state
            .clone()
            .unwrap_or_else(|| format!("{}/.local/state/opencode", self.home_dir()))
    }

    async fn ensure_session(&self, session_id: &str, directory: String) -> Value {
        let mut sessions = self.sessions.lock().await;
        if let Some(existing) = sessions.get(session_id) {
            return existing.to_value();
        }

        let now = self.now_ms();
        let record = OpenCodeSessionRecord {
            id: session_id.to_string(),
            slug: format!("session-{}", session_id),
            project_id: self.default_project_id.clone(),
            directory,
            parent_id: None,
            title: format!("Session {}", session_id),
            version: "0".to_string(),
            created_at: now,
            updated_at: now,
            share_url: None,
        };
        let value = record.to_value();
        sessions.insert(session_id.to_string(), record);
        drop(sessions);

        self.emit_event(session_event("session.created", &value));
        value
    }

    fn config_dir(&self) -> String {
        self.config
            .fixed_config
            .clone()
            .unwrap_or_else(|| format!("{}/.config/opencode", self.home_dir()))
    }

    fn branch_name(&self) -> String {
        self.config
            .fixed_branch
            .clone()
            .unwrap_or_else(|| "main".to_string())
    }

    fn default_agent(&self) -> String {
        self.config
            .fixed_agent
            .clone()
            .unwrap_or_else(|| "opencode".to_string())
    }

    async fn update_runtime(
        &self,
        session_id: &str,
        update: impl FnOnce(&mut OpenCodeSessionRuntime),
    ) -> OpenCodeSessionRuntime {
        let mut runtimes = self.session_runtime.lock().await;
        let entry = runtimes
            .entry(session_id.to_string())
            .or_insert_with(OpenCodeSessionRuntime::default);
        update(entry);
        entry.clone()
    }
}

/// Combined app state with OpenCode state.
pub struct OpenCodeAppState {
    pub inner: Arc<AppState>,
    pub opencode: Arc<OpenCodeState>,
}

impl OpenCodeAppState {
    pub fn new(inner: Arc<AppState>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            opencode: Arc::new(OpenCodeState::new()),
        })
    }
}

async fn ensure_backing_session(
    state: &Arc<OpenCodeAppState>,
    session_id: &str,
    agent: &str,
) -> Result<(), SandboxError> {
    let request = CreateSessionRequest {
        agent: agent.to_string(),
        agent_mode: None,
        permission_mode: None,
        model: None,
        variant: None,
        agent_version: None,
    };
    match state
        .inner
        .session_manager()
        .create_session(session_id.to_string(), request)
        .await
    {
        Ok(_) => Ok(()),
        Err(SandboxError::SessionAlreadyExists { .. }) => Ok(()),
        Err(err) => Err(err),
    }
}

async fn ensure_session_stream(state: Arc<OpenCodeAppState>, session_id: String) {
    let should_spawn = {
        let mut streams = state.opencode.session_streams.lock().await;
        if streams.contains_key(&session_id) {
            false
        } else {
            streams.insert(session_id.clone(), true);
            true
        }
    };
    if !should_spawn {
        return;
    }

    tokio::spawn(async move {
        let subscription = match state
            .inner
            .session_manager()
            .subscribe(&session_id, 0)
            .await
        {
            Ok(subscription) => subscription,
            Err(_) => {
                let mut streams = state.opencode.session_streams.lock().await;
                streams.remove(&session_id);
                return;
            }
        };

        for event in subscription.initial_events {
            apply_universal_event(state.clone(), event).await;
        }
        let mut receiver = subscription.receiver;
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    apply_universal_event(state.clone(), event).await;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
        let mut streams = state.opencode.session_streams.lock().await;
        streams.remove(&session_id);
    });
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenCodeCreateSessionRequest {
    title: Option<String>,
    #[serde(rename = "parentID")]
    parent_id: Option<String>,
    permission: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenCodeUpdateSessionRequest {
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DirectoryQuery {
    directory: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolQuery {
    directory: Option<String>,
    provider: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FindTextQuery {
    directory: Option<String>,
    pattern: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FindFilesQuery {
    directory: Option<String>,
    query: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FindSymbolsQuery {
    directory: Option<String>,
    query: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FileContentQuery {
    directory: Option<String>,
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionMessageRequest {
    parts: Option<Vec<Value>>,
    #[serde(rename = "messageID")]
    message_id: Option<String>,
    agent: Option<String>,
    model: Option<Value>,
    system: Option<String>,
    variant: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionCommandRequest {
    command: Option<String>,
    arguments: Option<String>,
    #[serde(rename = "messageID")]
    message_id: Option<String>,
    agent: Option<String>,
    model: Option<String>,
    variant: Option<String>,
    parts: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionShellRequest {
    command: Option<String>,
    agent: Option<String>,
    model: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionSummarizeRequest {
    #[serde(rename = "providerID")]
    provider_id: Option<String>,
    #[serde(rename = "modelID")]
    model_id: Option<String>,
    auto: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PermissionReplyRequest {
    response: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PermissionGlobalReplyRequest {
    reply: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PtyCreateRequest {
    command: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    title: Option<String>,
}

fn next_id(prefix: &str, counter: &AtomicU64) -> String {
    let id = counter.fetch_add(1, Ordering::Relaxed);
    format!("{}{}", prefix, id)
}

fn bad_request(message: &str) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "data": {},
            "errors": [{"message": message}],
            "success": false,
        })),
    )
}

fn not_found(message: &str) -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "name": "NotFoundError",
            "data": {"message": message},
        })),
    )
}

fn bool_ok(value: bool) -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(json!(value)))
}

fn build_user_message(
    session_id: &str,
    message_id: &str,
    created_at: i64,
    agent: &str,
    provider_id: &str,
    model_id: &str,
) -> Value {
    json!({
        "id": message_id,
        "sessionID": session_id,
        "role": "user",
        "time": {"created": created_at},
        "agent": agent,
        "model": {"providerID": provider_id, "modelID": model_id},
    })
}

fn build_assistant_message(
    session_id: &str,
    message_id: &str,
    parent_id: &str,
    created_at: i64,
    directory: &str,
    worktree: &str,
    agent: &str,
    provider_id: &str,
    model_id: &str,
) -> Value {
    json!({
        "id": message_id,
        "sessionID": session_id,
        "role": "assistant",
        "time": {"created": created_at},
        "parentID": parent_id,
        "modelID": model_id,
        "providerID": provider_id,
        "mode": "default",
        "agent": agent,
        "path": {"cwd": directory, "root": worktree},
        "cost": 0,
        "finish": "stop",
        "tokens": {
            "input": 0,
            "output": 0,
            "reasoning": 0,
            "cache": {"read": 0, "write": 0}
        }
    })
}

fn build_text_part(session_id: &str, message_id: &str, text: &str) -> Value {
    json!({
        "id": next_id("part_", &PART_COUNTER),
        "sessionID": session_id,
        "messageID": message_id,
        "type": "text",
        "text": text,
    })
}

fn part_id_from_input(input: &Value) -> String {
    input
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .unwrap_or_else(|| next_id("part_", &PART_COUNTER))
}

fn build_file_part(session_id: &str, message_id: &str, input: &Value) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("id".to_string(), json!(part_id_from_input(input)));
    map.insert("sessionID".to_string(), json!(session_id));
    map.insert("messageID".to_string(), json!(message_id));
    map.insert("type".to_string(), json!("file"));
    map.insert(
        "mime".to_string(),
        input
            .get("mime")
            .cloned()
            .unwrap_or_else(|| json!("application/octet-stream")),
    );
    map.insert(
        "url".to_string(),
        input.get("url").cloned().unwrap_or_else(|| json!("")),
    );
    if let Some(filename) = input.get("filename") {
        map.insert("filename".to_string(), filename.clone());
    }
    if let Some(source) = input.get("source") {
        map.insert("source".to_string(), source.clone());
    }
    Value::Object(map)
}

fn build_agent_part(session_id: &str, message_id: &str, input: &Value) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("id".to_string(), json!(part_id_from_input(input)));
    map.insert("sessionID".to_string(), json!(session_id));
    map.insert("messageID".to_string(), json!(message_id));
    map.insert("type".to_string(), json!("agent"));
    map.insert(
        "name".to_string(),
        input.get("name").cloned().unwrap_or_else(|| json!("")),
    );
    if let Some(source) = input.get("source") {
        map.insert("source".to_string(), source.clone());
    }
    Value::Object(map)
}

fn build_subtask_part(session_id: &str, message_id: &str, input: &Value) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("id".to_string(), json!(part_id_from_input(input)));
    map.insert("sessionID".to_string(), json!(session_id));
    map.insert("messageID".to_string(), json!(message_id));
    map.insert("type".to_string(), json!("subtask"));
    map.insert(
        "prompt".to_string(),
        input.get("prompt").cloned().unwrap_or_else(|| json!("")),
    );
    map.insert(
        "description".to_string(),
        input
            .get("description")
            .cloned()
            .unwrap_or_else(|| json!("")),
    );
    map.insert(
        "agent".to_string(),
        input.get("agent").cloned().unwrap_or_else(|| json!("")),
    );
    if let Some(model) = input.get("model") {
        map.insert("model".to_string(), model.clone());
    }
    if let Some(command) = input.get("command") {
        map.insert("command".to_string(), command.clone());
    }
    Value::Object(map)
}

fn normalize_part(session_id: &str, message_id: &str, input: &Value) -> Value {
    match input.get("type").and_then(|v| v.as_str()) {
        Some("file") => build_file_part(session_id, message_id, input),
        Some("agent") => build_agent_part(session_id, message_id, input),
        Some("subtask") => build_subtask_part(session_id, message_id, input),
        _ => build_text_part(
            session_id,
            message_id,
            input
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim(),
        ),
    }
}

fn message_id_for_sequence(sequence: u64) -> String {
    format!("msg_{:020}", sequence)
}

fn unique_assistant_message_id(
    runtime: &OpenCodeSessionRuntime,
    parent_id: Option<&String>,
    sequence: u64,
) -> String {
    let base = match parent_id {
        Some(parent) => format!("{parent}_assistant"),
        None => message_id_for_sequence(sequence),
    };
    if runtime.message_id_for_item.values().any(|id| id == &base) {
        format!("{base}_{:020}", sequence)
    } else {
        base
    }
}


fn extract_text_from_content(parts: &[ContentPart]) -> Option<String> {
    let mut text = String::new();
    for part in parts {
        match part {
            ContentPart::Text { text: chunk } => {
                text.push_str(chunk);
            }
            ContentPart::Json { json } => {
                if let Ok(chunk) = serde_json::to_string(json) {
                    text.push_str(&chunk);
                }
            }
            ContentPart::Status { label, detail } => {
                text.push_str(label);
                if let Some(detail) = detail {
                    if !detail.is_empty() {
                        text.push_str(": ");
                        text.push_str(detail);
                    }
                }
            }
            _ => {}
        }
    }
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn build_text_part_with_id(session_id: &str, message_id: &str, part_id: &str, text: &str) -> Value {
    json!({
        "id": part_id,
        "sessionID": session_id,
        "messageID": message_id,
        "type": "text",
        "text": text,
    })
}

fn build_reasoning_part(
    session_id: &str,
    message_id: &str,
    part_id: &str,
    text: &str,
    now: i64,
) -> Value {
    json!({
        "id": part_id,
        "sessionID": session_id,
        "messageID": message_id,
        "type": "reasoning",
        "text": text,
        "metadata": {},
        "time": {"start": now, "end": now},
    })
}

fn build_tool_part(
    session_id: &str,
    message_id: &str,
    part_id: &str,
    call_id: &str,
    tool: &str,
    state: Value,
) -> Value {
    json!({
        "id": part_id,
        "sessionID": session_id,
        "messageID": message_id,
        "type": "tool",
        "callID": call_id,
        "tool": tool,
        "state": state,
        "metadata": {},
    })
}

fn session_event(event_type: &str, session: &Value) -> Value {
    json!({
        "type": event_type,
        "properties": {"info": session}
    })
}

fn message_event(event_type: &str, message: &Value) -> Value {
    let session_id = message
        .get("sessionID")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let mut props = serde_json::Map::new();
    props.insert("info".to_string(), message.clone());
    if let Some(session_id) = session_id {
        props.insert("sessionID".to_string(), json!(session_id));
    }
    Value::Object({
        let mut map = serde_json::Map::new();
        map.insert("type".to_string(), json!(event_type));
        map.insert("properties".to_string(), Value::Object(props));
        map
    })
}

fn part_event_with_delta(event_type: &str, part: &Value, delta: Option<&str>) -> Value {
    let mut props = serde_json::Map::new();
    props.insert("part".to_string(), part.clone());
    if let Some(session_id) = part.get("sessionID").and_then(|v| v.as_str()) {
        props.insert("sessionID".to_string(), json!(session_id));
    }
    if let Some(message_id) = part.get("messageID").and_then(|v| v.as_str()) {
        props.insert("messageID".to_string(), json!(message_id));
    }
    if let Some(delta) = delta {
        props.insert("delta".to_string(), json!(delta));
    }
    Value::Object({
        let mut map = serde_json::Map::new();
        map.insert("type".to_string(), json!(event_type));
        map.insert("properties".to_string(), Value::Object(props));
        map
    })
}

fn part_event(event_type: &str, part: &Value) -> Value {
    part_event_with_delta(event_type, part, None)
}

fn permission_event(event_type: &str, permission: &Value) -> Value {
    json!({
        "type": event_type,
        "properties": {"request": permission}
    })
}

fn message_id_from_info(info: &Value) -> Option<String> {
    info.get("id").and_then(|v| v.as_str()).map(|v| v.to_string())
}

async fn upsert_message_info(
    state: &OpenCodeState,
    session_id: &str,
    info: Value,
) -> Vec<Value> {
    let mut messages = state.messages.lock().await;
    let entry = messages.entry(session_id.to_string()).or_default();
    let message_id = message_id_from_info(&info);
    if let Some(message_id) = message_id.clone() {
        if let Some(existing) = entry
            .iter_mut()
            .find(|record| message_id_from_info(&record.info).as_deref() == Some(message_id.as_str()))
        {
            existing.info = info.clone();
        } else {
            entry.push(OpenCodeMessageRecord {
                info: info.clone(),
                parts: Vec::new(),
            });
        }
        entry.sort_by(|a, b| {
            let a_id = message_id_from_info(&a.info).unwrap_or_default();
            let b_id = message_id_from_info(&b.info).unwrap_or_default();
            a_id.cmp(&b_id)
        });
    }
    entry.iter().map(|record| record.info.clone()).collect()
}

async fn upsert_message_part(
    state: &OpenCodeState,
    session_id: &str,
    message_id: &str,
    part: Value,
) {
    let mut messages = state.messages.lock().await;
    let entry = messages.entry(session_id.to_string()).or_default();
    let record = if let Some(record) = entry
        .iter_mut()
        .find(|record| message_id_from_info(&record.info).as_deref() == Some(message_id))
    {
        record
    } else {
        entry.push(OpenCodeMessageRecord {
            info: json!({"id": message_id, "sessionID": session_id, "role": "assistant", "time": {"created": 0}}),
            parts: Vec::new(),
        });
        entry.last_mut().expect("record just inserted")
    };

    let part_id = part.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if let Some(existing) = record
        .parts
        .iter_mut()
        .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(part_id))
    {
        *existing = part;
    } else {
        record.parts.push(part);
    }
    record.parts.sort_by(|a, b| {
        let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
        a_id.cmp(b_id)
    });
}

async fn session_directory(state: &OpenCodeState, session_id: &str) -> String {
    let sessions = state.sessions.lock().await;
    if let Some(session) = sessions.get(session_id) {
        return session.directory.clone();
    }
    std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(|v| v.to_string()))
        .unwrap_or_else(|| ".".to_string())
}

async fn apply_universal_event(state: Arc<OpenCodeAppState>, event: UniversalEvent) {
    match event.event_type {
        UniversalEventType::ItemStarted | UniversalEventType::ItemCompleted => {
            if let UniversalEventData::Item(ItemEventData { item }) = &event.data {
                apply_item_event(state, event.clone(), item.clone()).await;
            }
        }
        UniversalEventType::ItemDelta => {
            if let UniversalEventData::ItemDelta(ItemDeltaData {
                item_id,
                native_item_id,
                delta,
            }) = &event.data
            {
                apply_item_delta(
                    state,
                    event.clone(),
                    item_id.clone(),
                    native_item_id.clone(),
                    delta.clone(),
                )
                .await;
            }
        }
        UniversalEventType::SessionEnded => {
            let session_id = event.session_id.clone();
            state.opencode.emit_event(json!({
                "type": "session.status",
                "properties": {"sessionID": session_id, "status": {"type": "idle"}}
            }));
            state.opencode.emit_event(json!({
                "type": "session.idle",
                "properties": {"sessionID": event.session_id}
            }));
        }
        _ => {}
    }
}

async fn apply_item_event(
    state: Arc<OpenCodeAppState>,
    event: UniversalEvent,
    item: UniversalItem,
) {
    if item.kind != ItemKind::Message {
        return;
    }
    if matches!(item.role, Some(ItemRole::User)) {
        return;
    }
    let session_id = event.session_id.clone();
    let item_id_key = if item.item_id.is_empty() {
        None
    } else {
        Some(item.item_id.clone())
    };
    let native_id_key = item.native_item_id.clone();
    let mut message_id: Option<String> = None;
    let mut parent_id: Option<String> = None;
    let runtime = state
        .opencode
        .update_runtime(&session_id, |runtime| {
            parent_id = item
                .parent_id
                .as_ref()
                .and_then(|parent| runtime.message_id_for_item.get(parent).cloned())
                .or_else(|| runtime.last_user_message_id.clone());
            if let Some(existing) = item_id_key
                .clone()
                .and_then(|key| runtime.message_id_for_item.get(&key).cloned())
                .or_else(|| {
                    native_id_key
                        .clone()
                        .and_then(|key| runtime.message_id_for_item.get(&key).cloned())
                })
            {
                message_id = Some(existing);
            } else {
                let new_id =
                    unique_assistant_message_id(runtime, parent_id.as_ref(), event.sequence);
                message_id = Some(new_id);
            }
            if let Some(id) = message_id.clone() {
                if let Some(item_key) = item_id_key.clone() {
                    runtime
                        .message_id_for_item
                        .insert(item_key, id.clone());
                }
                if let Some(native_key) = native_id_key.clone() {
                    runtime
                        .message_id_for_item
                        .insert(native_key, id.clone());
                }
            }
        })
        .await;
    let message_id = message_id
        .unwrap_or_else(|| unique_assistant_message_id(&runtime, parent_id.as_ref(), event.sequence));
    let parent_id = parent_id.or_else(|| runtime.last_user_message_id.clone());
    let agent = runtime
        .last_agent
        .clone()
        .unwrap_or_else(|| state.opencode.default_agent());
    let provider_id = runtime
        .last_model_provider
        .clone()
        .unwrap_or_else(|| "openai".to_string());
    let model_id = runtime
        .last_model_id
        .clone()
        .unwrap_or_else(|| "gpt-4o".to_string());
    let directory = session_directory(&state.opencode, &session_id).await;
    let worktree = state.opencode.worktree_for(&directory);
    let now = state.opencode.now_ms();

    let mut info = build_assistant_message(
        &session_id,
        &message_id,
        parent_id.as_deref().unwrap_or(""),
        now,
        &directory,
        &worktree,
        &agent,
        &provider_id,
        &model_id,
    );
    if event.event_type == UniversalEventType::ItemCompleted {
        if let Some(obj) = info.as_object_mut() {
            if let Some(time) = obj.get_mut("time").and_then(|v| v.as_object_mut()) {
                time.insert("completed".to_string(), json!(now));
            }
        }
    }
    upsert_message_info(&state.opencode, &session_id, info.clone()).await;
    state
        .opencode
        .emit_event(message_event("message.updated", &info));

    let mut runtime = state
        .opencode
        .update_runtime(&session_id, |runtime| {
            if runtime.last_user_message_id.is_none() {
                runtime.last_user_message_id = parent_id.clone();
            }
        })
        .await;

    if let Some(text) = extract_text_from_content(&item.content) {
        let part_id = runtime
            .part_id_by_message
            .entry(message_id.clone())
            .or_insert_with(|| format!("{}_text", message_id))
            .clone();
        runtime.text_by_message.insert(message_id.clone(), text.clone());
        let part = build_text_part_with_id(&session_id, &message_id, &part_id, &text);
        upsert_message_part(&state.opencode, &session_id, &message_id, part.clone()).await;
        state
            .opencode
            .emit_event(part_event("message.part.updated", &part));
        let _ = state
            .opencode
            .update_runtime(&session_id, |runtime| {
                runtime
                    .text_by_message
                    .insert(message_id.clone(), text.clone());
                runtime
                    .part_id_by_message
                    .insert(message_id.clone(), part_id.clone());
            })
            .await;
    }

    for part in item.content.iter() {
        match part {
            ContentPart::Reasoning { text, .. } => {
                let part_id = next_id("part_", &PART_COUNTER);
                let reasoning_part =
                    build_reasoning_part(&session_id, &message_id, &part_id, text, now);
                upsert_message_part(&state.opencode, &session_id, &message_id, reasoning_part.clone())
                    .await;
                state
                    .opencode
                    .emit_event(part_event("message.part.updated", &reasoning_part));
            }
            ContentPart::ToolCall {
                name,
                arguments,
                call_id,
            } => {
                let part_id = runtime
                    .tool_part_by_call
                    .entry(call_id.clone())
                    .or_insert_with(|| next_id("part_", &PART_COUNTER))
                    .clone();
                let state_value = json!({
                    "status": "pending",
                    "input": {"arguments": arguments},
                    "raw": arguments,
                });
                let tool_part =
                    build_tool_part(&session_id, &message_id, &part_id, call_id, name, state_value);
                upsert_message_part(&state.opencode, &session_id, &message_id, tool_part.clone())
                    .await;
                state
                    .opencode
                    .emit_event(part_event("message.part.updated", &tool_part));
                let _ = state
                    .opencode
                    .update_runtime(&session_id, |runtime| {
                        runtime
                            .tool_part_by_call
                            .insert(call_id.clone(), part_id.clone());
                    })
                    .await;
            }
            ContentPart::ToolResult { call_id, output } => {
                let part_id = runtime
                    .tool_part_by_call
                    .entry(call_id.clone())
                    .or_insert_with(|| next_id("part_", &PART_COUNTER))
                    .clone();
                let state_value = json!({
                    "status": "completed",
                    "input": {},
                    "output": output,
                    "title": "Tool result",
                    "metadata": {},
                    "time": {"start": now, "end": now},
                    "attachments": [],
                });
                let tool_part = build_tool_part(
                    &session_id,
                    &message_id,
                    &part_id,
                    call_id,
                    "tool",
                    state_value,
                );
                upsert_message_part(&state.opencode, &session_id, &message_id, tool_part.clone())
                    .await;
                state
                    .opencode
                    .emit_event(part_event("message.part.updated", &tool_part));
                let _ = state
                    .opencode
                    .update_runtime(&session_id, |runtime| {
                        runtime
                            .tool_part_by_call
                            .insert(call_id.clone(), part_id.clone());
                    })
                    .await;
            }
            _ => {}
        }
    }

    if event.event_type == UniversalEventType::ItemCompleted {
        state.opencode.emit_event(json!({
            "type": "session.status",
            "properties": {
                "sessionID": session_id,
                "status": {"type": "idle"}
            }
        }));
        state.opencode.emit_event(json!({
            "type": "session.idle",
            "properties": { "sessionID": session_id }
        }));
    }
}

async fn apply_item_delta(
    state: Arc<OpenCodeAppState>,
    event: UniversalEvent,
    item_id: String,
    native_item_id: Option<String>,
    delta: String,
) {
    let session_id = event.session_id.clone();
    let item_id_key = if item_id.is_empty() { None } else { Some(item_id) };
    let native_id_key = native_item_id;
    let is_user_delta = item_id_key
        .as_ref()
        .map(|value| value.starts_with("user_"))
        .unwrap_or(false)
        || native_id_key
            .as_ref()
            .map(|value| value.starts_with("user_"))
            .unwrap_or(false);
    if is_user_delta {
        return;
    }
    let mut message_id: Option<String> = None;
    let mut parent_id: Option<String> = None;
    let runtime = state
        .opencode
        .update_runtime(&session_id, |runtime| {
            parent_id = runtime.last_user_message_id.clone();
            if let Some(existing) = item_id_key
                .clone()
                .and_then(|key| runtime.message_id_for_item.get(&key).cloned())
                .or_else(|| {
                    native_id_key
                        .clone()
                        .and_then(|key| runtime.message_id_for_item.get(&key).cloned())
                })
            {
                message_id = Some(existing);
            } else {
                let new_id =
                    unique_assistant_message_id(runtime, parent_id.as_ref(), event.sequence);
                message_id = Some(new_id);
            }
            if let Some(id) = message_id.clone() {
                if let Some(item_key) = item_id_key.clone() {
                    runtime
                        .message_id_for_item
                        .insert(item_key, id.clone());
                }
                if let Some(native_key) = native_id_key.clone() {
                    runtime
                        .message_id_for_item
                        .insert(native_key, id.clone());
                }
            }
        })
        .await;
    let message_id = message_id
        .unwrap_or_else(|| unique_assistant_message_id(&runtime, parent_id.as_ref(), event.sequence));
    let parent_id = parent_id.or_else(|| runtime.last_user_message_id.clone());
    let directory = session_directory(&state.opencode, &session_id).await;
    let worktree = state.opencode.worktree_for(&directory);
    let now = state.opencode.now_ms();
    let agent = runtime
        .last_agent
        .clone()
        .unwrap_or_else(|| state.opencode.default_agent());
    let provider_id = runtime
        .last_model_provider
        .clone()
        .unwrap_or_else(|| "openai".to_string());
    let model_id = runtime
        .last_model_id
        .clone()
        .unwrap_or_else(|| "gpt-4o".to_string());
    let info = build_assistant_message(
        &session_id,
        &message_id,
        parent_id.as_deref().unwrap_or(""),
        now,
        &directory,
        &worktree,
        &agent,
        &provider_id,
        &model_id,
    );
    upsert_message_info(&state.opencode, &session_id, info.clone()).await;
    state
        .opencode
        .emit_event(message_event("message.updated", &info));
    let mut text = runtime
        .text_by_message
        .get(&message_id)
        .cloned()
        .unwrap_or_default();
    text.push_str(&delta);
    let part_id = runtime
        .part_id_by_message
        .get(&message_id)
        .cloned()
        .unwrap_or_else(|| format!("{}_text", message_id));
    let part = build_text_part_with_id(&session_id, &message_id, &part_id, &text);
    upsert_message_part(&state.opencode, &session_id, &message_id, part.clone()).await;
    state
        .opencode
        .emit_event(part_event("message.part.updated", &part));
    let _ = state
        .opencode
        .update_runtime(&session_id, |runtime| {
            runtime.text_by_message.insert(message_id.clone(), text);
            runtime
                .part_id_by_message
                .insert(message_id.clone(), part_id.clone());
        })
        .await;
}

/// Build OpenCode-compatible router.
pub fn build_opencode_router(state: Arc<OpenCodeAppState>) -> Router {
    Router::new()
        // Core metadata
        .route("/agent", get(oc_agent_list))
        .route("/command", get(oc_command_list))
        .route("/config", get(oc_config_get).patch(oc_config_patch))
        .route("/config/providers", get(oc_config_providers))
        .route("/event", get(oc_event_subscribe))
        .route("/global/event", get(oc_global_event))
        .route("/global/health", get(oc_global_health))
        .route("/global/config", get(oc_global_config_get).patch(oc_global_config_patch))
        .route("/global/dispose", post(oc_global_dispose))
        .route("/instance/dispose", post(oc_instance_dispose))
        .route("/log", post(oc_log))
        .route("/lsp", get(oc_lsp_status))
        .route("/formatter", get(oc_formatter_status))
        .route("/path", get(oc_path))
        .route("/vcs", get(oc_vcs))
        .route("/project", get(oc_project_list))
        .route("/project/current", get(oc_project_current))
        .route("/project/:projectID", patch(oc_project_update))
        // Sessions
        .route("/session", post(oc_session_create).get(oc_session_list))
        .route("/session/status", get(oc_session_status))
        .route(
            "/session/:sessionID",
            get(oc_session_get)
                .patch(oc_session_update)
                .delete(oc_session_delete),
        )
        .route("/session/:sessionID/abort", post(oc_session_abort))
        .route("/session/:sessionID/children", get(oc_session_children))
        .route("/session/:sessionID/init", post(oc_session_init))
        .route("/session/:sessionID/fork", post(oc_session_fork))
        .route("/session/:sessionID/diff", get(oc_session_diff))
        .route("/session/:sessionID/summarize", post(oc_session_summarize))
        .route(
            "/session/:sessionID/message",
            post(oc_session_message_create).get(oc_session_messages),
        )
        .route(
            "/session/:sessionID/message/:messageID",
            get(oc_session_message_get),
        )
        .route(
            "/session/:sessionID/message/:messageID/part/:partID",
            patch(oc_message_part_update).delete(oc_message_part_delete),
        )
        .route("/session/:sessionID/prompt_async", post(oc_session_prompt_async))
        .route("/session/:sessionID/command", post(oc_session_command))
        .route("/session/:sessionID/shell", post(oc_session_shell))
        .route("/session/:sessionID/revert", post(oc_session_revert))
        .route("/session/:sessionID/unrevert", post(oc_session_unrevert))
        .route(
            "/session/:sessionID/permissions/:permissionID",
            post(oc_session_permission_reply),
        )
        .route("/session/:sessionID/share", post(oc_session_share).delete(oc_session_unshare))
        .route("/session/:sessionID/todo", get(oc_session_todo))
        // Permissions + questions (global)
        .route("/permission", get(oc_permission_list))
        .route("/permission/:requestID/reply", post(oc_permission_reply))
        .route("/question", get(oc_question_list))
        .route("/question/:requestID/reply", post(oc_question_reply))
        .route("/question/:requestID/reject", post(oc_question_reject))
        // Providers
        .route("/provider", get(oc_provider_list))
        .route("/provider/auth", get(oc_provider_auth))
        .route(
            "/provider/:providerID/oauth/authorize",
            post(oc_provider_oauth_authorize),
        )
        .route(
            "/provider/:providerID/oauth/callback",
            post(oc_provider_oauth_callback),
        )
        // Auth
        .route("/auth/:providerID", put(oc_auth_set).delete(oc_auth_remove))
        // PTY
        .route("/pty", get(oc_pty_list).post(oc_pty_create))
        .route(
            "/pty/:ptyID",
            get(oc_pty_get).put(oc_pty_update).delete(oc_pty_delete),
        )
        .route("/pty/:ptyID/connect", get(oc_pty_connect))
        // Files
        .route("/file", get(oc_file_list))
        .route("/file/content", get(oc_file_content))
        .route("/file/status", get(oc_file_status))
        // Find
        .route("/find", get(oc_find_text))
        .route("/find/file", get(oc_find_files))
        .route("/find/symbol", get(oc_find_symbols))
        // MCP
        .route("/mcp", get(oc_mcp_list).post(oc_mcp_register))
        .route("/mcp/:name/auth", post(oc_mcp_auth).delete(oc_mcp_auth_remove))
        .route("/mcp/:name/auth/callback", post(oc_mcp_auth_callback))
        .route("/mcp/:name/auth/authenticate", post(oc_mcp_authenticate))
        .route("/mcp/:name/connect", post(oc_mcp_connect))
        .route("/mcp/:name/disconnect", post(oc_mcp_disconnect))
        // Experimental
        .route("/experimental/tool/ids", get(oc_tool_ids))
        .route("/experimental/tool", get(oc_tool_list))
        .route("/experimental/resource", get(oc_resource_list))
        .route(
            "/experimental/worktree",
            get(oc_worktree_list).post(oc_worktree_create).delete(oc_worktree_delete),
        )
        .route("/experimental/worktree/reset", post(oc_worktree_reset))
        // Skills
        .route("/skill", get(oc_skill_list))
        // TUI
        .route("/tui/control/next", get(oc_tui_next))
        .route("/tui/control/response", post(oc_tui_response))
        .route("/tui/append-prompt", post(oc_tui_append_prompt))
        .route("/tui/open-help", post(oc_tui_open_help))
        .route("/tui/open-sessions", post(oc_tui_open_sessions))
        .route("/tui/open-themes", post(oc_tui_open_themes))
        .route("/tui/open-models", post(oc_tui_open_models))
        .route("/tui/submit-prompt", post(oc_tui_submit_prompt))
        .route("/tui/clear-prompt", post(oc_tui_clear_prompt))
        .route("/tui/execute-command", post(oc_tui_execute_command))
        .route("/tui/show-toast", post(oc_tui_show_toast))
        .route("/tui/publish", post(oc_tui_publish))
        .route("/tui/select-session", post(oc_tui_select_session))
        .with_state(state)
}

// ===================================================================================
// Handler implementations
// ===================================================================================

async fn oc_agent_list(State(state): State<Arc<OpenCodeAppState>>) -> impl IntoResponse {
    let agent_name = state.opencode.default_agent();
    let agent = json!({
        "name": agent_name,
        "description": "OpenCode compatibility stub",
        "mode": "all",
        "native": false,
        "hidden": false,
        "permission": [],
        "options": {},
    });
    (StatusCode::OK, Json(json!([agent])))
}

async fn oc_command_list() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn oc_config_get() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({})))
}

async fn oc_config_patch(Json(body): Json<Value>) -> impl IntoResponse {
    (StatusCode::OK, Json(body))
}

async fn oc_config_providers() -> impl IntoResponse {
    let providers = json!({
        "providers": [
            {
                "id": "openai",
                "name": "OpenAI",
                "source": "api",
                "env": ["OPENAI_API_KEY", "CODEX_API_KEY"],
                "key": "stub",
                "options": {},
                "models": {
                    "gpt-4o": {
                        "id": "gpt-4o",
                        "providerID": "openai",
                        "api": {
                            "id": "openai",
                            "url": "https://api.openai.com/v1",
                            "npm": "openai"
                        },
                        "name": "gpt-4o",
                        "capabilities": {
                            "temperature": true,
                            "reasoning": true,
                            "attachment": false,
                            "toolcall": true,
                            "input": {
                                "text": true,
                                "audio": false,
                                "image": false,
                                "video": false,
                                "pdf": false
                            },
                            "output": {
                                "text": true,
                                "audio": false,
                                "image": false,
                                "video": false,
                                "pdf": false
                            },
                            "interleaved": false
                        },
                        "cost": {
                            "input": 0,
                            "output": 0,
                            "cache": {
                                "read": 0,
                                "write": 0
                            }
                        },
                        "limit": {
                            "context": 128000,
                            "output": 4096
                        },
                        "status": "active",
                        "options": {},
                        "headers": {},
                        "release_date": "2024-05-13",
                        "variants": {}
                    }
                }
            }
        ],
        "default": {
            "openai": "gpt-4o"
        }
    });
    (StatusCode::OK, Json(providers))
}

async fn oc_event_subscribe(
    State(state): State<Arc<OpenCodeAppState>>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let receiver = state.opencode.subscribe();
    let directory = state.opencode.directory_for(&headers, query.directory.as_ref());
    let branch = state.opencode.branch_name();
    state.opencode.emit_event(json!({
        "type": "server.connected",
        "properties": {}
    }));
    state.opencode.emit_event(json!({
        "type": "worktree.ready",
        "properties": {
            "name": directory,
            "branch": branch,
        }
    }));

    let heartbeat_payload = json!({
        "type": "server.heartbeat",
        "properties": {}
    });
    let stream = stream::unfold((receiver, interval(std::time::Duration::from_secs(30))), move |(mut rx, mut ticker)| {
        let heartbeat = heartbeat_payload.clone();
        async move {
            tokio::select! {
                _ = ticker.tick() => {
                    let sse_event = Event::default()
                        .json_data(&heartbeat)
                        .unwrap_or_else(|_| Event::default().data("{}"));
                    Some((Ok(sse_event), (rx, ticker)))
                }
                event = rx.recv() => {
                    match event {
                        Ok(event) => {
                            let sse_event = Event::default()
                                .json_data(&event)
                                .unwrap_or_else(|_| Event::default().data("{}"));
                            Some((Ok(sse_event), (rx, ticker)))
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            Some((Ok(Event::default().comment("lagged")), (rx, ticker)))
                        }
                        Err(broadcast::error::RecvError::Closed) => None,
                    }
                }
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

async fn oc_global_event(
    State(state): State<Arc<OpenCodeAppState>>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let receiver = state.opencode.subscribe();
    let directory = state.opencode.directory_for(&headers, query.directory.as_ref());
    let branch = state.opencode.branch_name();
    state.opencode.emit_event(json!({
        "type": "server.connected",
        "properties": {}
    }));
    state.opencode.emit_event(json!({
        "type": "worktree.ready",
        "properties": {
            "name": directory.clone(),
            "branch": branch,
        }
    }));

    let heartbeat_payload = json!({
        "payload": {
            "type": "server.heartbeat",
            "properties": {}
        }
    });
    let stream = stream::unfold((receiver, interval(std::time::Duration::from_secs(30))), move |(mut rx, mut ticker)| {
        let directory = directory.clone();
        let heartbeat = heartbeat_payload.clone();
        async move {
            tokio::select! {
                _ = ticker.tick() => {
                    let sse_event = Event::default()
                        .json_data(&heartbeat)
                        .unwrap_or_else(|_| Event::default().data("{}"));
                    Some((Ok(sse_event), (rx, ticker)))
                }
                event = rx.recv() => {
                    match event {
                        Ok(event) => {
                            let payload = json!({"directory": directory, "payload": event});
                            let sse_event = Event::default()
                                .json_data(&payload)
                                .unwrap_or_else(|_| Event::default().data("{}"));
                            Some((Ok(sse_event), (rx, ticker)))
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            Some((Ok(Event::default().comment("lagged")), (rx, ticker)))
                        }
                        Err(broadcast::error::RecvError::Closed) => None,
                    }
                }
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

async fn oc_global_health() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "healthy": true,
            "version": env!("CARGO_PKG_VERSION"),
        })),
    )
}

async fn oc_global_config_get() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({})))
}

async fn oc_global_config_patch(Json(body): Json<Value>) -> impl IntoResponse {
    (StatusCode::OK, Json(body))
}

async fn oc_global_dispose() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_instance_dispose() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_log() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_lsp_status() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn oc_formatter_status() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn oc_path(State(state): State<Arc<OpenCodeAppState>>, headers: HeaderMap) -> impl IntoResponse {
    let directory = state.opencode.directory_for(&headers, None);
    let worktree = state.opencode.worktree_for(&directory);
    (
        StatusCode::OK,
        Json(json!({
            "home": state.opencode.home_dir(),
            "state": state.opencode.state_dir(),
            "config": state.opencode.config_dir(),
            "worktree": worktree,
            "directory": directory,
        })),
    )
}

async fn oc_vcs(State(state): State<Arc<OpenCodeAppState>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "branch": state.opencode.branch_name(),
        })),
    )
}

async fn oc_project_list(State(state): State<Arc<OpenCodeAppState>>, headers: HeaderMap) -> impl IntoResponse {
    let directory = state.opencode.directory_for(&headers, None);
    let worktree = state.opencode.worktree_for(&directory);
    let now = state.opencode.now_ms();
    let project = json!({
        "id": state.opencode.default_project_id.clone(),
        "worktree": worktree,
        "vcs": "git",
        "name": "sandbox-agent",
        "time": {"created": now, "updated": now},
        "sandboxes": [],
    });
    (StatusCode::OK, Json(json!([project])))
}

async fn oc_project_current(State(state): State<Arc<OpenCodeAppState>>, headers: HeaderMap) -> impl IntoResponse {
    let directory = state.opencode.directory_for(&headers, None);
    let worktree = state.opencode.worktree_for(&directory);
    let now = state.opencode.now_ms();
    (
        StatusCode::OK,
        Json(json!({
        "id": state.opencode.default_project_id.clone(),
        "worktree": worktree,
        "vcs": "git",
        "name": "sandbox-agent",
        "time": {"created": now, "updated": now},
        "sandboxes": [],
    })),
    )
}

async fn oc_project_update(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(_project_id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    oc_project_current(State(state), headers).await
}

async fn oc_session_create(
    State(state): State<Arc<OpenCodeAppState>>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
    body: Option<Json<OpenCodeCreateSessionRequest>>,
) -> impl IntoResponse {
    let body = body.map(|j| j.0).unwrap_or(OpenCodeCreateSessionRequest {
        title: None,
        parent_id: None,
        permission: None,
    });
    let directory = state.opencode.directory_for(&headers, query.directory.as_ref());
    let now = state.opencode.now_ms();
    let id = next_id("ses_", &SESSION_COUNTER);
    let slug = format!("session-{}", id);
    let title = body.title.unwrap_or_else(|| format!("Session {}", id));
    let record = OpenCodeSessionRecord {
        id: id.clone(),
        slug,
        project_id: state.opencode.default_project_id.clone(),
        directory,
        parent_id: body.parent_id,
        title,
        version: "0".to_string(),
        created_at: now,
        updated_at: now,
        share_url: None,
    };

    let session_value = record.to_value();

    let mut sessions = state.opencode.sessions.lock().await;
    sessions.insert(id.clone(), record);
    drop(sessions);

    let agent = state.opencode.default_agent();
    if let Err(err) = ensure_backing_session(&state, &id, &agent).await {
        tracing::warn!(
            target = "sandbox_agent::opencode",
            ?err,
            "failed to create backing session"
        );
    } else {
        ensure_session_stream(state.clone(), id.clone()).await;
    }

    state
        .opencode
        .emit_event(session_event("session.created", &session_value));

    (StatusCode::OK, Json(session_value))
}

async fn oc_session_list(State(state): State<Arc<OpenCodeAppState>>) -> impl IntoResponse {
    let sessions = state.opencode.sessions.lock().await;
    let values: Vec<Value> = sessions.values().map(|s| s.to_value()).collect();
    (StatusCode::OK, Json(json!(values)))
}

async fn oc_session_get(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
) -> impl IntoResponse {
    let directory = state.opencode.directory_for(&headers, query.directory.as_ref());
    let value = state.opencode.ensure_session(&session_id, directory).await;
    let agent = state.opencode.default_agent();
    if let Err(err) = ensure_backing_session(&state, &session_id, &agent).await {
        tracing::warn!(
            target = "sandbox_agent::opencode",
            ?err,
            "failed to ensure backing session"
        );
    } else {
        ensure_session_stream(state.clone(), session_id.clone()).await;
    }
    (StatusCode::OK, Json(value)).into_response()
}

async fn oc_session_update(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<OpenCodeUpdateSessionRequest>,
) -> impl IntoResponse {
    let mut sessions = state.opencode.sessions.lock().await;
    if let Some(session) = sessions.get_mut(&session_id) {
        if let Some(title) = body.title {
            session.title = title;
            session.updated_at = state.opencode.now_ms();
        }
        let value = session.to_value();
        state
            .opencode
            .emit_event(session_event("session.updated", &value));
        return (StatusCode::OK, Json(value)).into_response();
    }
    not_found("Session not found").into_response()
}

async fn oc_session_delete(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let mut sessions = state.opencode.sessions.lock().await;
    if let Some(session) = sessions.remove(&session_id) {
        state
            .opencode
            .emit_event(session_event("session.deleted", &session.to_value()));
        return bool_ok(true).into_response();
    }
    not_found("Session not found").into_response()
}

async fn oc_session_status(State(state): State<Arc<OpenCodeAppState>>) -> impl IntoResponse {
    let sessions = state.opencode.sessions.lock().await;
    let mut status_map = serde_json::Map::new();
    for id in sessions.keys() {
        status_map.insert(id.clone(), json!({"type": "idle"}));
    }
    (StatusCode::OK, Json(Value::Object(status_map)))
}

async fn oc_session_abort(
    State(_state): State<Arc<OpenCodeAppState>>,
    Path(_session_id): Path<String>,
) -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_session_children() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn oc_session_init() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_session_fork(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
) -> impl IntoResponse {
    let directory = state.opencode.directory_for(&headers, query.directory.as_ref());
    let now = state.opencode.now_ms();
    let id = next_id("ses_", &SESSION_COUNTER);
    let slug = format!("session-{}", id);
    let title = format!("Fork of {}", session_id);
    let record = OpenCodeSessionRecord {
        id: id.clone(),
        slug,
        project_id: state.opencode.default_project_id.clone(),
        directory,
        parent_id: Some(session_id),
        title,
        version: "0".to_string(),
        created_at: now,
        updated_at: now,
        share_url: None,
    };

    let value = record.to_value();
    let mut sessions = state.opencode.sessions.lock().await;
    sessions.insert(id.clone(), record);
    drop(sessions);

    state
        .opencode
        .emit_event(session_event("session.created", &value));

    (StatusCode::OK, Json(value))
}

async fn oc_session_diff() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn oc_session_summarize(
    Json(body): Json<SessionSummarizeRequest>,
) -> impl IntoResponse {
    if body.provider_id.is_none() || body.model_id.is_none() {
        return bad_request("providerID and modelID are required");
    }
    bool_ok(true)
}

async fn oc_session_messages(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let messages = state.opencode.messages.lock().await;
    let entries = messages.get(&session_id).cloned().unwrap_or_default();
    let values: Vec<Value> = entries
        .into_iter()
        .map(|record| json!({"info": record.info, "parts": record.parts}))
        .collect();
    (StatusCode::OK, Json(json!(values)))
}

async fn oc_session_message_create(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
    Json(body): Json<SessionMessageRequest>,
) -> impl IntoResponse {
    if std::env::var("OPENCODE_COMPAT_LOG_BODY").is_ok() {
        tracing::info!(target = "sandbox_agent::opencode", ?body, "opencode prompt body");
    }
    let directory = state.opencode.directory_for(&headers, query.directory.as_ref());
    let _ = state
        .opencode
        .ensure_session(&session_id, directory.clone())
        .await;
    let worktree = state.opencode.worktree_for(&directory);
    let agent = body
        .agent
        .clone()
        .unwrap_or_else(|| state.opencode.default_agent());
    let provider_id = body
        .model
        .as_ref()
        .and_then(|v| v.get("providerID"))
        .and_then(|v| v.as_str())
        .unwrap_or("openai");
    let model_id = body
        .model
        .as_ref()
        .and_then(|v| v.get("modelID"))
        .and_then(|v| v.as_str())
        .unwrap_or("gpt-4o");

    let parts_input = body.parts.unwrap_or_default();
    if parts_input.is_empty() {
        return bad_request("parts are required").into_response();
    }

    let now = state.opencode.now_ms();
    let user_message_id = body
        .message_id
        .clone()
        .unwrap_or_else(|| next_id("msg_", &MESSAGE_COUNTER));

    state.opencode.emit_event(json!({
        "type": "session.status",
        "properties": {
            "sessionID": session_id,
            "status": {"type": "busy"}
        }
    }));

    let mut user_message = build_user_message(
        &session_id,
        &user_message_id,
        now,
        &agent,
        provider_id,
        model_id,
    );
    if let Some(obj) = user_message.as_object_mut() {
        if let Some(time) = obj.get_mut("time").and_then(|v| v.as_object_mut()) {
            time.insert("completed".to_string(), json!(now));
        }
    }

    let parts: Vec<Value> = parts_input
        .iter()
        .map(|part| normalize_part(&session_id, &user_message_id, part))
        .collect();

    upsert_message_info(&state.opencode, &session_id, user_message.clone()).await;
    for part in &parts {
        upsert_message_part(&state.opencode, &session_id, &user_message_id, part.clone()).await;
    }

    state
        .opencode
        .emit_event(message_event("message.updated", &user_message));
    for part in &parts {
        state
            .opencode
            .emit_event(part_event("message.part.updated", part));
    }

    let _ = state
        .opencode
        .update_runtime(&session_id, |runtime| {
            runtime.last_user_message_id = Some(user_message_id.clone());
            runtime.last_agent = Some(agent.clone());
            runtime.last_model_provider = Some(provider_id.to_string());
            runtime.last_model_id = Some(model_id.to_string());
        })
        .await;

    if let Err(err) = ensure_backing_session(&state, &session_id, &agent).await {
        tracing::warn!(
            target = "sandbox_agent::opencode",
            ?err,
            "failed to ensure backing session"
        );
    } else {
        ensure_session_stream(state.clone(), session_id.clone()).await;
    }

    let prompt_text = parts_input
        .iter()
        .find_map(|part| part.get("text").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    if !prompt_text.is_empty() {
        if let Err(err) = state
            .inner
            .session_manager()
            .send_message(session_id.clone(), prompt_text)
            .await
        {
            tracing::warn!(
                target = "sandbox_agent::opencode",
                ?err,
                "failed to send message to backing agent"
            );
        }
    }

    let assistant_message = build_assistant_message(
        &session_id,
        &format!("{user_message_id}_pending"),
        &user_message_id,
        now,
        &directory,
        &worktree,
        &agent,
        provider_id,
        model_id,
    );

    (
        StatusCode::OK,
        Json(json!({
            "info": assistant_message,
            "parts": [],
        })),
    )
        .into_response()
}

async fn oc_session_message_get(
    State(state): State<Arc<OpenCodeAppState>>,
    Path((session_id, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let messages = state.opencode.messages.lock().await;
    if let Some(entries) = messages.get(&session_id) {
        if let Some(record) = entries.iter().find(|record| {
            record
                .info
                .get("id")
                .and_then(|v| v.as_str())
                .map(|id| id == message_id)
                .unwrap_or(false)
        }) {
            return (
                StatusCode::OK,
                Json(json!({
                    "info": record.info.clone(),
                    "parts": record.parts.clone()
                })),
            )
                .into_response();
        }
    }
    not_found("Message not found").into_response()
}

async fn oc_message_part_update(
    State(state): State<Arc<OpenCodeAppState>>,
    Path((session_id, message_id, part_id)): Path<(String, String, String)>,
    Json(mut part_value): Json<Value>,
) -> impl IntoResponse {
    if let Some(obj) = part_value.as_object_mut() {
        obj.insert("id".to_string(), json!(part_id));
        obj.insert("sessionID".to_string(), json!(session_id));
        obj.insert("messageID".to_string(), json!(message_id));
    }

    state
        .opencode
        .emit_event(part_event("message.part.updated", &part_value));

    (StatusCode::OK, Json(part_value))
}

async fn oc_message_part_delete(
    State(state): State<Arc<OpenCodeAppState>>,
    Path((session_id, message_id, part_id)): Path<(String, String, String)>,
) -> impl IntoResponse {
    let part_value = json!({
        "id": part_id,
        "sessionID": session_id,
        "messageID": message_id,
        "type": "text",
        "text": "",
    });
    state
        .opencode
        .emit_event(part_event("message.part.removed", &part_value));
    bool_ok(true)
}

async fn oc_session_prompt_async(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
    Json(body): Json<SessionMessageRequest>,
) -> impl IntoResponse {
    let _ = oc_session_message_create(
        State(state),
        Path(session_id),
        headers,
        Query(query),
        Json(body),
    )
    .await;
    StatusCode::NO_CONTENT
}

async fn oc_session_command(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
    Json(body): Json<SessionCommandRequest>,
) -> impl IntoResponse {
    if body.command.is_none() || body.arguments.is_none() {
        return bad_request("command and arguments are required").into_response();
    }
    let directory = state.opencode.directory_for(&headers, query.directory.as_ref());
    let worktree = state.opencode.worktree_for(&directory);
    let now = state.opencode.now_ms();
    let assistant_message_id = next_id("msg_", &MESSAGE_COUNTER);
    let agent = body
        .agent
        .clone()
        .unwrap_or_else(|| state.opencode.default_agent());
    let assistant_message = build_assistant_message(
        &session_id,
        &assistant_message_id,
        "msg_parent",
        now,
        &directory,
        &worktree,
        &agent,
        "openai",
        "gpt-4o",
    );

    (
        StatusCode::OK,
        Json(json!({
            "info": assistant_message,
            "parts": [],
        })),
    )
        .into_response()
}

async fn oc_session_shell(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
    Json(body): Json<SessionShellRequest>,
) -> impl IntoResponse {
    if body.command.is_none() || body.agent.is_none() {
        return bad_request("agent and command are required").into_response();
    }
    let directory = state.opencode.directory_for(&headers, query.directory.as_ref());
    let worktree = state.opencode.worktree_for(&directory);
    let now = state.opencode.now_ms();
    let assistant_message_id = next_id("msg_", &MESSAGE_COUNTER);
    let assistant_message = build_assistant_message(
        &session_id,
        &assistant_message_id,
        "msg_parent",
        now,
        &directory,
        &worktree,
        body.agent.as_deref().unwrap_or("opencode"),
        body.model
            .as_ref()
            .and_then(|v| v.get("providerID"))
            .and_then(|v| v.as_str())
            .unwrap_or("stub"),
        body.model
            .as_ref()
            .and_then(|v| v.get("modelID"))
            .and_then(|v| v.as_str())
            .unwrap_or("stub"),
    );
    (StatusCode::OK, Json(assistant_message)).into_response()
}

async fn oc_session_revert(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
) -> impl IntoResponse {
    oc_session_get(State(state), Path(session_id), headers, Query(query)).await
}

async fn oc_session_unrevert(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
) -> impl IntoResponse {
    oc_session_get(State(state), Path(session_id), headers, Query(query)).await
}

async fn oc_session_permission_reply(
    State(state): State<Arc<OpenCodeAppState>>,
    Path((session_id, permission_id)): Path<(String, String)>,
    Json(body): Json<PermissionReplyRequest>,
) -> impl IntoResponse {
    let permission = json!({
        "id": permission_id,
        "sessionID": session_id,
        "permission": body.response.unwrap_or_else(|| "once".to_string()),
        "patterns": [],
        "metadata": {},
        "always": [],
    });
    state
        .opencode
        .emit_event(permission_event("permission.replied", &permission));
    bool_ok(true)
}

async fn oc_session_share(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let mut sessions = state.opencode.sessions.lock().await;
    if let Some(session) = sessions.get_mut(&session_id) {
        session.share_url = Some(format!("https://share.local/{}", session_id));
        let value = session.to_value();
        return (StatusCode::OK, Json(value)).into_response();
    }
    not_found("Session not found").into_response()
}

async fn oc_session_unshare(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let mut sessions = state.opencode.sessions.lock().await;
    if let Some(session) = sessions.get_mut(&session_id) {
        session.share_url = None;
        let value = session.to_value();
        return (StatusCode::OK, Json(value)).into_response();
    }
    not_found("Session not found").into_response()
}

async fn oc_session_todo() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn oc_permission_list() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn oc_permission_reply(
    Path(request_id): Path<String>,
    Json(body): Json<PermissionGlobalReplyRequest>,
) -> impl IntoResponse {
    let permission = json!({
        "id": request_id,
        "sessionID": "ses_stub",
        "permission": body.reply.unwrap_or_else(|| "once".to_string()),
        "patterns": [],
        "metadata": {},
        "always": [],
    });
    let _ = permission;
    bool_ok(true)
}

async fn oc_question_list() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn oc_question_reply(Path(_request_id): Path<String>) -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_question_reject(Path(_request_id): Path<String>) -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_provider_list() -> impl IntoResponse {
    let providers = json!({
        "all": [
            {
                "id": "openai",
                "name": "OpenAI",
                "env": ["OPENAI_API_KEY", "CODEX_API_KEY"],
                "models": {
                    "gpt-4o": {
                        "id": "gpt-4o",
                        "name": "gpt-4o",
                        "release_date": "2024-05-13",
                        "attachment": false,
                        "reasoning": true,
                        "temperature": true,
                        "tool_call": true,
                        "options": {},
                        "limit": {
                            "context": 128000,
                            "output": 4096
                        }
                    }
                }
            }
        ],
        "default": {
            "openai": "gpt-4o"
        },
        "connected": ["openai"]
    });
    (StatusCode::OK, Json(providers))
}

async fn oc_provider_auth() -> impl IntoResponse {
    let auth = json!({
        "openai": [
            {"type": "api", "label": "API key"}
        ]
    });
    (StatusCode::OK, Json(auth))
}


async fn oc_provider_oauth_authorize(Path(provider_id): Path<String>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "url": format!("https://auth.local/{}/authorize", provider_id),
            "method": "auto",
            "instructions": "stub",
        })),
    )
}

async fn oc_provider_oauth_callback(Path(_provider_id): Path<String>) -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_auth_set(Path(_provider_id): Path<String>, Json(_body): Json<Value>) -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_auth_remove(Path(_provider_id): Path<String>) -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_pty_list(State(state): State<Arc<OpenCodeAppState>>) -> impl IntoResponse {
    let ptys = state.opencode.ptys.lock().await;
    let values: Vec<Value> = ptys.values().map(|p| p.to_value()).collect();
    (StatusCode::OK, Json(json!(values)))
}

async fn oc_pty_create(
    State(state): State<Arc<OpenCodeAppState>>,
    headers: HeaderMap,
    Query(query): Query<DirectoryQuery>,
    Json(body): Json<PtyCreateRequest>,
) -> impl IntoResponse {
    let directory = state.opencode.directory_for(&headers, query.directory.as_ref());
    let id = next_id("pty_", &PTY_COUNTER);
    let record = OpenCodePtyRecord {
        id: id.clone(),
        title: body.title.unwrap_or_else(|| "PTY".to_string()),
        command: body.command.unwrap_or_else(|| "bash".to_string()),
        args: body.args.unwrap_or_default(),
        cwd: body.cwd.unwrap_or_else(|| directory),
        status: "running".to_string(),
        pid: 0,
    };
    let value = record.to_value();
    let mut ptys = state.opencode.ptys.lock().await;
    ptys.insert(id, record);
    drop(ptys);

    state
        .opencode
        .emit_event(json!({"type": "pty.created", "properties": {"pty": value}}));

    (StatusCode::OK, Json(value))
}

async fn oc_pty_get(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(pty_id): Path<String>,
) -> impl IntoResponse {
    let ptys = state.opencode.ptys.lock().await;
    if let Some(pty) = ptys.get(&pty_id) {
        return (StatusCode::OK, Json(pty.to_value())).into_response();
    }
    not_found("PTY not found").into_response()
}

async fn oc_pty_update(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(pty_id): Path<String>,
    Json(body): Json<PtyCreateRequest>,
) -> impl IntoResponse {
    let mut ptys = state.opencode.ptys.lock().await;
    if let Some(pty) = ptys.get_mut(&pty_id) {
        if let Some(title) = body.title {
            pty.title = title;
        }
        if let Some(command) = body.command {
            pty.command = command;
        }
        if let Some(args) = body.args {
            pty.args = args;
        }
        if let Some(cwd) = body.cwd {
            pty.cwd = cwd;
        }
        let value = pty.to_value();
        state
            .opencode
            .emit_event(json!({"type": "pty.updated", "properties": {"pty": value}}));
        return (StatusCode::OK, Json(value)).into_response();
    }
    not_found("PTY not found").into_response()
}

async fn oc_pty_delete(
    State(state): State<Arc<OpenCodeAppState>>,
    Path(pty_id): Path<String>,
) -> impl IntoResponse {
    let mut ptys = state.opencode.ptys.lock().await;
    if let Some(pty) = ptys.remove(&pty_id) {
        state
            .opencode
            .emit_event(json!({"type": "pty.deleted", "properties": {"pty": pty.to_value()}}));
        return bool_ok(true).into_response();
    }
    not_found("PTY not found").into_response()
}

async fn oc_pty_connect(Path(_pty_id): Path<String>) -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_file_list() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn oc_file_content(Query(query): Query<FileContentQuery>) -> impl IntoResponse {
    if query.path.is_none() {
        return bad_request("path is required").into_response();
    }
    (
        StatusCode::OK,
        Json(json!({
            "type": "text",
            "content": "",
        })),
    )
        .into_response()
}

async fn oc_file_status() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([]))).into_response()
}

async fn oc_find_text(Query(query): Query<FindTextQuery>) -> impl IntoResponse {
    if query.pattern.is_none() {
        return bad_request("pattern is required").into_response();
    }
    (StatusCode::OK, Json(json!([]))).into_response()
}

async fn oc_find_files(Query(query): Query<FindFilesQuery>) -> impl IntoResponse {
    if query.query.is_none() {
        return bad_request("query is required").into_response();
    }
    (StatusCode::OK, Json(json!([]))).into_response()
}

async fn oc_find_symbols(Query(query): Query<FindSymbolsQuery>) -> impl IntoResponse {
    if query.query.is_none() {
        return bad_request("query is required").into_response();
    }
    (StatusCode::OK, Json(json!([]))).into_response()
}

async fn oc_mcp_list() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({})))
}

async fn oc_mcp_register() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({})))
}

async fn oc_mcp_auth(
    Path(_name): Path<String>,
    _body: Option<Json<Value>>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "needs_auth"})))
}

async fn oc_mcp_auth_remove(Path(_name): Path<String>) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "disabled"})))
}

async fn oc_mcp_auth_callback(
    Path(_name): Path<String>,
    _body: Option<Json<Value>>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "needs_auth"})))
}

async fn oc_mcp_authenticate(
    Path(_name): Path<String>,
    _body: Option<Json<Value>>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "needs_auth"})))
}

async fn oc_mcp_connect(Path(_name): Path<String>) -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_mcp_disconnect(Path(_name): Path<String>) -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tool_ids() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn oc_tool_list(Query(query): Query<ToolQuery>) -> impl IntoResponse {
    if query.provider.is_none() || query.model.is_none() {
        return bad_request("provider and model are required").into_response();
    }
    (StatusCode::OK, Json(json!([]))).into_response()
}

async fn oc_resource_list() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({})))
}

async fn oc_worktree_list(State(state): State<Arc<OpenCodeAppState>>, headers: HeaderMap) -> impl IntoResponse {
    let directory = state.opencode.directory_for(&headers, None);
    let worktree = state.opencode.worktree_for(&directory);
    (StatusCode::OK, Json(json!([worktree])))
}

async fn oc_worktree_create(State(state): State<Arc<OpenCodeAppState>>, headers: HeaderMap) -> impl IntoResponse {
    let directory = state.opencode.directory_for(&headers, None);
    let worktree = state.opencode.worktree_for(&directory);
    (
        StatusCode::OK,
        Json(json!({
            "name": "worktree",
            "branch": state.opencode.branch_name(),
            "directory": worktree,
        })),
    )
}

async fn oc_worktree_delete() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_worktree_reset() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_skill_list() -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn oc_tui_next() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"path": "", "body": {}})))
}

async fn oc_tui_response() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tui_append_prompt() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tui_open_help() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tui_open_sessions() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tui_open_themes() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tui_open_models() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tui_submit_prompt() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tui_clear_prompt() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tui_execute_command() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tui_show_toast() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tui_publish() -> impl IntoResponse {
    bool_ok(true)
}

async fn oc_tui_select_session() -> impl IntoResponse {
    bool_ok(true)
}
