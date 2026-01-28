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

console.log(
	"\x1b[33m[NOTE]\x1b[0m Daytona Tier 3+ required to access api.anthropic.com and api.openai.com.\n" +
		"       Tier 1/2 sandboxes have restricted network access that will cause 'Agent Process Exited' errors.\n" +
		"       See: https://www.daytona.io/docs/en/network-limits/\n",
);

const SNAPSHOT = "sandbox-agent-ready-v2";

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
				// Install agents
				"sandbox-agent install-agent claude",
				"sandbox-agent install-agent codex",
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

// NOTE: Tier 1/2 sandboxes have restricted network access that cannot be overridden
// If you're on Tier 1/2 and see "Agent Process Exited", contact Daytona to whitelist
// api.anthropic.com and api.openai.com for your organization
// See: https://www.daytona.io/docs/en/network-limits/
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
console.log("Sandbox env:", envCheck.result || "(none)");

const binCheck = await sandbox.process.executeCommand(
	"ls -la /root/.local/share/sandbox-agent/bin/",
);
console.log("Agent binaries:", binCheck.result);

// Network connectivity test
console.log("Testing network connectivity...");
const netTest = await sandbox.process.executeCommand(
	"curl -s -o /dev/null -w '%{http_code}' --connect-timeout 5 https://api.anthropic.com/v1/messages 2>&1 || echo 'FAILED'",
);
const httpCode = netTest.result?.trim();
if (httpCode === "405" || httpCode === "401") {
	console.log("api.anthropic.com: reachable");
} else if (httpCode === "000" || httpCode === "FAILED" || !httpCode) {
	console.log("\x1b[31mapi.anthropic.com: UNREACHABLE - Tier 1/2 network restriction detected\x1b[0m");
	console.log("Claude/Codex will fail. Upgrade to Tier 3+ or contact Daytona support.");
} else {
	console.log(`api.anthropic.com: ${httpCode}`);
}

const baseUrl = (await sandbox.getSignedPreviewUrl(3000, 4 * 60 * 60)).url;
logInspectorUrl({ baseUrl });

const cleanup = async () => {
	// Show server logs before cleanup
	const logs = await sandbox.process.executeCommand(
		"cat /tmp/sandbox-agent.log 2>/dev/null | tail -50",
	);
	if (logs.result) {
		console.log("\n--- Server logs ---");
		console.log(logs.result);
	}
	console.log("Cleaning up...");
	await sandbox.delete(60);
	process.exit(0);
};
process.once("SIGINT", cleanup);
process.once("SIGTERM", cleanup);

await runPrompt({ baseUrl });
await cleanup();
