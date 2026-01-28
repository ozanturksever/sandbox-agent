---
name: Sandbox
description: Documentation and capabilities reference for Sandbox
metadata:
    mintlify-proj: sandbox
    version: "1.0"
---

## Capabilities

Sandbox Agent SDK provides a unified interface for orchestrating multiple coding agents within isolated sandbox environments. Agents can execute code, interact with files, handle human approvals, and stream real-time events through a standardized API. The daemon normalizes agent-specific behaviors into a universal event schema, enabling consistent multi-agent support across Claude Code, Codex, OpenCode, and Amp.

## Skills

### Session Management
- **Create sessions**: Initialize agent sessions with `POST /v1/sessions/` specifying agent type, mode, and permission settings
- **Session lifecycle**: Track session state including pending questions, permissions, and event history
- **Session configuration**: Set agent mode (build/plan/custom), permission mode (default/plan/bypass), and optional model overrides
- **Session state tracking**: Monitor session_id, agent type, agent_mode, permission_mode, model, events, pending_questions, pending_permissions, and termination status

### Message Handling
- **Send messages**: Post user messages via `POST /v1/sessions/{id}/messages` to trigger agent execution
- **Stream responses**: Use `POST /v1/sessions/{id}/messages/stream` for single-turn streaming with real-time event output
- **Message content**: Support text, tool calls, tool results, file references, images, reasoning, and status updates
- **Multi-turn conversations**: Resume sessions using agent-specific flags (--resume for Claude, --continue for Amp)

### Event Streaming
- **Poll events**: Retrieve events via `GET /v1/sessions/{id}/events` with offset/limit pagination
- **Stream events (SSE)**: Subscribe to real-time events using `GET /v1/sessions/{id}/events/sse` for Server-Sent Events
- **Event types**: Receive Message, Started, Error, QuestionAsked, PermissionAsked, and Unknown events
- **Event metadata**: Each event includes id, timestamp, session_id, agent, and structured data payload

### Human-in-the-Loop (HITL)
- **Answer questions**: Reply to agent questions via `POST /v1/sessions/{id}/questions/{id}/reply` with selected options
- **Reject questions**: Decline agent questions using `POST /v1/sessions/{id}/questions/{id}/reject`
- **Permission prompts**: Grant or deny permissions via `POST /v1/sessions/{id}/permissions/{id}/reply` with reply mode (once/always/reject)
- **Plan approval**: Normalize Claude plan approval into question events for consistent HITL workflows

### Agent Management
- **List agents**: Discover available agents with `GET /v1/agents` including installation status and versions
- **Install agents**: Install agent binaries via `POST /v1/agents/{id}/install` with auto-installation on session creation
- **Agent modes**: Query supported modes per agent using `GET /v1/agents/{id}/modes`
- **Supported agents**: Claude Code, Codex, OpenCode, Amp, and Mock agents for testing

### Content Rendering
- **Text content**: Render standard chat messages and assistant responses
- **Tool calls**: Display tool execution requests with parameters and expected results
- **Tool results**: Show tool execution outcomes and return values
- **File operations**: Preview file read/write/patch operations with file_ref content parts
- **Images**: Render image outputs from agent execution
- **Reasoning**: Display agent reasoning and planning steps when supported
- **Status updates**: Show progress indicators and execution status

### Agent Capabilities Detection
- **Capability flags**: Check agent support for tool_calls, tool_results, questions, permissions, plan_mode, reasoning, status, and item_started
- **UI affordances**: Enable/disable UI components based on agent capabilities (tool panels, approval buttons, reasoning displays)
- **Feature coverage**: Verify support for streaming, file operations, HITL workflows, and plan approval normalization

## Workflows

### Basic Chat Workflow
1. Create a session: `POST /v1/sessions/` with agent="claude", agentMode="build"
2. Send a message: `POST /v1/sessions/{id}/messages` with user prompt
3. Stream events: `GET /v1/sessions/{id}/events/sse` to receive real-time responses
4. Parse events: Extract text, tool calls, and status updates from UniversalEvent objects
5. Render UI: Display content based on event types and agent capabilities

### Tool Execution Workflow
1. Send message requesting code execution
2. Receive tool_call event with function name and parameters
3. Agent executes tool and emits tool_result event
4. Continue conversation with tool results available to agent
5. Stream continues until agent completes response

### Human Approval Workflow
1. Agent emits permissionAsked event with action requiring approval
2. Display permission prompt to user with options (once/always/reject)
3. Reply via `POST /v1/sessions/{id}/permissions/{id}/reply` with chosen mode
4. Agent resumes execution with approval decision
5. Continue streaming remaining events

### Plan-Only Execution
1. Create session with agentMode="plan" and permissionMode="plan"
2. Send message to agent
3. Receive questionAsked event with plan approval options
4. User reviews plan and replies with approval/rejection
5. Agent executes approved plan or stops based on response

### Multi-Turn Conversation
1. Create initial session and send first message
2. Stream and process all events from first turn
3. Send follow-up message to same session_id
4. Agent resumes with session history via --resume/--continue flags
5. Continue streaming new events from offset

### File-Based Workflow
1. Send message requesting file operations
2. Receive file_ref content parts showing read/write/patch previews
3. Agent executes file operations in sandbox
4. Receive tool_result events confirming changes
5. Display file diffs and confirmations to user

## Integration

### Deployment Targets
- **Local development**: Run daemon on localhost with `sandbox-agent server --token "$SANDBOX_TOKEN" --host 127.0.0.1 --port 2468`
- **Docker**: Deploy in containers with volume mounts for artifacts: `docker run -p 2468:2468 -v "$PWD/artifacts:/artifacts" ...`
- **E2B**: Run daemon inside E2B sandboxes with network access and agent binary installation
- **Daytona**: Deploy in Daytona workspaces with port forwarding to expose daemon
- **Vercel Sandboxes**: Run daemon inside Vercel Sandboxes for serverless execution

### Client Integration
- **TypeScript SDK**: Use `npm install sandbox-agent` for typed client with session and event management
- **HTTP API**: Direct REST calls with Bearer token authentication via `Authorization: Bearer <token>`
- **CLI**: Mirror HTTP API with `sandbox-agent` commands for sessions, agents, and messages
- **SSE Streaming**: Integrate Server-Sent Events for real-time event consumption

### Agent Providers
- **Claude Code**: Anthropic's coding agent with JSONL streaming and session resumption
- **Codex**: OpenAI's agent with JSON-RPC over stdio and thread-based sessions
- **OpenCode**: Multi-provider agent with SSE or JSONL streaming
- **Amp**: Sourcegraph's agent with JSONL streaming and dynamic flag detection

## Context

### Universal Event Schema
All agents emit events normalized to UniversalEvent with id, timestamp, session_id, agent, and data. The schema includes UniversalMessage (role, parts, metadata), UniversalMessagePart (text, tool_call, tool_result, file, image, reasoning, status), and HITL events (questionAsked, permissionAsked).

### Agent Architecture Patterns
Subprocess agents (Claude, Amp) spawn new processes per message with automatic termination. Server agents (Codex, OpenCode) run persistent processes with multiplexed sessions via RPC. The daemon abstracts these differences through unified session management and event conversion.

### Permission and Agent Modes
Agent mode controls behavior/system prompt strategy (build/plan/custom). Permission mode controls capability restrictions (default/plan/bypass). These are independent configurations that must be set separately during session creation.

### Session State Tracking
Sessions maintain in-memory state including full event history, pending questions/permissions, and broadcaster channels for SSE streaming. Session state is not persisted to disk; clients must implement their own storage if needed.

### Event Streaming Semantics
Events are assigned monotonically increasing IDs per session. Polling uses offset/limit pagination (offset is exclusive). SSE streaming uses the same offset semantics for resumption. New events are broadcast to all SSE subscribers immediately upon recording.

### Content Part Types
Content parts include text (chat messages), tool_call (function invocation), tool_result (execution outcome), file_ref (file operations), image (visual output), reasoning (agent planning), status (progress), and error (failures). UI rendering should branch on part.type for appropriate display.

### Agent Compatibility
Claude Code supports session resumption via --resume flag. Codex uses thread IDs for multi-turn conversations. OpenCode discovers modes via server API. Amp uses --continue flag for resumption. Mock agents are built-in for testing without external dependencies.

---

> For additional documentation and navigation, see: https://sandboxagent.dev/docs/llms.txt