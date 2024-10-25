# Define base arguments for versioning and optimization
ARG RUST_NIGHTLY_VERSION=nightly-2024-10-22
ARG RUST_TARGET_CPU=native
#ARG RUSTFLAGS="-C target-cpu=${RUST_TARGET_CPU} -C opt-level=3 -C codegen-units=1 -C lto=fat -C embed-bitcode=yes -C strip=symbols -Z share-generics=y -Z threads=8 -Zdylib-lto --cfg tokio_unstable"
#ARG RUSTFLAGS="--cfg tokio_unstable"

ARG RUSTFLAGS="--cfg tokio_unstable -C link-arg=-latomic"

# Use Alpine as base image with specific version for reproducibility
FROM alpine:3.20.3 AS packages

# Install essential build packages
RUN apk update && \
    apk add --no-cache \
        curl=8.10.1-r0 \
        build-base=0.5-r3 \
        openssl-dev=3.3.2-r1 \
        openssl-libs-static \
        pkgconfig \
        musl-dev=1.2.5-r0 \
        cmake=3.29.3-r0 \
        perl=5.38.2-r0 \
        gcc \
        libatomic \
        linux-headers=6.6-r0

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

# Create minimal runtime image for hyperion-proxy
FROM scratch AS hyperion-proxy

# Copy the optimized binary
COPY --from=builder /app/build/hyperion-proxy /

# Configure container metadata
LABEL org.opencontainers.image.source="https://github.com/yourusername/hyperion-proxy" \
      org.opencontainers.image.description="Optimized Hyperion Proxy Server" \
      org.opencontainers.image.version="1.0.0"

# Expose the service port
EXPOSE 8080

# Set resource limits and runtime parameters
ENV RUST_BACKTRACE=1 \
    RUST_LOG=info

# Define the entrypoint with runtime optimizations
ENTRYPOINT ["/hyperion-proxy"]

# Create minimal runtime image for nyc
FROM scratch AS nyc

# Copy the optimized binary
COPY --from=builder /app/build/nyc /

# Configure container metadata
LABEL org.opencontainers.image.source="https://github.com/yourusername/nyc" \
      org.opencontainers.image.description="Optimized NYC Server" \
      org.opencontainers.image.version="1.0.0"

# Set resource limits and runtime parameters
ENV RUST_BACKTRACE=1 \
    RUST_LOG=info

# Define the entrypoint with runtime optimizations
ENTRYPOINT ["/nyc"]