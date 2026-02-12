/**
 * OOSS Integration Module for sandbox-agent SDK
 *
 * This module provides all the components needed to integrate sandbox-agent with
 * the OOSS (OS-022) sandboxing platform:
 *
 * - **Convex Streaming**: Real-time event streaming to Convex for persistence/UI
 * - **Session Persistence**: Store sessions in AgentFS KV for recovery
 * - **Workspace Rules**: Enforce file access rules at the agent level
 * - **OOSS Context**: Attach workspace/workload context to all operations
 *
 * @example
 * ```typescript
 * import { SandboxAgent } from 'sandbox-agent';
 * import {
 *   wrapWithOOSS,
 *   type OOSSContext,
 * } from 'sandbox-agent/ooss';
 *
 * // Create base client
 * const client = await SandboxAgent.connect({ baseUrl: 'http://localhost:8080' });
 *
 * // Wrap with OOSS integration
 * const oossClient = wrapWithOOSS(client, {
 *   workspaceId: 'ws_abc',
 *   workloadId: 'wl_xyz',
 *   sandboxId: 'sbx_123',
 *   trustClass: 'agent',
 * }, {
 *   convex: {
 *     convexClient,
 *     eventMutation: 'sandbox:recordAgentEvent',
 *   },
 *   workspaceRules: {
 *     allowedPaths: ['/workspace/**'],
 *     deniedPaths: ['/workspace/.env'],
 *   },
 * });
 *
 * // Use the OOSS-aware client
 * const session = await oossClient.createSession('sess_123', {
 *   agent: 'claude-code',
 * });
 *
 * // Events automatically stream to Convex
 * for await (const event of oossClient.streamTurn('sess_123', {
 *   content: 'Fix the bug',
 * })) {
 *   console.log(event.type, event.data);
 * }
 * ```
 *
 * @packageDocumentation
 */

// Types
export type {
  OOSSContext,
  ConvexClientLike,
  ConvexEventStreamConfig,
  SensitiveOpCallback,
  WorkspaceRulesConfig,
  AgentFSKvLike,
  SessionPersistenceConfig,
  PersistedSession,
  PersistedMessageHistory,
  OOSSEnrichedEvent,
  OOSSClientConfig,
  PermissionCheckResult,
  UniversalEvent,
} from './types.ts';

export { PermissionDeniedError } from './types.ts';

// Convex Streaming
export {
  ConvexEventStreamer,
  createConvexEventStreamer,
  streamWithConvex,
} from './convex-streamer.ts';

// Session Persistence
export {
  AgentFSSessionPersistence,
  createSessionPersistence,
} from './session-persistence.ts';

// Workspace Rules
export {
  WorkspaceRulesEnforcer,
  createWorkspaceRulesEnforcer,
  matchPathPattern,
  checkPathPatterns,
} from './workspace-rules.ts';

// OOSS-Aware Client
export {
  OOSSAwareSandboxAgent,
  createOOSSAwareSandboxAgent,
  wrapWithOOSS,
} from './ooss-client.ts';

export type {
  OOSSCompatibleClient,
} from './ooss-client.ts';
