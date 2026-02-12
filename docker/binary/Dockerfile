# syntax=docker/dockerfile:1.13.0
# ============================================================================
# sandbox-agent binary builder — multi-arch (amd64 + arm64)
#
# Produces a minimal image with just /usr/local/bin/sandbox-agent.
# Used as a source stage for COPY --from in other Dockerfiles (e.g. sidecar).
#
# Build:
#   docker buildx build --platform linux/amd64,linux/arm64 \
#     -t ghcr.io/logsign/sandbox-agent:latest \
#     -f docker/binary/Dockerfile --push .
# ============================================================================

# ============================================================================
# AMD64 Builder - Uses cross-tools musl toolchain
# ============================================================================
FROM --platform=linux/amd64 rust:1.88.0 AS builder-amd64

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    musl-tools \
    musl-dev \
    llvm-14-dev \
    libclang-14-dev \
    clang-14 \
    libssl-dev \
    pkg-config \
    ca-certificates \
    g++ \
    g++-multilib \
    git \
    curl \
    wget && \
    rm -rf /var/lib/apt/lists/*

# Download cross-tools musl toolchain
RUN wget -q https://github.com/cross-tools/musl-cross/releases/latest/download/x86_64-unknown-linux-musl.tar.xz && \
    tar -xf x86_64-unknown-linux-musl.tar.xz -C /opt/ && \
    rm x86_64-unknown-linux-musl.tar.xz && \
    rustup target add x86_64-unknown-linux-musl

ENV PATH="/opt/x86_64-unknown-linux-musl/bin:$PATH" \
    LIBCLANG_PATH=/usr/lib/llvm-14/lib \
    CLANG_PATH=/usr/bin/clang-14 \
    CC_x86_64_unknown_linux_musl=x86_64-unknown-linux-musl-gcc \
    CXX_x86_64_unknown_linux_musl=x86_64-unknown-linux-musl-g++ \
    AR_x86_64_unknown_linux_musl=x86_64-unknown-linux-musl-ar \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=x86_64-unknown-linux-musl-gcc \
    CARGO_INCREMENTAL=0 \
    CARGO_NET_GIT_FETCH_WITH_CLI=true

# Build OpenSSL for musl
ENV SSL_VER=1.1.1w
RUN wget https://www.openssl.org/source/openssl-$SSL_VER.tar.gz && \
    tar -xzf openssl-$SSL_VER.tar.gz && \
    cd openssl-$SSL_VER && \
    ./Configure no-shared no-async --prefix=/musl --openssldir=/musl/ssl linux-x86_64 && \
    make -j$(nproc) && \
    make install_sw && \
    cd .. && \
    rm -rf openssl-$SSL_VER*

ENV OPENSSL_DIR=/musl \
    OPENSSL_INCLUDE_DIR=/musl/include \
    OPENSSL_LIB_DIR=/musl/lib \
    PKG_CONFIG_ALLOW_CROSS=1 \
    RUSTFLAGS="-C target-feature=+crt-static -C link-arg=-static-libgcc" \
    SANDBOX_AGENT_SKIP_INSPECTOR=1

WORKDIR /build
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build -p sandbox-agent --release --target x86_64-unknown-linux-musl && \
    cp target/x86_64-unknown-linux-musl/release/sandbox-agent /sandbox-agent

# ============================================================================
# ARM64 Builder - Uses Alpine with native musl
# ============================================================================
FROM --platform=linux/arm64 rust:1.88-alpine AS builder-arm64

RUN apk add --no-cache \
    musl-dev \
    clang \
    llvm-dev \
    openssl-dev \
    openssl-libs-static \
    pkgconfig \
    git \
    curl \
    build-base

RUN rustup target add aarch64-unknown-linux-musl

ENV CARGO_INCREMENTAL=0 \
    CARGO_NET_GIT_FETCH_WITH_CLI=true \
    RUSTFLAGS="-C target-feature=+crt-static" \
    SANDBOX_AGENT_SKIP_INSPECTOR=1

WORKDIR /build
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build -p sandbox-agent --release --target aarch64-unknown-linux-musl && \
    cp target/aarch64-unknown-linux-musl/release/sandbox-agent /sandbox-agent

# ============================================================================
# Select the appropriate builder based on target architecture
# ============================================================================
ARG TARGETARCH
FROM builder-${TARGETARCH} AS builder

# ============================================================================
# Minimal runtime — just the binary
# ============================================================================
FROM debian:bookworm-slim

COPY --from=builder /sandbox-agent /usr/local/bin/sandbox-agent
RUN chmod +x /usr/local/bin/sandbox-agent

ENTRYPOINT ["sandbox-agent"]
