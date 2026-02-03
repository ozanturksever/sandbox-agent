import { getSandbox, type Sandbox } from "@cloudflare/sandbox";

export { Sandbox } from "@cloudflare/sandbox";

type Env = {
  Bindings: {
    Sandbox: DurableObjectNamespace<Sandbox>;
    ASSETS: Fetcher;
    ANTHROPIC_API_KEY?: string;
    OPENAI_API_KEY?: string;
  };
};

const PORT = 8000;

/** Check if sandbox-agent is already running by probing its health endpoint */
async function isServerRunning(sandbox: Sandbox): Promise<boolean> {
  try {
    const result = await sandbox.exec(`curl -sf http://localhost:${PORT}/v1/health`);
    return result.success;
  } catch {
    return false;
  }
}

/** Ensure sandbox-agent is running in the container */
async function ensureRunning(sandbox: Sandbox, env: Env["Bindings"]): Promise<void> {
  if (await isServerRunning(sandbox)) return;

  // Set environment variables for agents
  const envVars: Record<string, string> = {};
  if (env.ANTHROPIC_API_KEY) envVars.ANTHROPIC_API_KEY = env.ANTHROPIC_API_KEY;
  if (env.OPENAI_API_KEY) envVars.OPENAI_API_KEY = env.OPENAI_API_KEY;
  await sandbox.setEnvVars(envVars);

  // Start sandbox-agent server as background process
  await sandbox.startProcess(`sandbox-agent server --no-token --host 0.0.0.0 --port ${PORT}`);

  // Poll health endpoint until server is ready (max ~6 seconds)
  for (let i = 0; i < 30; i++) {
    if (await isServerRunning(sandbox)) return;
    await new Promise((r) => setTimeout(r, 200));
  }
}

export default {
  async fetch(request: Request, env: Env["Bindings"]): Promise<Response> {
    const url = new URL(request.url);

    // Proxy requests to sandbox-agent: /sandbox/:name/v1/...
    const match = url.pathname.match(/^\/sandbox\/([^/]+)(\/.*)?$/);
    if (match) {
      if (!env.ANTHROPIC_API_KEY && !env.OPENAI_API_KEY) {
        return Response.json(
          { error: "ANTHROPIC_API_KEY or OPENAI_API_KEY must be set" },
          { status: 500 }
        );
      }

      const name = match[1];
      const path = match[2] || "/";
      const sandbox = getSandbox(env.Sandbox, name);

      await ensureRunning(sandbox, env);

      // Proxy request to container
      return sandbox.containerFetch(
        new Request(`http://localhost${path}${url.search}`, request),
        PORT
      );
    }

    // Serve frontend assets
    return env.ASSETS.fetch(request);
  },
} satisfies ExportedHandler<Env["Bindings"]>;
