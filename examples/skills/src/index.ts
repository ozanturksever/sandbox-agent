import { SandboxAgent } from "sandbox-agent";
import { detectAgent, buildInspectorUrl, generateSessionId } from "@sandbox-agent/example-shared";
import { startDockerSandbox } from "@sandbox-agent/example-shared/docker";

console.log("Starting sandbox...");
const { baseUrl, cleanup } = await startDockerSandbox({
  port: 3001,
});

console.log("Creating session with skill source...");
const client = await SandboxAgent.connect({ baseUrl });
const sessionId = generateSessionId();
await client.createSession(sessionId, {
  agent: detectAgent(),
  skills: {
    sources: [
      { type: "github", source: "rivet-dev/skills", skills: ["sandbox-agent"] },
    ],
  },
});
console.log(`  UI: ${buildInspectorUrl({ baseUrl, sessionId })}`);
console.log('  Try: "How do I start sandbox-agent?"');
console.log("  Press Ctrl+C to stop.");

const keepAlive = setInterval(() => {}, 60_000);
process.on("SIGINT", () => { clearInterval(keepAlive); cleanup().then(() => process.exit(0)); });
