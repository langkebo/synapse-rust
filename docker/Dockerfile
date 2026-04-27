ARG RUST_VERSION=1.93
ARG RUNTIME_BASE_IMAGE=gcr.io/distroless/cc-debian12
# Stage 1: Builder
FROM rust:${RUST_VERSION}-slim-bookworm AS builder

WORKDIR /workspace

# 环境变量优化
ENV CARGO_INCREMENTAL=0 \
    CARGO_NET_RETRY=10 \
    CARGO_HTTP_CHECK_REVOKE=false \
    CARGO_TERM_COLOR=always \
    CARGO_PROFILE_RELEASE_LTO=false \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    CARGO_PROFILE_RELEASE_STRIP=symbols \
    RUSTFLAGS="-C link-arg=-Wl,--gc-sections -C link-arg=-Wl,-z,relro,-z,now"

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    clang \
    file \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# 依赖缓存机制：先复制依赖配置文件
COPY Cargo.toml Cargo.lock ./

# 创建骨架项目以预先编译依赖
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    mkdir -p src/bin && echo "fn main() {}" > src/bin/healthcheck.rs && \
    cargo build --release --locked --bin synapse-rust --bin healthcheck && \
    rm -rf src/

# 复制真正的源码
COPY src ./src
COPY migrations ./migrations
COPY benches ./benches
COPY docker/db_migrate.sh docker/deploy/scripts/container-migrate.sh docker/healthcheck.sh docker/entrypoint.sh ./docker/

# 编译真正的二进制文件
# 注意：删除 target 目录确保完全重新编译所有源码
RUN rm -rf target && \
    cargo build --release --locked --bin synapse-rust --bin healthcheck && \
    install -Dm755 target/release/synapse-rust /out/app/synapse-rust && \
    install -Dm755 target/release/healthcheck /out/app/healthcheck && \
    install -Dm755 docker/db_migrate.sh /out/app/scripts/db_migrate.sh && \
    install -Dm755 docker/container-migrate.sh /out/app/scripts/container-migrate.sh && \
    install -Dm755 docker/healthcheck.sh /out/app/scripts/healthcheck.sh && \
    install -Dm755 docker/entrypoint.sh /out/app/entrypoint.sh && \
    mkdir -p /out/app/config /out/app/data /out/app/logs /out/app/migrations && \
    cp -R migrations/. /out/app/migrations/

FROM debian:bookworm-slim AS tools-base

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -g 1000 synapse && \
    useradd -u 1000 -g synapse -m -s /usr/sbin/nologin synapse

COPY --chown=1000:1000 --from=builder /out/app /app

FROM tools-base AS tools

USER root

RUN apt-get update && apt-get install -y --no-install-recommends \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
USER synapse

ENV RUST_LOG=info \
    RUST_BACKTRACE=1 \
    HOME=/app \
    SYNAPSE_CONFIG_PATH=/app/config/homeserver.yaml \
    SYNAPSE_HEALTHCHECK_HOST=127.0.0.1 \
    SYNAPSE_HEALTHCHECK_PORT=8008

EXPOSE 8008 8448 9090
HEALTHCHECK --interval=15s --timeout=10s --start-period=60s --retries=5 CMD ["/app/healthcheck"]
ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/synapse-rust"]

FROM tools-base AS runtime-libs

RUN set -eux; \
    lib_dir="$(dirname "$(find /lib /usr/lib -maxdepth 2 -type f -name 'libssl.so.3' | head -n 1)")"; \
    loader_path="$(find /lib /usr/lib -maxdepth 2 -type f -name 'ld-linux-*.so.*' | head -n 1)"; \
    loader_dir="$(dirname "${loader_path}")"; \
    mkdir -p "/out${lib_dir}" "/out${loader_dir}" /out/etc/ssl/certs; \
    cp -L "${lib_dir}/libssl.so.3" "/out${lib_dir}/"; \
    cp -L "${lib_dir}/libcrypto.so.3" "/out${lib_dir}/"; \
    cp -L "${lib_dir}/libgcc_s.so.1" "/out${lib_dir}/"; \
    cp -L "${lib_dir}/libm.so.6" "/out${lib_dir}/"; \
    cp -L "${lib_dir}/libc.so.6" "/out${lib_dir}/"; \
    cp -L "${loader_path}" "/out${loader_dir}/"; \
    cp -L /etc/ssl/certs/ca-certificates.crt /out/etc/ssl/certs/

FROM ${RUNTIME_BASE_IMAGE} AS runtime

LABEL org.opencontainers.image.title="synapse-rust" \
    org.opencontainers.image.description="Matrix homeserver implemented in Rust" \
    org.opencontainers.image.source="https://github.com/langkebo/synapse-rust" \
    org.opencontainers.image.vendor="synapse-rust"

ENV RUST_LOG=info \
    RUST_BACKTRACE=1 \
    HOME=/app \
    SYNAPSE_CONFIG_PATH=/app/config/homeserver.yaml \
    SYNAPSE_HEALTHCHECK_HOST=127.0.0.1 \
    SYNAPSE_HEALTHCHECK_PORT=8008

COPY --from=runtime-libs /out/ /
COPY --chown=1000:1000 --from=builder /out/app /app

WORKDIR /app
USER 1000:1000
EXPOSE 8008 8448 9090
HEALTHCHECK --interval=15s --timeout=10s --start-period=60s --retries=5 CMD ["/app/healthcheck"]
ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/synapse-rust"]
