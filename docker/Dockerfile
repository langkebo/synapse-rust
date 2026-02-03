# Stage 1: Runtime (Using pre-built binary from host)
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from host (must run cargo build on host first)
COPY target/debug/synapse-rust /usr/local/bin/synapse-rust

# Copy configuration and migrations
COPY ../homeserver.yaml ./
COPY ../migrations ./migrations

# Expose the Matrix port
EXPOSE 8008

# Health check mechanism
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8008/_matrix/client/versions || exit 1

# Environment variables
ENV RUST_LOG=debug

# Run the application
ENTRYPOINT ["synapse-rust"]
