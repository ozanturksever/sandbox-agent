/**
 * OOSS-Aware Sandbox Agent Client
 *
 * Wrapper around SandboxAgent that integrates Convex event streaming,
 * session persistence, and workspace rules enforcement.
 */

import type { SandboxAgent } from '../../client.ts';
import type {
  CreateSessionRequest,
  CreateSessionResponse,
  MessageRequest,
  TurnStreamQuery,
  UniversalEvent,
} from '../../types.ts';
import type {
  OOSSClientConfig,
  OOSSContext,
  ConvexEventStreamConfig,
  SessionPersistenceConfig,
  WorkspaceRulesConfig,
  PersistedSession,
} from './types.ts';
import { ConvexEventStreamer } from './convex-streamer.ts';
import { AgentFSSessionPersistence } from './session-persistence.ts';
import { WorkspaceRulesEnforcer } from './workspace-rules.ts';

/**
 * OOSS-Aware Sandbox Agent
 *
 * Wraps a SandboxAgent client with OOSS integrations:
 * - Convex event streaming
 * - Session persistence to AgentFS
 * - Workspace rules enforcement
 * - OOSS context injection
 */
export class OOSSAwareSandboxAgent {
  private client: SandboxAgent;
  private oossContext: OOSSContext;
  private convexStreamer?: ConvexEventStreamer;
  private sessionPersistence?: AgentFSSessionPersistence;
  private rulesEnforcer?: WorkspaceRulesEnforcer;

  constructor(client: SandboxAgent, config: OOSSClientConfig) {
    this.client = client;
    this.oossContext = config.oossContext;

    // Initialize Convex streaming if configured
    if (config.convex) {
      this.convexStreamer = new ConvexEventStreamer(config.convex);
      this.convexStreamer.setOOSSContext(config.oossContext);
    }

    // Initialize session persistence if configured
    if (config.persistence) {
      this.sessionPersistence = new AgentFSSessionPersistence(config.persistence);
    }

    // Initialize workspace rules if configured
    if (config.workspaceRules) {
      this.rulesEnforcer = new WorkspaceRulesEnforcer(config.workspaceRules);
      this.rulesEnforcer.setOOSSContext(config.oossContext);
    }
  }

  /**
   * Get the underlying SandboxAgent client
   */
  getClient(): SandboxAgent {
    return this.client;
  }

  /**
   * Get the OOSS context
   */
  getOOSSContext(): OOSSContext {
    return this.oossContext;
  }

  /**
   * Update the OOSS context
   */
  setOOSSContext(context: OOSSContext): void {
    this.oossContext = context;
    this.convexStreamer?.setOOSSContext(context);
    this.rulesEnforcer?.setOOSSContext(context);
  }

  /**
   * Get the Convex streamer (if configured)
   */
  getConvexStreamer(): ConvexEventStreamer | undefined {
    return this.convexStreamer;
  }

  /**
   * Get the session persistence manager (if configured)
   */
  getSessionPersistence(): AgentFSSessionPersistence | undefined {
    return this.sessionPersistence;
  }

  /**
   * Get the workspace rules enforcer (if configured)
   */
  getRulesEnforcer(): WorkspaceRulesEnforcer | undefined {
    return this.rulesEnforcer;
  }

  /**
   * Create a session with OOSS integration
   */
  async createSession(
    sessionId: string,
    request: CreateSessionRequest
  ): Promise<CreateSessionResponse> {
    // Create session via underlying client
    const response = await this.client.createSession(sessionId, request);

    // Persist session if configured
    if (this.sessionPersistence) {
      const persistedSession = this.sessionPersistence.createSession(
        sessionId,
        request.agent,
        {
          agentMode: request.agentMode ?? undefined,
          permissionMode: request.permissionMode ?? undefined,
          oossContext: this.oossContext,
          workspaceRules: this.rulesEnforcer?.getRules(),
        }
      );
      await this.sessionPersistence.saveSession(persistedSession);
    }

    return response;
  }

  /**
   * Restore a session from persistence
   */
  async restoreSession(sessionId: string): Promise<PersistedSession | undefined> {
    if (!this.sessionPersistence) {
      return undefined;
    }
    return this.sessionPersistence.loadSession(sessionId);
  }

  /**
   * Check if a session exists in persistence
   */
  async hasPersistedSession(sessionId: string): Promise<boolean> {
    if (!this.sessionPersistence) {
      return false;
    }
    return this.sessionPersistence.hasSession(sessionId);
  }

  /**
   * List all persisted sessions
   */
  async listPersistedSessions(): Promise<string[]> {
    if (!this.sessionPersistence) {
      return [];
    }
    return this.sessionPersistence.listSessions();
  }

  /**
   * Post a message and stream the turn with OOSS integration
   */
  async *streamTurn(
    sessionId: string,
    request: MessageRequest,
    query?: TurnStreamQuery,
    signal?: AbortSignal
  ): AsyncGenerator<UniversalEvent, void, void> {
    try {
      for await (const event of this.client.streamTurn(
        sessionId,
        request,
        query,
        signal
      )) {
        // Stream to Convex
        this.convexStreamer?.queueEvent(event);

        // Buffer for persistence
        this.sessionPersistence?.bufferEvent(sessionId, event);

        yield event;
      }
    } finally {
      // Flush Convex streamer
      await this.convexStreamer?.flush();

      // Update session lastActiveAt and flush buffered events
      if (this.sessionPersistence) {
        await this.sessionPersistence.touchSession(sessionId);
        await this.sessionPersistence.flushBufferedEvents(sessionId);
      }
    }
  }

  /**
   * Stream events from a session with OOSS integration
   */
  async *streamEvents(
    sessionId: string,
    query?: { afterId?: number },
    signal?: AbortSignal
  ): AsyncGenerator<UniversalEvent, void, void> {
    for await (const event of this.client.streamEvents(sessionId, query as any, signal)) {
      // Stream to Convex
      this.convexStreamer?.queueEvent(event);

      // Buffer for persistence
      this.sessionPersistence?.bufferEvent(sessionId, event);

      yield event;
    }
  }

  /**
   * Check if an operation is allowed by workspace rules
   */
  async checkPermission(
    operation: string,
    path: string
  ): Promise<{ allowed: boolean; reason?: string }> {
    if (!this.rulesEnforcer) {
      return { allowed: true };
    }
    return this.rulesEnforcer.checkPermission(operation, path);
  }

  /**
   * Check permission and throw if denied
   */
  async checkPermissionOrThrow(operation: string, path: string): Promise<void> {
    if (this.rulesEnforcer) {
      await this.rulesEnforcer.checkPermissionOrThrow(operation, path);
    }
  }

  /**
   * Update workspace rules
   */
  updateWorkspaceRules(rules: Partial<WorkspaceRulesConfig>): void {
    if (this.rulesEnforcer) {
      this.rulesEnforcer.updateRules(rules);
    }
  }

  /**
   * Terminate a session with cleanup
   */
  async terminateSession(sessionId: string): Promise<void> {
    await this.client.terminateSession(sessionId);

    // Flush any pending events
    await this.convexStreamer?.flush();

    // Flush persistence
    if (this.sessionPersistence) {
      await this.sessionPersistence.flushBufferedEvents(sessionId);
    }
  }

  /**
   * Delete a session and its persisted data
   */
  async deleteSession(sessionId: string): Promise<void> {
    await this.terminateSession(sessionId);

    // Delete from persistence
    if (this.sessionPersistence) {
      await this.sessionPersistence.deleteSession(sessionId);
    }
  }

  /**
   * Dispose of the client and flush all pending data
   */
  async dispose(): Promise<void> {
    // Flush Convex streamer
    await this.convexStreamer?.stop();

    // Flush session persistence
    await this.sessionPersistence?.flushAll();

    // Dispose underlying client
    await this.client.dispose();
  }
}

/**
 * Create an OOSS-aware sandbox agent wrapper
 */
export function createOOSSAwareSandboxAgent(
  client: SandboxAgent,
  config: OOSSClientConfig
): OOSSAwareSandboxAgent {
  return new OOSSAwareSandboxAgent(client, config);
}

/**
 * Wrap an existing SandboxAgent with OOSS integration
 */
export function wrapWithOOSS(
  client: SandboxAgent,
  oossContext: OOSSContext,
  options: {
    convex?: ConvexEventStreamConfig;
    persistence?: SessionPersistenceConfig;
    workspaceRules?: WorkspaceRulesConfig;
  } = {}
): OOSSAwareSandboxAgent {
  return new OOSSAwareSandboxAgent(client, {
    oossContext,
    ...options,
  });
}
