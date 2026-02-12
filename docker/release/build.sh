#!/bin/bash
set -euo pipefail

TARGET=${1:-x86_64-unknown-linux-musl}
VERSION=${2:-}

# Build arguments for Docker
BUILD_ARGS=""
if [ -n "$VERSION" ]; then
  BUILD_ARGS="--build-arg SANDBOX_AGENT_VERSION=$VERSION"
  echo "Building with version: $VERSION"
fi

case $TARGET in
  x86_64-unknown-linux-musl)
    echo "Building for Linux x86_64 musl"
    DOCKERFILE="linux-x86_64.Dockerfile"
    TARGET_STAGE="x86_64-builder"
    BINARY="sandbox-agent-$TARGET"
    GIGACODE="gigacode-$TARGET"
    ;;
  aarch64-unknown-linux-musl)
    echo "Building for Linux aarch64 musl"
    DOCKERFILE="linux-aarch64.Dockerfile"
    TARGET_STAGE="aarch64-builder"
    BINARY="sandbox-agent-$TARGET"
    GIGACODE="gigacode-$TARGET"
    ;;
  x86_64-pc-windows-gnu)
    echo "Building for Windows x86_64"
    DOCKERFILE="windows.Dockerfile"
    TARGET_STAGE=""
    BINARY="sandbox-agent-$TARGET.exe"
    GIGACODE="gigacode-$TARGET.exe"
    ;;
  x86_64-apple-darwin)
    echo "Building for macOS x86_64"
    DOCKERFILE="macos-x86_64.Dockerfile"
    TARGET_STAGE="x86_64-builder"
    BINARY="sandbox-agent-$TARGET"
    GIGACODE="gigacode-$TARGET"
    ;;
  aarch64-apple-darwin)
    echo "Building for macOS aarch64"
    DOCKERFILE="macos-aarch64.Dockerfile"
    TARGET_STAGE="aarch64-builder"
    BINARY="sandbox-agent-$TARGET"
    GIGACODE="gigacode-$TARGET"
    ;;
  *)
    echo "Unsupported target: $TARGET"
    exit 1
    ;;
 esac

# Detect if cross-compilation is needed (e.g. building arm64 on x86_64 host)
DOCKER_PLATFORM=""
case $TARGET in
  aarch64-*linux*) DOCKER_PLATFORM="linux/arm64" ;;
  x86_64-*linux*)  DOCKER_PLATFORM="linux/amd64" ;;
esac

CROSS_COMPILE=false
if [ -n "$DOCKER_PLATFORM" ]; then
  HOST_ARCH=$(uname -m)
  case "$HOST_ARCH" in
    x86_64|amd64)   HOST_PLATFORM="linux/amd64" ;;
    aarch64|arm64)  HOST_PLATFORM="linux/arm64" ;;
    *)              HOST_PLATFORM="" ;;
  esac
  if [ "$DOCKER_PLATFORM" != "$HOST_PLATFORM" ]; then
    CROSS_COMPILE=true
    echo "Cross-compiling: host=$HOST_PLATFORM target=$DOCKER_PLATFORM (using buildx + QEMU)"
  fi
fi

DOCKER_BUILDKIT=1
IMAGE_TAG="sandbox-agent-builder-$TARGET"

if [ "$CROSS_COMPILE" = true ]; then
  # Cross-compilation: use buildx with --platform to run under QEMU
  if [ -n "$TARGET_STAGE" ]; then
    docker buildx build --platform "$DOCKER_PLATFORM" \
      --target "$TARGET_STAGE" $BUILD_ARGS \
      -f "docker/release/$DOCKERFILE" \
      -t "$IMAGE_TAG" --load .
  else
    docker buildx build --platform "$DOCKER_PLATFORM" \
      $BUILD_ARGS \
      -f "docker/release/$DOCKERFILE" \
      -t "$IMAGE_TAG" --load .
  fi
else
  # Native build: plain docker build
  if [ -n "$TARGET_STAGE" ]; then
    docker build --target "$TARGET_STAGE" $BUILD_ARGS -f "docker/release/$DOCKERFILE" -t "$IMAGE_TAG" .
  else
    docker build $BUILD_ARGS -f "docker/release/$DOCKERFILE" -t "$IMAGE_TAG" .
  fi
fi

CONTAINER_ID=$(docker create "$IMAGE_TAG")
mkdir -p dist

docker cp "$CONTAINER_ID:/artifacts/$BINARY" "dist/"
docker cp "$CONTAINER_ID:/artifacts/$GIGACODE" "dist/"
docker rm "$CONTAINER_ID"

if [[ "$BINARY" != *.exe ]]; then
  chmod +x "dist/$BINARY"
  chmod +x "dist/$GIGACODE"
fi

echo "Binary saved to: dist/$BINARY"
echo "Binary saved to: dist/$GIGACODE"
