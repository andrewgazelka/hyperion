# Define an argument for the Rust nightly version
ARG RUST_NIGHTLY_VERSION=nightly-2024-04-24

# Use Debian Bookworm as base image
FROM debian:bookworm-slim as packages

# Install curl, build-essential, and OpenSSL development packages
RUN apt-get update && \
    apt-get install -y curl build-essential openssl libssl-dev pkg-config clang llvm lld mold cmake

FROM packages as builder

# Install Rust Nightly
ARG RUST_NIGHTLY_VERSION
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y && \
   $HOME/.cargo/bin/rustup default ${RUST_NIGHTLY_VERSION}

ENV PATH="/root/.cargo/bin:${PATH}"
ENV CARGO_HOME=/root/.cargo

# get mold path
RUN echo "mold path: $(which mold)"

# rust flags
ENV RUSTFLAGS="-Ctarget-cpu=native -Clinker=/usr/bin/clang -Clink-arg=--ld-path=/usr/bin/mold -Zshare-generics=y -Zthreads=0"

# Set the working directory
WORKDIR /app

COPY Cargo.toml Cargo.lock ./

COPY crates ./crates

FROM builder as release

RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/app/target \
    CARGO_TERM_COLOR=never cargo build --release --locked -p server -F trace-simple

RUN --mount=type=cache,target=/app/target \
    mkdir -p /build && \
    cp target/release/server /build/server

FROM builder as debug

RUN rustup component add rustc-codegen-cranelift-preview

ENV RUSTFLAGS="-Ctarget-cpu=native -Clinker=/usr/bin/clang -Clink-arg=--ld-path=/usr/bin/mold -Zshare-generics=y -Zthreads=0 -Zcodegen-backend=cranelift"

RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/app/target \
    CARGO_TERM_COLOR=never cargo build --locked -p server

RUN --mount=type=cache,target=/app/target \
    mkdir -p /build && \
    cp target/debug/server /build/server && \
    cp target/cargo-timings/cargo-timing.html /build/cargo-timing.html

FROM debian:bookworm-slim as debug-bin
COPY --from=debug /build/server /hyperion
ENTRYPOINT ["/hyperion"]

FROM debian:bookworm-slim as release-bin
COPY --from=release /build/server /hyperion
ENTRYPOINT ["/hyperion"]
