# Use Alpine as base image
FROM alpine:3.19 as packages

# Install curl, build-base (Alpine's equivalent of build-essential), and OpenSSL development packages
RUN apk update && \
    apk add --no-cache curl build-base openssl-dev pkgconfig musl-dev

FROM packages as builder

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Set the working directory
WORKDIR /app

# Copy
#COPY Cargo.toml Cargo.lock ./
#
#COPY backend/src ./backend/src
#COPY backend/Cargo.toml ./backend/
#
#COPY recompositor/src ./recompositor/src
#COPY recompositor/Cargo.toml ./recompositor/
#
#COPY protocol/src ./protocol/src
#COPY protocol/Cargo.toml ./protocol/
#
#COPY client/src ./client/src
#COPY client/Cargo.toml ./client/

# Build the source code
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked -p backend

# copy the target/release/backend outside of cache
RUN --mount=type=cache,target=/app/target \
    mkdir -p /build && \
    cp target/release/backend /build/backend

FROM scratch

COPY --from=builder /build/backend /

EXPOSE 8080
ENTRYPOINT ["/backend"]

