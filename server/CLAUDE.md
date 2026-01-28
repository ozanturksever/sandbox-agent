# Server Testing

## Test placement

Place all new tests under `server/packages/**/tests/` (or a package-specific `tests/` folder). Avoid inline tests inside source files unless there is no viable alternative.

## Test locations (overview)

- Sandbox-agent integration tests live under `server/packages/sandbox-agent/tests/`:
  - Agent flow coverage in `agent-flows/`
  - Agent management coverage in `agent-management/`
  - Shared server manager coverage in `server-manager/`
  - HTTP/SSE and snapshot coverage in `http/` (snapshots in `http/snapshots/`)
  - UI coverage in `ui/`
  - Shared helpers in `common/`
- Extracted agent schema roundtrip tests live under `server/packages/extracted-agent-schemas/tests/`

## Snapshot tests

The HTTP/SSE snapshot suite entrypoint lives in:
- `server/packages/sandbox-agent/tests/http_sse_snapshots.rs` (includes `tests/http/http_sse_snapshots.rs`)

Snapshots are written to:
- `server/packages/sandbox-agent/tests/http/snapshots/`

## Agent selection

`SANDBOX_TEST_AGENTS` controls which agents run. It accepts a comma-separated list or `all`.
If it is **not set**, tests will auto-detect installed agents by checking:
- binaries on `PATH`, and
- the default install dir (`$XDG_DATA_HOME/sandbox-agent/bin` or `./.sandbox-agent/bin`)

If no agents are found, tests fail with a clear error.

## Credential handling

Credentials are pulled from the host by default via `extract_all_credentials`:
- environment variables (e.g. `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`)
- local CLI configs (Claude/Codex/Amp/OpenCode)

You can override host credentials for tests with:
- `SANDBOX_TEST_ANTHROPIC_API_KEY`
- `SANDBOX_TEST_OPENAI_API_KEY`

If `SANDBOX_TEST_AGENTS` includes an agent that requires a provider credential and it is missing,
tests fail before starting.

## Credential health checks

Before running agent tests, credentials are validated with minimal API calls:
- Anthropic: `GET https://api.anthropic.com/v1/models`
  - `x-api-key` for API keys
  - `Authorization: Bearer` for OAuth tokens
  - `anthropic-version: 2023-06-01`
- OpenAI: `GET https://api.openai.com/v1/models` with `Authorization: Bearer`

401/403 yields a hard failure (`invalid credentials`). Other non-2xx responses or network
errors fail with a health-check error.

Health checks run in a blocking thread to avoid Tokio runtime drop errors inside async tests.

## Snapshot stability

To keep snapshots deterministic:
- Use the mock agent as the **master** event sequence; all other agents must match its behavior 1:1.
- Snapshots should compare a **canonical event skeleton** (event order matters) with strict ordering across:
  - `item.started` → `item.delta` → `item.completed`
  - presence/absence of `session.ended`
  - permission/question request and resolution flows
- Scrub non-deterministic fields from snapshots:
  - IDs, timestamps, native IDs
  - text content, tool inputs/outputs, provider-specific metadata
  - `source` and `synthetic` flags (these are implementation details)
- The sandbox-agent is responsible for emitting **synthetic events** so that real agents match the mock sequence exactly.
- Event streams are truncated after the first assistant or error event.
- Permission flow snapshots are truncated after the permission request (or first assistant) event.
- Unknown events are preserved as `kind: unknown` (raw payload in universal schema).
- Prefer snapshot-based event skeleton assertions over manual event-order assertions in tests.

## Typical commands

Run only Claude snapshots:
```
SANDBOX_TEST_AGENTS=claude cargo test -p sandbox-agent --test http_sse_snapshots
```

Run all detected agents:
```
cargo test -p sandbox-agent --test http_sse_snapshots
```

## Universal Schema

When modifying agent conversion code in `server/packages/universal-agent-schema/src/agents/` or adding/changing properties on the universal schema, update the feature matrix in `README.md` to reflect which agents support which features.
