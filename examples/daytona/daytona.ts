import { Daytona } from "@daytonaio/sdk";
import { pathToFileURL, fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import {
  ensureUrl,
  logInspectorUrl,
  runPrompt,
  waitForHealth,
} from "@sandbox-agent/example-shared";

const DEFAULT_PORT = 3000;
const BINARY_PATH = resolve(dirname(fileURLToPath(import.meta.url)), "../../target/release/sandbox-agent");

export async function setupDaytonaSandboxAgent(): Promise<{
  baseUrl: string;
  token: string;
  extraHeaders: Record<string, string>;
  cleanup: () => Promise<void>;
}> {
  const token = process.env.SANDBOX_TOKEN || "";
  const port = Number.parseInt(process.env.SANDBOX_PORT || "", 10) || DEFAULT_PORT;
  const language = process.env.DAYTONA_LANGUAGE || "typescript";

  const daytona = new Daytona();
  console.log("Creating sandbox...");
  const sandbox = await daytona.create({ language });

  // Daytona sandboxes can't reach releases.rivet.dev, so upload binary directly
  console.log("Uploading sandbox-agent...");
  await sandbox.fs.uploadFile(BINARY_PATH, "/home/daytona/sandbox-agent");
  await sandbox.process.executeCommand("chmod +x /home/daytona/sandbox-agent");

  console.log("Starting server...");
  const tokenFlag = token ? `--token ${token}` : "--no-token";
  await sandbox.process.executeCommand(
    `nohup /home/daytona/sandbox-agent server ${tokenFlag} --host 0.0.0.0 --port ${port} >/tmp/sandbox-agent.log 2>&1 &`
  );

  const preview = await sandbox.getPreviewLink(port);
  const extraHeaders: Record<string, string> = {
    "x-daytona-skip-preview-warning": "true",
  };
  if (preview.token) {
    extraHeaders["x-daytona-preview-token"] = preview.token;
  }

  const baseUrl = ensureUrl(preview.url);
  console.log("Waiting for health...");
  await waitForHealth({ baseUrl, token, extraHeaders });
  logInspectorUrl({ baseUrl, token });

  return {
    baseUrl,
    token,
    extraHeaders,
    cleanup: async () => {
      try { await sandbox.delete(60); } catch {}
    },
  };
}

async function main(): Promise<void> {
  const { baseUrl, token, extraHeaders, cleanup } = await setupDaytonaSandboxAgent();

  process.on("SIGINT", () => void cleanup().then(() => process.exit(0)));
  process.on("SIGTERM", () => void cleanup().then(() => process.exit(0)));

  await runPrompt({ baseUrl, token, extraHeaders });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}
