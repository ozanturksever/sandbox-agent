use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::sse::Event;
use axum::response::{IntoResponse, Response, Sse};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use sandbox_daemon_error::{AgentError as AgentErrorPayload, ProblemDetails, SandboxError};
use futures::stream;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

#[derive(Debug, Clone)]
pub struct AppState {
    pub auth: AuthConfig,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub token: Option<String>,
}

impl AuthConfig {
    pub fn disabled() -> Self {
        Self { token: None }
    }

    pub fn with_token(token: String) -> Self {
        Self { token: Some(token) }
    }
}

pub fn build_router(state: AppState) -> Router {
    let shared = Arc::new(state);

    let router = Router::new()
        .route("/agents", get(list_agents))
        .route("/agents/:agent/install", post(install_agent))
        .route("/agents/:agent/modes", get(get_agent_modes))
        .route("/sessions/:session_id", post(create_session))
        .route("/sessions/:session_id/messages", post(post_message))
        .route("/sessions/:session_id/events", get(get_events))
        .route("/sessions/:session_id/events/sse", get(get_events_sse))
        .route(
            "/sessions/:session_id/questions/:question_id/reply",
            post(reply_question),
        )
        .route(
            "/sessions/:session_id/questions/:question_id/reject",
            post(reject_question),
        )
        .route(
            "/sessions/:session_id/permissions/:permission_id/reply",
            post(reply_permission),
        )
        .with_state(shared.clone());

    if shared.auth.token.is_some() {
        router.layer(axum::middleware::from_fn_with_state(shared, require_token))
    } else {
        router
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        install_agent,
        get_agent_modes,
        list_agents,
        create_session,
        post_message,
        get_events,
        get_events_sse,
        reply_question,
        reject_question,
        reply_permission
    ),
    components(
        schemas(
            AgentInstallRequest,
            AgentModeInfo,
            AgentModesResponse,
            AgentInfo,
            AgentListResponse,
            CreateSessionRequest,
            CreateSessionResponse,
            MessageRequest,
            EventsQuery,
            EventsResponse,
            UniversalEvent,
            UniversalEventData,
            NoopMessage,
            QuestionReplyRequest,
            PermissionReplyRequest,
            PermissionReply,
            ProblemDetails,
            AgentErrorPayload
        )
    ),
    tags(
        (name = "agents", description = "Agent management"),
        (name = "sessions", description = "Session management")
    )
)]
pub struct ApiDoc;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Sandbox(#[from] SandboxError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let problem: ProblemDetails = match &self {
            ApiError::Sandbox(err) => err.to_problem_details(),
        };
        let status = StatusCode::from_u16(problem.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(problem)).into_response()
    }
}

async fn require_token(
    State(state): State<Arc<AppState>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let expected = match &state.auth.token {
        Some(token) => token.as_str(),
        None => return Ok(next.run(req).await),
    };

    let provided = extract_token(req.headers());
    if provided.as_deref() == Some(expected) {
        Ok(next.run(req).await)
    } else {
        Err(SandboxError::TokenInvalid {
            message: Some("missing or invalid token".to_string()),
        }
        .into())
    }
}

fn extract_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(value) = value.to_str() {
            let value = value.trim();
            if let Some(stripped) = value.strip_prefix("Bearer ") {
                return Some(stripped.to_string());
            }
            if let Some(stripped) = value.strip_prefix("Token ") {
                return Some(stripped.to_string());
            }
        }
    }

    if let Some(value) = headers.get("x-sandbox-token") {
        if let Ok(value) = value.to_str() {
            return Some(value.to_string());
        }
    }

    None
}

// TODO: Replace NoopMessage with universal agent schema once available.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema, Default)]
pub struct NoopMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UniversalEvent {
    pub id: u64,
    pub timestamp: String,
    pub session_id: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    pub data: UniversalEventData,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(untagged)]
#[allow(non_snake_case)]
pub enum UniversalEventData {
    Message { message: NoopMessage },
    Started { started: NoopMessage },
    Error { error: NoopMessage },
    QuestionAsked { questionAsked: NoopMessage },
    PermissionAsked { permissionAsked: NoopMessage },
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentInstallRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reinstall: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentModeInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentModesResponse {
    pub modes: Vec<AgentModeInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub id: String,
    pub installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentListResponse {
    pub agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate_token: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionResponse {
    pub healthy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AgentErrorPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessageRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventsQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventsResponse {
    pub events: Vec<UniversalEvent>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestionReplyRequest {
    pub answers: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermissionReplyRequest {
    pub reply: PermissionReply,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PermissionReply {
    Once,
    Always,
    Reject,
}

impl std::str::FromStr for PermissionReply {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "once" => Ok(Self::Once),
            "always" => Ok(Self::Always),
            "reject" => Ok(Self::Reject),
            _ => Err(format!("invalid permission reply: {value}")),
        }
    }
}

#[utoipa::path(
    post,
    path = "/agents/{agent}/install",
    request_body = AgentInstallRequest,
    responses(
        (status = 204, description = "Agent installed"),
        (status = 400, body = ProblemDetails),
        (status = 404, body = ProblemDetails),
        (status = 500, body = ProblemDetails)
    ),
    params(("agent" = String, Path, description = "Agent id")),
    tag = "agents"
)]
async fn install_agent(
    Path(agent): Path<String>,
    Json(_request): Json<AgentInstallRequest>,
) -> Result<StatusCode, ApiError> {
    validate_agent(&agent)?;
    // TODO: Hook this up to sandbox agent management once available.
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/agents/{agent}/modes",
    responses(
        (status = 200, body = AgentModesResponse),
        (status = 400, body = ProblemDetails)
    ),
    params(("agent" = String, Path, description = "Agent id")),
    tag = "agents"
)]
async fn get_agent_modes(Path(agent): Path<String>) -> Result<Json<AgentModesResponse>, ApiError> {
    validate_agent(&agent)?;
    let modes = vec![
        AgentModeInfo {
            id: "build".to_string(),
            name: "Build".to_string(),
            description: "Default build mode".to_string(),
        },
        AgentModeInfo {
            id: "plan".to_string(),
            name: "Plan".to_string(),
            description: "Planning mode".to_string(),
        },
    ];
    Ok(Json(AgentModesResponse { modes }))
}

#[utoipa::path(
    get,
    path = "/agents",
    responses((status = 200, body = AgentListResponse)),
    tag = "agents"
)]
async fn list_agents() -> Result<Json<AgentListResponse>, ApiError> {
    let agents = known_agents()
        .into_iter()
        .map(|agent| AgentInfo {
            id: agent.to_string(),
            installed: false,
            version: None,
            path: None,
        })
        .collect();

    Ok(Json(AgentListResponse { agents }))
}

#[utoipa::path(
    post,
    path = "/sessions/{session_id}",
    request_body = CreateSessionRequest,
    responses(
        (status = 200, body = CreateSessionResponse),
        (status = 400, body = ProblemDetails),
        (status = 409, body = ProblemDetails)
    ),
    params(("session_id" = String, Path, description = "Client session id")),
    tag = "sessions"
)]
async fn create_session(
    Path(session_id): Path<String>,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, ApiError> {
    validate_agent(&request.agent)?;
    let _ = session_id;
    // TODO: Hook this up to sandbox session management once available.
    Ok(Json(CreateSessionResponse {
        healthy: true,
        error: None,
        agent_session_id: None,
    }))
}

#[utoipa::path(
    post,
    path = "/sessions/{session_id}/messages",
    request_body = MessageRequest,
    responses(
        (status = 204, description = "Message accepted"),
        (status = 404, body = ProblemDetails)
    ),
    params(("session_id" = String, Path, description = "Session id")),
    tag = "sessions"
)]
async fn post_message(
    Path(session_id): Path<String>,
    Json(_request): Json<MessageRequest>,
) -> Result<StatusCode, ApiError> {
    let _ = session_id;
    // TODO: Hook this up to sandbox session messaging once available.
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}/events",
    params(
        ("session_id" = String, Path, description = "Session id"),
        ("offset" = Option<u64>, Query, description = "Last seen event id (exclusive)"),
        ("limit" = Option<u64>, Query, description = "Max events to return")
    ),
    responses(
        (status = 200, body = EventsResponse),
        (status = 404, body = ProblemDetails)
    ),
    tag = "sessions"
)]
async fn get_events(
    Path(session_id): Path<String>,
    Query(_query): Query<EventsQuery>,
) -> Result<Json<EventsResponse>, ApiError> {
    let _ = session_id;
    // TODO: Hook this up to sandbox session events once available.
    Ok(Json(EventsResponse {
        events: Vec::new(),
        has_more: false,
    }))
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}/events/sse",
    params(
        ("session_id" = String, Path, description = "Session id"),
        ("offset" = Option<u64>, Query, description = "Last seen event id (exclusive)")
    ),
    responses((status = 200, description = "SSE event stream")),
    tag = "sessions"
)]
async fn get_events_sse(
    Path(session_id): Path<String>,
    Query(_query): Query<EventsQuery>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let _ = session_id;
    // TODO: Hook this up to sandbox session events once available.
    let stream = stream::empty::<Result<Event, Infallible>>();
    Ok(Sse::new(stream))
}

#[utoipa::path(
    post,
    path = "/sessions/{session_id}/questions/{question_id}/reply",
    request_body = QuestionReplyRequest,
    responses(
        (status = 204, description = "Question answered"),
        (status = 404, body = ProblemDetails)
    ),
    params(
        ("session_id" = String, Path, description = "Session id"),
        ("question_id" = String, Path, description = "Question id")
    ),
    tag = "sessions"
)]
async fn reply_question(
    Path((_session_id, _question_id)): Path<(String, String)>,
    Json(_request): Json<QuestionReplyRequest>,
) -> Result<StatusCode, ApiError> {
    // TODO: Hook this up to sandbox question handling once available.
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/sessions/{session_id}/questions/{question_id}/reject",
    responses(
        (status = 204, description = "Question rejected"),
        (status = 404, body = ProblemDetails)
    ),
    params(
        ("session_id" = String, Path, description = "Session id"),
        ("question_id" = String, Path, description = "Question id")
    ),
    tag = "sessions"
)]
async fn reject_question(
    Path((_session_id, _question_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    // TODO: Hook this up to sandbox question handling once available.
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/sessions/{session_id}/permissions/{permission_id}/reply",
    request_body = PermissionReplyRequest,
    responses(
        (status = 204, description = "Permission reply accepted"),
        (status = 404, body = ProblemDetails)
    ),
    params(
        ("session_id" = String, Path, description = "Session id"),
        ("permission_id" = String, Path, description = "Permission id")
    ),
    tag = "sessions"
)]
async fn reply_permission(
    Path((_session_id, _permission_id)): Path<(String, String)>,
    Json(_request): Json<PermissionReplyRequest>,
) -> Result<StatusCode, ApiError> {
    // TODO: Hook this up to sandbox permission handling once available.
    Ok(StatusCode::NO_CONTENT)
}

fn known_agents() -> Vec<&'static str> {
    vec!["claude", "codex", "opencode", "amp"]
}

fn validate_agent(agent: &str) -> Result<(), ApiError> {
    if known_agents().iter().any(|known| known == &agent) {
        Ok(())
    } else {
        Err(SandboxError::UnsupportedAgent {
            agent: agent.to_string(),
        }
        .into())
    }
}

pub fn add_token_header(headers: &mut HeaderMap, token: &str) {
    let value = format!("Bearer {token}");
    if let Ok(header) = HeaderValue::from_str(&value) {
        headers.insert(axum::http::header::AUTHORIZATION, header);
    }
}
