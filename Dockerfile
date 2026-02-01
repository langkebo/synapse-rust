# Stage 1: Build
FROM rust:1.93-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy source to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -f target/release/deps/synapse_rust*

# Copy source code
COPY src ./src
COPY migrations ./migrations

# Build real application
RUN cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    libpq5 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy binary
COPY --from=builder /app/target/release/synapse-rust /usr/local/bin/synapse-rust

# Copy config and migrations (though usually mounted)
COPY homeserver.yaml ./
COPY migrations ./migrations

# Health check script
RUN echo '#!/bin/sh\ncurl -f http://localhost:8008/_matrix/client/versions || exit 1' > /usr/local/bin/healthcheck.sh && \
    chmod +x /usr/local/bin/healthcheck.sh

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/healthcheck.sh"]

# Environment variables
ENV RUST_LOG=info
ENV SYNAPSE_CONFIG_PATH=/app/homeserver.yaml

EXPOSE 8008

ENTRYPOINT ["synapse-rust"]
