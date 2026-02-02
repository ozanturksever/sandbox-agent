import { Sandbox } from "@e2b/code-interpreter";
import { logInspectorUrl, runPrompt, waitForHealth } from "@sandbox-agent/example-shared";

export async function setupE2BSandboxAgent(): Promise<{
  baseUrl: string;
  token?: string;
  cleanup: () => Promise<void>;
}> {
  const envs: Record<string, string> = {};
  if (process.env.ANTHROPIC_API_KEY) envs.ANTHROPIC_API_KEY = process.env.ANTHROPIC_API_KEY;
  if (process.env.OPENAI_API_KEY) envs.OPENAI_API_KEY = process.env.OPENAI_API_KEY;

  const sandbox = await Sandbox.create({ allowInternetAccess: true, envs });
  const run = async (cmd: string) => {
    const result = await sandbox.commands.run(cmd);
    if (result.exitCode !== 0) throw new Error(`Command failed: ${cmd}\n${result.stderr}`);
    return result;
  };

  console.log("Installing sandbox-agent...");
  await run("curl -fsSL https://releases.rivet.dev/sandbox-agent/latest/install.sh | sh");

  console.log("Installing agents...");
  await run("sandbox-agent install-agent claude");
  await run("sandbox-agent install-agent codex");

  console.log("Starting server...");
  await sandbox.commands.run("sandbox-agent server --no-token --host 0.0.0.0 --port 3000", { background: true });

  const baseUrl = `https://${sandbox.getHost(3000)}`;

  // Wait for server to be ready
  console.log("Waiting for server...");
  await waitForHealth({ baseUrl });

  const cleanup = async () => {
    console.log("Cleaning up...");
    await sandbox.kill();
  };

  return { baseUrl, cleanup };
}

// Run interactively if executed directly
const isMainModule = import.meta.url === `file://${process.argv[1]}`;
if (isMainModule) {
  if (!process.env.OPENAI_API_KEY && !process.env.ANTHROPIC_API_KEY) {
    throw new Error("E2B_API_KEY and (OPENAI_API_KEY or ANTHROPIC_API_KEY) required");
  }

  const { baseUrl, cleanup } = await setupE2BSandboxAgent();
  logInspectorUrl({ baseUrl });

  process.once("SIGINT", async () => {
    await cleanup();
    process.exit(0);
  });
  process.once("SIGTERM", async () => {
    await cleanup();
    process.exit(0);
  });

  await runPrompt(baseUrl);
  await cleanup();
}
