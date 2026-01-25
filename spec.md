i need to build a library that is a universal api to work with agents

## glossary

- agent = claude code, codex, and opencode -> the acutal binary/sdk that runs the coding agent
- agent mode = what the agent does, for example build/plan agent mode
- model = claude, codex, gemni, etc -> the model that's use din the agent
- variant = variant on the model if exists, eg low, mid, high, xhigh for codex

## concepts

### universal api types

we need to define a universal base type for input & output from agents that is a common denominator for all agent schemas

this also needs to support quesitons (ie human in the loop)

### working with the agents

these agents all have differnet ways of working with them.

- claude code uses headless mode
- codex uses a typescript sdk
- opencode uses a server

## component: daemon

this is what runs inside the sandbox to manage everything

this is a rust component that exposes an http server

**router**

use axum for routing and utoipa for the json schema and schemars for generating json schemas. see how this is done in:
- ~/rivet
	- engine/packages/config-schema-gen/build.rs
	- ~/rivet/engine/packages/api-public/src/router.rs (but use thiserror instead of anyhow)

we need a standard thiserror for error responses. return errors as RFC 7807 Problem Details

### cli

it's ran with a token like this using clap:

sandbox-daemon --token <token> --host xxxx --port xxxx

(you can specify --no-token too)

also expose a CLI endpoint for every http endpoint we have (specify this in claude.md to keep this to date) so we can do:

sandbox-daemon sessions get-messages --endpoint xxxx --token xxxx

### http api

POST /agents/{}/install (this will install the agent)
{}

POST /sessions/{} (will install agent if not already installed)
>
{
	agent:"claud"|"codex"|"opencode",
	model?:string,
	variant?:string,
    token?: string,
    validateToken?: boolean
    healthy: boolean,
    error?: AgentError
}

POST /sessions/{}/messages
{
    message: string
}

GET /sessions/{}/events?offset=x&limit=x
<
{
	events: UniversalEvent[],
	hasMore: bool
}

GET /sessions/{}/events/sse?offset=x
- same as bove but using sse

types:

type UniversalEvent = { message: UniversalMessage } | { started: Started } | { error: CrashInfo };

type AgentError = { tokenError: ... } | { processExisted: ... } | { installFailed: ... } | etc

### schema converters

we need to have a 2 way conversion for both:

- universal agent input message <-> agent input message
- universal agent event <-> agent event

for messages, we need to have a sepcial universal message type for failed to parse with the raw json that we attempted to parse

### managing agents

> **Note:** We do NOT use JS SDKs for agent communication. All agents are spawned as subprocesses or accessed via a shared server. This keeps the daemon language-agnostic (Rust) and avoids Node.js dependencies.

#### agent comparison

| Agent | Provider | Binary | Install Method | Session ID | Streaming Format |
|-------|----------|--------|----------------|------------|------------------|
| Claude Code | Anthropic | `claude` | curl installer (native binary) | `session_id` (string) | JSONL via stdout |
| Codex | OpenAI | `codex` | GitHub releases / Homebrew (Rust binary) | `thread_id` (string) | JSONL via stdout |
| OpenCode | Multi-provider | `opencode` | curl installer (Go binary) | `session_id` (string) | SSE or JSONL |
| Amp | Sourcegraph | `amp` | curl installer (bundled Bun) | `session_id` (string) | JSONL via stdout |

#### spawning approaches

There are two ways to spawn agents:

##### 1. subprocess per session

Each session spawns a dedicated agent subprocess that lives for the duration of the session.

**How it works:**
- On session create, spawn the agent binary with appropriate flags
- Communicate via stdin/stdout using JSONL
- Process terminates when session ends or times out

**Agents that support this:**
- **Claude Code**: `claude --print --output-format stream-json --verbose --dangerously-skip-permissions [--resume SESSION_ID] "PROMPT"`
- **Codex**: `codex exec --json --dangerously-bypass-approvals-and-sandbox "PROMPT"` or `codex exec resume --last`
- **Amp**: `amp --print --output-format stream-json --dangerously-skip-permissions "PROMPT"`

**Pros:**
- Simple implementation
- Process isolation per session
- No shared state to manage

**Cons:**
- Higher latency (process startup per message)
- More resource usage (one process per active session)
- No connection reuse

##### 2. shared server (preferred for OpenCode)

A single long-running server handles multiple sessions. The daemon connects to this server via HTTP/SSE.

**How it works:**
- On daemon startup (or first session for an agent), start the server if not running
- Server listens on a port (e.g., 4200-4300 range for OpenCode)
- Sessions are created/managed via HTTP API
- Events streamed via SSE

**Agents that support this:**
- **OpenCode**: `opencode serve --port PORT` starts the server, then use HTTP API:
  - `POST /session` - create session
  - `POST /session/{id}/prompt` - send message
  - `GET /event/subscribe` - SSE event stream
  - Supports questions/permissions via `/question/reply`, `/permission/reply`

**Pros:**
- Lower latency (no process startup per message)
- Shared resources across sessions
- Better for high-throughput scenarios
- Native support for SSE streaming

**Cons:**
- More complex lifecycle management
- Need to handle server crashes/restarts
- Shared state between sessions

#### which approach to use

| Agent | Recommended Approach | Reason |
|-------|---------------------|--------|
| Claude Code | Subprocess per session | No server mode available |
| Codex | Subprocess per session | No server mode available |
| OpenCode | Shared server | Native server support, lower latency |
| Amp | Subprocess per session | No server mode available |

#### installation

Before spawning, agents must be installed. **Prefer native installers over npm** - they have no Node.js dependency and are simpler to manage.

| Agent | Native Install (preferred) | Fallback (npm) | Verify |
|-------|---------------------------|----------------|--------|
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash` | `npm i -g @anthropic-ai/claude-code` | `claude --version` |
| Codex | `brew install --cask codex` or [GitHub Releases](https://github.com/openai/codex/releases) | `npm i -g @openai/codex` | `codex --version` |
| OpenCode | `curl -fsSL https://opencode.ai/install \| bash` | `npm i -g opencode-ai` | `opencode --version` |
| Amp | `curl -fsSL https://ampcode.com/install.sh \| bash` | `npm i -g @sourcegraph/amp` | `amp --version` |

**Notes:**
- Claude Code native installer: signed by Anthropic, notarized by Apple on macOS
- Codex: Rust binary, download from GitHub releases and rename to `codex`
- OpenCode: Go binary, also available via Homebrew (`brew install anomalyco/tap/opencode`), Scoop, Nix
- Amp: bundles its own Bun runtime, no prerequisites needed

#### communication

**Subprocess mode (Claude Code, Codex, Amp):**
1. Spawn process with appropriate flags
2. Close stdin immediately after sending prompt (for single-turn) or keep open (for multi-turn)
3. Read JSONL events from stdout line-by-line
4. Parse each line as JSON and convert to `UniversalEvent`
5. Capture session/thread ID from events for resumption
6. Handle process exit/timeout

**Server mode (OpenCode):**
1. Ensure server is running (`opencode serve --port PORT`)
2. Create session via `POST /session`
3. Send prompts via `POST /session/{id}/prompt` (async version for streaming)
4. Subscribe to events via `GET /event/subscribe` (SSE)
5. Handle questions/permissions via dedicated endpoints
6. Session persists across multiple prompts

#### credential passing

| Agent | Env Var | Config File |
|-------|---------|-------------|
| Claude Code | `ANTHROPIC_API_KEY` | `~/.claude.json`, `~/.claude/.credentials.json` |
| Codex | `OPENAI_API_KEY` or `CODEX_API_KEY` | `~/.codex/auth.json` |
| OpenCode | `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` | `~/.local/share/opencode/auth.json` |
| Amp | `ANTHROPIC_API_KEY` | Uses Claude Code credentials |

When spawning subprocesses, pass the API key via environment variable. For OpenCode server mode, the server reads credentials from its config on startup.

### testing

TODO

## component: sdks

we need to auto-generate types from our json schema for these languages

- typescript sdk
	- also need to support standard schema
	- can run in inline mode that doesn't require this
- python sdk

## spec todo

- generate common denominator with conversion functions
- how do we handle HIL
- how do you run each of these agents
- what else do we need, like todo, etc?
- how can we dump the spec for all of the agents somehow

## future problems to visit

- api features
    - list agent modes available
    - list models available
    - handle planning mode
- api key gateway
- configuring mcp/skills/etc
- process management inside container
- otel
- better authentication systems
- s3-based file system
- ai sdk compatability for their ecosystem (useChat, etc)
- resumable messages
- todo lists
- all other features
- misc
    - bootstrap tool that extracts tokens from the current system
- management ui
- skill
- pre-package these as bun binaries instead of npm installations

## future work

- provide a pty to access the agent data
- other agent features like file system

## misc

comparison to agentapi:
- it does not use the pty since we need to get more information from the agent

