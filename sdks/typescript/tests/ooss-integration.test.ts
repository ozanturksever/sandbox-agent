/**
 * Tests for OOSS Integration Module
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  ConvexEventStreamer,
  createConvexEventStreamer,
  AgentFSSessionPersistence,
  createSessionPersistence,
  WorkspaceRulesEnforcer,
  createWorkspaceRulesEnforcer,
  matchPathPattern,
  checkPathPatterns,
  PermissionDeniedError,
  type OOSSContext,
  type ConvexClientLike,
  type AgentFSKvLike,
  type UniversalEvent,
} from '../src/integrations/ooss/index.ts';

// Mock Convex client
function createMockConvexClient(): ConvexClientLike & { calls: Array<{ name: string; args: Record<string, unknown> }> } {
  const calls: Array<{ name: string; args: Record<string, unknown> }> = [];
  return {
    calls,
    mutation: vi.fn(async (name: string, args: Record<string, unknown>) => {
      calls.push({ name, args });
      return { success: true };
    }),
  };
}

// Mock AgentFS KV store
function createMockKvStore(): AgentFSKvLike & { store: Map<string, unknown> } {
  const store = new Map<string, unknown>();
  return {
    store,
    get: vi.fn(async <T>(key: string): Promise<T | undefined> => {
      return store.get(key) as T | undefined;
    }),
    set: vi.fn(async (key: string, value: unknown): Promise<void> => {
      store.set(key, value);
    }),
    delete: vi.fn(async (key: string): Promise<void> => {
      store.delete(key);
    }),
    list: vi.fn(async (prefix?: string): Promise<string[]> => {
      const keys = Array.from(store.keys());
      if (prefix) {
        return keys.filter(k => k.startsWith(prefix));
      }
      return keys;
    }),
  };
}

// Mock universal event
function createMockEvent(id: number, type: string = 'item'): UniversalEvent {
  return {
    id,
    timestamp: new Date().toISOString(),
    sessionId: 'test-session',
    agent: 'claude-code',
    type: type as any,
    data: { content: `Event ${id}` },
  } as UniversalEvent;
}

const mockOOSSContext: OOSSContext = {
  workspaceId: 'ws_test',
  workloadId: 'wl_test',
  sandboxId: 'sbx_test',
  trustClass: 'agent',
};

describe('ConvexEventStreamer', () => {
  let streamer: ConvexEventStreamer;
  let mockClient: ReturnType<typeof createMockConvexClient>;

  beforeEach(() => {
    vi.useFakeTimers();
    mockClient = createMockConvexClient();
    streamer = createConvexEventStreamer({
      convexClient: mockClient,
      eventMutation: 'sandbox:recordAgentEvent',
      batchSize: 3,
      flushIntervalMs: 100,
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('should queue events and flush when batch size is reached', async () => {
    streamer.setOOSSContext(mockOOSSContext);

    streamer.queueEvent(createMockEvent(1));
    streamer.queueEvent(createMockEvent(2));
    expect(mockClient.calls.length).toBe(0);

    streamer.queueEvent(createMockEvent(3));
    // Should trigger flush
    await vi.runAllTimersAsync();

    expect(mockClient.calls.length).toBe(1);
    expect(mockClient.calls[0].name).toBe('sandbox:recordAgentEvent');
    expect((mockClient.calls[0].args.events as any[]).length).toBe(3);
  });

  it('should flush on interval even if batch is not full', async () => {
    streamer.queueEvent(createMockEvent(1));
    expect(mockClient.calls.length).toBe(0);

    await vi.advanceTimersByTimeAsync(150);

    expect(mockClient.calls.length).toBe(1);
    expect((mockClient.calls[0].args.events as any[]).length).toBe(1);
  });

  it('should attach OOSS context to events', async () => {
    streamer.setOOSSContext(mockOOSSContext);
    streamer.queueEvent(createMockEvent(1));

    await streamer.flush();

    const events = mockClient.calls[0].args.events as any[];
    expect(events[0].oossContext).toEqual(mockOOSSContext);
  });

  it('should report buffer size correctly', () => {
    expect(streamer.getBufferSize()).toBe(0);
    streamer.queueEvent(createMockEvent(1));
    expect(streamer.getBufferSize()).toBe(1);
    streamer.queueEvent(createMockEvent(2));
    expect(streamer.getBufferSize()).toBe(2);
  });
});

describe('AgentFSSessionPersistence', () => {
  let persistence: AgentFSSessionPersistence;
  let mockKv: ReturnType<typeof createMockKvStore>;

  beforeEach(() => {
    mockKv = createMockKvStore();
    persistence = createSessionPersistence({ kv: mockKv });
  });

  it('should save and load sessions', async () => {
    const session = persistence.createSession('sess_123', 'claude-code', {
      oossContext: mockOOSSContext,
    });

    await persistence.saveSession(session);

    const loaded = await persistence.loadSession('sess_123');
    expect(loaded).toBeDefined();
    expect(loaded?.id).toBe('sess_123');
    expect(loaded?.agent).toBe('claude-code');
    expect(loaded?.oossContext).toEqual(mockOOSSContext);
  });

  it('should list sessions', async () => {
    const session1 = persistence.createSession('sess_1', 'claude-code');
    const session2 = persistence.createSession('sess_2', 'codex');

    await persistence.saveSession(session1);
    await persistence.saveSession(session2);

    const sessions = await persistence.listSessions();
    expect(sessions).toContain('sess_1');
    expect(sessions).toContain('sess_2');
  });

  it('should delete sessions', async () => {
    const session = persistence.createSession('sess_to_delete', 'claude-code');
    await persistence.saveSession(session);

    expect(await persistence.hasSession('sess_to_delete')).toBe(true);

    await persistence.deleteSession('sess_to_delete');

    expect(await persistence.hasSession('sess_to_delete')).toBe(false);
  });

  it('should save and load message history', async () => {
    const events = [createMockEvent(1), createMockEvent(2), createMockEvent(3)];

    await persistence.saveMessageHistory('sess_123', events);

    const history = await persistence.loadMessageHistory('sess_123');
    expect(history).toBeDefined();
    expect(history?.events.length).toBe(3);
    expect(history?.lastEventId).toBe(3);
  });

  it('should append events to history', async () => {
    await persistence.saveMessageHistory('sess_123', [createMockEvent(1)]);
    await persistence.appendEvents('sess_123', [createMockEvent(2), createMockEvent(3)]);

    const history = await persistence.loadMessageHistory('sess_123');
    expect(history?.events.length).toBe(3);
  });
});

describe('WorkspaceRulesEnforcer', () => {
  it('should allow paths matching allowed patterns', async () => {
    const enforcer = createWorkspaceRulesEnforcer({
      allowedPaths: ['/workspace/**'],
      deniedPaths: [],
    });

    const result = await enforcer.checkPermission('read', '/workspace/src/index.ts');
    expect(result.allowed).toBe(true);
  });

  it('should deny paths not matching allowed patterns', async () => {
    const enforcer = createWorkspaceRulesEnforcer({
      allowedPaths: ['/workspace/**'],
      deniedPaths: [],
    });

    const result = await enforcer.checkPermission('read', '/etc/passwd');
    expect(result.allowed).toBe(false);
  });

  it('should deny paths matching denied patterns (precedence)', async () => {
    const enforcer = createWorkspaceRulesEnforcer({
      allowedPaths: ['/workspace/**'],
      deniedPaths: ['/workspace/.env', '/workspace/secrets/**'],
    });

    const result1 = await enforcer.checkPermission('read', '/workspace/.env');
    expect(result1.allowed).toBe(false);

    const result2 = await enforcer.checkPermission('read', '/workspace/secrets/api-key.txt');
    expect(result2.allowed).toBe(false);

    const result3 = await enforcer.checkPermission('read', '/workspace/src/index.ts');
    expect(result3.allowed).toBe(true);
  });

  it('should call sensitive op callback for sensitive operations', async () => {
    const onSensitiveOp = vi.fn().mockResolvedValue(false);

    const enforcer = createWorkspaceRulesEnforcer({
      allowedPaths: ['/workspace/**'],
      deniedPaths: [],
      sensitiveOps: ['delete', 'execute'],
      onSensitiveOp,
    });
    enforcer.setOOSSContext(mockOOSSContext);

    const result = await enforcer.checkPermission('delete', '/workspace/important.txt');
    expect(result.allowed).toBe(false);
    expect(onSensitiveOp).toHaveBeenCalledWith('delete', '/workspace/important.txt', mockOOSSContext);
  });

  it('should throw PermissionDeniedError on checkPermissionOrThrow', async () => {
    const enforcer = createWorkspaceRulesEnforcer({
      allowedPaths: ['/workspace/**'],
      deniedPaths: ['/workspace/.env'],
    });

    await expect(
      enforcer.checkPermissionOrThrow('read', '/workspace/.env')
    ).rejects.toThrow(PermissionDeniedError);
  });
});

describe('matchPathPattern', () => {
  it('should match exact paths', () => {
    expect(matchPathPattern('/workspace/file.txt', '/workspace/file.txt')).toBe(true);
    expect(matchPathPattern('/workspace/file.txt', '/workspace/other.txt')).toBe(false);
  });

  it('should match single-segment wildcards', () => {
    expect(matchPathPattern('/workspace/file.txt', '/workspace/*')).toBe(true);
    expect(matchPathPattern('/workspace/src/file.txt', '/workspace/*')).toBe(false);
  });

  it('should match multi-segment wildcards', () => {
    expect(matchPathPattern('/workspace/src/deep/file.txt', '/workspace/**')).toBe(true);
    expect(matchPathPattern('/workspace/file.txt', '/workspace/**')).toBe(true);
    expect(matchPathPattern('/other/file.txt', '/workspace/**')).toBe(false);
  });

  it('should handle patterns without leading slash', () => {
    expect(matchPathPattern('workspace/file.txt', 'workspace/**')).toBe(true);
  });
});

describe('checkPathPatterns', () => {
  it('should check against both allowed and denied patterns', () => {
    const result = checkPathPatterns(
      '/workspace/.env',
      ['/workspace/**'],
      ['/workspace/.env']
    );
    expect(result.allowed).toBe(false);
    expect(result.reason).toContain('denied pattern');
  });

  it('should allow if no patterns are specified', () => {
    const result = checkPathPatterns('/any/path', [], []);
    expect(result.allowed).toBe(true);
  });
});
