/**
 * Workspace Rules Enforcer for sandbox-agent
 *
 * Enforces workspace rules at the agent level, checking file operations
 * against allowed/denied patterns and calling back to Convex for sensitive ops.
 */

import type {
  OOSSContext,
  WorkspaceRulesConfig,
  SensitiveOpCallback,
  PermissionCheckResult,
} from './types.ts';
import { PermissionDeniedError } from './types.ts';

/**
 * Workspace rules enforcer
 *
 * Checks file operations against workspace rules and optionally calls back
 * to Convex for sensitive operations.
 */
export class WorkspaceRulesEnforcer {
  private config: WorkspaceRulesConfig;
  private oossContext?: OOSSContext;
  private cachedPatterns?: {
    allowedRegexes: RegExp[];
    deniedRegexes: RegExp[];
  };

  constructor(config: WorkspaceRulesConfig) {
    this.config = config;
    this.cachePatterns();
  }

  /**
   * Set the OOSS context for permission callbacks
   */
  setOOSSContext(context: OOSSContext): void {
    this.oossContext = context;
  }

  /**
   * Update the workspace rules
   */
  updateRules(config: Partial<WorkspaceRulesConfig>): void {
    this.config = { ...this.config, ...config };
    this.cachePatterns();
  }

  /**
   * Get the current rules configuration
   */
  getRules(): WorkspaceRulesConfig {
    return { ...this.config };
  }

  /**
   * Check if an operation on a path is allowed
   */
  async checkPermission(
    operation: string,
    path: string
  ): Promise<PermissionCheckResult> {
    // First check local patterns
    const localResult = this.checkLocalPatterns(path);
    if (!localResult.allowed) {
      return localResult;
    }

    // Check if this is a sensitive operation requiring callback
    if (this.isSensitiveOp(operation)) {
      return this.checkSensitiveOp(operation, path);
    }

    return localResult;
  }

  /**
   * Check permission and throw if denied
   */
  async checkPermissionOrThrow(operation: string, path: string): Promise<void> {
    const result = await this.checkPermission(operation, path);
    if (!result.allowed) {
      throw new PermissionDeniedError(operation, path, result.reason);
    }
  }

  /**
   * Check if an operation is considered sensitive
   */
  isSensitiveOp(operation: string): boolean {
    return this.config.sensitiveOps?.includes(operation) ?? false;
  }

  /**
   * Check path against local patterns (synchronous)
   */
  checkLocalPatterns(path: string): PermissionCheckResult {
    if (!this.cachedPatterns) {
      this.cachePatterns();
    }

    const { allowedRegexes, deniedRegexes } = this.cachedPatterns!;

    // Normalize path to have leading slash
    const normalizedPath = path.startsWith('/') ? path : '/' + path;

    // Check denied patterns first (deny takes precedence)
    for (let i = 0; i < deniedRegexes.length; i++) {
      if (deniedRegexes[i].test(normalizedPath)) {
        return {
          allowed: false,
          reason: `Path matches denied pattern: ${this.config.deniedPaths[i]}`,
          source: 'local',
        };
      }
    }

    // If there are allowed patterns, path must match at least one
    if (allowedRegexes.length > 0) {
      const matchesAllowed = allowedRegexes.some((regex) => regex.test(normalizedPath));
      if (!matchesAllowed) {
        return {
          allowed: false,
          reason: 'Path does not match any allowed pattern',
          source: 'local',
        };
      }
    }

    return { allowed: true, source: 'local' };
  }

  /**
   * Check sensitive operation via callback
   */
  private async checkSensitiveOp(
    operation: string,
    path: string
  ): Promise<PermissionCheckResult> {
    if (!this.config.onSensitiveOp) {
      // No callback configured, allow by default
      return { allowed: true, source: 'local' };
    }

    if (!this.oossContext) {
      // No context available, deny for safety
      return {
        allowed: false,
        reason: 'Sensitive operation requires OOSS context',
        source: 'callback',
      };
    }

    try {
      const allowed = await this.config.onSensitiveOp(
        operation,
        path,
        this.oossContext
      );
      return {
        allowed,
        reason: allowed ? undefined : 'Denied by workspace rules callback',
        source: 'callback',
      };
    } catch (err) {
      // Callback failed, deny for safety
      return {
        allowed: false,
        reason: `Permission callback failed: ${err instanceof Error ? err.message : String(err)}`,
        source: 'callback',
      };
    }
  }

  /**
   * Pre-compile glob patterns to regex for performance
   */
  private cachePatterns(): void {
    this.cachedPatterns = {
      allowedRegexes: this.config.allowedPaths.map((p) =>
        this.globToRegex(p)
      ),
      deniedRegexes: this.config.deniedPaths.map((p) => this.globToRegex(p)),
    };
  }

  /**
   * Convert glob pattern to regex
   *
   * Supports:
   * - `*` matches any single path segment
   * - `**` matches any number of path segments
   * - Exact string matching otherwise
   */
  private globToRegex(pattern: string): RegExp {
    // Normalize path
    const normalizedPattern = pattern.startsWith('/') ? pattern : '/' + pattern;

    // Convert glob pattern to regex
    const regexPattern = normalizedPattern
      .replace(/[.+^${}()|[\]\\]/g, '\\$&') // Escape special regex chars
      .replace(/\*\*/g, '§§') // Temporarily replace **
      .replace(/\*/g, '[^/]*') // * matches single segment
      .replace(/§§/g, '.*'); // ** matches any path

    return new RegExp(`^${regexPattern}$`);
  }
}

/**
 * Create a workspace rules enforcer
 */
export function createWorkspaceRulesEnforcer(
  config: WorkspaceRulesConfig
): WorkspaceRulesEnforcer {
  return new WorkspaceRulesEnforcer(config);
}

/**
 * Match a path against a glob-like pattern
 */
export function matchPathPattern(path: string, pattern: string): boolean {
  // Normalize paths
  const normalizedPath = path.startsWith('/') ? path : '/' + path;
  const normalizedPattern = pattern.startsWith('/') ? pattern : '/' + pattern;

  // Convert glob pattern to regex
  const regexPattern = normalizedPattern
    .replace(/[.+^${}()|[\]\\]/g, '\\$&')
    .replace(/\*\*/g, '§§')
    .replace(/\*/g, '[^/]*')
    .replace(/§§/g, '.*');

  const regex = new RegExp(`^${regexPattern}$`);
  return regex.test(normalizedPath);
}

/**
 * Check a path against allowed and denied patterns (utility function)
 */
export function checkPathPatterns(
  path: string,
  allowedPatterns: string[],
  deniedPatterns: string[]
): PermissionCheckResult {
  // Check denied patterns first
  for (const pattern of deniedPatterns) {
    if (matchPathPattern(path, pattern)) {
      return {
        allowed: false,
        reason: `Path matches denied pattern: ${pattern}`,
        source: 'local',
      };
    }
  }

  // If there are allowed patterns, path must match at least one
  if (allowedPatterns.length > 0) {
    const matchesAllowed = allowedPatterns.some((pattern) =>
      matchPathPattern(path, pattern)
    );
    if (!matchesAllowed) {
      return {
        allowed: false,
        reason: 'Path does not match any allowed pattern',
        source: 'local',
      };
    }
  }

  return { allowed: true, source: 'local' };
}
