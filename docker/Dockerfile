# ============================================================================
# Synapse Rust - Production Docker Image
# ============================================================================
# Build Strategy: Multi-stage build with Rust compilation
#
# This Dockerfile compiles Rust code in the build stage and then creates a
# minimal production image with just the necessary runtime dependencies.
# Benefits:
# - No need for local Rust installation
# - Consistent builds in controlled environment
# - Smaller final image
#
# Build Commands:
#   cd /Users/ljf/Desktop/hu/synapse-rust
#   docker build -f docker/Dockerfile -t synapse-rust:latest .
#
# Run Commands:
#   cd /Users/ljf/Desktop/hu/synapse-rust/docker
#   docker compose up -d
# ============================================================================

# ============================================================================
# Stage 1: Build - Compile Rust code
# ============================================================================
FROM rust:bookworm AS builder

LABEL maintainer="Synapse Rust Team"
LABEL stage="builder"

# Set working directory
WORKDIR /app

# Copy Cargo files first for better caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to download dependencies
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs

# Download dependencies
RUN cargo fetch --locked

# Copy the actual source code
COPY src ./src
COPY migrations ./migrations

# Compile in release mode
RUN cargo build --release

# ============================================================================
# Stage 2: Runtime - Minimal production image
# ============================================================================
FROM debian:bookworm-slim AS runtime

LABEL maintainer="Synapse Rust Team"
LABEL version="1.0.0"
LABEL description="Synapse Rust Matrix Homeserver"

# ============================================================================
# Install Runtime Dependencies
# ============================================================================
RUN apt-get update && apt-get install -y --no-install-recommends \
    libpq5 \
    libssl3 \
    ca-certificates \
    curl \
    tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# ============================================================================
# Security: Create Non-Root User
# ============================================================================
RUN groupadd --system synapse && \
    useradd --system --gid synapse --home-dir /app --shell /usr/sbin/nologin synapse

# ============================================================================
# Directory Structure
# ============================================================================
RUN mkdir -p /app/bin \
    /app/data \
    /app/data/media \
    /app/data/media/thumbnails \
    /app/data/media/quarantine \
    /app/data/media/tmp \
    /app/logs \
    /app/config \
    /app/migrations && \
    chown -R synapse:synapse /app && \
    chmod -R 755 /app && \
    chmod 777 /app/data/media/tmp

# ============================================================================
# Copy Compiled Binaries from Builder Stage
# ============================================================================
COPY --from=builder --chown=synapse:synapse /app/target/release/synapse-rust /usr/local/bin/synapse-rust
COPY --from=builder --chown=synapse:synapse /app/target/release/generate_test_keypair /usr/local/bin/generate_test_keypair

# Copy Configuration Files
COPY --chown=synapse:synapse docker/config/homeserver.yaml /app/config/homeserver.yaml

# Copy Migrations Directory
COPY --chown=synapse:synapse migrations /app/migrations

# Make binaries executable with restricted permissions
RUN chmod +x /usr/local/bin/synapse-rust /usr/local/bin/generate_test_keypair && \
    chmod 500 /usr/local/bin/synapse-rust && \
    chmod 500 /usr/local/bin/generate_test_keypair

# ============================================================================
# Environment Variables
# ============================================================================
ENV DATABASE_URL=postgres://synapse:synapse@db:5432/synapse_test
ENV REDIS_URL=redis://redis:6379
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1
ENV SERVER_NAME=cjystx.top
ENV SYNAPSE_CONFIG_PATH=/app/config/homeserver.yaml
ENV TZ=Asia/Shanghai
ENV PATH=/usr/local/bin:/usr/bin:/bin

# ============================================================================
# Working Directory
# ============================================================================
WORKDIR /app

# ============================================================================
# Security: Switch to Non-Root User
# ============================================================================
USER synapse

# ============================================================================
# Port Exposure
# - 8008: Client API (HTTP)
# - 8448: Federation API (HTTPS)
# - 9090: Metrics (internal)
# ============================================================================
EXPOSE 8008 8448 9090

# ============================================================================
# Health Checks
# ============================================================================
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD curl -sf http://localhost:8008/_matrix/federation/v1/version || exit 1

# ============================================================================
# Entry Point
# ============================================================================
CMD ["synapse-rust"]
