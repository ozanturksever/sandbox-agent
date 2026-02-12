# Contributing

Documentation lives in `docs/` (Mintlify). Start with:

- `docs/index.mdx` for the overview
- `docs/quickstart.mdx` to run the daemon
- `docs/http-api.mdx` and `docs/cli.mdx` for API references

## Development Setup

### Prerequisites

- Rust (latest stable)
- Node.js 20+
- pnpm 9+
- [just](https://github.com/casey/just) (optional, but recommended)

### Quickstart

Run the agent locally:

```bash
sandbox-agent --token "$SANDBOX_TOKEN" --host 127.0.0.1 --port 2468
```

Extract API keys from local agent configs (Claude Code, Codex, OpenCode, Amp):

```bash
# Print env vars
sandbox-agent credentials extract-env

# Export to current shell
eval "$(sandbox-agent credentials extract-env --export)"
```

Run the web console (includes all dependencies):

```bash
pnpm dev -F @sandbox-agent/inspector
# or
just dev
```

### Common Commands

```bash
# Run checks (cargo check, fmt, typecheck)
just check

# Run tests
just test

# Format code
just fmt

# Build the agent
just build
```

## Releasing

Releases are built **locally** and uploaded to GitHub Releases. This avoids CI runner availability issues (especially for ARM) and gives fast, reproducible builds.

### Prerequisites

- **Docker** — all targets are built inside Docker containers for reproducibility
- **`gh` CLI** — authenticated (`gh auth login`) for creating GitHub Releases

### Supported Platforms

| Target | Architecture | OS |
|--------|-------------|----|
| `x86_64-unknown-linux-musl` | amd64 | Linux |
| `aarch64-unknown-linux-musl` | arm64 | Linux |
| `x86_64-apple-darwin` | amd64 | macOS |
| `aarch64-apple-darwin` | arm64 | macOS |

### Creating a Release

Use `scripts/local-release.sh` (or the `just` shortcuts) to build all binaries and create a GitHub Release:

```bash
# Build all 4 targets and create a GitHub Release
./scripts/local-release.sh v0.2.0-fork.4
# or
just release-local v0.2.0-fork.4

# Dry run — build binaries only, no GitHub Release
./scripts/local-release.sh v0.2.0-fork.4 --dry-run
# or
just release-local-dry v0.2.0-fork.4

# Build only Linux targets
./scripts/local-release.sh v0.2.0-fork.4 --targets linux
# or
just release-local-linux v0.2.0-fork.4

# Build only macOS targets
./scripts/local-release.sh v0.2.0-fork.4 --targets macos

# Build a single target
./scripts/local-release.sh v0.2.0-fork.4 --targets aarch64-unknown-linux-musl

# Retry without cleaning dist/ (reuses already-built binaries)
./scripts/local-release.sh v0.2.0-fork.4 --no-clean
```

### What the Release Script Does

1. Builds each target via `docker/release/build.sh` (all targets are built inside Docker for reproducibility)
2. Creates SHA256 checksums (`dist/SHA256SUMS.txt`)
3. Deletes any existing GitHub Release with the same tag
4. Creates a new GitHub Release with all binaries attached
5. Auto-detects your GitHub repo from `git remote`

Binaries are output to `dist/` and include both `sandbox-agent` and `gigacode` for each target.

### Docker Images (Optional)

Docker image builds are **not** part of the default release. If needed, use the GitHub Actions workflow with the Docker option enabled:

1. Go to **GitHub → Actions → "Fork Release"**
2. Click **"Run workflow"**
3. Enter the tag and check **"Build and push Docker images"**

This builds multi-arch Docker images (amd64 + arm64) and pushes them to GHCR.

### Upstream Release (not for fork use)

The TypeScript release scripts in `scripts/release/main.ts` are for the **upstream** project's release pipeline. They use Depot runners, R2 storage, npm/crates.io publishing, and Docker Hub. See `just release --help` for details.

## Project Structure

```
sandbox-daemon/
├── server/packages/     # Rust crates
│   ├── sandbox-agent/   # Main agent binary
│   ├── agent-schema/    # Agent-specific schemas (Claude, Codex, etc.)
│   └── ...
├── sdks/
│   ├── typescript/      # TypeScript SDK (npm: sandbox-agent)
│   └── cli/             # CLI wrapper (npm: @sandbox-agent/cli)
├── frontend/packages/
│   └── inspector/       # Web console UI
├── docs/                # Mintlify documentation
└── scripts/release/     # Release automation
```
