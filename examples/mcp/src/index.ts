import { SandboxAgent } from "sandbox-agent";
import { detectAgent, buildInspectorUrl, generateSessionId } from "@sandbox-agent/example-shared";
import { startDockerSandbox } from "@sandbox-agent/example-shared/docker";

console.log("Starting sandbox...");
const { baseUrl, cleanup } = await startDockerSandbox({
  port: 3002,
  setupCommands: [
    "npm install -g --silent @modelcontextprotocol/server-everything@2026.1.26",
  ],
});

console.log("Creating session with everything MCP server...");
const client = await SandboxAgent.connect({ baseUrl });
const sessionId = generateSessionId();
await client.createSession(sessionId, {
  agent: detectAgent(),
  mcp: {
    everything: {
      type: "local",
      command: ["mcp-server-everything"],
      timeoutMs: 10000,
    },
  },
});
console.log(`  UI: ${buildInspectorUrl({ baseUrl, sessionId })}`);
console.log('  Try: "generate a random number between 1 and 100"');
console.log("  Press Ctrl+C to stop.");

const keepAlive = setInterval(() => {}, 60_000);
process.on("SIGINT", () => { clearInterval(keepAlive); cleanup().then(() => process.exit(0)); });
