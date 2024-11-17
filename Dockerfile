# Define base arguments for versioning and optimization
ARG RUST_NIGHTLY_VERSION=nightly-2024-11-11
ARG RUST_TARGET_CPU=native
#ARG RUSTFLAGS="-C target-cpu=${RUST_TARGET_CPU} -Z share-generics=y -Z threads=8 --cfg tokio_unstable"


ARG PATH_TO_LIBVOIDSTAR=/usr/lib/libvoidstar.so


ARG RUSTFLAGS=" \
                  -Ccodegen-units=1 \
                  -Cpasses=sancov-module \
                  -Cllvm-args=-sanitizer-coverage-level=3 \
                  -Cllvm-args=-sanitizer-coverage-trace-pc-guard \
                  -Clink-args=-Wl,--build-id  \
                  -Clink-args=-Wl,-z,nostart-stop-gc \
                  -L${PATH_TO_LIBVOIDSTAR} \
                  -lvoidstar"



ARG CARGO_HOME=/usr/local/cargo

# Use Ubuntu as base image
FROM ubuntu:22.04 AS packages

# Prevent apt from prompting for user input
ENV DEBIAN_FRONTEND=noninteractive

# Install essential build packages
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
    && rm -rf /var/lib/apt/lists/*

# Base builder stage with Rust installation
FROM packages AS builder-base
ARG RUST_NIGHTLY_VERSION
ARG RUSTFLAGS
ARG PATH_TO_LIBVOIDSTAR
ARG CARGO_HOME
ENV RUSTFLAGS=${RUSTFLAGS}
ENV CARGO_HOME=${CARGO_HOME}
ENV PATH_TO_LIBVOIDSTAR=${PATH_TO_LIBVOIDSTAR}

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain ${RUST_NIGHTLY_VERSION} && \
    $CARGO_HOME/bin/rustup component add rust-src && \
    $CARGO_HOME/bin/rustc --version
ENV PATH="${CARGO_HOME}/bin:${PATH}"

# copy libvoidstar.so to /usr/lib
COPY ./libvoidstar.so /usr/lib/libvoidstar.so

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates
COPY events ./events

RUN --mount=type=cache,target=${CARGO_HOME}/registry \
    --mount=type=cache,target=${CARGO_HOME}/git \
    --mount=type=cache,target=/app/target \
    cargo fetch

# Debug builder
FROM builder-base AS build-debug
ARG CARGO_HOME
RUN --mount=type=cache,target=${CARGO_HOME}/registry \
    --mount=type=cache,target=${CARGO_HOME}/git \
    --mount=type=cache,target=/app/target \
    cargo build --frozen && \
    mkdir -p /app/build && \
    cp target/debug/hyperion-proxy /app/build/ && \
    cp target/debug/proof-of-concept /app/build/ && \
    cp target/debug/hyperion-bot /app/build/

# Release builder
FROM builder-base AS build-release
ARG CARGO_HOME
RUN --mount=type=cache,target=${CARGO_HOME}/registry \
    --mount=type=cache,target=${CARGO_HOME}/git \
    --mount=type=cache,target=/app/target \
    cargo build --profile release-full --frozen && \
    mkdir -p /app/build && \
    cp target/release-full/hyperion-proxy /app/build/ && \
    cp target/release-full/proof-of-concept /app/build/ && \
    cp target/release-full/hyperion-bot /app/build/

# Runtime base image
FROM ubuntu:22.04 AS runtime-base
RUN apt-get update && \
    apt-get install -y \
        libssl3 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*
ENV RUST_BACKTRACE=1 \
    RUST_LOG=info

COPY --from=builder-base /usr/lib/libvoidstar.so /usr/lib/libvoidstar.so

# Hyperion Proxy Debug
FROM runtime-base AS hyperion-proxy-debug
COPY --from=build-debug /app/build/hyperion-proxy /
LABEL org.opencontainers.image.source="https://github.com/yourusername/hyperion-proxy" \
      org.opencontainers.image.description="Debug Build - Hyperion Proxy Server" \
      org.opencontainers.image.version="1.0.0"
EXPOSE 8080
ENTRYPOINT ["/hyperion-proxy"]
CMD ["0.0.0.0:8080"]

# Hyperion Proxy Release
FROM runtime-base AS hyperion-proxy-release
COPY --from=build-release /app/build/hyperion-proxy /
LABEL org.opencontainers.image.source="https://github.com/yourusername/hyperion-proxy" \
      org.opencontainers.image.description="Release Build - Hyperion Proxy Server" \
      org.opencontainers.image.version="1.0.0"
EXPOSE 8080
ENTRYPOINT ["/hyperion-proxy"]
CMD ["0.0.0.0:8080"]

# Proof of Concept Debug
FROM runtime-base AS proof-of-concept-debug
COPY --from=build-debug /app/build/proof-of-concept /
LABEL org.opencontainers.image.source="https://github.com/yourusername/proof-of-concept" \
      org.opencontainers.image.description="Debug Build - Proof of Concept Server" \
      org.opencontainers.image.version="1.0.0"
ENTRYPOINT ["/proof-of-concept"]
CMD ["--ip", "0.0.0.0", "--port", "35565"]

# Proof of Concept Release
FROM runtime-base AS proof-of-concept-release
COPY --from=build-release /app/build/proof-of-concept /
LABEL org.opencontainers.image.source="https://github.com/yourusername/proof-of-concept" \
      org.opencontainers.image.description="Release Build - Proof of Concept Server" \
      org.opencontainers.image.version="1.0.0"
ENTRYPOINT ["/proof-of-concept"]
CMD ["--ip", "0.0.0.0", "--port", "35565"]

# Hyperion Bot Debug
FROM runtime-base AS hyperion-bot-debug
COPY --from=build-debug /app/build/hyperion-bot /
LABEL org.opencontainers.image.source="https://github.com/yourusername/hyperion-bot" \
      org.opencontainers.image.description="Debug Build - Hyperion Bot" \
      org.opencontainers.image.version="1.0.0"
ENTRYPOINT ["/hyperion-bot"]
CMD ["--ip", "0.0.0.0", "--port", "35565"]

# Hyperion Bot Release
FROM runtime-base AS hyperion-bot-release
COPY --from=build-release /app/build/hyperion-bot /
LABEL org.opencontainers.image.source="https://github.com/yourusername/hyperion-bot" \
      org.opencontainers.image.description="Release Build - Hyperion Bot" \
      org.opencontainers.image.version="1.0.0"