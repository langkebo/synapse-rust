# ============================================================================
# synapse-rust Dockerfile - Optimized Version
# 版本: 2.0.2
# 优化: 匹配项目Rust版本、slim镜像、并行编译、缓存层
# ============================================================================

# ========================================
# 构建阶段 - 使用与项目一致的Rust版本
# ========================================
FROM rust:1.93.0-slim-bookworm AS builder

WORKDIR /app

# 设置环境变量优化编译
ENV CARGO_INCREMENTAL=1
ENV CARGO_NET_RETRY=10
ENV RUSTFLAGS="-C codegen-units=4"
ENV CARGO_HTTP_CHECK_REVOKE=false

# 安装最小依赖
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# 复制 Cargo 文件 (利用缓存层)
COPY Cargo.toml Cargo.lock ./

# 创建虚拟 src 目录以缓存依赖
RUN mkdir -p src benches && \
    echo "fn main() {}" > src/main.rs && \
    echo "" > benches/database_bench.rs && \
    echo "" > benches/cache_benchmarks.rs && \
    echo "" > benches/concurrency_benchmarks.rs && \
    echo "" > benches/metrics_benchmarks.rs && \
    echo "" > benches/collections_benchmarks.rs && \
    cargo build --release && \
    rm -rf src benches

# 复制实际源代码
COPY src ./src

# 构建最终二进制
RUN cargo build --release

# ========================================
# 运行阶段 - 最小化镜像
# ========================================
FROM debian:bookworm-slim

WORKDIR /app

# 安装运行时依赖 (单层优化)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    postgresql-client \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# 创建非 root 用户和目录 (单层优化)
RUN useradd -r -s /bin/false synapse && \
    mkdir -p /app/logs/migrations /app/data && \
    chown -R synapse:synapse /app

# 复制二进制文件
COPY --from=builder /app/target/release/synapse-rust /app/synapse-rust

# 复制迁移文件和脚本
COPY migrations/ /app/migrations/
COPY scripts/db_migrate.sh /app/scripts/
COPY scripts/verify_migration.sh /app/scripts/
COPY scripts/docker-entrypoint.sh /app/scripts/

# 设置权限
RUN chmod +x /app/scripts/*.sh /app/synapse-rust

# 环境变量
ENV DATABASE_URL=postgres://synapse:synapse@postgres:5432/synapse
ENV RUN_MIGRATIONS=true
ENV VERIFY_MIGRATIONS=true
ENV DB_WAIT_ATTEMPTS=30

# 健康检查
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8008/health || exit 1

# 暴露端口
EXPOSE 8008 8448

# 切换用户
USER synapse

# 入口点
ENTRYPOINT ["/app/scripts/docker-entrypoint.sh"]

# 默认命令
CMD ["/app/synapse-rust"]
