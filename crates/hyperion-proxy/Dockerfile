# Define an argument for the Rust nightly version
ARG RUST_NIGHTLY_VERSION=nightly-2024-03-16

# Use Alpine as base image
FROM alpine:3.19 as packages

# Install curl, build-base (Alpine's equivalent of build-essential), and OpenSSL development packages
RUN apk update && \
    apk add --no-cache curl build-base openssl-dev pkgconfig musl-dev

FROM packages as builder

# Install Rust Nightly
ARG RUST_NIGHTLY_VERSION
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y && \
   $HOME/.cargo/bin/rustup default ${RUST_NIGHTLY_VERSION}

ENV PATH="/root/.cargo/bin:${PATH}"

# Set the working directory
WORKDIR /app

# Copy the Cargo configuration and source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build the source code using Rust Nightly
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked

# Copy the built executable from the cache to a clean directory
RUN --mount=type=cache,target=/app/target \
    mkdir -p /build && \
    cp target/release/hyperion-proxy /build/hyperion-proxy

FROM scratch

# Copy the built executable into the final image
COPY --from=builder /build/hyperion-proxy /

EXPOSE 8080
ENTRYPOINT ["/hyperion-proxy"]
