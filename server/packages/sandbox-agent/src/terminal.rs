//! WebSocket-based PTY terminal manager.
//!
//! Provides a general-purpose terminal that can run any command as a PTY process:
//! - Plain shell: `/bin/bash`
//! - Agents interactively: `claude`, `opencode`, `codex`
//! - Any program: `python3`, `node`, `vim`, etc.
//!
//! The PTY is exposed over WebSocket with binary frames for I/O and
//! text frames for JSON control messages (resize, ping).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use futures::stream::StreamExt;
use futures::SinkExt;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use sandbox_agent_error::SandboxError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};
use utoipa::ToSchema;

use crate::router::{ApiError, AppState, AuthConfig};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Maximum number of concurrent terminal sessions per sandbox.
const MAX_TERMINAL_SESSIONS: usize = 16;

static TERMINAL_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_terminal_id() -> String {
    let n = TERMINAL_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("term_{n}")
}

/// A live PTY session.
struct PtySession {
    id: String,
    command: String,
    args: Vec<String>,
    cwd: String,
    cols: u16,
    rows: u16,
    pid: Option<u32>,
    created_at: Instant,
    /// PTY master handle — used for resize.
    master: Box<dyn MasterPty + Send>,
    /// Writer end of the PTY master — taken on WS connect, returned on disconnect.
    writer: Option<Box<dyn Write + Send>>,
    /// Reader end — taken on WS connect, returned on disconnect.
    reader: Option<Box<dyn Read + Send>>,
    /// Child process handle.
    child: Box<dyn portable_pty::Child + Send>,
    /// Whether a WebSocket client is currently connected.
    connected: bool,
}

impl PtySession {
    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn to_info(&mut self) -> TerminalInfo {
        let alive = self.is_alive();
        TerminalInfo {
            id: self.id.clone(),
            command: self.command.clone(),
            args: self.args.clone(),
            cwd: self.cwd.clone(),
            cols: self.cols,
            rows: self.rows,
            pid: self.pid,
            connected: self.connected,
            alive,
            uptime_secs: self.created_at.elapsed().as_secs(),
        }
    }
}

/// Manages all PTY sessions for this sandbox-agent instance.
pub struct TerminalManager {
    sessions: Mutex<HashMap<String, PtySession>>,
}

impl std::fmt::Debug for TerminalManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalManager").finish()
    }
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Spawn a new PTY session.
    pub async fn create(&self, req: CreateTerminalRequest) -> Result<TerminalInfo, SandboxError> {
        let mut sessions = self.sessions.lock().await;
        if sessions.len() >= MAX_TERMINAL_SESSIONS {
            return Err(SandboxError::InvalidRequest {
                message: format!(
                    "maximum of {} concurrent terminal sessions reached",
                    MAX_TERMINAL_SESSIONS
                ),
            });
        }

        let pty_system = native_pty_system();
        let cols = req.cols.unwrap_or(80);
        let rows = req.rows.unwrap_or(24);
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system.openpty(size).map_err(|err| SandboxError::StreamError {
            message: format!("failed to open PTY: {err}"),
        })?;

        let cmd_str = req.command.as_deref().unwrap_or("/bin/bash");
        let mut cmd = CommandBuilder::new(cmd_str);
        if let Some(args) = &req.args {
            for arg in args {
                cmd.arg(arg);
            }
        }
        let cwd = req
            .cwd
            .clone()
            .unwrap_or_else(|| std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "/".to_string()));
        cmd.cwd(&cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        if let Some(env) = &req.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|err| SandboxError::StreamError {
                message: format!("failed to spawn command '{cmd_str}': {err}"),
            })?;

        let pid = child.process_id();
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|err| SandboxError::StreamError {
                message: format!("failed to clone PTY reader: {err}"),
            })?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|err| SandboxError::StreamError {
                message: format!("failed to take PTY writer: {err}"),
            })?;

        let id = next_terminal_id();
        let args = req.args.clone().unwrap_or_default();
        info!(
            terminal_id = %id,
            command = %cmd_str,
            pid = ?pid,
            "terminal created"
        );

        let session = PtySession {
            id: id.clone(),
            command: cmd_str.to_string(),
            args: args.clone(),
            cwd: cwd.clone(),
            cols,
            rows,
            pid,
            created_at: Instant::now(),
            master: pair.master,
            writer: Some(writer),
            reader: Some(reader),
            child,
            connected: false,
        };

        let info = TerminalInfo {
            id: id.clone(),
            command: cmd_str.to_string(),
            args,
            cwd,
            cols,
            rows,
            pid,
            connected: false,
            alive: true,
            uptime_secs: 0,
        };

        sessions.insert(id, session);
        Ok(info)
    }

    /// List all terminal sessions.
    pub async fn list(&self) -> Vec<TerminalInfo> {
        let mut sessions = self.sessions.lock().await;
        sessions.values_mut().map(|s| s.to_info()).collect()
    }

    /// Get info for a single terminal.
    pub async fn get(&self, id: &str) -> Option<TerminalInfo> {
        let mut sessions = self.sessions.lock().await;
        sessions.get_mut(id).map(|s| s.to_info())
    }

    /// Resize a terminal.
    pub async fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), SandboxError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(id).ok_or_else(|| SandboxError::InvalidRequest {
            message: format!("terminal '{id}' not found"),
        })?;
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| SandboxError::StreamError {
                message: format!("failed to resize terminal: {err}"),
            })?;
        session.cols = cols;
        session.rows = rows;
        Ok(())
    }

    /// Kill and remove a terminal session.
    pub async fn kill(&self, id: &str) -> Result<(), SandboxError> {
        let mut sessions = self.sessions.lock().await;
        let mut session = sessions.remove(id).ok_or_else(|| SandboxError::InvalidRequest {
            message: format!("terminal '{id}' not found"),
        })?;
        let _ = session.child.kill();
        info!(terminal_id = %id, "terminal killed");
        Ok(())
    }

    /// Take the reader and writer for a WebSocket connection.
    /// Returns None if the terminal doesn't exist or is already connected.
    async fn take_io(
        &self,
        id: &str,
    ) -> Option<(
        Box<dyn Read + Send>,
        Box<dyn Write + Send>,
    )> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(id)?;
        if session.connected {
            return None; // Already has a WebSocket connected
        }
        let reader = session.reader.take()?;
        let writer = session.writer.take()?;
        session.connected = true;
        Some((reader, writer))
    }

    /// Return the reader and writer after a WebSocket disconnects.
    async fn return_io(
        &self,
        id: &str,
        reader: Box<dyn Read + Send>,
        writer: Box<dyn Write + Send>,
    ) {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(id) {
            session.reader = Some(reader);
            session.writer = Some(writer);
            session.connected = false;
        }
    }

    /// Mark a terminal as disconnected without returning the reader
    /// (used when the PTY process has exited).
    async fn mark_disconnected(&self, id: &str) {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(id) {
            session.connected = false;
        }
    }

    /// Kill all terminal sessions. Called on server shutdown.
    pub async fn shutdown(&self) {
        let mut sessions = self.sessions.lock().await;
        for (id, session) in sessions.iter_mut() {
            let _ = session.child.kill();
            info!(terminal_id = %id, "terminal killed (shutdown)");
        }
        sessions.clear();
    }
}

/// Drive a WebSocket ↔ PTY bridge.
async fn run_terminal_ws(
    manager: Arc<TerminalManager>,
    terminal_id: String,
    mut socket: WebSocket,
) {
    let io = manager.take_io(&terminal_id).await;
    let Some((reader, writer)) = io else {
        let _ = socket
            .send(Message::Close(None))
            .await;
        return;
    };

    // Channel for PTY output → WebSocket sender task
    let (pty_tx, mut pty_rx) = mpsc::channel::<Vec<u8>>(64);

    // Clone manager/id for the resize handler and cleanup
    let mgr = manager.clone();
    let tid = terminal_id.clone();

    // Task 1: PTY reader → channel (blocking I/O in a blocking thread)
    let read_handle = tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break reader,
                Ok(n) => {
                    if pty_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break reader;
                    }
                }
                Err(_) => break reader,
            }
        }
    });

    // We split the WebSocket and run send/recv concurrently.
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Forward PTY output to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(data) = pty_rx.recv().await {
            if ws_sender.send(Message::Binary(data)).await.is_err() {
                break;
            }
        }
        // Close the WS when PTY output ends
        let _ = ws_sender
            .send(Message::Text(
                serde_json::json!({"type":"exit"}).to_string(),
            ))
            .await;
        let _ = ws_sender.close().await;
    });

    // Receive from WebSocket, write to PTY.
    // Writer is moved into a Mutex so we can return it after the task ends.
    let writer_mu = Arc::new(std::sync::Mutex::new(Some(writer)));
    let writer_mu2 = writer_mu.clone();
    let recv_tid = tid.clone();
    let recv_mgr = mgr.clone();
    let recv_task = tokio::spawn(async move {
        let mut writer_guard = writer_mu2.lock().unwrap().take().unwrap();
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Binary(data) => {
                    if writer_guard.write_all(&data).is_err() {
                        break;
                    }
                    let _ = writer_guard.flush();
                }
                Message::Text(text) => {
                    // Try to parse as JSON control message
                    if let Ok(ctrl) = serde_json::from_str::<ControlMessage>(&text) {
                        match ctrl.msg_type.as_str() {
                            "resize" => {
                                if let (Some(cols), Some(rows)) = (ctrl.cols, ctrl.rows) {
                                    let _ = recv_mgr.resize(&recv_tid, cols, rows).await;
                                }
                            }
                            "ping" => {
                                // Client ping — no-op, WS layer handles keepalive
                            }
                            _ => {}
                        }
                    } else {
                        // Treat unrecognized text as input (some xterm.js configs send text)
                        if writer_guard.write_all(text.as_bytes()).is_err() {
                            break;
                        }
                        let _ = writer_guard.flush();
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        // Return writer for potential reconnection
        *writer_mu2.lock().unwrap() = Some(writer_guard);
    });

    // Wait for either side to finish
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    // Return the reader and writer so the terminal can be reconnected
    let returned_writer = writer_mu.lock().unwrap().take();
    match (read_handle.await, returned_writer) {
        (Ok(reader), Some(writer)) => {
            mgr.return_io(&tid, reader, writer).await;
        }
        _ => {
            mgr.mark_disconnected(&tid).await;
        }
    }

    info!(terminal_id = %tid, "WebSocket disconnected");
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request to create a new terminal session.
#[derive(Debug, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct CreateTerminalRequest {
    /// Command to run. Default: "/bin/bash".
    /// Can be any executable: "bash", "claude", "opencode", "codex", "python3", "node", etc.
    #[serde(default)]
    pub command: Option<String>,
    /// Command arguments (e.g. \["--model", "sonnet"\] for claude).
    #[serde(default)]
    pub args: Option<Vec<String>>,
    /// Extra environment variables to set (e.g. API keys for agents).
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    /// Working directory. Default: current directory (/workspace).
    #[serde(default)]
    pub cwd: Option<String>,
    /// Initial terminal columns. Default: 80.
    #[serde(default)]
    pub cols: Option<u16>,
    /// Initial terminal rows. Default: 24.
    #[serde(default)]
    pub rows: Option<u16>,
}

/// Information about a terminal session.
#[derive(Debug, Serialize, JsonSchema, ToSchema)]
pub struct TerminalInfo {
    /// Terminal session ID.
    pub id: String,
    /// Command that was launched.
    pub command: String,
    /// Command arguments.
    pub args: Vec<String>,
    /// Working directory.
    pub cwd: String,
    /// Current terminal columns.
    pub cols: u16,
    /// Current terminal rows.
    pub rows: u16,
    /// OS process ID of the child, if available.
    pub pid: Option<u32>,
    /// Whether a WebSocket client is currently connected.
    pub connected: bool,
    /// Whether the child process is still alive.
    pub alive: bool,
    /// Seconds since creation.
    pub uptime_secs: u64,
}

/// Request to resize a terminal.
#[derive(Debug, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct ResizeRequest {
    /// New column count.
    pub cols: u16,
    /// New row count.
    pub rows: u16,
}

/// WebSocket control message (sent as JSON text frame).
#[derive(Debug, Deserialize)]
struct ControlMessage {
    #[serde(rename = "type")]
    msg_type: String,
    cols: Option<u16>,
    rows: Option<u16>,
}

/// Query parameters for WebSocket terminal connection.
#[derive(Debug, Deserialize)]
pub struct TerminalWsQuery {
    /// Auth token (alternative to Authorization header for WebSocket connections).
    pub token: Option<String>,
}

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

/// List terminals
///
/// List all active terminal sessions.
#[utoipa::path(
    get,
    path = "/v1/terminal",
    responses(
        (status = 200, description = "List of terminal sessions", body = Vec<TerminalInfo>)
    ),
    tag = "terminal"
)]
pub async fn list_terminals(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let terminals = state.terminal_manager.list().await;
    (StatusCode::OK, Json(terminals))
}

/// Create terminal
///
/// Create a new terminal session with the specified command.
#[utoipa::path(
    post,
    path = "/v1/terminal",
    request_body = CreateTerminalRequest,
    responses(
        (status = 200, description = "Terminal session created", body = TerminalInfo),
        (status = 400, description = "Invalid request")
    ),
    tag = "terminal"
)]
pub async fn create_terminal(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTerminalRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let info = state.terminal_manager.create(req).await?;
    Ok((StatusCode::OK, Json(info)))
}

/// Get terminal
///
/// Get info about a specific terminal session.
#[utoipa::path(
    get,
    path = "/v1/terminal/{terminal_id}",
    params(("terminal_id" = String, Path, description = "Terminal session ID")),
    responses(
        (status = 200, description = "Terminal session info", body = TerminalInfo),
        (status = 404, description = "Terminal not found")
    ),
    tag = "terminal"
)]
pub async fn get_terminal(
    State(state): State<Arc<AppState>>,
    Path(terminal_id): Path<String>,
) -> impl IntoResponse {
    match state.terminal_manager.get(&terminal_id).await {
        Some(info) => (StatusCode::OK, Json(serde_json::to_value(info).unwrap())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "terminal not found"})),
        )
            .into_response(),
    }
}

/// Delete terminal
///
/// Kill and remove a terminal session.
#[utoipa::path(
    delete,
    path = "/v1/terminal/{terminal_id}",
    params(("terminal_id" = String, Path, description = "Terminal session ID")),
    responses(
        (status = 204, description = "Terminal deleted"),
        (status = 404, description = "Terminal not found")
    ),
    tag = "terminal"
)]
pub async fn delete_terminal(
    State(state): State<Arc<AppState>>,
    Path(terminal_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state.terminal_manager.kill(&terminal_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Resize terminal
///
/// Change the terminal dimensions (columns and rows).
#[utoipa::path(
    post,
    path = "/v1/terminal/{terminal_id}/resize",
    params(("terminal_id" = String, Path, description = "Terminal session ID")),
    request_body = ResizeRequest,
    responses(
        (status = 204, description = "Terminal resized"),
        (status = 404, description = "Terminal not found")
    ),
    tag = "terminal"
)]
pub async fn resize_terminal(
    State(state): State<Arc<AppState>>,
    Path(terminal_id): Path<String>,
    Json(req): Json<ResizeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .terminal_manager
        .resize(&terminal_id, req.cols, req.rows)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Connect terminal WebSocket
///
/// Upgrade to a WebSocket connection for interactive PTY I/O.
/// Binary frames carry raw terminal data. Text frames carry JSON control
/// messages (e.g. `{"type":"resize","cols":120,"rows":40}`).
/// Auth token can be passed as `?token=` query param or via Authorization header.
#[utoipa::path(
    get,
    path = "/v1/terminal/{terminal_id}/ws",
    params(
        ("terminal_id" = String, Path, description = "Terminal session ID"),
        ("token" = Option<String>, Query, description = "Auth token (alternative to Authorization header)")
    ),
    responses(
        (status = 101, description = "WebSocket upgrade"),
        (status = 404, description = "Terminal not found"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "terminal"
)]
pub async fn terminal_ws(
    State(state): State<Arc<AppState>>,
    Path(terminal_id): Path<String>,
    Query(query): Query<TerminalWsQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ApiError> {
    // Validate auth — check query param token or Authorization header.
    // The normal require_token middleware doesn't run on WS upgrade for
    // browser clients that can't set headers, so we check here.
    if let Some(ref expected) = state.auth().token {
        let provided = query.token.as_deref().or_else(|| {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        });
        match provided {
            Some(t) if t == expected => {}
            _ => {
                return Err(ApiError::Sandbox(SandboxError::TokenInvalid {
                    message: Some("invalid or missing token".to_string()),
                }));
            }
        }
    }

    // Verify terminal exists
    if state.terminal_manager.get(&terminal_id).await.is_none() {
        return Err(ApiError::Sandbox(SandboxError::InvalidRequest {
            message: format!("terminal '{terminal_id}' not found"),
        }));
    }

    let manager = state.terminal_manager.clone();
    let tid = terminal_id.clone();

    Ok(ws.on_upgrade(move |socket| run_terminal_ws(manager, tid, socket)))
}
