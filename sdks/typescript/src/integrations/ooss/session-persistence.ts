/**
 * Session Persistence for sandbox-agent using AgentFS KV
 *
 * Persists session state to AgentFS KV store for recovery across restarts.
 */

import type { UniversalEvent } from '../../types.ts';
import type {
  AgentFSKvLike,
  OOSSContext,
  PersistedSession,
  PersistedMessageHistory,
  SessionPersistenceConfig,
  WorkspaceRulesConfig,
} from './types.ts';

// KV key prefixes
const SESSION_KEY_PREFIX = 'session:';
const MESSAGES_KEY_PREFIX = 'session:messages:';
const SESSION_LIST_KEY = 'sessions:list';

/**
 * Session persistence manager using AgentFS KV store
 */
export class AgentFSSessionPersistence {
  private kv: AgentFSKvLike;
  private config: Required<Omit<SessionPersistenceConfig, 'kv'>>;
  private saveTimeouts: Map<string, ReturnType<typeof setTimeout>> = new Map();
  private eventBuffers: Map<string, UniversalEvent[]> = new Map();

  constructor(config: SessionPersistenceConfig) {
    this.kv = config.kv;
    this.config = {
      autoSave: config.autoSave ?? false,
      saveDebounceMs: config.saveDebounceMs ?? 1000,
    };
  }

  /**
   * Save a session to the KV store
   */
  async saveSession(session: PersistedSession): Promise<void> {
    const key = `${SESSION_KEY_PREFIX}${session.id}`;
    await this.kv.set(key, session);

    // Update session list
    await this.addToSessionList(session.id);
  }

  /**
   * Load a session from the KV store
   */
  async loadSession(sessionId: string): Promise<PersistedSession | undefined> {
    const key = `${SESSION_KEY_PREFIX}${sessionId}`;
    return this.kv.get<PersistedSession>(key);
  }

  /**
   * Delete a session from the KV store
   */
  async deleteSession(sessionId: string): Promise<void> {
    const sessionKey = `${SESSION_KEY_PREFIX}${sessionId}`;
    const messagesKey = `${MESSAGES_KEY_PREFIX}${sessionId}`;

    await Promise.all([
      this.kv.delete(sessionKey),
      this.kv.delete(messagesKey),
    ]);

    await this.removeFromSessionList(sessionId);

    // Clear any pending saves
    const timeout = this.saveTimeouts.get(sessionId);
    if (timeout) {
      clearTimeout(timeout);
      this.saveTimeouts.delete(sessionId);
    }
    this.eventBuffers.delete(sessionId);
  }

  /**
   * Save message history for a session
   */
  async saveMessageHistory(sessionId: string, events: UniversalEvent[]): Promise<void> {
    const key = `${MESSAGES_KEY_PREFIX}${sessionId}`;
    // Get last event ID - UniversalEvent has 'id' property
    let lastEventId = 0;
    if (events.length > 0) {
      const lastEvent = events[events.length - 1];
      lastEventId = (lastEvent as unknown as { id: number }).id ?? 0;
    }
    const history: PersistedMessageHistory = {
      sessionId,
      events,
      lastEventId,
    };
    await this.kv.set(key, history);
  }

  /**
   * Load message history for a session
   */
  async loadMessageHistory(sessionId: string): Promise<PersistedMessageHistory | undefined> {
    const key = `${MESSAGES_KEY_PREFIX}${sessionId}`;
    return this.kv.get<PersistedMessageHistory>(key);
  }

  /**
   * Append events to a session's history (used for incremental saves)
   */
  async appendEvents(sessionId: string, events: UniversalEvent[]): Promise<void> {
    if (events.length === 0) return;

    const existing = await this.loadMessageHistory(sessionId);
    const allEvents = existing ? [...existing.events, ...events] : events;
    await this.saveMessageHistory(sessionId, allEvents);
  }

  /**
   * Buffer an event for later saving (with debounce)
   */
  bufferEvent(sessionId: string, event: UniversalEvent): void {
    // Add to buffer
    let buffer = this.eventBuffers.get(sessionId);
    if (!buffer) {
      buffer = [];
      this.eventBuffers.set(sessionId, buffer);
    }
    buffer.push(event);

    // Schedule save if autoSave is enabled
    if (this.config.autoSave) {
      this.scheduleSave(sessionId);
    }
  }

  /**
   * Schedule a debounced save for a session
   */
  private scheduleSave(sessionId: string): void {
    // Clear existing timeout
    const existing = this.saveTimeouts.get(sessionId);
    if (existing) {
      clearTimeout(existing);
    }

    // Schedule new save
    const timeout = setTimeout(() => {
      this.flushBufferedEvents(sessionId).catch((err) => {
        console.error('[SessionPersistence] Failed to save buffered events:', err);
      });
    }, this.config.saveDebounceMs);

    this.saveTimeouts.set(sessionId, timeout);
  }

  /**
   * Flush buffered events for a session to KV store
   */
  async flushBufferedEvents(sessionId: string): Promise<void> {
    const buffer = this.eventBuffers.get(sessionId);
    if (!buffer || buffer.length === 0) return;

    // Clear buffer first to prevent duplicates
    this.eventBuffers.set(sessionId, []);

    // Clear timeout
    const timeout = this.saveTimeouts.get(sessionId);
    if (timeout) {
      clearTimeout(timeout);
      this.saveTimeouts.delete(sessionId);
    }

    // Save events
    await this.appendEvents(sessionId, buffer);
  }

  /**
   * Flush all buffered events
   */
  async flushAll(): Promise<void> {
    const sessionIds = Array.from(this.eventBuffers.keys());
    await Promise.all(
      sessionIds.map((sessionId) => this.flushBufferedEvents(sessionId))
    );
  }

  /**
   * List all persisted session IDs
   */
  async listSessions(): Promise<string[]> {
    const list = await this.kv.get<string[]>(SESSION_LIST_KEY);
    return list ?? [];
  }

  /**
   * Add a session ID to the list
   */
  private async addToSessionList(sessionId: string): Promise<void> {
    const list = await this.listSessions();
    if (!list.includes(sessionId)) {
      list.push(sessionId);
      await this.kv.set(SESSION_LIST_KEY, list);
    }
  }

  /**
   * Remove a session ID from the list
   */
  private async removeFromSessionList(sessionId: string): Promise<void> {
    const list = await this.listSessions();
    const index = list.indexOf(sessionId);
    if (index !== -1) {
      list.splice(index, 1);
      await this.kv.set(SESSION_LIST_KEY, list);
    }
  }

  /**
   * Create a new persisted session object
   */
  createSession(
    sessionId: string,
    agent: string,
    options: {
      agentMode?: string;
      permissionMode?: string;
      oossContext?: OOSSContext;
      workspaceRules?: WorkspaceRulesConfig;
      metadata?: Record<string, unknown>;
    } = {}
  ): PersistedSession {
    const now = new Date().toISOString();
    return {
      id: sessionId,
      agent,
      agentMode: options.agentMode,
      permissionMode: options.permissionMode,
      createdAt: now,
      lastActiveAt: now,
      oossContext: options.oossContext,
      workspaceRules: options.workspaceRules,
      metadata: options.metadata,
    };
  }

  /**
   * Update the lastActiveAt timestamp
   */
  async touchSession(sessionId: string): Promise<void> {
    const session = await this.loadSession(sessionId);
    if (session) {
      session.lastActiveAt = new Date().toISOString();
      await this.saveSession(session);
    }
  }

  /**
   * Check if a session exists
   */
  async hasSession(sessionId: string): Promise<boolean> {
    const session = await this.loadSession(sessionId);
    return session !== undefined;
  }
}

/**
 * Create a session persistence manager
 */
export function createSessionPersistence(
  config: SessionPersistenceConfig
): AgentFSSessionPersistence {
  return new AgentFSSessionPersistence(config);
}
