/**
 * OOSS Integration Types for sandbox-agent SDK
 *
 * These types define the interfaces for integrating sandbox-agent with the OOSS platform,
 * including Convex event streaming, session persistence, and workspace rules enforcement.
 */

import type { UniversalEvent } from '../../types.ts';
export type { UniversalEvent };

/**
 * OOSS context included with events and sessions
 */
export interface OOSSContext {
  /** Workspace identifier */
  workspaceId: string;
  /** Workload identifier */
  workloadId: string;
  /** Sandbox identifier */
  sandboxId: string;
  /** Trust class for permission evaluation */
  trustClass: string;
}

/**
 * Minimal Convex client interface
 */
export interface ConvexClientLike {
  mutation(name: string, args: Record<string, unknown>): Promise<unknown>;
  query?(name: string, args: Record<string, unknown>): Promise<unknown>;
}

/**
 * Configuration for Convex event streaming
 */
export interface ConvexEventStreamConfig {
  /** Convex client instance */
  convexClient: ConvexClientLike;
  /** Mutation path for recording events (e.g., "sandbox:recordAgentEvent") */
  eventMutation: string;
  /** Number of events to batch before sending (default: 5) */
  batchSize?: number;
  /** Interval in ms to flush batch even if not full (default: 500) */
  flushIntervalMs?: number;
  /** Whether to continue streaming if Convex is temporarily unavailable (default: true) */
  resilient?: boolean;
  /** Maximum number of events to buffer when Convex is unavailable (default: 1000) */
  maxBufferSize?: number;
}

/**
 * Callback for sensitive operations that require Convex approval
 */
export type SensitiveOpCallback = (
  operation: string,
  path: string,
  context: OOSSContext
) => Promise<boolean>;

/**
 * Configuration for workspace rules enforcement
 */
export interface WorkspaceRulesConfig {
  /** Allowed path patterns (glob-style) */
  allowedPaths: string[];
  /** Denied path patterns (glob-style, takes precedence over allowed) */
  deniedPaths: string[];
  /** Operations that require Convex callback for approval */
  sensitiveOps?: string[];
  /** Callback for sensitive operations */
  onSensitiveOp?: SensitiveOpCallback;
}

/**
 * Minimal AgentFS KV interface for session persistence
 */
export interface AgentFSKvLike {
  get<T = unknown>(key: string): Promise<T | undefined>;
  set(key: string, value: unknown): Promise<void>;
  delete(key: string): Promise<void>;
  list?(prefix?: string): Promise<string[]>;
}

/**
 * Configuration for session persistence
 */
export interface SessionPersistenceConfig {
  /** AgentFS KV store instance */
  kv: AgentFSKvLike;
  /** Whether to auto-save on each event (default: false, saves on turn end) */
  autoSave?: boolean;
  /** Save debounce interval in ms when autoSave is true (default: 1000) */
  saveDebounceMs?: number;
}

/**
 * Persisted session state
 */
export interface PersistedSession {
  /** Session ID */
  id: string;
  /** Agent type (e.g., "claude-code", "codex") */
  agent: string;
  /** Agent mode (e.g., "code", "architect") */
  agentMode?: string;
  /** Permission mode */
  permissionMode?: string;
  /** When session was created */
  createdAt: string;
  /** When session was last active */
  lastActiveAt: string;
  /** OOSS context */
  oossContext?: OOSSContext;
  /** Workspace rules (if configured) */
  workspaceRules?: WorkspaceRulesConfig;
  /** Custom metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Persisted message history (stored separately for size)
 */
export interface PersistedMessageHistory {
  /** Session ID */
  sessionId: string;
  /** All events in the session */
  events: UniversalEvent[];
  /** Last event ID for resumption */
  lastEventId: number;
}

/**
 * Event with OOSS context attached
 */
export interface OOSSEnrichedEvent extends UniversalEvent {
  /** OOSS context */
  oossContext?: OOSSContext;
}

/**
 * Configuration for OOSSAwareSandboxAgent
 */
export interface OOSSClientConfig {
  /** OOSS context for the sandbox */
  oossContext: OOSSContext;
  /** Convex streaming configuration (optional) */
  convex?: ConvexEventStreamConfig;
  /** Session persistence configuration (optional) */
  persistence?: SessionPersistenceConfig;
  /** Workspace rules configuration (optional) */
  workspaceRules?: WorkspaceRulesConfig;
}

/**
 * Permission check result
 */
export interface PermissionCheckResult {
  /** Whether the operation is allowed */
  allowed: boolean;
  /** Reason for denial (if not allowed) */
  reason?: string;
  /** Whether this was a local check or required callback */
  source: 'local' | 'callback';
}

/**
 * Error thrown when a permission is denied
 */
export class PermissionDeniedError extends Error {
  public readonly code = 'EACCES';
  public readonly operation: string;
  public readonly path: string;
  public readonly reason?: string;

  constructor(operation: string, path: string, reason?: string) {
    const message = reason
      ? `Permission denied: ${operation} on ${path} - ${reason}`
      : `Permission denied: ${operation} on ${path}`;
    super(message);
    this.name = 'PermissionDeniedError';
    this.operation = operation;
    this.path = path;
    this.reason = reason;
  }
}
