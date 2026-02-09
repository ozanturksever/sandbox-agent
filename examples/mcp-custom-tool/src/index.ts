import { SandboxAgent } from "sandbox-agent";
import { detectAgent, buildInspectorUrl, generateSessionId } from "@sandbox-agent/example-shared";
import { startDockerSandbox } from "@sandbox-agent/example-shared/docker";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Verify the bundled MCP server exists (built by `pnpm build:mcp`).
const serverFile = path.resolve(__dirname, "../dist/mcp-server.cjs");
if (!fs.existsSync(serverFile)) {
  console.error("Error: dist/mcp-server.cjs not found. Run `pnpm build:mcp` first.");
  process.exit(1);
}

// Start a Docker container running sandbox-agent.
console.log("Starting sandbox...");
const { baseUrl, cleanup } = await startDockerSandbox({ port: 3004 });

// Upload the bundled MCP server into the sandbox filesystem.
console.log("Uploading MCP server bundle...");
const client = await SandboxAgent.connect({ baseUrl });

const bundle = await fs.promises.readFile(serverFile);
const written = await client.writeFsFile(
  { path: "/opt/mcp/custom-tools/mcp-server.cjs" },
  bundle,
);
console.log(`  Written: ${written.path} (${written.bytesWritten} bytes)`);

// Create a session with the uploaded MCP server as a local command.
console.log("Creating session with custom MCP tool...");
const sessionId = generateSessionId();
await client.createSession(sessionId, {
  agent: detectAgent(),
  mcp: {
    customTools: {
      type: "local",
      command: ["node", "/opt/mcp/custom-tools/mcp-server.cjs"],
    },
  },
});
console.log(`  UI: ${buildInspectorUrl({ baseUrl, sessionId })}`);
console.log('  Try: "generate a random number between 1 and 100"');
console.log("  Press Ctrl+C to stop.");

const keepAlive = setInterval(() => {}, 60_000);
process.on("SIGINT", () => { clearInterval(keepAlive); cleanup().then(() => process.exit(0)); });
