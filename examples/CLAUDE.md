# Examples Instructions

## Docker Isolation

- Docker examples must behave like standalone sandboxes.
- Do not bind mount host files or host directories into Docker example containers.
- If an example needs tools, skills, or MCP servers, install them inside the container during setup.

## Testing Examples

Examples can be tested by starting them in the background and communicating directly with the sandbox-agent API:

1. Start the example: `SANDBOX_AGENT_DEV=1 pnpm start &`
2. Note the base URL and session ID from the output.
3. Send messages: `curl -X POST http://127.0.0.1:<port>/v1/sessions/<sessionId>/messages -H "Content-Type: application/json" -d '{"message":"..."}'`
4. Poll events: `curl http://127.0.0.1:<port>/v1/sessions/<sessionId>/events`
5. Approve permissions: `curl -X POST http://127.0.0.1:<port>/v1/sessions/<sessionId>/permissions/<permissionId>/reply -H "Content-Type: application/json" -d '{"reply":"once"}'`
