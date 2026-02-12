#!/bin/bash
# ============================================================================
# Local Release — build binaries and create a GitHub Release from your machine
#
# Builds sandbox-agent + gigacode for:
#   - x86_64-unknown-linux-musl   (amd64 Linux)
#   - aarch64-unknown-linux-musl  (arm64 Linux)
#   - x86_64-apple-darwin         (amd64 macOS)
#   - aarch64-apple-darwin         (arm64 macOS)
#
# All builds use Docker (via docker/release/build.sh) for reproducibility.
#
# Usage:
#   ./scripts/local-release.sh <tag>
#   ./scripts/local-release.sh v0.2.0-fork.3
#   ./scripts/local-release.sh v0.2.0-fork.3 --dry-run
#   ./scripts/local-release.sh v0.2.0-fork.3 --targets linux
#   ./scripts/local-release.sh v0.2.0-fork.3 --targets "aarch64-unknown-linux-musl"
#
# Options:
#   --dry-run       Build binaries but don't create a GitHub Release
#   --targets TYPE  Which targets to build:
#                     all    — all 4 targets (default)
#                     linux  — amd64 + arm64 Linux only
#                     macos  — amd64 + arm64 macOS only
#                     TARGET — a single specific target string
#   --no-clean      Don't clean dist/ before building
#   --repo REPO     GitHub repo for release (default: auto-detect from git remote)
#
# Prerequisites:
#   - Docker (for building all targets)
#   - gh CLI (for creating GitHub Releases)
#
# ============================================================================

set -euo pipefail

# Navigate to sandbox-agent root
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SA_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$SA_ROOT"

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------

ALL_TARGETS=(
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-musl
  x86_64-apple-darwin
  aarch64-apple-darwin
)

LINUX_TARGETS=(
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-musl
)

MACOS_TARGETS=(
  x86_64-apple-darwin
  aarch64-apple-darwin
)

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------

TAG=""
DRY_RUN=false
CLEAN=true
TARGET_SET="all"
GH_REPO=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)   DRY_RUN=true; shift ;;
    --no-clean)  CLEAN=false; shift ;;
    --targets)   TARGET_SET="$2"; shift 2 ;;
    --repo)      GH_REPO="$2"; shift 2 ;;
    --help|-h)
      sed -n '2,/^# =====/p' "$0" | grep '^#' | sed 's/^# \?//'
      exit 0
      ;;
    -*)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
    *)
      if [[ -z "$TAG" ]]; then
        TAG="$1"
      else
        echo "Unexpected argument: $1" >&2
        exit 1
      fi
      shift
      ;;
  esac
done

if [[ -z "$TAG" ]]; then
  echo "Usage: $0 <tag> [options]"
  echo "       $0 v0.2.0-fork.3"
  echo "       $0 v0.2.0-fork.3 --dry-run"
  echo "       $0 --help"
  exit 1
fi

# Resolve targets
case "$TARGET_SET" in
  all)    TARGETS=("${ALL_TARGETS[@]}") ;;
  linux)  TARGETS=("${LINUX_TARGETS[@]}") ;;
  macos)  TARGETS=("${MACOS_TARGETS[@]}") ;;
  *)      TARGETS=("$TARGET_SET") ;;
esac

# Auto-detect GitHub repo from git remote
if [[ -z "$GH_REPO" ]]; then
  REMOTE_URL=$(git remote get-url origin 2>/dev/null || true)
  if [[ "$REMOTE_URL" =~ github\.com[:/](.+)\.git$ ]]; then
    GH_REPO="${BASH_REMATCH[1]}"
  elif [[ "$REMOTE_URL" =~ github\.com[:/](.+)$ ]]; then
    GH_REPO="${BASH_REMATCH[1]}"
  fi
  if [[ -z "$GH_REPO" ]]; then
    echo "ERROR: Could not detect GitHub repo from git remote. Use --repo OWNER/REPO" >&2
    exit 1
  fi
fi

# ---------------------------------------------------------------------------
# Preflight checks
# ---------------------------------------------------------------------------

echo "=== sandbox-agent Local Release ==="
echo "  Tag:       $TAG"
echo "  Repo:      $GH_REPO"
echo "  Targets:   ${TARGETS[*]}"
echo "  Dry run:   $DRY_RUN"
echo ""

# Check Docker
if ! docker version >/dev/null 2>&1; then
  echo "ERROR: Docker is not running. All builds require Docker." >&2
  exit 1
fi
echo "✓ Docker available"

# Check gh CLI (only needed for upload)
if [[ "$DRY_RUN" == "false" ]]; then
  if ! gh auth status >/dev/null 2>&1; then
    echo "ERROR: gh CLI is not authenticated. Run 'gh auth login' first." >&2
    exit 1
  fi
  echo "✓ gh CLI authenticated"
fi

# Check build.sh exists
if [[ ! -x "docker/release/build.sh" ]]; then
  echo "ERROR: docker/release/build.sh not found or not executable" >&2
  exit 1
fi
echo "✓ Build script found"
echo ""

# ---------------------------------------------------------------------------
# Clean dist/
# ---------------------------------------------------------------------------

if [[ "$CLEAN" == "true" ]]; then
  echo "Cleaning dist/..."
  rm -rf dist/
fi
mkdir -p dist

# ---------------------------------------------------------------------------
# Build all targets
# ---------------------------------------------------------------------------

FAILED=()
SUCCEEDED=()
START_TIME=$SECONDS

for target in "${TARGETS[@]}"; do
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "Building: $target"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  TARGET_START=$SECONDS
  if docker/release/build.sh "$target" "$TAG"; then
    ELAPSED=$(( SECONDS - TARGET_START ))
    echo "✓ $target built successfully (${ELAPSED}s)"
    SUCCEEDED+=("$target")
  else
    ELAPSED=$(( SECONDS - TARGET_START ))
    echo "✗ $target FAILED (${ELAPSED}s)"
    FAILED+=("$target")
  fi
done

TOTAL_BUILD_TIME=$(( SECONDS - START_TIME ))

# ---------------------------------------------------------------------------
# Build summary
# ---------------------------------------------------------------------------

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Build Summary (${TOTAL_BUILD_TIME}s total)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

for t in "${SUCCEEDED[@]+"${SUCCEEDED[@]}"}"; do
  echo "  ✓ $t"
done
for t in "${FAILED[@]+"${FAILED[@]}"}"; do
  echo "  ✗ $t (FAILED)"
done

if [[ ${#FAILED[@]} -gt 0 ]]; then
  echo ""
  echo "ERROR: ${#FAILED[@]} target(s) failed. Fix the errors above and retry." >&2
  echo "       You can retry just the failed targets with:" >&2
  for t in "${FAILED[@]}"; do
    echo "         $0 $TAG --no-clean --targets $t" >&2
  done
  exit 1
fi

# ---------------------------------------------------------------------------
# List artifacts and create checksums
# ---------------------------------------------------------------------------

echo ""
echo "Artifacts in dist/:"
ls -lh dist/
echo ""

echo "Creating SHA256SUMS.txt..."
cd dist
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum * > SHA256SUMS.txt
else
  # macOS uses shasum
  shasum -a 256 * > SHA256SUMS.txt
fi
cat SHA256SUMS.txt
cd ..
echo ""

# ---------------------------------------------------------------------------
# Create GitHub Release
# ---------------------------------------------------------------------------

if [[ "$DRY_RUN" == "true" ]]; then
  echo "=== Dry run complete ==="
  echo "Binaries are in dist/. To upload manually:"
  echo "  gh release create $TAG --repo $GH_REPO --title $TAG --generate-notes dist/*"
  exit 0
fi

echo "Creating GitHub Release: $TAG"
echo "  Repo: $GH_REPO"
echo ""

# Delete existing release if it exists (for re-runs)
if gh release view "$TAG" --repo "$GH_REPO" >/dev/null 2>&1; then
  echo "Release $TAG already exists. Deleting it first..."
  gh release delete "$TAG" --repo "$GH_REPO" --yes --cleanup-tag 2>/dev/null || true
fi

gh release create "$TAG" \
  --repo "$GH_REPO" \
  --title "$TAG" \
  --generate-notes \
  dist/*

echo ""
echo "=== Release complete ==="
echo "  https://github.com/$GH_REPO/releases/tag/$TAG"
