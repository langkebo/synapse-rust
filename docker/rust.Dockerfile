# Stage 1: Recipe Generator
FROM rust:1.93-slim-bookworm AS chef
RUN cargo install cargo-chef
WORKDIR /app

# Stage 2: Recipe Planner
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Builder
FROM chef AS builder
ARG CARGO_PROFILE=release
COPY --from=planner /app/recipe.json recipe.json

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Build dependencies
RUN cargo chef cook --recipe-path recipe.json --profile ${CARGO_PROFILE}

# Build application
COPY . .
# Ensure SQLX offline mode if needed
ENV SQLX_OFFLINE=true
RUN cargo build --profile ${CARGO_PROFILE}

# Stage 4: Runtime
FROM debian:bookworm-slim AS runtime
ARG CARGO_PROFILE=release

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    libpq5 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy binary
COPY --from=builder /app/target/${CARGO_PROFILE}/synapse-rust /usr/local/bin/synapse-rust

# Copy migrations (config and data usually mounted)
COPY migrations ./migrations

# Health check script
RUN echo '#!/bin/sh\ncurl -f http://localhost:8008/_matrix/client/versions || exit 1' > /usr/local/bin/healthcheck.sh && \
    chmod +x /usr/local/bin/healthcheck.sh

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/healthcheck.sh"]

# Environment variables
ENV RUST_LOG=info
ENV SYNAPSE_CONFIG_PATH=/app/config/homeserver.yaml

EXPOSE 8008

ENTRYPOINT ["synapse-rust"]
