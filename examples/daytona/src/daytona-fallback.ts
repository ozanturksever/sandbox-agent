import { Daytona, Image } from "@daytonaio/sdk";
import { logInspectorUrl, runPrompt } from "@sandbox-agent/example-shared";

if (
	!process.env.DAYTONA_API_KEY ||
	(!process.env.OPENAI_API_KEY && !process.env.ANTHROPIC_API_KEY)
) {
	throw new Error(
		"DAYTONA_API_KEY and (OPENAI_API_KEY or ANTHROPIC_API_KEY) required",
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
if (process.env.ANTHROPIC_API_KEY) envVars.ANTHROPIC_API_KEY = process.env.ANTHROPIC_API_KEY;
if (process.env.OPENAI_API_KEY) envVars.OPENAI_API_KEY = process.env.OPENAI_API_KEY;

const sandbox = await daytona.create({
	snapshot: SNAPSHOT,
	envVars,
});

console.log("Starting server...");
await sandbox.process.executeCommand(
	"nohup sandbox-agent server --no-token --host 0.0.0.0 --port 3000 >/tmp/sandbox-agent.log 2>&1 &",
);

const baseUrl = (await sandbox.getSignedPreviewUrl(3000, 4 * 60 * 60)).url;
logInspectorUrl({ baseUrl });

const cleanup = async () => {
	console.log("Cleaning up...");
	await sandbox.delete(60);
	process.exit(0);
};
process.once("SIGINT", cleanup);
process.once("SIGTERM", cleanup);

await runPrompt({ baseUrl });
await cleanup();
