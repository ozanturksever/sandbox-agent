use std::io::Write;

use clap::{Args, Parser, Subcommand};
use reqwest::blocking::Client as HttpClient;
use reqwest::Method;
use sandbox_daemon_core::router::{
    AgentInstallRequest, AppState, AuthConfig, CreateSessionRequest, MessageRequest,
    PermissionReply, PermissionReplyRequest, QuestionReplyRequest,
};
use sandbox_daemon_core::router::{AgentListResponse, AgentModesResponse, CreateSessionResponse, EventsResponse};
use sandbox_daemon_core::router::build_router;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use tower_http::cors::{Any, CorsLayer};

#[derive(Parser, Debug)]
#[command(name = "sandbox-daemon")]
#[command(about = "Sandbox daemon for managing coding agents", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 8787)]
    port: u16,

    #[arg(long)]
    token: Option<String>,

    #[arg(long)]
    no_token: bool,

    #[arg(long = "cors-allow-origin")]
    cors_allow_origin: Vec<String>,

    #[arg(long = "cors-allow-method")]
    cors_allow_method: Vec<String>,

    #[arg(long = "cors-allow-header")]
    cors_allow_header: Vec<String>,

    #[arg(long = "cors-allow-credentials")]
    cors_allow_credentials: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    Agents(AgentsArgs),
    Sessions(SessionsArgs),
}

#[derive(Args, Debug)]
struct AgentsArgs {
    #[command(subcommand)]
    command: AgentsCommand,
}

#[derive(Args, Debug)]
struct SessionsArgs {
    #[command(subcommand)]
    command: SessionsCommand,
}

#[derive(Subcommand, Debug)]
enum AgentsCommand {
    List(ClientArgs),
    Install(InstallAgentArgs),
    Modes(AgentModesArgs),
}

#[derive(Subcommand, Debug)]
enum SessionsCommand {
    Create(CreateSessionArgs),
    #[command(name = "send-message")]
    SendMessage(SessionMessageArgs),
    #[command(name = "get-messages")]
    GetMessages(SessionEventsArgs),
    #[command(name = "events")]
    Events(SessionEventsArgs),
    #[command(name = "events-sse")]
    EventsSse(SessionEventsSseArgs),
    #[command(name = "reply-question")]
    ReplyQuestion(QuestionReplyArgs),
    #[command(name = "reject-question")]
    RejectQuestion(QuestionRejectArgs),
    #[command(name = "reply-permission")]
    ReplyPermission(PermissionReplyArgs),
}

#[derive(Args, Debug, Clone)]
struct ClientArgs {
    #[arg(long)]
    endpoint: Option<String>,
}

#[derive(Args, Debug)]
struct InstallAgentArgs {
    agent: String,
    #[arg(long)]
    reinstall: bool,
    #[command(flatten)]
    client: ClientArgs,
}

#[derive(Args, Debug)]
struct AgentModesArgs {
    agent: String,
    #[command(flatten)]
    client: ClientArgs,
}

#[derive(Args, Debug)]
struct CreateSessionArgs {
    session_id: String,
    #[arg(long)]
    agent: String,
    #[arg(long)]
    agent_mode: Option<String>,
    #[arg(long)]
    permission_mode: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    variant: Option<String>,
    #[arg(long = "agent-token")]
    agent_token: Option<String>,
    #[arg(long)]
    validate_token: bool,
    #[arg(long)]
    agent_version: Option<String>,
    #[command(flatten)]
    client: ClientArgs,
}

#[derive(Args, Debug)]
struct SessionMessageArgs {
    session_id: String,
    #[arg(long)]
    message: String,
    #[command(flatten)]
    client: ClientArgs,
}

#[derive(Args, Debug)]
struct SessionEventsArgs {
    session_id: String,
    #[arg(long)]
    offset: Option<u64>,
    #[arg(long)]
    limit: Option<u64>,
    #[command(flatten)]
    client: ClientArgs,
}

#[derive(Args, Debug)]
struct SessionEventsSseArgs {
    session_id: String,
    #[arg(long)]
    offset: Option<u64>,
    #[command(flatten)]
    client: ClientArgs,
}

#[derive(Args, Debug)]
struct QuestionReplyArgs {
    session_id: String,
    question_id: String,
    #[arg(long)]
    answers: String,
    #[command(flatten)]
    client: ClientArgs,
}

#[derive(Args, Debug)]
struct QuestionRejectArgs {
    session_id: String,
    question_id: String,
    #[command(flatten)]
    client: ClientArgs,
}

#[derive(Args, Debug)]
struct PermissionReplyArgs {
    session_id: String,
    permission_id: String,
    #[arg(long)]
    reply: PermissionReply,
    #[command(flatten)]
    client: ClientArgs,
}

#[derive(Debug, Error)]
enum CliError {
    #[error("missing --token or --no-token for server mode")]
    MissingToken,
    #[error("invalid cors origin: {0}")]
    InvalidCorsOrigin(String),
    #[error("invalid cors method: {0}")]
    InvalidCorsMethod(String),
    #[error("invalid cors header: {0}")]
    InvalidCorsHeader(String),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("server error: {0}")]
    Server(String),
    #[error("unexpected http status: {0}")]
    HttpStatus(reqwest::StatusCode),
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Some(command) => run_client(command, &cli),
        None => run_server(&cli),
    };

    if let Err(err) = result {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run_server(cli: &Cli) -> Result<(), CliError> {
    let auth = if cli.no_token {
        AuthConfig::disabled()
    } else if let Some(token) = cli.token.clone() {
        AuthConfig::with_token(token)
    } else {
        return Err(CliError::MissingToken);
    };

    let state = AppState { auth };
    let mut router = build_router(state);

    if let Some(cors) = build_cors_layer(cli)? {
        router = router.layer(cors);
    }

    let addr = format!("{}:{}", cli.host, cli.port);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| CliError::Server(err.to_string()))?;

    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, router)
            .await
            .map_err(|err| CliError::Server(err.to_string()))
    })
}

fn run_client(command: &Command, cli: &Cli) -> Result<(), CliError> {
    match command {
        Command::Agents(subcommand) => run_agents(&subcommand.command, cli),
        Command::Sessions(subcommand) => run_sessions(&subcommand.command, cli),
    }
}

fn run_agents(command: &AgentsCommand, cli: &Cli) -> Result<(), CliError> {
    match command {
        AgentsCommand::List(args) => {
            let ctx = ClientContext::new(cli, args)?;
            let response = ctx.get("/agents")?;
            print_json_response::<AgentListResponse>(response)
        }
        AgentsCommand::Install(args) => {
            let ctx = ClientContext::new(cli, &args.client)?;
            let body = AgentInstallRequest {
                reinstall: if args.reinstall { Some(true) } else { None },
            };
            let path = format!("/agents/{}/install", args.agent);
            let response = ctx.post(&path, &body)?;
            print_empty_response(response)
        }
        AgentsCommand::Modes(args) => {
            let ctx = ClientContext::new(cli, &args.client)?;
            let path = format!("/agents/{}/modes", args.agent);
            let response = ctx.get(&path)?;
            print_json_response::<AgentModesResponse>(response)
        }
    }
}

fn run_sessions(command: &SessionsCommand, cli: &Cli) -> Result<(), CliError> {
    match command {
        SessionsCommand::Create(args) => {
            let ctx = ClientContext::new(cli, &args.client)?;
            let body = CreateSessionRequest {
                agent: args.agent.clone(),
                agent_mode: args.agent_mode.clone(),
                permission_mode: args.permission_mode.clone(),
                model: args.model.clone(),
                variant: args.variant.clone(),
                token: args.agent_token.clone(),
                validate_token: if args.validate_token { Some(true) } else { None },
                agent_version: args.agent_version.clone(),
            };
            let path = format!("/sessions/{}", args.session_id);
            let response = ctx.post(&path, &body)?;
            print_json_response::<CreateSessionResponse>(response)
        }
        SessionsCommand::SendMessage(args) => {
            let ctx = ClientContext::new(cli, &args.client)?;
            let body = MessageRequest {
                message: args.message.clone(),
            };
            let path = format!("/sessions/{}/messages", args.session_id);
            let response = ctx.post(&path, &body)?;
            print_empty_response(response)
        }
        SessionsCommand::GetMessages(args) | SessionsCommand::Events(args) => {
            let ctx = ClientContext::new(cli, &args.client)?;
            let path = format!("/sessions/{}/events", args.session_id);
            let response = ctx.get_with_query(&path, &[ ("offset", args.offset), ("limit", args.limit) ])?;
            print_json_response::<EventsResponse>(response)
        }
        SessionsCommand::EventsSse(args) => {
            let ctx = ClientContext::new(cli, &args.client)?;
            let path = format!("/sessions/{}/events/sse", args.session_id);
            let response = ctx.get_with_query(&path, &[("offset", args.offset)])?;
            print_text_response(response)
        }
        SessionsCommand::ReplyQuestion(args) => {
            let ctx = ClientContext::new(cli, &args.client)?;
            let answers: Vec<Vec<String>> = serde_json::from_str(&args.answers)?;
            let body = QuestionReplyRequest { answers };
            let path = format!(
                "/sessions/{}/questions/{}/reply",
                args.session_id, args.question_id
            );
            let response = ctx.post(&path, &body)?;
            print_empty_response(response)
        }
        SessionsCommand::RejectQuestion(args) => {
            let ctx = ClientContext::new(cli, &args.client)?;
            let path = format!(
                "/sessions/{}/questions/{}/reject",
                args.session_id, args.question_id
            );
            let response = ctx.post_empty(&path)?;
            print_empty_response(response)
        }
        SessionsCommand::ReplyPermission(args) => {
            let ctx = ClientContext::new(cli, &args.client)?;
            let body = PermissionReplyRequest {
                reply: args.reply.clone(),
            };
            let path = format!(
                "/sessions/{}/permissions/{}/reply",
                args.session_id, args.permission_id
            );
            let response = ctx.post(&path, &body)?;
            print_empty_response(response)
        }
    }
}

fn build_cors_layer(cli: &Cli) -> Result<Option<CorsLayer>, CliError> {
    let has_config = !cli.cors_allow_origin.is_empty()
        || !cli.cors_allow_method.is_empty()
        || !cli.cors_allow_header.is_empty()
        || cli.cors_allow_credentials;

    if !has_config {
        return Ok(None);
    }

    let mut cors = CorsLayer::new();

    if cli.cors_allow_origin.is_empty() {
        cors = cors.allow_origin(Any);
    } else {
        let mut origins = Vec::new();
        for origin in &cli.cors_allow_origin {
            let value = origin
                .parse()
                .map_err(|_| CliError::InvalidCorsOrigin(origin.clone()))?;
            origins.push(value);
        }
        cors = cors.allow_origin(origins);
    }

    if cli.cors_allow_method.is_empty() {
        cors = cors.allow_methods(Any);
    } else {
        let mut methods = Vec::new();
        for method in &cli.cors_allow_method {
            let parsed = method
                .parse()
                .map_err(|_| CliError::InvalidCorsMethod(method.clone()))?;
            methods.push(parsed);
        }
        cors = cors.allow_methods(methods);
    }

    if cli.cors_allow_header.is_empty() {
        cors = cors.allow_headers(Any);
    } else {
        let mut headers = Vec::new();
        for header in &cli.cors_allow_header {
            let parsed = header
                .parse()
                .map_err(|_| CliError::InvalidCorsHeader(header.clone()))?;
            headers.push(parsed);
        }
        cors = cors.allow_headers(headers);
    }

    if cli.cors_allow_credentials {
        cors = cors.allow_credentials(true);
    }

    Ok(Some(cors))
}

struct ClientContext {
    endpoint: String,
    token: Option<String>,
    client: HttpClient,
}

impl ClientContext {
    fn new(cli: &Cli, args: &ClientArgs) -> Result<Self, CliError> {
        let endpoint = args
            .endpoint
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}", cli.host, cli.port));
        let token = if cli.no_token { None } else { cli.token.clone() };
        let client = HttpClient::builder().build()?;
        Ok(Self {
            endpoint,
            token,
            client,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.endpoint.trim_end_matches('/'), path)
    }

    fn request(&self, method: Method, path: &str) -> reqwest::blocking::RequestBuilder {
        let url = self.url(path);
        let mut builder = self.client.request(method, url);
        if let Some(token) = &self.token {
            builder = builder.bearer_auth(token);
        }
        builder
    }

    fn get(&self, path: &str) -> Result<reqwest::blocking::Response, CliError> {
        Ok(self.request(Method::GET, path).send()?)
    }

    fn get_with_query(
        &self,
        path: &str,
        query: &[(&str, Option<u64>)],
    ) -> Result<reqwest::blocking::Response, CliError> {
        let mut request = self.request(Method::GET, path);
        for (key, value) in query {
            if let Some(value) = value {
                request = request.query(&[(key, value)]);
            }
        }
        Ok(request.send()?)
    }

    fn post<T: Serialize>(&self, path: &str, body: &T) -> Result<reqwest::blocking::Response, CliError> {
        Ok(self.request(Method::POST, path).json(body).send()?)
    }

    fn post_empty(&self, path: &str) -> Result<reqwest::blocking::Response, CliError> {
        Ok(self.request(Method::POST, path).send()?)
    }
}

fn print_json_response<T: serde::de::DeserializeOwned + Serialize>(
    response: reqwest::blocking::Response,
) -> Result<(), CliError> {
    let status = response.status();
    let text = response.text()?;

    if !status.is_success() {
        print_error_body(&text)?;
        return Err(CliError::HttpStatus(status));
    }

    let parsed: T = serde_json::from_str(&text)?;
    let pretty = serde_json::to_string_pretty(&parsed)?;
    println!("{pretty}");
    Ok(())
}

fn print_text_response(response: reqwest::blocking::Response) -> Result<(), CliError> {
    let status = response.status();
    let text = response.text()?;

    if !status.is_success() {
        print_error_body(&text)?;
        return Err(CliError::HttpStatus(status));
    }

    print!("{text}");
    std::io::stdout().flush()?;
    Ok(())
}

fn print_empty_response(response: reqwest::blocking::Response) -> Result<(), CliError> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let text = response.text()?;
    print_error_body(&text)?;
    Err(CliError::HttpStatus(status))
}

fn print_error_body(text: &str) -> Result<(), CliError> {
    if let Ok(json) = serde_json::from_str::<Value>(text) {
        let pretty = serde_json::to_string_pretty(&json)?;
        eprintln!("{pretty}");
    } else {
        eprintln!("{text}");
    }
    Ok(())
}
