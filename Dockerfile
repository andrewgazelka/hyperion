# https://nnethercote.github.io/perf-book/compile-times.html
# https://nnethercote.github.io/perf-book/build-configuration.html#minimizing-compile-times

# Define an argument for the Rust nightly version
ARG RUST_NIGHTLY_VERSION=nightly-2024-04-02

# Use Alpine as base image
FROM alpine:3.19 as packages

# Install curl, build-base (Alpine's equivalent of build-essential), and OpenSSL development packages
RUN apk update && \
    apk add --no-cache curl build-base openssl-dev pkgconfig musl-dev clang llvm lld mold cmake

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

# RUN cargo install --version 0.10.2 iai-callgrind-runner

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

RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/app/target \
    CARGO_TERM_COLOR=never cargo build --timings --locked -p server

RUN --mount=type=cache,target=/app/target \
    mkdir -p /build && \
    cp target/debug/server /build/server && \
    cp target/cargo-timings/cargo-timing.html /build/cargo-timing.html

# FROM rust as cli

# RUN apt-get update && apt-get install -y linux-perf

# RUN cargo install flamegraph

# COPY --from=release /build/server /

# EXPOSE 25565

# ENTRYPOINT ["bash"]

FROM scratch as debug-bin
COPY --from=debug /build/server /hyperion
ENTRYPOINT ["/hyperion"]


FROM scratch as release-bin
COPY --from=release /build/server /hyperion
ENTRYPOINT ["/hyperion"]

