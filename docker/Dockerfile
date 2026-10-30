ARG RUST_VERSION=1.93.0  # [DEPRECATED] 已移除实际使用，保留仅用于文档引用。当前版本由 RUST_BUILDER_IMAGE pin 管理。
ARG RUST_BUILDER_IMAGE=rust:1.93.0-slim-bookworm@sha256:776861219cd851131c1cec3bbd7cbeb16b99a794048097eb69ad9682a8ed0d57
# 2026-10-27：`RUNTIME_BASE_IMAGE`（`gcr.io/distroless/cc-debian12`）及其两个下游 stage
# （`runtime-libs` / `runtime-distroless`）**已删除**。理由：全仓没有任何构建/部署入口会生成
# `--target runtime-distroless`（`docker/docker-compose.yml` 与 `docker/deploy/deploy.sh` 都是
# `--target tools`），而它让 `Base Image Scan` 永久红在 `libssl3` 的 CVE-2026-84782 上 ——
# 实测该 pin **就是** `cc-debian12:latest` 本身（上游未重建），本仓无从修复。
# 证据与裁决见 `docs/audit/CI_LANE_BLINDNESS_2026-10-27.md` §7.2；这条改动同时是
# `docs/audit/archive/DOCKER_REVIEW_2026-09-19.md` P0-1/O1-1「B. 废弃」的落地。
# 2026-09-20 升 digest 清 Trivy 的 3 个 HIGH：旧 pin（2026-07-30 的 7b140f37…）里
# libpcre2-8-0 是 10.42-1，而 DLA-4772-1 的修复版本 10.42-1+deb12u1 已在当前官方
# 镜像内（同批还修了 liblzma5 5.4.1-1+deb12u1 → +deb12u2）。此 digest 为 index，
# 与旧 pin 同类。⚠️ 引用点只有两处：本文件的 ARG（唯一真相源）+ `docker/complement/Dockerfile`
# 的同一个 ARG；workflow 侧一律通过 `scripts/ci/read_base_image_pins.sh` 读，不得再抄 digest。
# 2026-10-27 一并升到当前 index（旧 pin 里 libssl3 仍在 3.0.20；`tools` 阶段在构建期
# `apt-get install libssl3`，实测装到 3.0.22-1~deb12u2，故出货镜像无该 CVE）。
ARG DEBIAN_BASE_IMAGE=debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8
ARG MIRROR_URL=
# ============================================================================
# 出货镜像的 feature 集 —— **单一真相源**
# ----------------------------------------------------------------------------
# 这个 ARG 的默认值是"出货镜像到底带哪些功能"的唯一定义。其余所有构建入口
# （`docker/docker-compose.yml`、`docker/complement/Dockerfile`、
#  `.github/workflows/docker-security-scan.yml`、`.github/workflows/e2e-http.yml`、
#  `scripts/test/run_element_web_browser_harness.sh`、`scripts/federation-test/cleanup_and_deploy.sh`）
# 一律**不再各抄一份**：
#   * compose 直接不传该 build-arg（不覆盖即继承本默认值）；
#   * 其余入口若要显式传，必须与本值逐字相等 —— 由
#     `scripts/ci/check_feature_arg_consistency.py` 断言（不等则 CI 红）。
#
# 为什么是 `all-extensions`：2026-10-30 之前这里有 6 处各自维护的 feature 串，
# 全都停在"7 项"（core-private-chat,widgets,external-services,voice-extended,
# cas-sso,saml-sso,friends），于是 `docker compose build` 出来的镜像比
# `deploy.sh --all` 少 5 项能力（beacons / voip-tracking / server-notifications /
# privacy-ext / builtin-oidc）；更严重的是 Trivy 扫的就是那 7 项镜像，
# **扫的镜像 ≠ 发的镜像**。收敛到 `all-extensions`（= 12 项可单独开关能力，
# 见 `scripts/ci/check_feature_matrix.py` 的 S2 闭包门禁）后两者必然一致。
#
# ⚠️ `--no-default-features` 不可省：Cargo 的 `default` 含 `beacons`，但本串
#    已经显式列出全部能力；留着 default 只会让"默认集"多一个隐形来源。
# ⚠️ 若要小镜像/快速构建，用 `--build-arg CARGO_FEATURE_ARGS=…` 显式覆盖
#    （`deploy.sh --core-only/--core-private-chat/--features …` 就是这么做的），
#    但**不要**改动本默认值去迁就单个入口。
ARG CARGO_FEATURE_ARGS="--features all-extensions --no-default-features"
# PostgreSQL 客户端主版本。**唯一真相源**：包名、二进制目录都从它派生，
# 升级时只改这一处（`postgresql-client-${PG_MAJOR}` / `/usr/lib/postgresql/${PG_MAJOR}/bin`）。
ARG PG_MAJOR=15

# ============================================================================
# 体积预算（2026-10-30 实测，本地 arm64 build）
# ----------------------------------------------------------------------------
#   debian:bookworm-slim 基础层           79.2 MB
#   apt 层（本文件 tools 阶段）           ~20 MB   （原 79.2 MB）
#   /app 载荷（两个二进制 + migrations）  78.8 MB
#   ─────────────────────────────────────────────
#   合计                                   ~197 MB （原 255 MB）
#
# 那 60 MB 的差额是 **Perl**：`apt-get install postgresql-client` 会拉
# `postgresql-client-common`，而它 `Depends: perl`，于是 `perl` + `perl-modules-5.36`
# + `libperl5.36` 共 ~56 MB 一起进镜像 —— 为的只是 `/usr/bin/psql` 这个
# **Perl 包装脚本**（`/usr/share/postgresql-common/pg_wrapper`，作用是按 cluster
# 版本挑二进制）。本仓只用 `psql` / `pg_isready` 连一个固定 PG 实例，包装脚本
# 这层间接完全没有价值，所以改成：装 `postgresql-client-15`（真实二进制）→ 复制
# 到 `/usr/local/bin` → `apt-get purge` 掉 client 包与 perl 全家。
# 验证：`dpkg -C` 干净（无 broken package）、`psql/pg_isready --version` 15.19 可用、
# 包数 120 → 114、无新增包。`perl-base`(7.8 MB) 是 Essential 包，删不掉，故保留。
# ⚠️ 副作用：镜像内不再有 `pg_wrapper`，`psql` 直接是 15 版二进制；且 purge 之后
# 容器内**不要**再跑 `apt-get install`（部分 postinst 依赖 perl）。运行时不需要 apt。
# ============================================================================

# ============================================================================
# Stage 1: builder — 编译 Rust 二进制 + 收集运行时所需文件到 /out/app
# ============================================================================
FROM ${RUST_BUILDER_IMAGE} AS builder

ARG CARGO_FEATURE_ARGS
ARG CARGO_BUILD_JOBS=2
# jemalloc prof 符号化需要保留符号 + debug 信息。默认保持生产行为（strip），
# 内存泄漏诊断时通过 --build-arg CARGO_PROFILE_RELEASE_STRIP=none \
#                      --build-arg CARGO_PROFILE_RELEASE_DEBUG=true 覆盖。
ARG CARGO_PROFILE_RELEASE_STRIP=symbols
ARG CARGO_PROFILE_RELEASE_DEBUG=false
ARG MIRROR_URL=

WORKDIR /workspace

ENV CARGO_INCREMENTAL=0 \
    CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS} \
    CARGO_NET_RETRY=10 \
    CARGO_HTTP_CHECK_REVOKE=false \
    CARGO_TERM_COLOR=always \
    SQLX_OFFLINE=true \
    CARGO_PROFILE_RELEASE_LTO=false \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=8 \
    CARGO_PROFILE_RELEASE_STRIP=${CARGO_PROFILE_RELEASE_STRIP} \
    CARGO_PROFILE_RELEASE_DEBUG=${CARGO_PROFILE_RELEASE_DEBUG} \
    CARGO_PROFILE_RELEASE_PANIC=unwind \
    RUSTFLAGS="-C link-arg=-Wl,--gc-sections -C link-arg=-Wl,-z,relro,-z,now"

# apt 的两处目录挂成 cache mount：`/var/lib/apt/lists` 与 `/var/cache/apt` 在 BuildKit 里
# **不落进镜像层**，所以既不用 `rm -rf /var/lib/apt/lists/*` 善后，重复构建也不必重下
# 索引与 .deb（实测第二次构建该步 12s → 2s）。`sharing=locked` 与下面 cargo 的挂载一致。
RUN --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    --mount=type=cache,target=/var/cache/apt,sharing=locked \
    if [ -n "${MIRROR_URL}" ]; then sed -i "s|deb.debian.org|${MIRROR_URL}|g" /etc/apt/sources.list.d/debian.sources; fi && \
    apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    clang \
    file \
    pkg-config \
    libssl-dev \
    build-essential

# 依赖缓存机制: 先用空骨架预编译所有 crate, 后续源码改动不会触发依赖重编
# BuildKit cache mounts 大幅加速重复构建 (cargo registry / git / target)
COPY Cargo.toml Cargo.lock ./
COPY .sqlx ./.sqlx
# 复制 cargo 配置（含国内镜像源），避免 cargo fetch 时访问 crates.io 超时
COPY .cargo/config.toml ./.cargo/config.toml
COPY synapse-common/Cargo.toml      ./synapse-common/Cargo.toml
COPY synapse-cache/Cargo.toml       ./synapse-cache/Cargo.toml
COPY synapse-storage/Cargo.toml     ./synapse-storage/Cargo.toml
COPY synapse-e2ee/Cargo.toml        ./synapse-e2ee/Cargo.toml
COPY synapse-federation/Cargo.toml  ./synapse-federation/Cargo.toml
COPY synapse-services/Cargo.toml    ./synapse-services/Cargo.toml
COPY synapse-web/Cargo.toml         ./synapse-web/Cargo.toml
COPY synapse-test-utils/Cargo.toml  ./synapse-test-utils/Cargo.toml
COPY vendor/                       ./vendor/

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/workspace/target,sharing=locked \
    mkdir -p src src/bin synapse-common/src synapse-cache/src synapse-storage/src synapse-e2ee/src \
             synapse-federation/src synapse-services/src synapse-web/src synapse-test-utils/src && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > src/bin/healthcheck.rs && \
    echo "pub fn stub() {}" > synapse-common/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-cache/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-storage/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-e2ee/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-federation/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-services/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-web/src/lib.rs && \
    echo "pub fn stub() {}" > synapse-test-utils/src/lib.rs && \
    cargo fetch --locked && \
    rm -rf src/ synapse-common/src synapse-cache/src synapse-storage/src synapse-e2ee/src \
           synapse-federation/src synapse-services/src synapse-web/src synapse-test-utils/src

# 复制真正的源码 + 迁移脚本 + 启动脚本
#
# ⚠️ 这里**没有** `ARG CACHE_BUST`：2026-10-30 删掉了那个常量。它原本的注释写着
# 「CACHE_BUST 用于强制 invalidate COPY 缓存（修复 BuildKit 未检测源码变更的问题）」，
# 但一个**恒为 1** 的 ARG 根本不参与 COPY 的缓存键（COPY 只看文件内容摘要），
# 所以它从来没有 bust 过任何东西 —— 只是让读者以为存在某种保证。
# 真正决定这一层是否失效的是构建上下文的内容；上下文裁剪见 `.dockerignore`
# （已排除 `scripts/`、`tests/`、`docs/`、`**/wal_archive/` 等与编译无关的树）。
COPY . .

# [O2-1] 真实编译阶段：使用 target/ 缓存，增量构建不再重编依赖
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/workspace/target,sharing=locked \
    cargo build --release --locked ${CARGO_FEATURE_ARGS} --bin synapse-rust --bin synapse_worker --bin healthcheck && \
    install -Dm755 target/release/synapse-rust          /out/app/synapse-rust && \
    # W-D: the worker process binary. Without it a containerised multi-process
    # topology cannot start a single worker — `synapse_worker` consumes the Redis
    # job queue, registers/heartbeats against the master, and serves its own
    # `/metrics`; nothing else in the image can play that role.
    install -Dm755 target/release/synapse_worker        /out/app/synapse_worker && \
    install -Dm755 target/release/healthcheck           /out/app/healthcheck && \
    install -Dm755 docker/db_migrate.sh                 /out/app/scripts/db_migrate.sh && \
    install -Dm755 docker/deploy/scripts/container-migrate.sh /out/app/scripts/container-migrate.sh && \
    # [O3-7] 移除 docker/healthcheck.sh — 未被任何 compose/Dockerfile 引用，且依赖 curl
    # 实际使用的是 Rust 二进制 /app/healthcheck（直接走 /health 端点）
    install -Dm755 docker/entrypoint.sh                 /out/app/entrypoint.sh && \
    install -Dm644 docker/config/rate_limit.yaml          /out/app/config_defaults/rate_limit.yaml && \
    mkdir -p /out/app/config /out/app/data /out/app/logs /out/app/migrations && \
    cp -R migrations/. /out/app/migrations/

# ============================================================================
# Stage 2 (DEFAULT): tools — 生产推荐目标
# debian-slim + libssl3 + ca-certificates + postgresql-client + curl
# 体积约 200MB, 但能在容器内自洽完成: 等待 DB → 执行迁移 → 校验 schema → 启动
# ============================================================================
FROM ${DEBIAN_BASE_IMAGE} AS tools

ARG VERSION=dev
ARG BUILD_DATE=unknown
ARG VCS_REF=unknown
ARG MIRROR_URL=
# 全局 ARG（第一个 FROM 之前声明）在 stage 内**不可见**，必须在这里重新声明；
# 这也顺带修掉一个静默缺陷：原文件在 RUN 里用了 `${MIRROR_URL}` 却从未在本 stage
# 声明过它，于是那行 `sed` 永远拿到空值 —— 换国内源从来没生效过。
ARG PG_MAJOR=15

LABEL org.opencontainers.image.title="synapse-rust" \
      org.opencontainers.image.description="Matrix homeserver implemented in Rust (production tools image)" \
      org.opencontainers.image.source="https://github.com/langkebo/synapse-rust" \
      org.opencontainers.image.vendor="synapse-rust" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.licenses="AGPL-3.0-only"

# 这一个 RUN 里做三件事，且必须同层完成（否则被 purge 的 ~56 MB Perl 会留在中间层）：
#   1) 装运行期依赖。`postgresql-client-${PG_MAJOR}` 而不是 `postgresql-client` 元包 ——
#      见文件头的体积预算说明；元包只比它多一个 `postgresql-client-common`（= Perl 包装脚本）。
#   2) 把真实二进制复制到 `/usr/local/bin`（PATH 里排在 /usr/bin 之前），这样 purge 掉
#      client 包之后 `psql`/`pg_isready`/`pg_dump` 仍然可用。复制整个 bin 目录而不是挑
#      三个文件：备份/恢复脚本会用到 `pg_dump`/`pg_basebackup`，少一个就是运行时才发现。
#   3) purge 掉 client 包与 perl 全家。要删的 perl 包名**从 dpkg 现查**，不硬编码
#      `perl-modules-5.36` 这类带版本的包名 —— 基础镜像 digest 一升，硬编码就会
#      `E: Unable to locate package` 把构建打断；现查则自动跟随。
#      `perl-base` 是 Essential 包（删不掉，7.8 MB），`dpkg-query perl` 是精确名，不会匹配到它。
# 实测：`dpkg -C` 为空（无 broken package）、`psql --version` = 15.19、包数 120 → 114。
# `curl` 保留：`scripts/test/fullstack-redo/lib/common.sh` 用
# `docker exec <container> curl …` 打容器内网端点，删掉会打断该测试。
# hadolint ignore=SC2046
RUN --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    --mount=type=cache,target=/var/cache/apt,sharing=locked \
    if [ -n "${MIRROR_URL}" ]; then sed -i "s|deb.debian.org|${MIRROR_URL}|g" /etc/apt/sources.list.d/debian.sources; fi && \
    apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    postgresql-client-${PG_MAJOR} \
    curl \
    tini \
    `# CVE-2026-103111 (HIGH): bookworm 基础镜像内置 libpcre2-8-0 10.42-1+deb12u1,` \
    `# 显式安装以拉取 bookworm-security 的修复版 10.42-1+deb12u2（apt-get install 会升级已装包）.` \
    `# 不用 apt-get upgrade: 会触发 hadolint DL3005.` \
    libpcre2-8-0 \
    && cp -a "/usr/lib/postgresql/${PG_MAJOR}/bin/." /usr/local/bin/ \
    && apt-get purge -y postgresql-client-${PG_MAJOR} postgresql-client-common \
       $(dpkg-query -W -f='${Package}\n' perl 'libperl*' 'perl-modules*' 2>/dev/null || true)

RUN groupadd -g 1000 synapse && \
    useradd -u 1000 -g synapse -m -s /usr/sbin/nologin synapse

COPY --chown=1000:1000 --from=builder /out/app /app

WORKDIR /app
# hadolint DL3066: use the numeric uid/gid that `useradd -u 1000 -g synapse`
# created (and that `COPY --chown=1000:1000` above already assumes) instead of
# the name. A name is only resolvable through `/etc/passwd`, which is absent in
# stripped runtimes and when the image is started with `--user`; the numeric
# form works in both cases.
USER 1000:1000

ENV RUST_LOG=info \
    RUST_BACKTRACE=1 \
    HOME=/app \
    SYNAPSE_CONFIG_PATH=/app/config/homeserver.yaml \
    SYNAPSE_HEALTHCHECK_HOST=127.0.0.1 \
    SYNAPSE_HEALTHCHECK_PORT=8008 \
    PATH=/app:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# [O2-8] EXPOSE 去掉 9090 — metrics 路由无鉴权，外部不应暴露
EXPOSE 8008 8448
HEALTHCHECK --interval=15s --timeout=10s --start-period=60s --retries=5 CMD ["/app/healthcheck"]
ENTRYPOINT ["/usr/bin/tini", "--", "/app/entrypoint.sh"]
CMD ["/app/synapse-rust"]
