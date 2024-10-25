# Define base arguments for versioning and optimization
ARG RUST_NIGHTLY_VERSION=nightly-2024-10-22
ARG RUST_TARGET_CPU=native
#ARG RUSTFLAGS="-C target-cpu=${RUST_TARGET_CPU} -C opt-level=3 -C codegen-units=1 -C lto=fat -C embed-bitcode=yes -C strip=symbols -Z share-generics=y -Z threads=8 -Z dylib-lto --cfg tokio_unstable"
#ARG RUSTFLAGS="--cfg tokio_unstable"
ARG RUSTFLAGS="--cfg tokio_unstable -C link-arg=-latomic"


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

# Builder stage
FROM packages AS builder

# Install Rust Nightly with specific optimizations
ARG RUST_NIGHTLY_VERSION
ARG RUSTFLAGS
ENV RUSTFLAGS=${RUSTFLAGS}

# Install Rust with target-specific optimizations
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain ${RUST_NIGHTLY_VERSION} && \
    $HOME/.cargo/bin/rustup component add rust-src && \
    $HOME/.cargo/bin/rustc --version

# Add Cargo to PATH
ENV PATH="/root/.cargo/bin:${PATH}"

# Set the working directory
WORKDIR /app

# Copy project files
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates
COPY events ./events

# Build the application with optimizations and caching
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --locked && \
    mkdir -p /app/build && \
    cp target/debug/hyperion-proxy /app/build/ && \
    cp target/debug/nyc /app/build/

FROM ubuntu:22.04 AS runtime-base
RUN apt-get update && \
    apt-get install -y \
        libssl3 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# Set common environment variables
ENV RUST_BACKTRACE=1 \
    RUST_LOG=info


FROM runtime-base AS hyperion-proxy
# Copy the optimized binary
COPY --from=builder /app/build/hyperion-proxy /
# Configure container metadata
LABEL org.opencontainers.image.source="https://github.com/yourusername/hyperion-proxy" \
      org.opencontainers.image.description="Optimized Hyperion Proxy Server" \
      org.opencontainers.image.version="1.0.0"
# Expose the service port
EXPOSE 8080
# Define the entrypoint with runtime optimizations
ENTRYPOINT ["/hyperion-proxy"]
# Set default command (can be overridden in docker-compose.yml)
CMD ["0.0.0.0:8080"]

# Create runtime image for nyc
FROM runtime-base AS nyc
COPY --from=builder /app/build/nyc /
LABEL org.opencontainers.image.source="https://github.com/yourusername/nyc" \
      org.opencontainers.image.description="Optimized NYC Server" \
      org.opencontainers.image.version="1.0.0"
# Define the entrypoint with runtime optimizations
ENTRYPOINT ["/nyc"]
# Set default command (can be overridden in docker-compose.yml)
CMD ["--ip", "0.0.0.0", "--port", "35565"]
