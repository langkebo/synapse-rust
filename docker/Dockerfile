# ============================================================================
# Synapse Rust - Production Docker Image
# ============================================================================
# Build Strategy: Pre-compiled binary deployment
#
# This Dockerfile uses pre-compiled binaries from cargo build --release
# Benefits:
# - Faster build times (no Rust compilation in Docker)
# - Consistent builds (binary compiled in controlled environment)
# - Smaller final image (no build tools included)
#
# Build Commands:
#   cd /home/hula/synapse_rust/synapse-rust
#   cargo build --release
#   docker build -f docker/Dockerfile -t synapse-rust:latest .
#
# Run Commands:
#   cd /home/hula/synapse_rust/synapse-rust/docker
#   docker compose up -d
# ============================================================================

# ============================================================================
# Stage: Runtime - Minimal production image
# ============================================================================
FROM debian:bookworm-slim AS runtime

LABEL maintainer="Synapse Rust Team"
LABEL version="2.0.0"
LABEL description="Synapse Rust Matrix Homeserver with Auto Migration"

# ============================================================================
# Install Runtime Dependencies
# ============================================================================
RUN apt-get update && apt-get install -y --no-install-recommends \
    libpq5 \
    libssl3 \
    ca-certificates \
    curl \
    tzdata \
    postgresql-client \
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
    /app/data/keys \
    /app/logs \
    /app/config \
    /app/migrations \
    /app/scripts && \
    chown -R synapse:synapse /app && \
    chmod -R 755 /app && \
    chmod 777 /app/data/media/tmp

# ============================================================================
# Copy Pre-compiled Binaries
# Build context is synapse-rust directory (parent of docker/)
# ============================================================================
COPY --chown=synapse:synapse target/release/synapse-rust /usr/local/bin/synapse-rust
COPY --chown=synapse:synapse target/release/generate_test_keypair /usr/local/bin/generate_test_keypair

# Copy Configuration Files
COPY --chown=synapse:synapse docker/config/homeserver.yaml /app/config/homeserver.yaml

# Copy Migrations Directory
COPY --chown=synapse:synapse migrations /app/migrations

# Copy Entrypoint and Migration Scripts
COPY --chown=synapse:synapse docker/docker-entrypoint.sh /app/scripts/docker-entrypoint.sh
COPY --chown=synapse:synapse docker/run-migrations.sh /app/scripts/run-migrations.sh

# Make binaries and scripts executable with restricted permissions
RUN chmod +x /usr/local/bin/synapse-rust /usr/local/bin/generate_test_keypair && \
    chmod 500 /usr/local/bin/synapse-rust && \
    chmod 500 /usr/local/bin/generate_test_keypair && \
    chmod +x /app/scripts/docker-entrypoint.sh /app/scripts/run-migrations.sh && \
    chmod 550 /app/scripts/docker-entrypoint.sh && \
    chmod 550 /app/scripts/run-migrations.sh

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

ENV RUN_MIGRATIONS=true
ENV MIGRATION_TIMEOUT=300
ENV DB_WAIT_ATTEMPTS=30
ENV DB_WAIT_INTERVAL=2
ENV STOP_ON_MIGRATION_FAILURE=true

# ============================================================================
# Working Directory
# ============================================================================
WORKDIR /app

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
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=5 \
    CMD curl -sf http://localhost:8008/_matrix/federation/v1/version || exit 1

# ============================================================================
# Entry Point
# ============================================================================
ENTRYPOINT ["/app/scripts/docker-entrypoint.sh"]
CMD ["synapse-rust"]
