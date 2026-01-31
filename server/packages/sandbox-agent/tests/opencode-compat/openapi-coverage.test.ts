import { describe, it, expect, beforeAll, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSandboxAgent, buildSandboxAgent, type SandboxAgentHandle } from "./helpers/spawn";

type Endpoint = { method: string; path: string };

type RequestCase = {
  method: string;
  path: string;
  query?: Record<string, string>;
  body?: unknown;
  expectStatus?: number;
  sse?: boolean;
};

function extractEndpoints(): Endpoint[] {
  const __dirname = dirname(fileURLToPath(import.meta.url));
  const sdkPath = resolve(__dirname, "node_modules/@opencode-ai/sdk/dist/v2/gen/sdk.gen.js");
  const content = readFileSync(sdkPath, "utf8");
  const re = /\.((?:get|post|patch|delete|put))\(\{\s*\n\s*url: \"([^\"]+)\"/g;
  const endpoints: Endpoint[] = [];
  const seen = new Set<string>();
  let match: RegExpExecArray | null;
  while ((match = re.exec(content))) {
    const method = match[1].toUpperCase();
    const path = match[2];
    const key = `${method} ${path}`;
    if (!seen.has(key)) {
      seen.add(key);
      endpoints.push({ method, path });
    }
  }
  endpoints.sort((a, b) => a.path.localeCompare(b.path) || a.method.localeCompare(b.method));
  return endpoints;
}

async function readJsonSafe(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

async function readSse(response: Response): Promise<unknown> {
  if (!response.body) return null;
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    if (buffer.includes("\n\n")) break;
  }
  await reader.cancel();
  const block = buffer.split("\n\n")[0];
  const lines = block.split("\n");
  const eventLine = lines.find((line) => line.startsWith("event:"));
  const dataLine = lines.find((line) => line.startsWith("data:"));
  let data: unknown = null;
  if (dataLine) {
    const payload = dataLine.replace(/^data:\s*/, "").trim();
    try {
      data = JSON.parse(payload);
    } catch {
      data = payload;
    }
  }
  return { event: eventLine?.replace(/^event:\s*/, ""), data };
}

function resolvePath(path: string, ids: Record<string, string>): string {
  return path
    .replace("{sessionID}", ids.sessionID)
    .replace("{messageID}", ids.messageID)
    .replace("{partID}", ids.partID)
    .replace("{ptyID}", ids.ptyID)
    .replace("{projectID}", ids.projectID)
    .replace("{providerID}", ids.providerID)
    .replace("{requestID}", ids.requestID)
    .replace("{permissionID}", ids.permissionID)
    .replace("{questionID}", ids.questionID)
    .replace("{name}", ids.name);
}

function buildRequestCases(endpoints: Endpoint[], ids: Record<string, string>): RequestCase[] {
  const cases: RequestCase[] = [];
  for (const endpoint of endpoints) {
    const key = `${endpoint.method} ${endpoint.path}`;
    const path = resolvePath(endpoint.path, ids);
    const req: RequestCase = { method: endpoint.method, path };

    if (endpoint.path === "/event" || endpoint.path === "/global/event") {
      req.sse = true;
    }

    if (endpoint.path === "/find") {
      req.query = { pattern: "test" };
    }
    if (endpoint.path === "/find/file") {
      req.query = { query: "test" };
    }
    if (endpoint.path === "/find/symbol") {
      req.query = { query: "test" };
    }
    if (endpoint.path === "/file/content") {
      req.query = { path: "README.md" };
    }
    if (endpoint.path === "/experimental/tool") {
      req.query = { provider: "stub", model: "stub" };
    }

    switch (key) {
      case "POST /session":
        req.body = { title: "Test Session" };
        break;
      case "PATCH /session/{sessionID}":
        req.body = { title: "Updated" };
        break;
      case "POST /session/{sessionID}/message":
        req.body = { parts: [{ type: "text", text: "Hello" }] };
        break;
      case "POST /session/{sessionID}/prompt_async":
        req.body = { parts: [{ type: "text", text: "Async" }] };
        req.expectStatus = 204;
        break;
      case "POST /session/{sessionID}/command":
        req.body = { command: "echo", arguments: "hi" };
        break;
      case "POST /session/{sessionID}/shell":
        req.body = {
          agent: "opencode",
          command: "ls",
          model: { providerID: "stub", modelID: "stub" },
        };
        break;
      case "POST /session/{sessionID}/summarize":
        req.body = { providerID: "stub", modelID: "stub" };
        break;
      case "POST /session/{sessionID}/permissions/{permissionID}":
        req.body = { response: "once" };
        break;
      case "PATCH /session/{sessionID}/message/{messageID}/part/{partID}":
        req.body = { type: "text", text: "updated" };
        break;
      case "POST /permission/{requestID}/reply":
        req.body = { reply: "once" };
        break;
      case "POST /question/{requestID}/reply":
        req.body = { answers: [] };
        break;
      case "PUT /auth/{providerID}":
        req.body = { type: "api", key: "stub" };
        break;
      case "POST /provider/{providerID}/oauth/authorize":
        req.body = { method: 0 };
        break;
      case "POST /provider/{providerID}/oauth/callback":
        req.body = { method: 0, code: "stub" };
        break;
      case "POST /log":
        req.body = { service: "test", level: "info", message: "hello" };
        break;
      case "POST /mcp":
        req.body = { name: ids.name, config: { type: "local", command: ["echo", "hi"] } };
        break;
      case "POST /mcp/{name}/auth/callback":
        req.body = { code: "stub" };
        break;
      case "POST /experimental/worktree/reset":
        req.body = { directory: "/workspace" };
        break;
      case "POST /pty":
        req.body = { command: "bash", args: [], cwd: "/workspace", title: "Test" };
        break;
      case "POST /tui/control/response":
      case "POST /tui/append-prompt":
      case "POST /tui/submit-prompt":
      case "POST /tui/execute-command":
      case "POST /tui/show-toast":
      case "POST /tui/publish":
      case "POST /tui/select-session":
      case "POST /global/config":
      case "PATCH /global/config":
      case "PATCH /config":
      case "POST /session/{sessionID}/init":
      case "POST /session/{sessionID}/abort":
      case "POST /session/{sessionID}/fork":
      case "POST /session/{sessionID}/revert":
      case "POST /session/{sessionID}/unrevert":
      case "POST /session/{sessionID}/share":
        req.body = req.body ?? {};
        break;
      default:
        break;
    }

    if (endpoint.path === "/session/{sessionID}/prompt_async") {
      req.expectStatus = 204;
    }
    cases.push(req);
  }
  return cases;
}

async function request(baseUrl: string, headers: HeadersInit, req: RequestCase) {
  const url = new URL(baseUrl + req.path);
  if (req.query) {
    for (const [key, value] of Object.entries(req.query)) {
      url.searchParams.set(key, value);
    }
  }
  const init: RequestInit = {
    method: req.method,
    headers: {
      "Content-Type": "application/json",
      ...headers,
    },
  };
  if (req.body !== undefined) {
    init.body = JSON.stringify(req.body);
  }
  const response = await fetch(url.toString(), init);
  let payload: unknown = null;
  if (req.sse) {
    payload = await readSse(response);
  } else if (response.status !== 204) {
    payload = await readJsonSafe(response);
  }
  return { status: response.status, body: payload };
}

describe("OpenCode OpenAPI coverage", () => {
  let handle: SandboxAgentHandle;
  let baseUrl: string;
  let headers: HeadersInit;
  let ids: Record<string, string>;

  beforeAll(async () => {
    await buildSandboxAgent();
  });

  beforeEach(async () => {
    handle = await spawnSandboxAgent({ opencodeCompat: true });
    baseUrl = `${handle.baseUrl}/opencode`;
    headers = { Authorization: `Bearer ${handle.token}` };

    const session = await request(baseUrl, headers, {
      method: "POST",
      path: "/session",
      body: { title: "Seed Session" },
    });
    const sessionId = (session.body as any)?.id ?? "ses_stub";

    await request(baseUrl, headers, {
      method: "POST",
      path: `/session/${sessionId}/message`,
      body: { parts: [{ type: "text", text: "Seed" }] },
    });

    const messages = await request(baseUrl, headers, {
      method: "GET",
      path: `/session/${sessionId}/message`,
    });
    const firstMessage = (messages.body as any)?.[0];
    const messageId = firstMessage?.info?.id ?? "msg_stub";
    const partId = firstMessage?.parts?.[0]?.id ?? "part_stub";

    const pty = await request(baseUrl, headers, {
      method: "POST",
      path: "/pty",
      body: { command: "bash", args: [], cwd: "/workspace", title: "Seed PTY" },
    });
    const ptyId = (pty.body as any)?.id ?? "pty_stub";

    const project = await request(baseUrl, headers, { method: "GET", path: "/project" });
    const projectId = (project.body as any)?.[0]?.id ?? "proj_stub";

    ids = {
      sessionID: sessionId,
      messageID: messageId,
      partID: partId,
      ptyID: ptyId,
      projectID: projectId,
      providerID: "provider_stub",
      requestID: "request_stub",
      permissionID: "per_stub",
      questionID: "que_stub",
      name: "mcp_stub",
    };
  });

  afterEach(async () => {
    await handle?.dispose();
  });

  it(
    "covers every OpenCode endpoint",
    { timeout: 60_000 },
    async () => {
      const endpoints = extractEndpoints();
      const cases = buildRequestCases(endpoints, ids);
      const results = [] as Array<{ endpoint: string; status: number; body: unknown }>;

      for (const req of cases) {
        const result = await request(baseUrl, headers, req);
        if (req.expectStatus !== undefined) {
          expect(result.status).toBe(req.expectStatus);
        }
        results.push({
          endpoint: `${req.method} ${req.path}`,
          status: result.status,
          body: result.body,
        });
      }

      expect(results).toMatchSnapshot();
    }
  );
});
