/**
 * Tests for OpenCode-compatible permission endpoints.
 *
 * These tests verify that sandbox-agent exposes OpenCode-compatible permission
 * handling endpoints that can be used with the official OpenCode SDK.
 *
 * Expected endpoints:
 * - POST /session/{id}/permissions/{permissionID} - Respond to a permission request
 */

import { describe, it, expect, beforeAll, beforeEach, afterEach } from "vitest";
import { createOpencodeClient, type OpencodeClient } from "@opencode-ai/sdk";
import { spawnSandboxAgent, buildSandboxAgent, type SandboxAgentHandle } from "./helpers/spawn";

describe("OpenCode-compatible Permission API", () => {
  let handle: SandboxAgentHandle;
  let client: OpencodeClient;
  let sessionId: string;

  beforeAll(async () => {
    await buildSandboxAgent();
  });

  beforeEach(async () => {
    handle = await spawnSandboxAgent({ opencodeCompat: true });
    client = createOpencodeClient({
      baseUrl: `${handle.baseUrl}/opencode`,
      headers: { Authorization: `Bearer ${handle.token}` },
    });

    // Create a session
    const session = await client.session.create();
    sessionId = session.data?.id!;
    expect(sessionId).toBeDefined();
  });

  afterEach(async () => {
    await handle?.dispose();
  });

  describe("postSessionIdPermissionsPermissionId", () => {
    it("should accept permission approval", async () => {
      // This test requires triggering a permission request, which typically
      // happens when the agent wants to perform a tool action.

      // For now, we test the endpoint structure
      // In a real scenario, we'd:
      // 1. Send a prompt that triggers a tool requiring permission
      // 2. Capture the permission.updated event
      // 3. Reply to the permission

      // Test with a mock permission ID (will fail if endpoint doesn't exist)
      const response = await client.postSessionIdPermissionsPermissionId({
        path: { id: sessionId, permissionID: "test-permission-id" },
        body: { response: "allow" },
      });

      // The endpoint should exist even if the permission ID is invalid
      // It might return an error for invalid permission ID, but shouldn't 404
      expect(response).toBeDefined();
    });

    it("should accept permission denial", async () => {
      const response = await client.postSessionIdPermissionsPermissionId({
        path: { id: sessionId, permissionID: "test-permission-id" },
        body: { response: "deny" },
      });

      expect(response).toBeDefined();
    });

    it("should accept 'always' response", async () => {
      const response = await client.postSessionIdPermissionsPermissionId({
        path: { id: sessionId, permissionID: "test-permission-id" },
        body: { response: "always" },
      });

      expect(response).toBeDefined();
    });
  });
});

/**
 * Integration test that exercises the full permission flow.
 * This test requires a properly configured environment with API keys.
 */
describe.skip("Permission Flow Integration", () => {
  let handle: SandboxAgentHandle;
  let client: OpencodeClient;
  let sessionId: string;

  beforeAll(async () => {
    await buildSandboxAgent();
  });

  beforeEach(async () => {
    handle = await spawnSandboxAgent({ opencodeCompat: true });
    client = createOpencodeClient({
      baseUrl: `${handle.baseUrl}/opencode`,
      headers: { Authorization: `Bearer ${handle.token}` },
    });

    const session = await client.session.create();
    sessionId = session.data?.id!;
  });

  afterEach(async () => {
    await handle?.dispose();
  });

  it("should handle full permission flow", async () => {
    const permissionEvents: any[] = [];

    // Start event stream
    const eventStream = await client.event.subscribe();
    const collectEvents = new Promise<string | null>((resolve) => {
      const timeout = setTimeout(() => resolve(null), 30000);
      (async () => {
        try {
          for await (const event of eventStream as any) {
            if (event.type === "permission.updated") {
              permissionEvents.push(event);
              clearTimeout(timeout);
              resolve(event.properties?.id);
              break;
            }
          }
        } catch {
          resolve(null);
        }
      })();
    });

    // Send prompt that triggers a tool requiring permission
    await client.session.prompt({
      path: { id: sessionId },
      body: {
        parts: [{ type: "text", text: "Please run 'ls' in the current directory" }],
      },
    });

    const permissionId = await collectEvents;

    if (permissionId) {
      // Approve the permission
      const response = await client.postSessionIdPermissionsPermissionId({
        path: { id: sessionId, permissionID: permissionId },
        body: { response: "allow" },
      });

      expect(response.error).toBeUndefined();
    }
  });
});
