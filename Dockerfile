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

COPY .cargo ./.cargo

COPY Cargo.toml Cargo.lock ./

COPY chunk/Cargo.toml ./chunk/Cargo.toml
COPY chunk/src ./chunk/src

COPY generator/Cargo.toml ./generator/Cargo.toml
COPY generator/build.rs ./generator/build.rs
COPY generator/generated.zip ./generator/generated.zip
COPY generator/src ./generator/src

COPY generator-build/Cargo.toml ./generator-build/Cargo.toml
COPY generator-build/src ./generator-build/src

COPY server/Cargo.toml ./server/Cargo.toml
COPY server/src ./server/src

# Define environment variable for Cargo home, if not using the default
ENV CARGO_HOME=/usr/local/cargo

# Build the source code using Rust Nightly with
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    RUSTFLAGS='-C target-cpu=native' cargo build --release --locked -p server

# Copy the built executable from the cache to a clean directory
RUN --mount=type=cache,target=/app/target \
    mkdir -p /build && \
    cp target/release/server /build/server

FROM scratch

# Copy the built executable into the final image
COPY --from=builder /build/server /

EXPOSE 25565
ENTRYPOINT ["/server"]
