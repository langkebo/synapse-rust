# Multi-stage build for optimal image size
FROM rust:1.93-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy project files
COPY Cargo.toml ./
COPY src ./src
COPY migrations ./migrations

# Build the application
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    libpq5 \
    ca-certificates \
    curl \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# Copy binary
COPY --from=builder /app/target/release/synapse-rust /usr/local/bin/synapse-rust

# Copy config from docker directory
COPY docker/config ./config
COPY docker/migrations ./migrations

# Environment variables
ENV RUST_LOG=info
ENV SYNAPSE_CONFIG_PATH=/app/config/homeserver.yaml
ENV DATABASE_URL=postgres://synapse:synapse@db:5432/synapse_test
ENV REDIS_URL=redis://redis:6379

# Expose port
EXPOSE 8008

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8008/_matrix/client/versions || exit 1

# Run the application
ENTRYPOINT ["synapse-rust"]
