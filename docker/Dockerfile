# ============================================================================
# synapse-rust Dockerfile - Multi-stage Build (Container Compilation)
# 版本: 3.0.0
# 特性: 容器内编译、依赖缓存层、最小化镜像、安全加固
# ============================================================================

# ========================================
# 阶段 1: 构建阶段
# ========================================
FROM rust:1.88-bookworm AS builder

WORKDIR /build

# 设置编译优化环境变量
ENV CARGO_INCREMENTAL=1
ENV CARGO_NET_RETRY=10
ENV CARGO_HTTP_CHECK_REVOKE=false
ENV RUSTFLAGS="-C codegen-units=4 -C target-cpu=native"
ENV CARGO_TERM_COLOR=always

# 安装编译依赖
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# 复制 Cargo 文件 (利用 Docker 缓存层)
COPY Cargo.toml Cargo.lock ./

# 复制源代码和迁移文件
COPY src ./src
COPY migrations ./migrations
COPY benches ./benches

# 构建项目
RUN cargo build --release

# ========================================
# 阶段 2: 运行阶段
# ========================================
FROM debian:bookworm-slim

WORKDIR /app

# 设置时区
ENV TZ=Asia/Shanghai

# 安装运行时依赖 (单层优化)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libpq5 \
    postgresql-client \
    curl \
    tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean \
    && ln -snf /usr/share/zoneinfo/$TZ /etc/localtime \
    && echo $TZ > /etc/timezone

# 创建非 root 用户和目录 (单层优化)
RUN useradd -r -s /bin/false synapse && \
    mkdir -p /app/logs/migrations /app/data /app/config && \
    chown -R synapse:synapse /app

# 从 builder 阶段复制编译好的二进制
COPY --from=builder /build/target/release/synapse-rust /app/synapse-rust

# 复制迁移文件和脚本
COPY migrations/ /app/migrations/
COPY scripts/db_migrate.sh /app/scripts/
COPY scripts/verify_migration.sh /app/scripts/
COPY scripts/docker-entrypoint.sh /app/scripts/

# 设置权限
RUN chmod +x /app/scripts/*.sh /app/synapse-rust && \
    chmod -R 755 /app/migrations && \
    chown -R synapse:synapse /app

# 环境变量
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1
ENV DATABASE_URL=postgres://synapse:synapse@db:5432/synapse_test
ENV RUN_MIGRATIONS=true
ENV VERIFY_SCHEMA=true
ENV DB_WAIT_ATTEMPTS=30
ENV DB_WAIT_INTERVAL=2
ENV MIGRATION_TIMEOUT=300

# 健康检查
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=5 \
    CMD curl -sf http://localhost:8008/health || exit 1

# 暴露端口
EXPOSE 8008 8448

# 切换用户
USER synapse

# 入口点
ENTRYPOINT ["/app/scripts/docker-entrypoint.sh"]

# 默认命令
CMD ["/app/synapse-rust"]
