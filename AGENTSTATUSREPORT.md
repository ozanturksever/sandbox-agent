# Agent Integration Status Report

## Infrastructure & Launch

| | Claude | Codex | OpenCode | Amp | Pi | Cursor | Codebuff | Mock |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Binary** | `claude` | `codex` | `opencode` | `amp` | `pi` | `cursor-agent` | `codebuff` | builtin |
| **Native required** | Yes | Yes | Yes | No | No | No | Yes | No |
| **ACP registry pkg** | `claude-code-acp` | `codex-acp` | `opencode` | `amp-acp` | `pi-acp` | `cursor-agent-acp` | — | — |
| **Launch subcommand** | — | — | `acp` | — | — | — | `acp` | — |
| **Unstable methods** | Yes | Yes | Yes | No | Yes | Yes | No | Yes |
| **Credentials** | ANTHROPIC | OPENAI | either | ANTHROPIC | none | none | own auth | none |
| **Models** | 4 | 5 | 68 | 1 | 1 | 35+ | — | 1 |
| **Modes** | — | — | build, plan | smart/deep/free/rush | — | — | DEFAULT/FREE/MAX/PLAN | — |

## Feature Capabilities (from `agent_capabilities_for`)

| Capability | Claude | Codex | OpenCode | Amp | Pi | Cursor | Codebuff | Mock |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| plan_mode | | **Y** | | | | **Y** | **Y** | **Y** |
| permissions | **Y** | **Y** | | | | **Y** | | **Y** |
| questions (HITL) | **Y** | | | | | | | **Y** |
| tool_calls | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| tool_results | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| text_messages | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| images | | **Y** | **Y** | | **Y** | **Y** | | **Y** |
| file_attachments | | **Y** | **Y** | | | | | **Y** |
| session_lifecycle | | **Y** | **Y** | | **Y** | **Y** | **Y** | **Y** |
| error_events | | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| reasoning | | **Y** | | | | | | **Y** |
| status | | **Y** | | | | | | **Y** |
| command_execution | | **Y** | | | | | | **Y** |
| file_changes | | **Y** | | | | | | **Y** |
| mcp_tools | **Y** | **Y** | **Y** | **Y** | | | | **Y** |
| streaming_deltas | **Y** | **Y** | **Y** | | **Y** | **Y** | **Y** | **Y** |
| item_started | | **Y** | **Y** | | **Y** | **Y** | | **Y** |
| **Score** | **7/17** | **16/17** | **10/17** | **5/17** | **8/17** | **10/17** | **7/17** | **17/17** |

## Test Coverage

| | Claude | Codex | OpenCode | Amp | Pi | Cursor | Codebuff | Mock |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| Agent process matrix | **Y** | **Y** | **Y** | | **Y** | **Y** | | |
| Dedicated flow tests | | | | | **Y** (7) | | **Y** (5) | |
| OC compat real-agent | **Y** | **Y** | **Y** | **Y** | | | | **Y** |
| OC compat events | | | | | | | | **Y** |
| E2E validated locally | | | **Y** | | | | **Y** | **Y** |

## Maturity Assessment

| Agent | Level | Notes |
|---|---|---|
| **Mock** | Reference impl | All 17 capabilities, builtin, full event test suite |
| **Codex** | Most complete | 16/17 caps, richest feature set (reasoning, file changes, commands) |
| **OpenCode** | Production | 10/17 caps, 68 models, validated with Kimi K2.5 free tier |
| **Cursor** | Production | 10/17 caps, 35+ models, permissions + plan mode |
| **Pi** | Production | 8/17 caps, 7 dedicated integration tests, no creds needed |
| **Claude** | Partial | 7/17 caps — uniquely has questions/HITL, but missing session_lifecycle, error_events, item_started |
| **Codebuff** | New/WIP | 7/17 caps, ACP bridge just added, e2e validated but needs ACP subcommand shipped in prod binary |
| **Amp** | Minimal | 5/17 caps — no streaming_deltas, no session_lifecycle, unstable disabled |

## Key Gaps

- **Claude**: No `session_lifecycle`, `error_events`, `images`, `reasoning` — surprising for a flagship agent. Likely the adapter predates these capabilities.
- **Amp**: Lowest score among real agents. No streaming deltas, no session lifecycle. Unstable disabled.
- **Codebuff**: Working e2e but the `acp` subcommand only exists in dev source — not yet in the published `codebuff` npm binary. Needs PR upstream.
- **shared_process**: Disabled for ALL agents (future feature).
- **questions (HITL)**: Only Claude and Mock support it.
