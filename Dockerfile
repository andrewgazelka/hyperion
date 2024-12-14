# Define base arguments for versioning and optimization
ARG RUST_NIGHTLY_VERSION=nightly-2024-11-29
ARG RUSTFLAGS="-Z share-generics=y -Z threads=8"
ARG CARGO_HOME=/usr/local/cargo

# Use Ubuntu as base image
FROM ubuntu:22.04 AS packages

# Prevent apt from prompting for user input
ENV DEBIAN_FRONTEND=noninteractive

# Install essential build packages
FROM ubuntu:22.04 AS packages
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && \
    apt-get install -y \
        curl \
        build-essential \
        libssl-dev \
        pkg-config \
        cmake \
        perl \
        gcc \
        linux-headers-generic \
        libclang1 \
        llvm-dev \
        libclang-dev \
    && rm -rf /var/lib/apt/lists/*

# Base builder stage with Rust installation
FROM packages AS builder-base
ARG RUST_NIGHTLY_VERSION
ARG RUSTFLAGS
ARG CARGO_HOME
ENV RUSTFLAGS=${RUSTFLAGS}
ENV CARGO_HOME=${CARGO_HOME}
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain ${RUST_NIGHTLY_VERSION} && \
    $CARGO_HOME/bin/rustup component add rust-src && \
    $CARGO_HOME/bin/rustc --version
ENV PATH="${CARGO_HOME}/bin:${PATH}"
WORKDIR /app
COPY . .

RUN --mount=type=cache,target=${CARGO_HOME}/registry \
    --mount=type=cache,target=${CARGO_HOME}/git \
    --mount=type=cache,target=/app/target \
    cargo fetch

# Release builder
FROM builder-base AS build-release
ARG CARGO_HOME
RUN --mount=type=cache,target=${CARGO_HOME}/registry \
    --mount=type=cache,target=${CARGO_HOME}/git \
    --mount=type=cache,target=/app/target \
    cargo build --profile release-full --frozen && \
    mkdir -p /app/build && \
    cp target/release-full/hyperion-proxy /app/build/ && \
    cp target/release-full/tag /app/build/

# Runtime base image
FROM ubuntu:22.04 AS runtime-base
RUN apt-get update && \
    apt-get install -y \
        libssl3 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*
ENV RUST_BACKTRACE=1 \
    RUST_LOG=info

# Hyperion Proxy Release
FROM runtime-base AS hyperion-proxy
COPY --from=build-release /app/build/hyperion-proxy /
LABEL org.opencontainers.image.source="https://github.com/andrewgazelka/hyperion" \
      org.opencontainers.image.description="Hyperion Proxy Server" \
      org.opencontainers.image.version="0.1.0"
EXPOSE 8080
ENTRYPOINT ["/hyperion-proxy"]
CMD ["0.0.0.0:8080"]
# NYC Release
FROM runtime-base AS tag
COPY --from=build-release /app/build/tag /
LABEL org.opencontainers.image.source="https://github.com/andrewgazelka/hyperion" \
      org.opencontainers.image.description="Hyperion Tag Event" \
      org.opencontainers.image.version="0.1.0"
ENTRYPOINT ["/tag"]
CMD ["--ip", "0.0.0.0", "--port", "35565"]
