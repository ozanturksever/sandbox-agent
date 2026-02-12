#!/usr/bin/env bash
set -euo pipefail

# Sandbox Agent installer
# Usage: curl -fsSL https://raw.githubusercontent.com/ozanturksever/sandbox-agent/main/install.sh | bash
#   or:  curl -fsSL ... | bash -s -- --version v0.2.0-fork.1
#   or:  curl -fsSL ... | bash -s -- --dir /custom/path
#
# Docker: docker pull ghcr.io/ozanturksever/sandbox-agent:latest

REPO="ozanturksever/sandbox-agent"
INSTALL_DIR="${HOME}/.local/bin"
VERSION=""
BINARY="sandbox-agent"

# ---------------------------------------------------------------------------
# Parse args
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version|-v) VERSION="$2"; shift 2 ;;
    --dir|-d)     INSTALL_DIR="$2"; shift 2 ;;
    --gigacode)   BINARY="gigacode"; shift ;;
    --help|-h)
      echo "Usage: install.sh [OPTIONS]"
      echo ""
      echo "Options:"
      echo "  --version, -v TAG   Install specific version (default: latest)"
      echo "  --dir, -d PATH      Install directory (default: ~/.local/bin)"
      echo "  --gigacode           Install gigacode binary instead of sandbox-agent"
      echo "  --help, -h           Show this help"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# ---------------------------------------------------------------------------
# Detect platform
# ---------------------------------------------------------------------------
detect_target() {
  local os arch target

  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux)
      case "$arch" in
        x86_64|amd64)  target="x86_64-unknown-linux-musl" ;;
        aarch64|arm64) target="aarch64-unknown-linux-musl" ;;
        *) echo "Error: unsupported Linux architecture: $arch" >&2; exit 1 ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        x86_64|amd64)  target="x86_64-apple-darwin" ;;
        aarch64|arm64) target="aarch64-apple-darwin" ;;
        *) echo "Error: unsupported macOS architecture: $arch" >&2; exit 1 ;;
      esac
      ;;
    MINGW*|MSYS*|CYGWIN*)
      target="x86_64-pc-windows-gnu"
      ;;
    *)
      echo "Error: unsupported OS: $os" >&2; exit 1
      ;;
  esac

  echo "$target"
}

# ---------------------------------------------------------------------------
# Resolve version
# ---------------------------------------------------------------------------
resolve_version() {
  if [[ -n "$VERSION" ]]; then
    echo "$VERSION"
    return
  fi

  local latest
  latest="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | head -1 | cut -d'"' -f4)"

  if [[ -z "$latest" ]]; then
    echo "Error: could not determine latest version" >&2
    exit 1
  fi

  echo "$latest"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  local target version asset_name url tmp existing_version

  target="$(detect_target)"
  version="$(resolve_version)"

  # Build asset name
  asset_name="${BINARY}-${target}"
  if [[ "$target" == *windows* ]]; then
    asset_name="${asset_name}.exe"
  fi

  url="https://github.com/${REPO}/releases/download/${version}/${asset_name}"

  echo "Installing ${BINARY} ${version}"
  echo "  Platform: ${target}"
  echo "  From:     ${url}"
  echo "  To:       ${INSTALL_DIR}/${BINARY}"
  echo ""

  # Check for existing installation
  if command -v "$BINARY" &>/dev/null; then
    existing_version="$("$BINARY" --version 2>/dev/null || echo "unknown")"
    echo "  Upgrading from: ${existing_version}"
    echo ""
  fi

  # Download
  tmp="$(mktemp)"
  trap 'rm -f "${tmp:-}"' EXIT

  if ! curl -fSL --progress-bar -o "$tmp" "$url"; then
    echo ""
    echo "Error: download failed."
    echo "  Check that version '${version}' exists and has a binary for '${target}'."
    echo "  Available releases: https://github.com/${REPO}/releases"
    exit 1
  fi

  # Install
  mkdir -p "$INSTALL_DIR"
  mv "$tmp" "${INSTALL_DIR}/${BINARY}"
  chmod +x "${INSTALL_DIR}/${BINARY}"

  # Verify
  if ! "${INSTALL_DIR}/${BINARY}" --version &>/dev/null; then
    echo "Warning: installed binary did not respond to --version (may still work)"
  fi

  echo "Installed ${BINARY} ${version} to ${INSTALL_DIR}/${BINARY}"

  # Check PATH
  if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    echo ""
    echo "  WARNING: ${INSTALL_DIR} is not in your PATH."
    echo ""
    echo "  Add it with:"
    echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo ""
    echo "  Or add to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
    echo "    echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.bashrc"
  fi
}

main
