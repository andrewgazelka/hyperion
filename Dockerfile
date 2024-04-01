# Define an argument for the Rust nightly version
ARG RUST_NIGHTLY_VERSION=nightly-2024-03-23

# Use Alpine as base image
FROM alpine:3.19 as packages

# Install curl, build-base (Alpine's equivalent of build-essential), and OpenSSL development packages
RUN apk update && \
    apk add --no-cache curl build-base openssl-dev pkgconfig musl-dev clang llvm lld mold valgrind

FROM packages as builder

# Install Rust Nightly
ARG RUST_NIGHTLY_VERSION
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y && \
   $HOME/.cargo/bin/rustup default ${RUST_NIGHTLY_VERSION}

ENV PATH="/root/.cargo/bin:${PATH}"
ENV CARGO_HOME=/root/.cargo

# rust flags
ENV RUSTFLAGS="-Ctarget-cpu=native -Zshare-generics=y -Zthreads=0"

RUN cargo install --version 0.10.2 iai-callgrind-runner

# Set the working directory
WORKDIR /app

# Copy the Cargo configuration and source code
COPY .cargo/config.toml .cargo/config.toml

COPY Cargo.toml Cargo.lock ./

COPY chunk/Cargo.toml ./chunk/Cargo.toml
COPY chunk/src ./chunk/src

COPY broadcast/Cargo.toml ./broadcast/Cargo.toml
COPY broadcast/benches ./broadcast/benches
COPY broadcast/src ./broadcast/src

COPY generator/Cargo.toml ./generator/Cargo.toml
COPY generator/build.rs ./generator/build.rs
COPY generator/generated.tar.zst ./generator/generated.tar.zst
COPY generator/src ./generator/src

COPY generator-build/Cargo.toml ./generator-build/Cargo.toml
COPY generator-build/src ./generator-build/src

COPY server/Cargo.toml ./server/Cargo.toml
COPY server/src ./server/src
COPY server/benches ./server/benches

FROM builder as release

RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/app/target \
    CARGO_TERM_COLOR=never cargo build --release --locked -p server

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

FROM rust as cli

RUN apt-get update && apt-get install -y linux-perf

RUN cargo install flamegraph

COPY --from=release /build/server /

EXPOSE 25565

ENTRYPOINT ["bash"]

FROM scratch as debug-bin
COPY --from=debug /build/server /
ENTRYPOINT ["/server"]


FROM scratch as release-bin
COPY --from=release /build/server /
ENTRYPOINT ["/server"]

FROM builder as bench

# install valgrind
RUN apk add --no-cache valgrind

RUN valgrind --version

ENV IAI_CALLGRIND_RUNNER=/cargo-home/bin/iai-callgrind-runner
ENV PATH="/cargo-home/bin:${PATH}"

RUN echo hi

RUN --mount=type=cache,target=/cargo-home \
    --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/app/target \
    IAI_CALLGRIND_RUNNER=/cargo-home/bin/iai-callgrind-runner cargo bench --locked > /app/bench.txt

RUN cat /app/bench.txt
