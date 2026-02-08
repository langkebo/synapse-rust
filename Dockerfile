# Stage 1: Builder
FROM rust:1.81-bookworm as builder

WORKDIR /usr/src/app

# Install build dependencies (if any specific C libraries are needed)
# RUN apt-get update && apt-get install -y ...

# Copy manifest and source
COPY . .

# Build for release
# We use --locked to ensure reproducible builds
RUN cargo build --release --locked

# Stage 2: Runtime
# Use distroless for minimal attack surface
FROM gcr.io/distroless/cc-debian12

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /usr/src/app/target/release/synapse-rust /app/synapse-rust

# Copy configuration templates or default config if needed
# COPY config/homeserver.yaml /app/config/homeserver.yaml

# Create necessary directories (if distroless allows, otherwise we might need a wrapper)
# Distroless is very minimal. We might need volume mounts for data.
# /app/data should be a volume.

# Expose the port
EXPOSE 8008

# Set environment variables
ENV RUST_LOG=info
ENV SYNAPSE_CONFIG_PATH=/app/config/homeserver.yaml

# Run the binary
CMD ["/app/synapse-rust"]
