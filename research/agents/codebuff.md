# Codebuff Research

Research notes on Codebuff's configuration, credential discovery, and runtime behavior for sandbox-agent integration.

## Overview

- **Provider**: Codebuff (Anthropic-based)
- **Execution Method**: CLI subprocess (`codebuff` command)
- **Session Persistence**: Session ID (via `--continue`)
- **SDK**: TypeScript SDK with streaming support

## Credential Discovery

### Priority Order

1. Environment variables: `CODEBUFF_API_KEY`
2. Config file: `~/.codebuff/config.json`
3. Auth file: `~/.codebuff/auth.json`

### Config File Locations

| Path | Description |
|------|-------------|
| `~/.codebuff/config.json` | Main configuration file |
| `~/.codebuff/auth.json` | Authentication tokens |

### API Key Field Names (checked in order)

```json
{
  "apiKey": "...",
  "token": "..."
}
```

## CLI Invocation

### Command Structure

```bash
codebuff \
  --stream-json \
  [--free | --max | --plan] \
  [--continue SESSION_ID] \
  [--timeout SECONDS] \
  [--cwd DIRECTORY] \
  "PROMPT"
```

### Arguments

| Flag | Description |
|------|-------------|
| `-n, --non-interactive` | Run without TUI, stream to stdout |
| `--json` | Output structured JSON at end (implies --non-interactive) |
| `--stream-json` | Output streaming JSONL events (one JSON per line, for sandbox-agent) |
| `--free` | Start in FREE mode (lighter model) |
| `--max` | Start in MAX mode (full capability) |
| `--plan` | Start in PLAN mode (planning first) |
| `--continue [id]` | Continue from previous conversation |
| `--timeout <seconds>` | Timeout for non-interactive mode |
| `--cwd <directory>` | Set working directory |
| `-q, --quiet` | Suppress streaming output |

### Environment Variables

- `CODEBUFF_API_KEY` - API key for authentication

## Streaming Response Format

Codebuff CLI in `--json` mode outputs newline-delimited JSON events (PrintModeEvent):

```json
{"type": "start", "agentId": "...", "model": "...", "messageHistoryLength": 0}
{"type": "text", "text": "...", "agentId": "..."}
{"type": "tool_call", "toolCallId": "...", "toolName": "...", "input": {...}}
{"type": "tool_result", "toolCallId": "...", "toolName": "...", "output": [...]}
{"type": "subagent_start", "agentId": "...", "agentType": "...", "displayName": "..."}
{"type": "subagent_finish", "agentId": "...", "agentType": "...", "displayName": "..."}
{"type": "reasoning_delta", "text": "...", "runId": "...", "ancestorRunIds": [...]}
{"type": "error", "message": "..."}
{"type": "finish", "agentId": "...", "totalCost": 0.0}
```

### Event Types

| Type | Description |
|------|-------------|
| `start` | Session started with agent info |
| `text` | Streaming text content |
| `tool_call` | Tool invocation |
| `tool_result` | Tool execution result |
| `tool_progress` | Partial tool output |
| `subagent_start` | Subagent spawned |
| `subagent_finish` | Subagent completed |
| `reasoning_delta` | Reasoning/thinking content |
| `error` | Error occurred |
| `finish` | Session completed with cost info |
| `download` | Binary download status |

### PrintModeEvent Schema

```typescript
type PrintModeEvent =
  | { type: 'start'; agentId?: string; model?: string; messageHistoryLength: number }
  | { type: 'error'; message: string }
  | { type: 'download'; version: string; status: 'complete' | 'failed' }
  | { type: 'tool_call'; toolCallId: string; toolName: string; input: Record<string, any>; agentId?: string; parentAgentId?: string }
  | { type: 'tool_result'; toolCallId: string; toolName: string; output: ToolResultOutput[]; parentAgentId?: string }
  | { type: 'tool_progress'; toolCallId: string; toolName: string; output: string; parentAgentId?: string }
  | { type: 'text'; text: string; agentId?: string }
  | { type: 'finish'; agentId?: string; totalCost: number }
  | { type: 'subagent_start'; agentId: string; agentType: string; displayName: string; model?: string; onlyChild: boolean; parentAgentId?: string; params?: Record<string, any>; prompt?: string }
  | { type: 'subagent_finish'; agentId: string; agentType: string; displayName: string; model?: string; onlyChild: boolean; parentAgentId?: string; params?: Record<string, any>; prompt?: string }
  | { type: 'reasoning_delta'; text: string; ancestorRunIds: string[]; runId: string }
```

## Agent Modes

| Mode | CLI Flag | Description |
|------|----------|-------------|
| `FREE` | `--free` | Lighter model, faster responses |
| `DEFAULT` | (none) | Standard balanced mode |
| `MAX` | `--max` | Full capability mode |
| `PLAN` | `--plan` | Planning-first mode |

## Session Management

- Session ID captured from `start` event or conversation state
- Use `--continue SESSION_ID` to resume a session
- Sessions stored internally by Codebuff CLI

## Question/Permission Handling

- Questions are handled via the `ask_user` tool
- Tool calls with `ask_user` emit question events
- Tool results resolve the question
- No native permission prompts (uses dangerously-skip-permissions equivalent)

## Conversion to Universal Format

Codebuff events are converted via `convert_codebuff::event_to_universal()`:

1. `start` → `session.started` with metadata
2. `text` → `item.delta` (streaming) + `item.completed` (final)
3. `reasoning_delta` → `item.delta` with reasoning visibility
4. `tool_call` → `item.started` + `item.completed` (ToolCall kind)
5. `tool_result` → `item.started` + `item.completed` (ToolResult kind)
6. `tool_progress` → `item.delta` for tool
7. `subagent_start` → `item.started` (Status kind with subagent info)
8. `subagent_finish` → `item.completed` (Status kind)
9. `error` → `error` event
10. `finish` → `session.ended` with cost metadata

## Notes

- Codebuff CLI manages its own authentication internally
- stdin is used for prompt input in non-interactive mode
- Working directory is set via `--cwd` option
- Subagents are spawned as nested items within the conversation
- Cost tracking is provided via `totalCost` in finish event
