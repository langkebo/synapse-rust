# ============================================================================
# Synapse Rust - Production Optimized Docker Image
# ============================================================================
# Deployment Strategy: Pre-compiled Binary Deployment
# 
# This Dockerfile is designed for production deployment using pre-compiled
# binaries from the local build. This approach provides:
# - Faster build times (no Rust compilation in Docker)
# - Consistent builds (binary compiled in controlled environment)
# - Smaller final image (no build tools included)
# 
# Build Flow:
# 1. Compile Rust application on host: cargo build --release
# 2. Copy binaries to docker/bin/
# 3. Build Docker image: docker build -t synapse-rust .
# 4. Run: docker compose up -d
# ============================================================================

# Stage: Runtime - Minimal production image
# ============================================================================
FROM debian:bookworm-slim AS runtime

# ============================================================================
# Install Runtime Dependencies
# ============================================================================
RUN apt-get update && apt-get install -y --no-install-recommends \
    libpq5 \
    libssl3 \
    ca-certificates \
    curl \
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
RUN mkdir -p /app/bin /app/data /app/logs /app/config && \
    chown -R synapse:synapse /app && \
    chmod 755 /app

# ============================================================================
# Copy Pre-compiled Binaries
# ============================================================================
# Copy the pre-compiled binaries from the build context
# Build context is parent directory, so paths are relative to parent
COPY --chown=synapse:synapse target/release/synapse-rust /usr/local/bin/synapse-rust
COPY --chown=synapse:synapse target/release/generate_test_keypair /usr/local/bin/generate_test_keypair

# Copy Configuration Files
# ============================================================================
COPY --chown=synapse:synapse docker/config/homeserver.yaml /app/config/homeserver.yaml

# Copy migrations
# ============================================================================
COPY --chown=synapse:synapse migrations /app/migrations

# Make binaries executable
RUN chmod +x /usr/local/bin/synapse-rust /usr/local/bin/generate_test_keypair

# ============================================================================
# Environment Variables
# ============================================================================
ENV DATABASE_URL=postgres://synapse:synapse@db:5432/synapse_test
ENV REDIS_URL=redis://redis:6379
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1
ENV SERVER_NAME=cjystx.top

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
# ============================================================================
EXPOSE 8008 8448

# ============================================================================
# Health Checks
# ============================================================================
# HTTP Health Check for Federation API
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:8008/_matrix/federation/v1/version || exit 1

# ============================================================================
# Entrypoint
# ============================================================================
ENTRYPOINT ["sh", "-c"]

# Default command with configuration path
CMD ["exec synapse-rust --config /app/config/homeserver.yaml"]

# ============================================================================
# OCI Labels for Container Metadata
# ============================================================================
LABEL org.opencontainers.image.title="Synapse Rust" \
      org.opencontainers.image.description="Matrix Homeserver implemented in Rust" \
      org.opencontainers.image.version="0.1.0" \
      org.opencontainers.image.source="https://github.com/synapse-rust/synapse-rust" \
      org.opencontainers.image.vendor="Synapse Rust Team" \
      org.opencontainers.image.licenses="MIT" \
      maintainer="Synapse Rust Team <dev@cjystx.top>" \
      com.matrix.server="cjystx.top" \
      com.matrix.domain="matrix.cjystx.top"

# ============================================================================
# Build Metadata (if set during build)
# ============================================================================
ARG BUILD_DATE
ARG VCS_REF
ARG VERSION

# ============================================================================
# Signal Handling
# ============================================================================
STOPSIGNAL SIGTERM

# ============================================================================
# Build Instructions (Commented - For Reference)
# ============================================================================
#
# 1. Compile Rust application on host:
#    cd /path/to/synapse_rust
#    cargo build --release
#
# 2. Copy binaries to docker directory:
#    mkdir -p docker/bin
#    cp target/release/synapse-rust docker/bin/
#    cp target/release/generate_test_keypair docker/bin/
#
# 3. Build Docker image:
#    cd docker
#    docker build -t synapse-rust:latest .
#
# 4. Run with docker compose:
#    docker compose up -d
#
# 5. Verify deployment:
#    curl http://localhost:8008/_matrix/federation/v1/version
#
# ============================================================================
