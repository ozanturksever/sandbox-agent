# syntax=docker/dockerfile:1.10.0
#
# Cross-compile sandbox-agent for aarch64-unknown-linux-musl (static binary).
#
# Key difference from linux-x86_64.Dockerfile:
#   - No OpenSSL cross-compilation needed (reqwest uses rustls-tls feature)
#   - Uses aarch64-unknown-linux-musl cross-toolchain
#
# Usage:
#   docker build --target aarch64-builder -f docker/release/linux-aarch64.Dockerfile \
#     -t sandbox-agent-builder-aarch64-unknown-linux-musl .

# Build inspector frontend
FROM node:22-alpine AS inspector-build
WORKDIR /app
RUN npm install -g pnpm

# Copy package files for workspaces
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY frontend/packages/inspector/package.json ./frontend/packages/inspector/
COPY sdks/cli-shared/package.json ./sdks/cli-shared/
COPY sdks/typescript/package.json ./sdks/typescript/

# Install dependencies
RUN pnpm install --filter @sandbox-agent/inspector...

# Copy SDK source (with pre-generated types from docs/openapi.json)
COPY docs/openapi.json ./docs/
COPY sdks/cli-shared ./sdks/cli-shared
COPY sdks/typescript ./sdks/typescript

# Build cli-shared and SDK (just tsup, skip generate since types are pre-generated)
RUN cd sdks/cli-shared && pnpm exec tsup
RUN cd sdks/typescript && SKIP_OPENAPI_GEN=1 pnpm exec tsup

# Copy inspector source and build
COPY frontend/packages/inspector ./frontend/packages/inspector
RUN cd frontend/packages/inspector && pnpm exec vite build

FROM rust:1.88.0 AS base

# Install dependencies for aarch64 cross-compilation
RUN apt-get update && apt-get install -y \
    llvm-14-dev \
    libclang-14-dev \
    clang-14 \
    pkg-config \
    ca-certificates \
    g++ \
    git \
    curl \
    wget && \
    rm -rf /var/lib/apt/lists/*

# Download aarch64 musl cross-toolchain
RUN wget -q https://github.com/cross-tools/musl-cross/releases/latest/download/aarch64-unknown-linux-musl.tar.xz && \
    tar -xf aarch64-unknown-linux-musl.tar.xz -C /opt/ && \
    rm aarch64-unknown-linux-musl.tar.xz

# Install musl target for aarch64
RUN rustup target add aarch64-unknown-linux-musl

# Set environment variables for aarch64 cross-compilation
# Note: No OpenSSL needed — reqwest uses rustls-tls
ENV PATH="/opt/aarch64-unknown-linux-musl/bin:$PATH" \
    LIBCLANG_PATH=/usr/lib/llvm-14/lib \
    CLANG_PATH=/usr/bin/clang-14 \
    CC_aarch64_unknown_linux_musl=aarch64-unknown-linux-musl-gcc \
    CXX_aarch64_unknown_linux_musl=aarch64-unknown-linux-musl-g++ \
    AR_aarch64_unknown_linux_musl=aarch64-unknown-linux-musl-ar \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-unknown-linux-musl-gcc \
    CARGO_INCREMENTAL=0 \
    RUSTFLAGS="-C target-feature=+crt-static -C link-arg=-static-libgcc" \
    CARGO_NET_GIT_FETCH_WITH_CLI=true

WORKDIR /build

# Build for aarch64
FROM base AS aarch64-builder

ARG SANDBOX_AGENT_VERSION
ENV SANDBOX_AGENT_VERSION=${SANDBOX_AGENT_VERSION}

# Copy the source code
COPY . .

# Copy pre-built inspector frontend
COPY --from=inspector-build /app/frontend/packages/inspector/dist ./frontend/packages/inspector/dist

# Build for Linux with musl (static binary) - aarch64
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build -p sandbox-agent --release --target aarch64-unknown-linux-musl && \
    mkdir -p /artifacts && \
    cp target/aarch64-unknown-linux-musl/release/sandbox-agent /artifacts/sandbox-agent-aarch64-unknown-linux-musl

CMD ["ls", "-la", "/artifacts"]
