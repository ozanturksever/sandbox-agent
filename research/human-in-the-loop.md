# Human-in-the-Loop Patterns

Comparison of how each coding agent handles interactive permission and question flows.

## Summary

| Agent | Permission System | Question System | Event Types |
|-------|-------------------|-----------------|-------------|
| **OpenCode** | Full (`permission.asked`) | Full (`question.asked`) | SSE events |
| **Claude** | `permissionMode` option | Via `AskUserQuestion` tool | Stream JSON |
| **Codex** | `sandboxMode` levels | None documented | JSONL events |
| **Amp** | `PermissionRule[]` with actions | None documented | Stream JSON |

---

## OpenCode (Most Complete)

OpenCode is the only agent with a fully interactive bidirectional HITL protocol. It emits events that require explicit responses via dedicated API endpoints.

### Permission Requests

```typescript
interface PermissionRequest {
  id: string;
  sessionID: string;
  permission: string;     // e.g., "file:write"
  patterns: string[];     // Affected paths
  metadata: Record<string, unknown>;
  always: string[];       // "Always allow" options
  tool?: { messageID: string; callID: string };
}
```

**Responding:**
```typescript
await clientV2.permission.reply({
  requestID: requestId,
  reply: "once" | "always" | "reject"
});
```

### Question Requests

```typescript
interface QuestionRequest {
  id: string;
  sessionID: string;
  questions: Array<{
    header?: string;
    question: string;
    options: Array<{ label: string; description?: string }>;
    multiSelect?: boolean;
  }>;
  tool?: { messageID: string; callID: string };
}
```

**Responding:**
```typescript
// Reply with selected options
await clientV2.question.reply({
  requestID: requestId,
  answers: [["selected option"]]  // Array of selected labels per question
});

// Or reject
await clientV2.question.reject({ requestID: requestId });
```

### Event Types

| Event | Description |
|-------|-------------|
| `permission.asked` | Agent requesting permission for an action |
| `question.asked` | Agent asking user a question with options |

---

## Claude Code

Claude uses static permission modes rather than interactive permission requests. Questions are handled via a built-in tool.

### Permission Modes (Static)

```typescript
interface Options {
  permissionMode?: 'default' | 'acceptEdits' | 'bypassPermissions' | 'plan';
}
```

| Mode | Behavior |
|------|----------|
| `default` | Normal permission prompts |
| `acceptEdits` | Auto-accept file edits |
| `bypassPermissions` | Skip all permission prompts |
| `plan` | Read-only exploration mode |

### Interactive Questions via Tool

Claude exposes an `AskUserQuestion` tool that the agent can invoke:

```typescript
interface AskUserQuestionInput {
  questions: Array<{
    question: string;
    header: string;       // Max 12 characters
    options: Array<{
      label: string;
      description: string;
    }>;
    multiSelect: boolean;
  }>;
}
```

The tool result contains user selections, but the SDK handles UI internally.

---

## Codex

Codex uses sandbox levels rather than interactive permission requests. No question system is documented in SDK mode.

### Sandbox Modes (Static)

```typescript
interface ThreadOptions {
  sandboxMode?: "read-only" | "workspace-write" | "danger-full-access";
}
```

| Mode | Behavior |
|------|----------|
| `read-only` | No file modifications allowed |
| `workspace-write` | Can modify files in workspace |
| `danger-full-access` | Full system access |

### CLI Flags

| Flag | Behavior |
|------|----------|
| `--full-auto` | Auto-approve with workspace-write sandbox |
| `--dangerously-bypass-approvals-and-sandbox` | Skip all prompts |

### No Interactive HITL

Codex does not expose events for permission or question requests in SDK mode. All approval decisions must be made upfront via sandbox mode selection.

---

## Amp

Amp uses declarative permission rules configured before execution. No SDK-level API for interactive responses is documented.

### Permission Rules (Declarative)

```typescript
interface PermissionRule {
  tool: string;  // Glob pattern: "Bash", "mcp__playwright__*"
  matches?: { [argumentName: string]: string | string[] | boolean };
  action: "allow" | "reject" | "ask" | "delegate";
  context?: "thread" | "subagent";
}
```

| Action | Behavior |
|--------|----------|
| `allow` | Automatically permit |
| `reject` | Automatically deny |
| `ask` | Prompt user (CLI handles internally) |
| `delegate` | Delegate to subagent context |

### Example Rules

```typescript
const permissions: PermissionRule[] = [
  { tool: "Read", action: "allow" },
  { tool: "Bash", matches: { command: "git *" }, action: "allow" },
  { tool: "Write", action: "ask" },
  { tool: "mcp__*", action: "reject" }
];
```

### No SDK-Level Response API

While `"ask"` suggests runtime prompting, Amp does not expose an API for programmatically responding to permission requests. The CLI handles user interaction internally.

---

## Architectural Comparison

### Fully Interactive (Bidirectional)
- **OpenCode**: Emits events, expects responses via API

### Tool-Based Questions
- **Claude**: Agent invokes `AskUserQuestion` tool, SDK handles UI

### Static/Declarative Only
- **Codex**: Sandbox mode set upfront
- **Amp**: Permission rules declared upfront

---

## Implications for Integration

| Approach | Pros | Cons |
|----------|------|------|
| **Interactive (OpenCode)** | Full control, runtime decisions | Complex to implement, requires event loop |
| **Tool-Based (Claude)** | Agent-driven, flexible | Limited to predefined tool schema |
| **Static (Codex/Amp)** | Simple, predictable | No runtime flexibility |

For building a unified agent interface:
1. **OpenCode** requires implementing question/permission response handlers
2. **Claude** can be used with `bypassPermissions` or handled via tool interception
3. **Codex/Amp** only need upfront configuration
