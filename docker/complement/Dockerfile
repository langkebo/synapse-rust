# Complement-compatible Dockerfile for synapse-rust
#
# This Dockerfile produces an image that follows the Complement base image
# contract (https://github.com/matrix-org/complement/blob/main/docs/homeserver-design-overview.md):
#   - /start.sh script that configures and starts the homeserver
#   - Listens on 8008 (client API) and 8448 (federation)
#   - Trusts the Complement CA certificate mounted at /complement/ca
#
# Build this image via:
#   docker build -t complement-synapse-rust -f docker/complement/Dockerfile .
#
# Run Complement tests via:
#   COMPLEMENT_BASE_IMAGE=complement-synapse-rust \
#     go test -v ./tests/complement/...

ARG RUST_VERSION=1.93.0
ARG RUST_BUILDER_IMAGE=rust:1.93.0-slim-bookworm@sha256:776861219cd851131c1cec3bbd7cbeb16b99a794048097eb69ad9682a8ed0d57
ARG DEBIAN_BASE_IMAGE=debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818

# ============================================================================
# Stage 1: builder — reuse the main project builder
# ============================================================================
FROM ${RUST_BUILDER_IMAGE} AS builder

ARG CARGO_FEATURE_ARGS="--features server,core-private-chat,widgets,external-services,voice-extended,cas-sso,saml-sso --no-default-features"
ARG CARGO_BUILD_JOBS=2

WORKDIR /workspace

ENV CARGO_INCREMENTAL=0 \
    CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS} \
    CARGO_NET_RETRY=10 \
    CARGO_HTTP_CHECK_REVOKE=false \
    CARGO_TERM_COLOR=always \
    SQLX_OFFLINE=true \
    CARGO_PROFILE_RELEASE_LTO=false \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=8 \
    CARGO_PROFILE_RELEASE_STRIP=symbols \
    CARGO_PROFILE_RELEASE_DEBUG=false \
    CARGO_PROFILE_RELEASE_PANIC=abort \
    RUSTFLAGS="-C link-arg=-Wl,--gc-sections -C link-arg=-Wl,-z,relro,-z,now"

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    clang \
    file \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY .sqlx ./.sqlx
COPY .cargo/config.toml ./.cargo/config.toml
COPY synapse-common/Cargo.toml      ./synapse-common/Cargo.toml
COPY synapse-cache/Cargo.toml       ./synapse-cache/Cargo.toml
COPY synapse-storage/Cargo.toml     ./synapse-storage/Cargo.toml
COPY synapse-e2ee/Cargo.toml        ./synapse-e2ee/Cargo.toml
COPY synapse-federation/Cargo.toml  ./synapse-federation/Cargo.toml
COPY synapse-services/Cargo.toml    ./synapse-services/Cargo.toml

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    mkdir -p src src/bin synapse-common/src synapse-cache/src synapse-storage/src synapse-e2ee/src \
             synapse-federation/src synapse-services/src && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > src/bin/healthcheck.rs && \
    echo "pub fn stub() {}" > synapse-common/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-cache/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-storage/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-e2ee/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-federation/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-services/src/lib.rs && \
    cargo fetch --locked && \
    rm -rf src/ synapse-common/src synapse-cache/src synapse-storage/src synapse-e2ee/src \
           synapse-federation/src synapse-services/src

COPY . .
RUN rm -f rust-toolchain rust-toolchain.toml

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo build --release --locked ${CARGO_FEATURE_ARGS} --bin synapse-rust --bin healthcheck && \
    install -Dm755 target/release/synapse-rust          /out/app/synapse-rust && \
    install -Dm755 target/release/healthcheck           /out/app/healthcheck && \
    install -Dm755 docker/db_migrate.sh                 /out/app/scripts/db_migrate.sh && \
    install -Dm755 docker/healthcheck.sh                /out/app/scripts/healthcheck.sh && \
    mkdir -p /out/app/config /out/app/data /out/app/logs /out/app/migrations && \
    cp -R migrations/. /out/app/migrations/

# ============================================================================
# Stage 2: Complement runtime image
# ============================================================================
FROM ${DEBIAN_BASE_IMAGE}

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    postgresql-client \
    curl \
    tini \
    bash \
    jq \
    && rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*

COPY --from=builder /out/app /app

# Complement start script — must be at /start.sh per the Complement contract
COPY docker/complement/start.sh /start.sh
RUN chmod +x /start.sh

WORKDIR /app

ENV RUST_LOG=info \
    RUST_BACKTRACE=1 \
    HOME=/app \
    SYNAPSE_CONFIG_PATH=/app/config/homeserver.yaml \
    SYNAPSE_HEALTHCHECK_HOST=127.0.0.1 \
    SYNAPSE_HEALTHCHECK_PORT=8008 \
    PATH=/app:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

EXPOSE 8008 8448

ENTRYPOINT ["/usr/bin/tini", "--", "/start.sh"]
