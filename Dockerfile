# Stage 1: Builder
FROM rust:1.93-bookworm AS builder

WORKDIR /usr/src/app

COPY Cargo.toml Cargo.lock ./
COPY benches ./benches
COPY migrations ./migrations
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo fetch --locked

COPY src ./src
RUN cargo build --release --locked

# Stage 2: Runtime
FROM gcr.io/distroless/cc-debian12

WORKDIR /app

COPY --from=builder /usr/src/app/target/release/synapse-rust /app/synapse-rust

EXPOSE 8008

ENV RUST_LOG=info
ENV SYNAPSE_CONFIG_PATH=/app/config/homeserver.yaml

CMD ["/app/synapse-rust"]
