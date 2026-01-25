# Sandbox Agent SDK

Universal API for running Claude Code, Codex, OpenCode, and Amp inside sandboxes.

- **Any coding agent**: Universal API to interact with all agents with full feature coverage
- **Server Mode**: Run as HTTP server from any sandbox provider or as TypeScript & Python SDK
- **Universal session schema**: Universal schema to store agent transcripts
- **Supports your sandbox provider**: Daytona, E2B, Vercel Sandboxes, and more
- **Lightweight, portable Rust binary**: Install anywhere with 1 curl command

## Architecture

- TODO
    - Embedded (runs agents locally)
    - Sandboxed

## Components

- Server: TODO
- SDK: TODO
- Inspector: inspect.sandboxagent.dev
- CLI: TODO

## Project Goals

This project aims to solve 3 problems with agents:

- **Universal Agent API**: Claude Code, Codex, Amp, and OpenCode all have put a lot of work in to the agent scaffold. Each have respective pros and cons and need to be easy to be swapped between.
- **Agent Transcript**: Maintaining agent transcripts is difficult since the agent manages its own sessions. This provides a simpler way to read and retrieve agent transcripts in your system.
- **Agents In Sandboxes**: There are many complications with running agents inside of sandbox providers. This lets you run a simple curl command to spawn an HTTP server for using any agent from within the sandbox.

Features out of scope:

- **Storage of sessions on disk**: Sessions are already stored by the respective coding agents on disk. It's assumed that the consumer is streaming data from this machine to an external storage, such as Postgres, ClickHouse, or Rivet.
- **Direct LLM wrappers**: Use the [Vercel AI SDK](https://ai-sdk.dev/docs/introduction) if you want to implement your own agent from scratch.
- **Git Repo Management**: Just use git commands or the features provided by your sandbox provider of choice.
- **Sandbox Provider API**: Sandbox providers have many nuanced differences in their API, it does not make sense for us to try to provide a custom layer. Instead, we opt to provide guides that let you integrate this project with sandbox providers.

## FAQ

**Why not use PTY?**
PTY-based approaches require parsing terminal escape sequences and dealing with interactive prompts. The agents we support all have machine-readable output modes (JSONL, HTTP APIs) that provide structured events, making integration more reliable.

**Why not use features that already exist on sandbox provider APIs?**
Sandbox providers focus on infrastructure (containers, VMs, networking). This project focuses specifically on coding agent orchestration—session management, HITL (human-in-the-loop) flows, and universal event schemas. These concerns are complementary.

**Does it support [platform]?**
The server is a single Rust binary that runs anywhere with a curl install. If your platform can run Linux binaries (Docker, VMs, etc.), it works. See the deployment guides for E2B, Daytona, Vercel Sandboxes, and Docker.

**Can I use this with my personal API keys?**
Yes. Use `sandbox-agent credentials extract-env` to extract API keys from your local agent configs (Claude Code, Codex, OpenCode, Amp) and pass them to the sandbox environment.
