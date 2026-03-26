# ============================================================================
# synapse-rust Dockerfile - Multi-stage Build
# 版本: 3.1.0
# 特性: 容器内编译、多架构支持、安全加固、资源限制
# ============================================================================

# ========================================
# 阶段 1: 构建阶段
# ========================================
ARG RUST_VERSION=1.93
FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /build

# 设置编译优化环境变量
ENV CARGO_INCREMENTAL=1 \
    CARGO_NET_RETRY=10 \
    CARGO_HTTP_CHECK_REVOKE=false \
    CARGO_TERM_COLOR=always \
    RUSTFLAGS="-C codegen-units=4 -C target-cpu=native"

# 安装编译依赖
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libpq-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# 复制 Cargo 文件 (利用 Docker 缓存层)
COPY Cargo.toml Cargo.lock ./

# 预编译依赖
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# 复制完整源代码
COPY src ./src
COPY migrations ./migrations
COPY benches ./benches
# tests 目录暂时注释

# 构建项目
RUN cargo build --release 2>&1 || cargo build --release

# Strip 二进制以减小镜像大小
RUN strip --strip-unneeded /build/target/release/synapse-rust || true

# ========================================
# 阶段 2: 运行阶段 (最小化)
# ========================================
FROM debian:bookworm-slim AS runtime

# 标签
LABEL maintainer="synapse-rust" \
      version="1.0.0" \
      description="Synapse Rust Matrix Homeserver"

# 设置时区
ENV TZ=Asia/Shanghai \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

# 安装运行时依赖
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libpq5 \
    postgresql-client \
    redis-tools \
    curl \
    tzdata \
    netcat-traditional \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean \
    && ln -snf /usr/share/zoneinfo/$TZ /etc/localtime \
    && echo $TZ > /etc/timezone

# 创建非 root 用户
RUN groupadd --gid 991 synapse && \
    useradd --uid=991 --gid=synapse --shell=/bin/false --create-home synapse && \
    mkdir -p /app/logs/migrations /app/data /app/config /app/scripts /app/migrations && \
    chown -R synapse:synapse /app

# 复制编译好的二进制
COPY --from=builder /build/target/release/synapse-rust /app/synapse-rust

# 复制迁移文件和脚本
COPY --chown=synapse:synapse migrations/ /app/migrations/
COPY --chown=synapse:synapse docker/docker-entrypoint.sh /app/scripts/
COPY --chown=synapse:synapse docker/db_migrate.sh /app/scripts/
COPY --chown=synapse:synapse docker/healthcheck.sh /app/scripts/

# 设置权限
RUN chmod +x /app/scripts/*.sh /app/synapse-rust && \
    chmod -R 755 /app/migrations && \
    chmod 750 /app/data

# 环境变量默认值
ENV RUST_LOG=info \
    RUST_BACKTRACE=1 \
    DATABASE_URL=postgres://synapse:synapse@db:5432/synapse \
    REDIS_URL=redis://redis:6379 \
    RUN_MIGRATIONS=true \
    VERIFY_SCHEMA=true \
    DB_WAIT_ATTEMPTS=30 \
    DB_WAIT_INTERVAL=2 \
    MIGRATION_TIMEOUT=300 \
    SYNAPSE_CONFIG_PATH=/app/config/homeserver.yaml \
    WORKER_MODE=single

# 健康检查
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=5 \
    CMD curl -sf http://localhost:8008/_matrix/federation/v1/version || exit 1

# 暴露端口
EXPOSE 8008 8448 9090

# 安全设置
USER synapse
WORKDIR /app

# 入口点
ENTRYPOINT ["/app/scripts/docker-entrypoint.sh"]
CMD ["/app/synapse-rust"]
