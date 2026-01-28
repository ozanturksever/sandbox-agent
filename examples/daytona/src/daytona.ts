import { Daytona, Image } from "@daytonaio/sdk";
import { logInspectorUrl, runPrompt } from "@sandbox-agent/example-shared";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

// Extract API key from Claude's config files
function getAnthropicApiKey(): string | undefined {
	if (process.env.ANTHROPIC_API_KEY) return process.env.ANTHROPIC_API_KEY;

	const home = homedir();
	const configPaths = [
		join(home, ".claude.json"),
		join(home, ".claude.json.api"),
	];

	for (const path of configPaths) {
		try {
			const data = JSON.parse(readFileSync(path, "utf-8"));
			const key = data.primaryApiKey || data.apiKey || data.anthropicApiKey;
			if (key?.startsWith("sk-ant-")) return key;
		} catch {
			// Ignore errors
		}
	}
	return undefined;
}

const anthropicKey = getAnthropicApiKey();
const openaiKey = process.env.OPENAI_API_KEY;

if (!process.env.DAYTONA_API_KEY || (!anthropicKey && !openaiKey)) {
	throw new Error(
		"DAYTONA_API_KEY and (ANTHROPIC_API_KEY or OPENAI_API_KEY) required",
	);
}

const SNAPSHOT = "sandbox-agent-ready";
const AGENT_BIN_DIR = "/root/.local/share/sandbox-agent/bin";

const daytona = new Daytona();

const hasSnapshot = await daytona.snapshot.get(SNAPSHOT).then(
	() => true,
	() => false,
);
if (!hasSnapshot) {
	console.log(`Creating snapshot '${SNAPSHOT}' (one-time setup, ~2-3min)...`);
	await daytona.snapshot.create(
		{
			name: SNAPSHOT,
			image: Image.base("ubuntu:22.04").runCommands(
				// Install dependencies
				"apt-get update && apt-get install -y curl ca-certificates",
				// Install sandbox-agent
				"curl -fsSL https://releases.rivet.dev/sandbox-agent/latest/install.sh | sh",
				// Create agent bin directory
				`mkdir -p ${AGENT_BIN_DIR}`,
				// Install Claude: get latest version, download binary
				`CLAUDE_VERSION=$(curl -fsSL https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases/latest) && ` +
					`curl -fsSL -o ${AGENT_BIN_DIR}/claude "https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases/$CLAUDE_VERSION/linux-x64/claude" && ` +
					`chmod +x ${AGENT_BIN_DIR}/claude`,
				// Install Codex: download tarball, extract binary
				`curl -fsSL -L https://github.com/openai/codex/releases/latest/download/codex-x86_64-unknown-linux-musl.tar.gz | tar -xzf - -C /tmp && ` +
					`find /tmp -name 'codex-x86_64-unknown-linux-musl' -exec mv {} ${AGENT_BIN_DIR}/codex \\; && ` +
					`chmod +x ${AGENT_BIN_DIR}/codex`,
			),
		},
		{ onLogs: (log) => console.log(`  ${log}`) },
	);
	console.log("Snapshot created. Future runs will be instant.");
}

console.log("Creating sandbox...");
const envVars: Record<string, string> = {};
if (anthropicKey) envVars.ANTHROPIC_API_KEY = anthropicKey;
if (openaiKey) envVars.OPENAI_API_KEY = openaiKey;

const sandbox = await daytona.create({
	snapshot: SNAPSHOT,
	envVars,
});

console.log("Starting server...");
await sandbox.process.executeCommand(
	"nohup sandbox-agent server --no-token --host 0.0.0.0 --port 3000 >/tmp/sandbox-agent.log 2>&1 &",
);

// Wait for server to be ready
await new Promise((r) => setTimeout(r, 2000));

// Debug: check environment and agent binaries
const envCheck = await sandbox.process.executeCommand(
	"env | grep -E 'ANTHROPIC|OPENAI' | sed 's/=.*/=<set>/'",
);
console.log("Sandbox env:", envCheck.result.output || "(none)");

const binCheck = await sandbox.process.executeCommand(
	`ls -la ${AGENT_BIN_DIR}/`,
);
console.log("Agent binaries:", binCheck.result.output);

const baseUrl = (await sandbox.getSignedPreviewUrl(3000, 4 * 60 * 60)).url;
logInspectorUrl({ baseUrl });

const cleanup = async () => {
	// Show server logs before cleanup
	const logs = await sandbox.process.executeCommand(
		"cat /tmp/sandbox-agent.log 2>/dev/null | tail -50",
	);
	if (logs.result.output) {
		console.log("\n--- Server logs ---");
		console.log(logs.result.output);
	}
	console.log("Cleaning up...");
	await sandbox.delete(60);
	process.exit(0);
};
process.once("SIGINT", cleanup);
process.once("SIGTERM", cleanup);

await runPrompt({ baseUrl });
await cleanup();
