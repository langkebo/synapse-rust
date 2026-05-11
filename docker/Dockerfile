ARG RUST_VERSION=1.93
ARG RUNTIME_BASE_IMAGE=gcr.io/distroless/cc-debian12
ARG CARGO_FEATURE_ARGS="--features server,core-private-chat --no-default-features"

# ============================================================================
# Stage 1: builder — 编译 Rust 二进制 + 收集运行时所需文件到 /out/app
# ============================================================================
FROM rust:${RUST_VERSION}-slim-bookworm AS builder

WORKDIR /workspace

ENV CARGO_INCREMENTAL=0 \
    CARGO_NET_RETRY=10 \
    CARGO_HTTP_CHECK_REVOKE=false \
    CARGO_TERM_COLOR=always \
    CARGO_PROFILE_RELEASE_LTO=thin \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
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

# 依赖缓存机制: 先用空骨架预编译所有 crate, 后续源码改动不会触发依赖重编
# BuildKit cache mounts 大幅加速重复构建 (cargo registry / git / target)
COPY Cargo.toml Cargo.lock ./

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/workspace/target,sharing=locked \
    mkdir src && echo "fn main() {}" > src/main.rs && \
    mkdir -p src/bin && echo "fn main() {}" > src/bin/healthcheck.rs && \
    cargo build --release --locked ${CARGO_FEATURE_ARGS} --bin synapse-rust --bin healthcheck && \
    rm -rf src/

# 复制真正的源码 + 迁移脚本 + 启动脚本
COPY src ./src
COPY migrations ./migrations
COPY benches ./benches
COPY docker/db_migrate.sh \
     docker/deploy/scripts/container-migrate.sh \
     docker/healthcheck.sh \
     docker/entrypoint.sh \
     ./docker/
COPY docker/config/rate_limit.yaml ./docker/config_defaults/rate_limit.yaml

# 强制重新链接最终二进制 (force_relink hack 触发 build script 在缓存中也重新执行)
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/workspace/target,sharing=locked \
    find target/release/.fingerprint -name '*synapse*' -delete 2>/dev/null; \
    find target/release/deps -name '*synapse*' -delete 2>/dev/null; \
    touch src/main.rs && \
    cargo build --release --locked ${CARGO_FEATURE_ARGS} --bin synapse-rust --bin healthcheck && \
    install -Dm755 target/release/synapse-rust          /out/app/synapse-rust && \
    install -Dm755 target/release/healthcheck           /out/app/healthcheck && \
    install -Dm755 docker/db_migrate.sh                 /out/app/scripts/db_migrate.sh && \
    install -Dm755 docker/container-migrate.sh          /out/app/scripts/container-migrate.sh && \
    install -Dm755 docker/healthcheck.sh                /out/app/scripts/healthcheck.sh && \
    install -Dm755 docker/entrypoint.sh                 /out/app/entrypoint.sh && \
    install -Dm644 docker/config_defaults/rate_limit.yaml /out/app/config_defaults/rate_limit.yaml && \
    mkdir -p /out/app/config /out/app/data /out/app/logs /out/app/migrations && \
    cp -R migrations/. /out/app/migrations/

# ============================================================================
# Stage 2: runtime-libs — 从 debian-slim 抽取 distroless 缺失的动态库
# 仅在 distroless `runtime` 目标使用. 默认 `tools` 目标不依赖此阶段.
# ============================================================================
FROM debian:bookworm-slim AS runtime-libs

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    lib_dir="$(dirname "$(find /lib /usr/lib -maxdepth 2 -type f -name 'libssl.so.3' | head -n 1)")"; \
    loader_path="$(find /lib /usr/lib -maxdepth 2 -type f -name 'ld-linux-*.so.*' | head -n 1)"; \
    loader_dir="$(dirname "${loader_path}")"; \
    mkdir -p "/out${lib_dir}" "/out${loader_dir}" /out/etc/ssl/certs; \
    cp -L "${lib_dir}/libssl.so.3"    "/out${lib_dir}/"; \
    cp -L "${lib_dir}/libcrypto.so.3" "/out${lib_dir}/"; \
    cp -L "${lib_dir}/libgcc_s.so.1"  "/out${lib_dir}/"; \
    cp -L "${lib_dir}/libm.so.6"      "/out${lib_dir}/"; \
    cp -L "${lib_dir}/libc.so.6"      "/out${lib_dir}/"; \
    cp -L "${loader_path}"            "/out${loader_dir}/"; \
    cp -L /etc/ssl/certs/ca-certificates.crt /out/etc/ssl/certs/

# ============================================================================
# Stage 3: runtime-distroless — 极简 distroless 镜像 (~30MB)
# 不含 psql, 故 entrypoint 中的迁移会跳过 (set RUN_MIGRATIONS=false).
# 用于追求最小镜像体积、由外部 Job/initContainer 负责迁移的场景.
# 选用方法: docker build --target runtime-distroless ...
# ============================================================================
FROM ${RUNTIME_BASE_IMAGE} AS runtime-distroless

ARG VERSION=dev
ARG BUILD_DATE=unknown
ARG VCS_REF=unknown

LABEL org.opencontainers.image.title="synapse-rust" \
      org.opencontainers.image.description="Matrix homeserver implemented in Rust (distroless variant)" \
      org.opencontainers.image.source="https://github.com/langkebo/synapse-rust" \
      org.opencontainers.image.vendor="synapse-rust" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.licenses="Apache-2.0"

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

# ============================================================================
# Stage 4 (DEFAULT): tools — 生产推荐目标
# debian-slim + libssl3 + ca-certificates + postgresql-client + curl
# 体积约 200MB, 但能在容器内自洽完成: 等待 DB → 执行迁移 → 校验 schema → 启动
# ============================================================================
FROM debian:bookworm-slim AS tools

ARG VERSION=dev
ARG BUILD_DATE=unknown
ARG VCS_REF=unknown

LABEL org.opencontainers.image.title="synapse-rust" \
      org.opencontainers.image.description="Matrix homeserver implemented in Rust (production tools image)" \
      org.opencontainers.image.source="https://github.com/langkebo/synapse-rust" \
      org.opencontainers.image.vendor="synapse-rust" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.licenses="Apache-2.0"

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    postgresql-client \
    curl \
    tini \
    && rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*

RUN groupadd -g 1000 synapse && \
    useradd -u 1000 -g synapse -m -s /usr/sbin/nologin synapse

COPY --chown=1000:1000 --from=builder /out/app /app

WORKDIR /app
USER synapse

ENV RUST_LOG=info \
    RUST_BACKTRACE=1 \
    HOME=/app \
    SYNAPSE_CONFIG_PATH=/app/config/homeserver.yaml \
    SYNAPSE_HEALTHCHECK_HOST=127.0.0.1 \
    SYNAPSE_HEALTHCHECK_PORT=8008 \
    PATH=/app:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

EXPOSE 8008 8448 9090
HEALTHCHECK --interval=15s --timeout=10s --start-period=60s --retries=5 CMD ["/app/healthcheck"]
ENTRYPOINT ["/usr/bin/tini", "--", "/app/entrypoint.sh"]
CMD ["/app/synapse-rust"]
