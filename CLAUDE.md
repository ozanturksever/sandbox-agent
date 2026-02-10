# Instructions

## SDK Modes

There are two ways to work with the SDKs:

- **Embedded**: Spawns the `sandbox-agent` server as a subprocess on a unique port and communicates with it locally. Useful for local development or when running the SDK and agent in the same environment.
- **Server**: Connects to a remotely running `sandbox-agent` server. The server is typically running inside a sandbox (e.g., Docker, E2B, Daytona, Vercel Sandboxes) and the SDK connects to it over HTTP.

## Agent Schemas

Agent schemas (Claude Code, Codex, OpenCode, Amp) are available for reference in `resources/agent-schemas/artifacts/json-schema/`.

Extraction methods:
- **Claude**: Uses `claude --output-format json --json-schema` CLI command
- **Codex**: Uses `codex app-server generate-json-schema` CLI command
- **OpenCode**: Fetches from GitHub OpenAPI spec
- **Amp**: Scrapes from `https://ampcode.com/manual/appendix?preview#message-schema`

All extractors have fallback schemas for when CLI/URL is unavailable.

Research on how different agents operate (CLI flags, streaming formats, HITL patterns, etc.) is in `research/agents/`. When adding or making changes to agent docs, follow the same structure as existing files.

Universal schema guidance:
- The universal schema should cover the full feature set of all agents.
- Conversions must be best-effort overlap without being lossy; preserve raw payloads when needed.
- **The mock agent acts as the reference implementation** for correct event behavior. Real agents should use synthetic events to match the mock agent's event patterns (e.g., emitting both daemon synthetic and agent native `session.started` events, proper `item.started` → `item.delta` → `item.completed` sequences).

## Spec Tracking

- Keep CLI subcommands in sync with every HTTP endpoint.
- Update `CLAUDE.md` to keep CLI endpoints in sync with HTTP API changes.
- When adding or modifying CLI commands, update `docs/cli.mdx` to reflect the changes.
- When changing the HTTP API, update the TypeScript SDK and CLI together.
- Do not make breaking changes to API endpoints.
- When changing API routes, ensure the HTTP/SSE test suite has full coverage of every route.
- When agent schema changes, ensure API tests cover the new schema and event shapes end-to-end.
- When the universal schema changes, update mock-agent events to cover the new fields or event types.
- Update `docs/conversion.md` whenever agent-native schema terms, synthetic events, identifier mappings, or conversion logic change.
- Never use synthetic data or mocked responses in tests.
- Never manually write agent types; always use generated types in `resources/agent-schemas/`. If types are broken, fix the generated types.
- The universal schema must provide consistent behavior across providers; avoid requiring frontend/client logic to special-case agents.
- The UI must reflect every field in AgentCapabilities (feature coverage); keep it in sync with `docs/session-transcript-schema.mdx` and `agent_capabilities_for`.
- When parsing agent data, if something is unexpected or does not match the schema, bail out and surface the error rather than trying to continue with partial parsing.
- When defining the universal schema, choose the option most compatible with native agent APIs, and add synthetics to fill gaps for other agents.
- Use `docs/session-transcript-schema.mdx` as the source of truth for schema terminology and keep it updated alongside schema changes.
- On parse failures, emit an `agent.unparsed` event (source=daemon, synthetic=true) and treat it as a test failure. Preserve raw payloads when `include_raw=true`.
- Track subagent support in `docs/conversion.md`. For now, normalize subagent activity into normal message/tool flow, but revisit explicit subagent modeling later.
- Keep the FAQ in `README.md` and `frontend/packages/website/src/components/FAQ.tsx` in sync. When adding or modifying FAQ entries, update both files.
- Update `research/wip-agent-support.md` as agent support changes are implemented.

### OpenAPI / utoipa requirements

Every `#[utoipa::path(...)]` handler function must have a doc comment where:
- The **first line** becomes the OpenAPI `summary` (short human-readable title, e.g. `"List Agents"`). This is used as the sidebar label and page heading in the docs site.
- The **remaining lines** become the OpenAPI `description` (one-sentence explanation of what the endpoint does).
- Every `responses(...)` entry must have a `description` (no empty descriptions).

When adding or modifying endpoints, regenerate `docs/openapi.json` and verify titles render correctly in the docs site.

### CLI ⇄ HTTP endpoint map (keep in sync)

- `sandbox-agent api agents list` ↔ `GET /v1/agents`
- `sandbox-agent api agents install` ↔ `POST /v1/agents/{agent}/install`
- `sandbox-agent api agents modes` ↔ `GET /v1/agents/{agent}/modes`
- `sandbox-agent api agents models` ↔ `GET /v1/agents/{agent}/models`
- `sandbox-agent api sessions list` ↔ `GET /v1/sessions`
- `sandbox-agent api sessions create` ↔ `POST /v1/sessions/{sessionId}`
- `sandbox-agent api sessions send-message` ↔ `POST /v1/sessions/{sessionId}/messages`
- `sandbox-agent api sessions send-message-stream` ↔ `POST /v1/sessions/{sessionId}/messages/stream`
- `sandbox-agent api sessions terminate` ↔ `POST /v1/sessions/{sessionId}/terminate`
- `sandbox-agent api sessions events` / `get-messages` ↔ `GET /v1/sessions/{sessionId}/events`
- `sandbox-agent api sessions events-sse` ↔ `GET /v1/sessions/{sessionId}/events/sse`
- `sandbox-agent api sessions reply-question` ↔ `POST /v1/sessions/{sessionId}/questions/{questionId}/reply`
- `sandbox-agent api sessions reject-question` ↔ `POST /v1/sessions/{sessionId}/questions/{questionId}/reject`
- `sandbox-agent api sessions reply-permission` ↔ `POST /v1/sessions/{sessionId}/permissions/{permissionId}/reply`
- `sandbox-agent api fs entries` ↔ `GET /v1/fs/entries`
- `sandbox-agent api fs read` ↔ `GET /v1/fs/file`
- `sandbox-agent api fs write` ↔ `PUT /v1/fs/file`
- `sandbox-agent api fs delete` ↔ `DELETE /v1/fs/entry`
- `sandbox-agent api fs mkdir` ↔ `POST /v1/fs/mkdir`
- `sandbox-agent api fs move` ↔ `POST /v1/fs/move`
- `sandbox-agent api fs stat` ↔ `GET /v1/fs/stat`
- `sandbox-agent api fs upload-batch` ↔ `POST /v1/fs/upload-batch`
- `sandbox-agent api terminal list` ↔ `GET /v1/terminal`
- `sandbox-agent api terminal create` ↔ `POST /v1/terminal`
- `sandbox-agent api terminal get` ↔ `GET /v1/terminal/{terminalId}`
- `sandbox-agent api terminal delete` ↔ `DELETE /v1/terminal/{terminalId}`
- `sandbox-agent api terminal resize` ↔ `POST /v1/terminal/{terminalId}/resize`
- `sandbox-agent api terminal connect` ↔ `GET /v1/terminal/{terminalId}/ws` (WebSocket)

## OpenCode Compatibility Layer

`sandbox-agent opencode` starts a sandbox-agent server and attaches an OpenCode session (uses `/opencode`).

### Session ownership

Sessions are stored **only** in sandbox-agent's v1 `SessionManager` — they are never sent to or stored in the native OpenCode server. The OpenCode TUI reads sessions via `GET /session` which the compat layer serves from the v1 store. The native OpenCode process has no knowledge of sessions.

### Proxy elimination strategy

The `/opencode` compat layer (`opencode_compat.rs`) historically proxied many endpoints to the native OpenCode server via `proxy_native_opencode()`. The goal is to **eliminate proxying** by implementing each endpoint natively using the v1 `SessionManager` as the single source of truth.

**Already de-proxied** (use v1 SessionManager directly):
- `GET /session` — `oc_session_list` reads from `SessionManager::list_sessions()`
- `GET /session/{id}` — `oc_session_get` reads from `SessionManager::get_session_info()`
- `GET /session/status` — `oc_session_status` derives busy/idle from v1 session `ended` flag
- `POST /tui/open-sessions` — returns `true` directly (TUI fetches sessions from `GET /session`)
- `POST /tui/select-session` — emits `tui.session.select` event via the OpenCode event broadcaster

**Still proxied** (none of these reference session IDs or the session list — all are session-agnostic):
- `GET /command` — command list
- `GET /config`, `PATCH /config` — project config read/write
- `GET /global/config`, `PATCH /global/config` — global config read/write
- `GET /tui/control/next`, `POST /tui/control/response` — TUI control loop
- `POST /tui/append-prompt`, `/tui/submit-prompt`, `/tui/clear-prompt` — prompt management
- `POST /tui/open-help`, `/tui/open-themes`, `/tui/open-models` — TUI navigation
- `POST /tui/execute-command`, `/tui/show-toast`, `/tui/publish` — TUI actions

When converting a proxied endpoint: add needed fields to `SessionState`/`SessionInfo` in `router.rs`, implement the logic natively in `opencode_compat.rs`, and use `session_info_to_opencode_value()` to format responses.

## Post-Release Testing

After cutting a release, verify the release works correctly. Run `/project:post-release-testing` to execute the testing agent.

## OpenCode Compatibility Tests

The OpenCode compatibility suite lives at `server/packages/sandbox-agent/tests/opencode-compat` and validates the `@opencode-ai/sdk` against the `/opencode` API. Run it with:

```bash
SANDBOX_AGENT_SKIP_INSPECTOR=1 pnpm --filter @sandbox-agent/opencode-compat-tests test
```

## Naming

- The product name is "Gigacode" (capital G, lowercase c). The CLI binary/package is `gigacode` (lowercase).

## Git Commits

- Do not include any co-authors in commit messages (no `Co-Authored-By` lines)
- Use conventional commits style (e.g., `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`)
- Keep commit messages to a single line
