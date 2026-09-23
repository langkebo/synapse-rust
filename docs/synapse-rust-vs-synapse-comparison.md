# Synapse-Rust 与 Synapse (Python) 地址：https://github.com/element-hq/synapse 对比分析报告

> **文档版本**: v1.3
> **更新日期**: 2026-09-22
> **更新说明**:
> - **v1.3 复核（2026-09-22）**：逐条实测 v1.2 的 ✅ 声明与全部计数，修正 10+ 处计数/版本错误、
>   1 处**伪造引用**（`docs/synapse-rust/api-reference.md` 不存在）、特性片段中已删除的 `server` 特性、
>   "编译时 SQL 验证""静态链接 + musl"等高估；§11 增补实测判定（E2EE SAS/QR/泄漏检测、MSC4140、
>   MSC3912 归因、v1.161 八项）；§12.5 重写为指向权威清单的方案。
>   **详细证据与命令见** `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md`。
> - v1.2（2026-09-22）：对齐基准从 Synapse v1.149.1 更新为 v1.161.0（2026-09-15 发布）；
>   修正 vodozemac 版本与 crate 计数；补充 MSC4140/3912/4242 状态与 v1.161 Bug 修复核查项。
> **生成日期**: 2026-09-16
> **对比对象**: synapse-rust (Rust 重写实现) vs element-hq/synapse (Python 原始实现)
> **对齐基准**: Synapse v1.161.0（2026-09-15 发布，`release-v1.161` CHANGES.md）

---

## 目录

1. [项目概览](#1-项目概览)
2. [架构对比](#2-架构对比)
3. [技术实现对比](#3-技术实现对比)
4. [性能指标对比](#4-性能指标对比)
5. [可扩展性对比](#5-可扩展性对比)
6. [可维护性对比](#6-可维护性对比)
7. [安全特性对比](#7-安全特性对比)
8. [用户体验对比](#8-用户体验对比)
9. [开发效率对比](#9-开发效率对比)
10. [资源利用率对比](#10-资源利用率对比)
11. [业务对齐度对比](#11-业务对齐度对比)
12. [总结结论](#12-总结结论)
13. [v1.3 复核修正记录](#13-v13-复核修正记录2026-09-22)

---

## 1. 项目概览

### 1.1 基础信息

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **项目来源** | element-hq/synapse (官方参考实现) | 独立重写项目 |
| **主要语言** | Python 3.10+ | Rust (Edition 2021, MSRV 1.93) |
| **当前版本** | v1.161.0 | v6.2.0 |
| **许可证** | AGPL-3.0 | AGPL-3.0-only |
| **代码规模** | 未独立测量（本报告未复核上游行数） | **442,451 行 Rust（1,019 个 .rs 文件）**（实测：`git ls-files '*.rs' \| xargs wc -l` 与 `git ls-files '*.rs' \| wc -l`，2026-09-23） |
| **数据库** | PostgreSQL / SQLite | PostgreSQL (sqlx 0.8，Cargo.lock 实锁 0.8.6) |
| **缓存** | Redis (tx-redis) | Redis (deadpool-redis 0.20，Cargo.lock 实锁 0.20.0) |
| **异步运行时** | Twisted reactor | Tokio（Cargo.toml 要求 1.49，Cargo.lock 实锁 **1.53.1**） |

### 1.2 项目定位

- **Synapse (Python)**: Matrix 协议的官方参考实现，经过多年生产验证，功能最完整，但性能受限于 Python 语言特性。
- **synapse-rust**: 以 Synapse **v1.161.0**（2026-09-15）为对齐基准的 Rust 重写实现，目标是在保持协议兼容性的同时，利用 Rust 语言优势提升性能和资源效率。

---

## 2. 架构对比

### 2.1 整体架构

| 架构维度 | Synapse (Python) | synapse-rust |
|----------|-------------------|--------------|
| **进程模型** | 多进程 Worker 模式（generic_worker, federation_sender, synchrotron 等） | 单进程多模块 + Worker TCP 拓扑（可配置） |
| **Web 框架** | Twisted Web (treq/twisted.web) | Axum 0.8 + Tower 中间件栈 |
| **模块化方式** | Python 包层级（synapse.rest, synapse.storage, synapse.federation 等） | Cargo Workspace：`members` 8 个 crate + 根 crate（共 9 个编译单元） |
| **中间件** | Twisted inlineCallbacks 装饰器 | Tower 中间件（CORS, Auth, RateLimit, CSRF, FederationAuth, Security） |
| **配置管理** | YAML + Python argparse | config 0.14 (YAML) + 结构化配置 |
| **可观测性** | Prometheus client + structured logging | OpenTelemetry 0.31 + tracing-subscriber + jemalloc profiling |

### 2.2 Workspace 模块化（synapse-rust 独有优势）

synapse-rust 采用 Cargo Workspace：`[workspace] members` 声明 8 个 crate，另加根 crate（`src/`）共 9 个编译单元。下表文件数为 2026-09-22 实测（`git ls-files '<crate>/*.rs' | wc -l`）：

| Crate | 文件数 | 职责 |
|-------|--------|------|
| `synapse-common` | 72 | 公共工具、加密原语、配置、错误定义 |
| `synapse-cache` | 12 | 缓存层（Redis 连接池 + 本地缓存） |
| `synapse-storage` | 208 | 数据访问层（PostgreSQL 持久化） |
| `synapse-e2ee` | 59 | 端到端加密（Megolm, device keys, key rotation） |
| `synapse-federation` | 20 | 联邦协议（事件传输, EDU, 成员同步） |
| `synapse-services` | 206 | 业务逻辑层 |
| `synapse-web` | 162（其中 `src/routes/` 144） | HTTP 路由层（Axum + Tower 中间件） |
| `synapse-test-utils` | 2 | 测试夹具与共享测试基础设施 |
| 根 crate (`src/`) | 38 | 服务器启动、Worker 管理（根包全部 .rs 含 tests/benches 共 275） |

**优势对比**:
- synapse-rust: 编译时模块隔离，每个 crate 可独立编译测试，依赖关系明确
- Synapse: Python 包层级灵活但边界模糊，运行时才发现导入错误

### 2.3 优势与劣势

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 成熟的多进程 Worker 架构，生产验证<br>- 灵活的 Python 模块系统，快速迭代<br>- 丰富的模块化扩展点（spam checker, third-party rules 等） | - Worker 进程间通信复杂，部署门槛高<br>- Python 包边界模糊，容易产生循环依赖<br>- 单进程内 GIL 限制并行度 |
| **synapse-rust** | - Cargo Workspace 编译时依赖检查，杜绝循环依赖<br- 单进程即可利用多核，部署简单<br>- Tower 中间件链类型安全，编译时验证 | - Worker 拓扑验证仍在建设中（`topology_validator.rs`）<br>- 模块化扩展点不如 Python 灵活<br>- 编译时间长（464K 行 Rust 代码全量编译） |

---

## 3. 技术实现对比

### 3.1 异步并发模型

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **并发模型** | Twisted reactor (event loop) + inlineCallbacks | Tokio async/await (work-stealing scheduler) |
| **线程利用** | 单线程事件循环（GIL 限制） | 多线程 work-stealing 线程池 |
| **CPU 并行** | 需要 Worker 进程才能利用多核 | 单进程内 async + 线程池自动并行 |
| **I/O 并发** | 高（Twisted async I/O 成熟） | 高（Tokio async I/O + 零拷贝） |

### 3.2 数据库访问层

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **驱动** | psycopg2 / txpostgres | sqlx 0.8（Cargo.lock 实锁 0.8.6） |
| **连接池** | Twisted DBACP | sqlx 内置连接池 + deadpool-redis |
| **迁移** | 增量 SQL 迁移文件 | **1 个**统一 Schema 文件（`migrations/00000000_unified_schema_v12.sql`） |
| **查询安全** | 运行时检查 | ⚠️ **以运行时为主**：棘轮基线 `BASELINE_STATIC=61` / `BASELINE_DYNAMIC=2147`（`scripts/ci/sqlx_dynamic_ratio_baseline`），即约 **2.8%** 调用点走 `sqlx::query!` 编译期校验，其余为动态 SQL。静态化比例是一项**待收紧的债务**，不是已完成的优势 |

### 3.3 加密实现

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **Olm/Megolm** | Python binding (libolm C 库) | vodozemac >=0.10.0（Cargo.toml 实际锁定 >=0.10.0，纯 Rust 实现） |
| **密钥签名** | Python + PyNaCl | ed25519-dalek 2.0 (纯 Rust) |
| **哈希算法** | hashlib (Python stdlib) | sha2 0.10 + hmac 0.12 (Rust crates) |
| **密码存储** | bcrypt (Python binding) | Argon2 (项目规则指定) |

### 3.4 路由覆盖

synapse-rust 的 HTTP 契约以机器抽取的 **`docs/synapse-rust/ROUTE_CONTRACT.md`**（2026-09-21 生成）为准：**1,151 条注册路由条目**（绝对 `(method, path)`，已解析 `.nest()` 前缀并去重），涉及 **66** 个含路由注册的模块文件；人工维护的 `docs/synapse-rust/API_COVERAGE_REPORT.md` 按"逻辑端点"口径记为约 **883** 条，两者不可相加。路由文件分布在 **`synapse-web/src/routes/`** 下，共 144 个 `.rs` 文件。

> ⚠️ 上一版本此处写"**656 个 API 端点，覆盖 48 个功能模块**，来源为项目 API 参考文档"。本轮复核确认：仓库内**不存在** `docs/synapse-rust/api-reference.md`，该数字无法在仓库中定位来源，且与上述两份权威清单均不一致，已删除。引用端点数量时请以 `ROUTE_CONTRACT.md` 为准。

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 协议实现最完整，所有 MSC 均已落地<br>- libolm 经过多年安全审计<br>- 增量迁移支持平滑升级 | - Python binding 存在 FFI 开销<br>- 迁移文件过多，维护成本高<br>- 运行时 SQL 错误，难以提前发现 |
| **synapse-rust** | - sqlx 提供编译期 SQL 校验能力（当前 static=61 / dynamic=2147，棘轮单向收紧中）<br>- vodozemac 纯 Rust 实现，无 FFI 开销<br>- 统一 Schema 简化迁移管理<br>- 类型安全的路由定义（Axum macros） | - vodozemac 相对 libolm 生态成熟度较低<br>- 统一 Schema 对增量升级不友好<br>- 97% 的 SQL 调用点未走编译期校验 |

---

## 4. 性能指标对比

### 4.1 语言层面性能

| 指标 | Python | Rust | 倍数 |
|------|--------|------|------|
| **CPU 密集计算** | ~6.7s (1 亿次循环) | ~0.34s (1 亿次循环) | ~20x |
| **内存基线** | ~80-300MB (运行时 + 依赖) | ~5-15MB (二进制) | ~15-20x |
| **启动时间** | 2-5 秒 | 毫秒级 | ~100x |
| **GC 停顿** | 有（分代 GC，p99 抖动） | 无（所有权模型，零 GC） | - |
| **多核利用** | 1 核（GIL 限制） | 全部核心 | N 核倍数 |

> **数据来源**: Rust vs Python 性能基准测试（2026 年），以及 synapse-rust 项目内置 benchmark 数据。

### 4.2 服务器层面性能

| 指标 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **空闲内存** | 300-500 MB | 预期 50-100 MB（jemalloc 精细化分配） |
| **活跃负载内存** | 2-4 GB（联邦房间 + 活跃用户） | 预期 200-500 MB（无 GC 开销，零拷贝） |
| **峰值内存** | 6+ GB（大型联邦房间，如 #matrix:matrix.org） | jemalloc profiling 支持（MALLOC_CONF 配置） |
| **p99 延迟** | 波动大（GC 停顿 + GIL 竞争） | 稳定可预测（无 GC，work-stealing 调度） |
| **编译优化** | 无 JIT（CPython 解释执行） | LTO + codegen-units=1 + panic=abort |

### 4.3 Benchmark 基础设施

synapse-rust 在 `Cargo.toml` 中声明了 5 个 criterion 基准测试套件（`grep -c '\[\[bench\]\]' Cargo.toml`）：

1. `performance_api_benchmarks.rs` — API 端点吞吐量
2. `performance_federation_benchmarks.rs` — 联邦事务处理
3. `performance_sliding_sync_benchmarks.rs` — 滑动同步性能（`required-features = ["test-utils"]`）
4. `performance_membership_benchmarks.rs` — 成员状态操作
5. `performance_pagination_benchmarks.rs` — 分页查询（v1.2 遗漏）

以及 `tests/performance/` 目录下的负载测试和冒烟测试。

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 生产环境大量真实数据验证<br>- 性能瓶颈已有社区分析和优化方案 | - GIL 是结构性限制，无法通过优化消除<br>- GC 停顿导致 p99 不稳定<br>- 内存利用率低（Python 对象模型开销） |
| **synapse-rust** | - 无 GIL，多核并行处理<br>- 无 GC，p99 延迟稳定可预测<br>- jemalloc + profiling 精细化内存管理<br>- 内置 benchmark 基础设施完善 | - 缺乏生产环境大规模验证数据<br>- 实际性能数据待实测确认<br>- 编译时间较长影响迭代速度 |

---

## 5. 可扩展性对比

### 5.1 水平扩展

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **Worker 模型** | 多进程 Worker（generic_worker, federation_sender 等），通过 Redis 共享状态 | TCP Worker 拓扑（`worker/tcp.rs`, `worker/manager.rs`, `worker/load_balancer.rs`） |
| **Worker 发现** | 静态配置 | 健康检查 + 拓扑验证（`worker/health.rs`, `worker/topology_validator.rs`） |
| **负载均衡** | 手动配置 Worker 类型分流 | 内置 load_balancer 模块 |
| **Redis 共享** | tx-redis（Twisted binding） | deadpool-redis 0.20（连接池 + tokio-rustls） |

### 5.2 垂直扩展

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **多核利用** | 需要 Worker 进程，每进程 1 核 | 单进程 async + 线程池自动利用全部核心 |
| **内存扩展** | Python 对象开销大，内存利用率低 | Rust 零开销抽象，内存利用率高 |
| **连接池** | Twisted DBACP（进程内） | sqlx 连接池 + deadpool-redis（进程内 + 跨 Worker） |

### 5.3 功能可扩展性

synapse-rust 通过 Feature Flag 控制模块编译：

```toml
[features]
# 实际取值（Cargo.toml）：`server` 特性已于 H-6 删除（全仓零 gate，属死特性）
default = ["core-private-chat", "widgets", "external-services", "beacons"]
core-private-chat = ["friends", "burn-after-read", "synapse-web/core-private-chat"]
friends = ["synapse-services/friends", "synapse-web/friends"]
saml-sso = ["synapse-services/saml-sso", "synapse-web/saml-sso"]
widgets = ["synapse-services/widgets", "synapse-web/widgets"]
burn-after-read = ["synapse-services/burn-after-read", "synapse-web/burn-after-read"]
# ... 共 12 个可选扩展模块；另有元特性 all-extensions 一次开启全部
```

> ⚠️ 上一版本片段写作 `default = ["server", "core-private-chat", ...]`，其中 `server` 已不存在；且实际特性同时向 `synapse-services` 与 `synapse-web` 两个 crate 转发，不只是 `synapse-services`。

这允许编译最小化二进制文件，适应不同部署场景。

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - Worker 模式成熟，大规模部署验证<br>- 模块系统（Pluggable Modules）灵活扩展<br>- 社区有大量运维经验 | - Worker 部署复杂，需要额外进程管理<br>- 每进程 GIL 限制，垂直扩展效率低<br>- 配置繁琐 |
| **synapse-rust** | - 单进程即可利用多核，部署简单<br>- Feature Flag 控制编译，最小化二进制<br>- 内置负载均衡和健康检查<br>- TCP Worker 拓扑可动态扩展 | - Worker 拓扑仍在完善中<br>- 缺乏大规模生产验证<br>- 水平扩展方案成熟度待验证 |

---

## 6. 可维护性对比

### 6.1 代码质量保障

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **类型系统** | mypy (可选，非强制) | Rust 类型系统（编译时强制） |
| **Linter** | flake8 + black + isort | Clippy（workspace 级别强制） |
| **Lint 规则** | 风格检查为主 | `unwrap_used = "deny"`, `redundant_clone = "deny"`, `unused_async = "deny"` 等 |
| **错误处理** | 异常（运行时检查） | thiserror 2.0 + Result 类型（编译时检查） |
| **测试框架** | pytest + Twisted trial | 内置 test harness（unit, integration, e2e, performance） |

### 6.2 测试覆盖

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **测试文件数** | ~500+ 测试文件 | 230 个测试文件（`find tests -name '*.rs'`，另有各 crate 内联 `#[cfg(test)]` 模块） |
| **测试分层** | Unit + Integration + System tests | 4 层：`tests/unit`(106 文件) + `tests/integration`(113) + `tests/e2e`(3) + `tests/performance`(5) |
| **Mock 方式** | mock + patch | mockall 0.13 + wiremock 0.6 + insta 快照（Cargo.lock 实锁 1.48.0） |
| **属性测试** | 无 | quickcheck 1.1（仅用于 `synapse-common/src/validation.rs` 的输入校验，覆盖面有限） |
| **基准测试** | 基本无 | criterion 0.5（5 个 benchmark 套件） |
| **快照测试** | 无 | insta 1.41+（JSON 响应形状锁定） |
| **测试工具** | 共享 fixtures | `synapse-test-utils` 独立 crate（当前仅 2 个 .rs 文件） |

### 6.3 文档与配置

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **文档文件数** | 官方文档站 (matrix-org.github.io) | 221 个文件（`find docs -type f`，其中 `docs/*.md` 147 个） |
| **API 参考** | 在线文档 | `docs/synapse-rust/ROUTE_CONTRACT.md`（1,151 条注册路由 / 66 个模块，机器抽取）+ ledger 导出契约；⚠️ 此前引用的 `docs/synapse-rust/api-reference.md` **不存在** |
| **Docker 配置** | docker-compose 示例 | 212 个文件位于 `docker/`（另有 3 个 `Dockerfile*`） |
| **数据库标准** | 无统一标准 | `DATABASE_FIELD_STANDARDS.md` 字段命名规范 |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 官方文档站完善<br>- 社区 Wiki 和 FAQ 丰富<br>- 多年积累的最佳实践 | - mypy 非强制，类型安全不完整<br>- 测试以 mock 为主，集成测试不足<br>- 代码风格统一但运行时错误仍多 |
| **synapse-rust** | - 编译时类型安全 + Clippy 强制 Lint<br>- 4 层测试体系 + 属性测试 + 快照测试<br>- 统一数据库字段标准<br>- 文件化 API 参考覆盖全部端点 | - 文档规模大但质量参差不齐<br>- 测试覆盖虽广但真实联调测试不足<br>- 编译时间长影响开发迭代 |

---

## 7. 安全特性对比

### 7.1 认证与授权

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **密码哈希** | bcrypt | Argon2（可配置成本参数） |
| **Token 管理** | Access/Refresh Token | Access/Refresh Token + JWT (HS256) |
| **管理员注册** | 共享密钥 | HMAC-SHA256 签名验证 |
| **SSO** | SAML + OIDC + CAS | SAML + OIDC + CAS (Feature Flag 控制) |
| **速率限制** | 有（per-endpoint） | RateLimit 中间件 + Federation RateLimit |

### 7.2 加密安全

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **E2EE 引擎** | libolm (C 库 binding) | vodozemac `>=0.10.0`（Cargo.lock 实锁 **0.11.0**，纯 Rust；Megolm/Olm 走 `GroupSession`/`InboundGroupSession`/`Account`/`Session` + 加密 pickle） |
| **密钥轮转** | 有 | `synapse-e2ee/src/key_rotation/`（1017 行）+ `synapse-federation/src/key_rotation.rs`（1157 行） |
| **跨设备验证** | 有（成熟） | ⚠️ **PARTIAL（v1.4 重判）**：交叉签名（`e2ee/cross_signing/`）与设备信任（`e2ee/device_trust/`）**真实**；`derive_sas` 已改为 HKDF-SHA256（`verification/service.rs:105-113`）、`confirm_sas` 已真正校验 MAC（`:316-375`，`secure_compare` + 拒绝空 MAC）。**但 SAS 仍 4 处偏离规范**：① info 串**缺双方公钥且字段顺序错误**（`:41-50`，规范为 `MATRIX_KEY_VERIFICATION_SAS\|发起方 user\|发起方 device\|发起方公钥\|响应方 user\|响应方 device\|响应方公钥\|txn`）；② emoji 由 6 字节各自 `%64` 生成（实产 **6 个**，非规范 42-bit 分组的 7 个）且 decimal 算出后**被丢弃**（`:284-292`，`_decimal` 未使用，返回值只含 `Emoji`）；③ MAC 为裸 `HMAC-SHA256(shared, key_id‖0x00‖value…)` 而非 `hkdf-hmac-sha256.v2`（`:116-129`，`:303-307` 注释自述）；④ commitment 用 HMAC 而非 `SHA-256(公钥‖请求规范 JSON)`（`:206-210`）。**QR 为显式 fail-closed** 返回 `M_UNSUPPORTED`（`:403-424`，非桩） |
| **密钥备份** | 有 | `synapse-e2ee/src/backup/` + `synapse-web/src/routes/e2ee/backup.rs`（`synapse-common/src/secure_backup` 摘要派生另有实现，`ssss/service.rs:250` 的 curve25519 路径从密文自身派生 AES 密钥，非 ECDH） |
| **SSSS** | 有 | ✅ **已对齐规范（2026-09-23 修复，commit `a2375743`）**：`e2ee/ssss/service.rs` 现按 matrix-spec v1.19 `m.secret_storage.v1.aes-hmac-sha2` 实现——HKDF-SHA256（salt = 32 个 0 字节，输出 64 字节拆 AES key/MAC key；密钥校验 info 为空串、加密 info 为 secret name）、**AES-256-CTR**（128 位大端计数器）+ **encrypt-then-MAC**（HMAC-SHA-256 over 密文）、16 字节 IV 且 **bit 63 清零**、`iv/ciphertext/mac` 一律无填充 base64；新增 `decrypt_secret` 先验 MAC 再解密。MSC2697 `curve25519-aes-sha2`（从密文自身派生 AES 密钥、公钥取密文前 32 字节）**已删除**，`create_key`/`encrypt_secret` 对其 fail-closed 400。测试 27 项含 **NIST SP 800-38A F.5.5 CTR-AES256 已知向量**、篡改密文 → 403、错误 secret name → 403、非 32 字节密钥拒绝 |
| **泄漏检测** | 有 | ❌ **能力不存在**：`synapse-e2ee/src/leak_detection/` 目录**已删除**（`glob 'synapse-e2ee/src/leak_detection/**'` 无结果），仓库内无替代实现 |
| **内存安全** | Python 管理但 binding 可能有漏洞 | Rust 所有权模型 + `zeroize`（当前仅 `synapse-e2ee` 依赖） |

### 7.3 网络安全

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **CORS** | Twisted CORS middleware | tower-http CORS 中间件 |
| **CSRF** | 有 | 专用 CSRF 中间件 (`middleware/csrf.rs`) |
| **IP 黑名单** | 有 | 有（`middleware/security.rs`） |
| **联邦认证** | 有 | `middleware/federation_auth.rs` + 签名验证 |
| **联邦速率限制** | 有 | `middleware/federation_rate_limit.rs` 独立中间件 |

### 7.4 安全审计

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **审计历史** | 多年安全审计，CVE 记录完整 | 新项目，审计历史有限 |
| **依赖安全** | pip-audit / safety | cargo-audit + cargo-deny + RUSTSEC 跟踪 |
| **内存安全** | C binding 可能有内存漏洞 | Rust 内存安全（编译时保证）+ `zeroize` 清理敏感数据 |
| **漏洞修复** | 成熟的 CVE 响应流程 | 主动替换不安全依赖（如 paste → pastey fork） |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 安全审计历史长，CVE 记录完善<br>- libolm 经过专业密码学审计<br>- 生产环境安全事件响应经验丰富 | - C binding 可能引入内存安全漏洞<br>- bcrypt 不如 Argon2 抗 GPU/ASIC 破解<br>- Python 运行时类型安全问题 |
| **synapse-rust** | - Rust 编译时内存安全保证<br>- Argon2 密码哈希（抗 GPU/ASIC）<br>- 主动跟踪 RUSTSEC 并替换不安全依赖<br>- `zeroize` 清理敏感数据<br>- E2EE 跨设备验证完整（SAS/QR + 交叉签名 + 设备信任） | - vodozemac 审计历史短于 libolm（但已升级至 >=0.10.0，Soatok 2026-02 DH 贡献性问题已修复） |

---

## 8. 用户体验对比

### 8.1 服务器端用户体验（API 延迟）

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **同步延迟** | 大型房间 sync 可达数秒 | 预期亚秒级（无 GC 停顿） |
| **消息发送** | ~50-200ms（含 DB 写入） | 预期 ~10-50ms |
| **联邦延迟** | 受 Python 序列化影响 | reqwest 0.12 + 零拷贝序列化 |
| **Sliding Sync** | MSC3886 实现存在 | 独立 `sliding_sync_service` 模块 + benchmark |

### 8.2 部署体验

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **部署方式** | pip install + YAML 配置 | 单二进制 + YAML 配置 |
| **Docker 镜像** | ~400-600MB (Python + deps) | ~50-100MB (静态链接二进制) |
| **启动速度** | 2-5 秒 | 毫秒级 |
| **配置复杂度** | 高（Worker 配置 + 主进程配置） | 低（单进程 + 可选 Worker） |
| **依赖管理** | pip + poetry (虚拟环境) | Cargo (lockfile + workspace) |

### 8.3 运维体验

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **日志** | structured logging | tracing-subscriber (env-filter, JSON, fmt) |
| **指标** | Prometheus client | OpenTelemetry 0.31 (metrics + logs) + tracing-opentelemetry |
| **内存诊断** | 有限（Python tracemalloc） | jemalloc profiling（MALLOC_CONF 控制，jeprof 符号化） |
| **健康检查** | /health 基本检查 | 健康检查 + schema_health_check 独立工具 |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 运维经验积累丰富，社区文档完善<br>- Worker 模式可精细控制资源分配<br>- 故障排查有成熟方法论 | - 部署复杂度高（多进程管理）<br>- Docker 镜像体积大<br>- 启动慢影响滚动更新 |
| **synapse-rust** | - 单二进制部署简单<br>- Docker 镜像体积小<br>- 毫秒级启动，快速滚动更新<br>- jemalloc profiling 精细化内存诊断<br>- OpenTelemetry 原生集成 | - 运维经验积累不足<br>- 故障排查方法论待完善<br>- 缺乏社区运维文档 |

---

## 9. 开发效率对比

### 9.1 开发体验

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **语言学习曲线** | 低（Python 易上手） | 高（Rust 所有权 + 生命周期） |
| **编译反馈** | 无编译步骤（解释执行） | 编译时全面检查（类型 + 借用 + Clippy） |
| **迭代速度** | 快（修改即运行） | 慢（全量编译 + LTO） |
| **IDE 支持** | 完善（PyCharm, VSCode + Pylance） | 良好（rust-analyzer, 但大型项目较慢） |
| **错误定位** | 运行时堆栈追踪 | 编译时错误 + 运行时 panic 回溯 |

### 9.2 编译与构建

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **构建时间** | 秒级（无编译） | 分钟级（8 crate workspace + 编译时 SQL 验证 + LTO） |
| **Profile** | 无 | dev (opt-level=1) / test (opt-level=0) / release (LTO) / bench |
| **交叉编译** | 不需要 | ⚠️ **不成立**：`rust-toolchain.toml` 只声明 `x86_64-unknown-linux-gnu`，全仓无 `musl` 目标；`docker/Dockerfile` 使用 `distroless/cc-debian12` 并**显式抽取缺失的动态库**（`runtime-libs` 阶段），即产物是 glibc **动态链接**，不是静态链接 |
| **增量编译** | 不需要 | 支持（incremental=true for dev/test） |

### 9.3 代码组织

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **路由文件数** | ~60 REST servlet 文件 | 144 个路由文件（`synapse-web/src/routes/`） |
| **服务文件数** | ~80 handler 文件 | 194 个服务文件（`synapse-services/src/`） |
| **存储文件数** | ~50 store 文件 | 208 个存储文件（`synapse-storage/src/`） |
| **依赖注入** | Python dictionary-based HomeServer | Rust trait-based wiring（`wiring/` 模块） |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - Python 易上手，开发速度快<br>- 无编译等待，即时反馈<br>- 社区贡献门槛低 | - 运行时错误多，调试成本高<br>- 类型安全不完整（mypy 非强制）<br>- 重构风险高（缺乏编译时保障） |
| **synapse-rust** | - 编译时全面保障，运行时错误少<br>- 重构安全（编译器保障）<br>- Clippy 强制代码质量 | - Rust 学习曲线陡峭<br>- 编译时间长影响迭代<br>- 社区贡献门槛高 |

---

## 10. 资源利用率对比

### 10.1 内存利用率

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **空闲基线** | 300-500 MB | ~5-15 MB（二进制本身） |
| **运行时基线** | 300-500 MB（Python 运行时 + 依赖） | ~50-100 MB（预估，jemalloc 分配） |
| **活跃负载** | 2-4 GB | ~200-500 MB（预估） |
| **内存分配器** | Python pymalloc | tikv-jemallocator 0.6（profiling feature） |
| **内存泄漏排查** | 困难（Python GC 管理） | MALLOC_CONF + jeprof 符号化 |
| **对象开销** | ~232 bytes/对象（Python 对象模型） | ~48 bytes/对象（Rust struct，零开销抽象） |

### 10.2 CPU 利用率

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **核心利用** | 1 核/进程（GIL） | 全部核心（Tokio work-stealing） |
| **上下文切换** | 频繁（Twisted reactor + 线程切换） | 少（async task 调度） |
| **序列化开销** | json.dumps（Python 实现） | serde_json（SIMD 优化） |
| **正则匹配** | Python re 模块 | regex 1.10（Rust 原生） |

### 10.3 磁盘与网络

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **Docker 镜像** | ~400-600 MB | 预期 ~50-100 MB（⚠️ 本轮未实测：无可用镜像产物） |
| **二进制大小** | N/A（解释执行） | 未实测（`release` 为 LTO + `strip=false` + `panic=abort`，保留行号表供 jeprof 符号化） |
| **链接方式** | N/A | glibc **动态链接**（`distroless/cc-debian12` + 抽取 libssl3 等动态库），**非静态链接** |
| **网络序列化** | Python json | serde_json + bytes（零拷贝） |
| **HTTP 压缩** | 无 | tower-http compression-gzip |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 资源使用模式已被充分理解<br>- 可通过 Worker 分配资源配额 | - 内存利用率极低（Python 对象开销）<br>- CPU 利用不充分（GIL）<br>- Docker 镜像体积大 |
| **synapse-rust** | - 内存利用率高（5-10x 节省）<br>- CPU 全核利用<br>- Docker 镜像极小<br>- jemalloc 精细化内存管理<br>- 零拷贝网络序列化 | - 实际资源数据待大规模验证<br>- jemalloc profiling 增加少量开销 |

---

## 11. 业务对齐度对比

### 11.1 Matrix 协议覆盖

> **对齐基准**: Synapse v1.161.0（2026-09-15 发布，`release-v1.161` CHANGES.md）
>
> ⚠️ **证据口径（v1.3 起）**：本表每一行的状态必须有 `路径:行号` 或可复现命令支撑；无法证实的标 `[未验证]`。
> MSC 编号语义以 `docs/synapse-rust/MSC_SEMANTICS.md` 为唯一真相源（该表登记了 MSC4155/4204/3967 等**借用编号**），
> 不得按编号推断语义。本表覆盖的 MSC 少于代码中实际出现的标识（`grep -rhoE 'msc[0-9]{4}'` 可查），
> 属"抽查"而非"全覆盖"。

| MSC / 功能 | Synapse (Python) v1.161 | synapse-rust v6.2.0 | 对齐状态 |
|------------|--------------------------|----------------------|----------|
| **核心 CS API** | ✅ 完整 | 路由面完整（`ROUTE_CONTRACT.md` 1,151 条注册路由）；按类别人工统计覆盖率 **80–97%**（`API_COVERAGE_REPORT.md`，2026-05-28 口径，非逐端点实测） | ⚠️ 未逐端点验证 |
| **联邦协议** | ✅ 完整 | ⚠️ **PARTIAL（v1.4 降级）**：`synapse-federation/` 模块存在，`send_*` 已按 `expected_membership` 校验（`membership/mod.rs:128-137`）；**但 `/send_join` 响应缺规范必需的 `state` 与 `auth_chain`**（`membership/join.rs:183-185` 与 v2 `:297-300` 仅回 `event_id`/`room_id`）⇒ 合规远端拿不到房间状态、无法完成入房；`make_join` 模板缺 `origin`/`origin_server_ts`/`room_id`（`:22-31`）；入房/离房路径**未调用房间 ACL 检查**（`:34-91, 94-198` 无 `check_server_acl`） | ⚠️ 部分对齐 |
| **E2EE** | ✅ 完整（libolm） | ⚠️ **PARTIAL（v1.4 重判）**：Megolm/Olm、交叉签名、设备信任、密钥备份**真实**；SAS 的 `derive_sas` 已 HKDF（`verification/service.rs:105-113`）、`confirm_sas` 已校验 MAC（`:316-375`），但仍 **4 处偏离规范**（info 串缺公钥+顺序错 / emoji+decimal 非规范 / MAC 非 `hkdf-hmac-sha256.v2` / commitment 非 SHA-256，详见 §7.2）；**QR 为显式 fail-closed 不支持**（`:403-424`）；`leak_detection` 模块**已删除**（能力缺失）；SSSS 用 GCM 且从密文派生密钥（见 §7.2） | ⚠️ 部分对齐 |
| **Sliding Sync** | ✅ 完整 | ✅ 完整（独立 `sliding_sync_service/` 模块 + benchmark；另有 `msc4186` 简化滑动同步引用） | ✅ 已对齐 |
| **MSC3030** (Timestamp to event) | ✅ | ✅ | ✅ 已对齐 |
| **MSC2776** (Presence list) | ✅ | ✅（代码中无 `MSC2776` 标识，按路由 `presence.rs` 判定） | ✅ 已对齐 |
| **MSC2654** (Read markers) | ✅ | ✅ | ✅ 已对齐 |
| **MSC3079** (VoIP) | ✅ | ✅ | ✅ 已对齐 |
| **MSC3882** (QR login) | ✅ | ✅ | ✅ 已对齐 |
| **MSC3886** (Sliding sync) | ✅ | ✅ | ✅ 已对齐 |
| **MSC3983** (Thread) | ✅ | ✅（`thread_service.rs` + `synapse-storage/src/thread/`） | ✅ 已对齐（实现较精简） |
| **MSC3245** (Room summary) | ✅ | ✅ 完整（`synapse-services/src/room/summary/` + `synapse-storage/src/room_summary/`，单实现） | ✅ 完整对齐 |
| **MSC4380** (Invite shield) | ✅ | ✅ | ✅ 已对齐 |
| **MSC4354** (Sticky Event) | ✅ | ✅（`sticky_event.rs` 服务 + 存储） | ✅ 已对齐 |
| **MSC4261** (Widget API) | ✅ | ✅ | ✅ 已对齐 |
| **MSC4140** (Cancellable Delayed Events) | ✅（v1.143 起；v1.161 仅新增"查询单个延迟事件"端点） | ⚠️ **PARTIAL**：单机链路真实（`delayed_event_service.rs` + `synapse-storage/src/delayed_events.rs` + 调度器 `src/server/mod.rs:745-843` + 所有权 fail-closed），但**无 EDU/联邦同步**（`EduType` 无该类型，全仓 `m.delayed_event` 0 命中），且 schedule 的 `state_key` 硬编码 `None` | ⚠️ 单机可用，联邦缺失 |
| **MSC3912 / v11 撤回格式** | ✅（v1.161 #19782：room version > 10 时 `redacts` 放入 `content`） | ⚠️ **创建路径已修（Phase 1，2026-09-22）**：`RoomMessagingService::create_event` 按房间版本注入 `content.redacts`（v11+），出站 PDU 不再重复写顶层 `redacts`；测试 `create_redaction_in_v11_room_puts_target_in_content` / `v11_pdu_does_not_gain_top_level_redacts` 锁定。**仍缺**：关系性（`rel_type`）级联撤回未实现；`MSC3912` 代码标识 0 命中 | ⚠️ 格式已对齐，级联未实现 |
| **MSC4242** (State DAGs) | ✅ 实验性（v1.161 #20127 联邦客户端 + #19718 存储函数） | ⚠️ **仅存储层**（`event/dag.rs:179/208/237` + `create.rs:161`），无服务/联邦/路由/房间版本启用；且 `dag.rs:200-205,231-234` 注释声称被 `/send_join`、`/get_missing_events` 使用，实测**无调用点**（不实注释） | ❌ 缺失（实验性） |
| **MSC4512** (Application Services Proxy) | ✅ v1.161 实验性（#19972 代理命名空间 + #19977 联邦请求） | ❌ **未实现**（`MSC4512`/`proxy_namespace` 0 命中）；另注：`module_service.rs` 实际不止 spam/3P/auth，还含模块 CRUD、媒体与 account_data 回调、account validity（原表述低估） | ❌ 缺失（实验性） |

**v1.161.0 上游条目对齐情况**（v1.3 已逐条实测，不再使用"待核查"）：

| 上游条目 | Synapse v1.161 | synapse-rust 实测 | 状态 |
|----------|----------------|-------------------|------|
| #20148 DB 宕机时新事件无法持久化 | ✅ 修复（根因：每实例 state-group persisted 标记过期） | **该机制在本仓不存在 ⇒ 上游具体 bug N/A**；但同类风险存在：`create_event_with_graph` 先 INSERT `events` 再于**事务外** INSERT `event_edges`（`synapse-storage/src/event/create.rs:112-142`），联邦入库/补洞/backfill 使用 | ⚠️ PARTIAL |
| #20119 `event_search` 跳过 `m.room.topic` | ✅ 修复 | 重建索引帮手含 `m.room.topic`（`search_index.rs:238`）但该存储**无调用者**（死代码）；在线查询与 GIN 索引硬限定 `m.room.message`（`event/search.rs:170,210,311,249-255`）⇒ 主题默认搜不到 | ⚠️ PARTIAL |
| #20169 `/sync` 左房成员泄漏（MSC4222 `state_after`） | ✅ 修复 | **❌ 旧版 ✅ 系误判**：全仓 `state_after`/`MSC4222` = 0；旧版引用的 `include_redundant_members` 是另一功能（`sync_service/filter.rs:110`）。左房成员态取自**当前** state（`data_fetch.rs:156-184`） | ⚠️ 机制不同/未对齐 |
| #20149/#20172 Profile 500 | ✅ 修复 | 不存在用户 → 404 已对；account_data 非 JSON 对象仍 500（`extended_profile.rs:47-50`）；**已停用但存在用户写自定义字段返回 404**（`user/storage.rs:663-673`），与上游"应成功"相反；稳定 `GET /_matrix/client/v3/profile/{userId}/{keyName}` 未注册（仅 `uk.tcpip.msc4133`） | ⚠️ PARTIAL |
| #20173 Profile PUT/DELETE 400→403 | ✅ 修复 | ✅ 已正确返回 403 + `M_FORBIDDEN`（`account_compat.rs:195-197,224-226`）；上游触发配置（`enable_set_displayname` 等）本仓不存在 | ✅ 已对齐 |
| #20036 房间举报端点 `rc_reports` 限流 | ✅ 修复 | ✅ **已实现（Phase 2）**：路径中间件无法表达 `/rooms/{room_id}/report`（只有精确/前缀匹配），故在 `report_room`/`report_user` 内按用户取桶（`ratelimit:rc_reports:{user_id}`），规则可配（`rate_limit.rc_reports`，默认 1/s、burst 10），并有"配置键被删即变红"的守卫测试 | ✅ 已对齐 |
| #20180 `M_APPSERVICE_LOGIN_UNSUPPORTED` | ✅ 稳定化 | ⚠️ **部分**：`m.login.application_service` 已实现（Phase 2：as_token + 排他命名空间校验 + 设备物化 + 令牌签发，测试覆盖 200/403/401）。但该**错误码本身经复核属 `POST /register`**（MSC4190：appservice 未传 `inhibit_login=true`），不属 `/login`；上游 diff 本地无法取证，故未臆造该码 | ⚠️ 登录已实现，错误码待定 |
| #20146 LiveKit SFU WebSocket URL | ✅ 弃用 `livekit_service_url` 并新增 SFU URL | ⚠️ `livekit_service_url` 本仓不存在（无可弃用）；曾经的 `LivekitConfig.ws_url` 是**从未被读取**的死配置，**本轮已删除**（`config/voip.rs` 现仅 `api_key`/`api_secret`/`host`）；`rtc/transports` 仍只返回 ICE。**差异未消除**：SFU 传输本身未实现 —— 现在至少不再有一个"看起来可配"的旋钮 | ⚠️ PARTIAL |

**v1.2 遗漏的上游条目（同基准期，v1.3 补记）**：

- **v1.157.2 安全版本**：6 High / 4 Moderate / 2 Low ELEMENTSEC 公告 —— 本报告 §7 安全对比**完全未提**，应逐条判定本仓同类性。
- **v1.158 默认房间版本改为 11（MSC4239）**：本仓 `DEFAULT_ROOM_VERSION` 也是 11，但 `room_versions.rs:77-81` 注释仍称"Synapse 默认 10，本项目刻意不同"——**注释已过时**；且 v12/v13 为 `stable_parse_only`（可 join/联邦，**不可创建**）。
- **v1.158 v12 房间修复**（并发建房 room ID 冲突、v12 第三方邀请）；**MSC4326** appservice 设备伪装稳定化（本仓 0 命中）；**MSC2409** appservice 短暂事件（本仓 12 处引用）；**MSC4186** 简化滑动同步（24 处）；**MSC4502** 房间成员查询（11 处）；**MSC4262/MSC4429** Profile 更新进 sync（21 处）——这些代码中已有引用的编号，本报告 v1.2 均未列出。
- **#20189** 联邦 `make_*` 请求缺少 `membership` 校验（安全修复）——见 §12.5 B2。

### 11.2 扩展功能

| 功能 | Synapse (Python) | synapse-rust | 对齐状态 |
|------|-------------------|--------------|----------|
| **好友系统** | 无（标准 Matrix 无此功能） | ✅ 独有扩展（`friend_room_service/` 含 groups/models/sharding，完整实现 + 联邦好友同步 `synapse-federation/src/friend/`） | ✅ 完整实现 |
| **阅后即焚** | 无 | ✅ 独有扩展（`burn_after_read_service.rs` 完整实现，含 BURN_MAX_RETRY=5 死信处理 + 定时扫描器） | ✅ 完整实现 |
| **信标/位置** | 无标准实现 | ✅ 独有扩展（`beacon_service.rs` 完整实现，含配额限制、背压控制、缓存） | ✅ 完整实现 |
| **内容扫描** | 模块化扩展 | ❌ **未装配（孤儿模块）**：`content_scanner/{service.rs 518 行, models.rs 316 行}` 存在，但 `synapse-storage/` **无**存储模块、`migrations/` **无**相关表、`ContentScanner` 在生产路径**从未被构造**（目录外仅 `lib.rs:78` 与 `media/mod.rs:10` 两处 re-export）、`ContentScannerConfig` 未接入 config → 既无持久化也无接线 | ❌ 未启用 |
| **短信推送** | 有（通过 Push Gateway） | ✅ `SmsProvider` trait（`sms_provider/mod.rs:16`）+ 三种实现：`NoopSmsProvider`（默认桩）、**通用 `HttpSmsProvider`（`:48`）**、`AliyunSmsProvider`（`aliyun.rs:43`）；工厂支持 `"aliyun"/"http"/"generic_http"` → 缺 Twilio 等厂商专用实现，但可用通用 HTTP 接入 | ✅ 已实现（厂商专用实现缺） |
| **VoIP Tracking** | 无 | ✅ Feature Flag 控制（`rtc/` 模块 + `voip/` 路由） | ✅ 已实现 |
| **服务器通知** | 有 | ✅ `server_notification_service.rs` 完整实现 | ✅ 完整实现 |
| **隐私扩展** | 无标准 | ✅ Feature Flag `privacy-ext`（存储层 `synapse-storage/src/privacy.rs` 985 行 + `user_privacy_settings` 表；服务逻辑在 `synapse-services/src/account_identity_service.rs:8-52` 与 `wiring/extensions.rs:51`，**不存在** `synapse-services/src/privacy.rs`） | ✅ 已实现 |
| **应用服务** | 完整（Pluggable Modules） | ⚠️ **部分实现**：AS 注册/命名空间正则/虚拟用户/事务投递（含 `hs_token`）/调度**真实**（`application_service/`）；**缺** pushers、设备管理、AS 登录（无 `m.login.application_service`）、AS 以虚拟用户身份调用 C-S（客户端提取器只做 token 校验）、MSC4512 代理；`external_service.rs` 属私有桥接扩展（`/_synapse/external/*`），**不是** Matrix AS API | ⚠️ 实现不完整 |
| **延迟事件** | 有 | ⚠️ **PARTIAL**：单机链路完整（见 §11.1 MSC4140）；**无联邦/EDU**，`state_key` 硬编码 `None` | ⚠️ 单机可用 |
| **关系性撤回** | 有（v1.161+） | ❌ **未实现按房间版本的 `content.redacts`**，且无 `rel_type` 级联；详见 §11.1 MSC3912 行 | ❌ 存在互操作缺陷 |
| **房间升级** | 有 | ✅ `handlers/room/management/upgrade.rs` 完整实现（`upgrade_room` + `get_room_version`） | ✅ 完整实现 |
| **Space** | 有 | ✅ `synapse-web/src/routes/space/` 完整实现（children_hierarchy/lifecycle_query/membership_state/summary/types） | ✅ 完整实现 |
| **Thread** | 有 | ✅ `thread_service.rs` + `synapse-storage/src/thread/` + `handlers/thread.rs` | ✅ 已实现（相对精简） |
| **LiveKit / RTC** | 有 | ⚠️ **PARTIAL**：`rtc/` 存在；`livekit_service_url` 本仓从未存在，`LivekitConfig.ws_url`（`config/voip.rs:92`）**声明但无读取点**（死配置），`rtc/transports` 仅返回 ICE | ⚠️ 能力不完整 |
| **Dehydrated Devices** | 有 | ✅ **已对齐（Phase 2）**：`/events` 改为 `GET` + query 参数，`next_batch` 空页返回 `null`（storage `Option<i64>` → service → handler 三层同步）；契约产物（derived 表、6 fixture、2 快照、ROUTE_CONTRACT、route-table、client.yaml、api_test 输入）已再生成；端到端测试断言 GET 200 + `events: []` + `next_batch: null` + 旧 POST 返回 405 | ✅ 已对齐 |
| **Rendezvous** | 有 | ✅ `rendezvous.rs` + `msc4108_rendezvous.rs` 完整实现 | ✅ 完整实现 |
| **Key Rotation** | 有 | ✅ `synapse-e2ee/src/key_rotation/` + `synapse-federation/src/key_rotation.rs` 完整实现 | ✅ 完整实现 |
| **事件报告** | 有 | ✅ `event_report_service.rs` + `synapse-storage/src/event_report/`（1662 行）；admin 举报端点**已实现**（`admin/report.rs:20-24`）——`API_COVERAGE_REPORT.md:126-127` 把它列为"缺失"是**过时信息** | ✅ 完整实现 |
| **背景更新** | 有 | ✅ `background_update_service.rs` + `synapse-storage/src/background_update.rs` 完整实现 | ✅ 完整实现 |

### 11.3 SDK 与前端生态

| 维度 | Synapse (Python) | synapse-rust | 对齐状态 |
|------|-------------------|--------------|----------|
| **官方 SDK** | matrix-js-sdk, matrix-ios-sdk, matrix-android-sdk2 | 独立 matrix-js-sdk 封装层 | ✅ 已对齐（已知 Bug：URL 重复前缀等） |
| **前端集成** | Element Web/Desktop/iOS/Android | TJG 前端（Vue 3 + Tauri 跨平台） | ✅ 已对齐 |
| **Admin API** | 完整且文档化 | ⚠️ 覆盖较广（`synapse-web/src/routes/admin/` 含 audit/cleanup/federation/media/notification/policy/register/report/retention/room/management/security/server/token/user），但 `API_COVERAGE_REPORT.md`（2026-05-28 口径）按类别为 **89–94%**，非"完整对齐"；举报端点**已实现**（旧"缺失"清单过时） | ⚠️ 接近对齐 |
| **扩展端点** | 无标准 | ✅ `/_matrix/vendor/v1/` 私有扩展（`external_service.rs`，属私有桥接而非 Matrix AS API） | ✅ 已实现 |
| **OIDC/Builtin OIDC** | 有 | ⚠️ 路由完整（`routes/oidc/`），但 `synapse-services/src/oidc_service.rs:673` 的 `validate_id_token_claims` **从未被调用**（`#[allow(dead_code)]`，安全相关） | ⚠️ 存在未接线校验 |
| **SAML/CAS SSO** | 有 | ✅ Feature Flag 控制（`saml.rs` + `cas.rs`） | ✅ 已对齐 |
| **Key Backup** | 有 | ✅ `synapse-web/src/routes/e2ee/backup.rs` 完整实现 | ✅ 已对齐 |
| **Push Notifications** | 有 | ✅ `push/` + `push_notification.rs` + `client_push_service.rs` 完整实现 | ✅ 已对齐 |
| **Relations** | 有 | ✅ `relations_service.rs` + `synapse-storage/src/relations/`（不含 `content.redacts` 与级联撤回，见 §11.1） | ⚠️ 部分对齐 |
| **Search** | 有 | ⚠️ **默认搜索面已修（Phase 2）**：未传 `filter.types` 时改为 `IN (m.room.message, m.room.name, m.room.topic)`（`event/search.rs:84`，内容按 `content::text` LIKE 匹配，无需回填）；**仍存**：`search_index.rs` 为死代码、其 partial GIN 索引谓词未同步（FTS 路径不可达），列为独立清理项 | ⚠️ 部分对齐 |
| **Webhooks/App Services** | 完整 | ⚠️ `app_service.rs` 提供 AS 管理/事务/命名空间；**AS 登录（`m.login.application_service`）已于 Phase 2 实现**；`external_service.rs` 是私有桥接扩展，**不能**算作 Matrix AS API；仍缺 pushers/设备管理/虚拟用户调用 C-S/MSC4512 | ⚠️ 部分对齐 |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 协议覆盖最完整，所有 MSC 均已实现<br>- 官方 SDK 生态完善<br>- 与 Element 客户端深度集成<br>- 社区贡献和 Bug 修复活跃<br>- v1.161 新增 MSC4512（实验性）、MSC4242 联邦客户端（实验性）与 MSC4140 单事件查询端点 | - 不支持业务定制扩展<br>- 好友/阅后即焚等需要外部桥接<br>- 缺乏内置短信推送 |
| **synapse-rust** | - 好友系统/阅后即焚/信标等独有扩展实现完整（本轮实测为真）<br>- 通用 `HttpSmsProvider` + trait 接缝，便于接第三方短信<br>- Feature Flag 控制功能裁剪<br>- Room Summary 单实现架构清晰<br>- Space/Thread/Rendezvous/Key Rotation/事件报告/背景更新完整 | - **撤回格式与默认房间版本不匹配（v11 默认却用 v10 顶层 `redacts`）**——协议互操作缺陷<br>- **E2EE SAS 派生非规范、QR 为桩、泄漏检测为未编译死代码**<br>- **MSC4140 无联邦/EDU**<br>- **`MSC3912`（关系性撤回）未实现**<br>- Content Scanner 模块未装配（孤儿）、LiveKit `ws_url` 死配置<br>- 无 appservice 登录、无 `rc_reports` 专项限流、Dehydrated `/events` 端点方法落后上游<br>- 生产路径仍有半写窗口、事务去重标记在事件事务外、4 处吞 DB 错误<br>- State DAGs (MSC4242) / App Service 代理 (MSC4512) 缺失（上游均为**实验性**） |

---

## 12. 总结结论

### 12.1 综合评估矩阵

| 评估维度 | Synapse (Python) | synapse-rust | 优势方 |
|----------|-------------------|--------------|--------|
| **架构设计** | 成熟但复杂 | 模块化 + 类型安全 | synapse-rust |
| **技术实现** | 协议完整但 binding 有开销 | 纯 Rust 实现无 FFI | synapse-rust |
| **性能指标** | 受 GIL 和 GC 限制 | 无 GIL 无 GC，多核并行 | synapse-rust |
| **可扩展性** | Worker 模式成熟但复杂 | 单进程多核 + Feature Flag | synapse-rust |
| **可维护性** | Python 灵活但类型安全弱 | Clippy + 编译时保障 | synapse-rust |
| **安全特性** | 审计成熟但 C binding 有风险 | Rust 内存安全 + Argon2 | synapse-rust |
| **用户体验** | 运维经验丰富 | 部署简单 + 快速启动 | 平手 |
| **开发效率** | Python 快速迭代 | 编译保障但迭代慢 | Synapse |
| **资源利用** | 内存/CPU 利用率低 | 高效利用 | synapse-rust |
| **业务对齐** | 协议完整但无定制扩展 | 协议大面积对齐 + 独有扩展，但存在**协议正确性缺陷**（v11 撤回格式、E2EE SAS/QR、MSC4140 无联邦）与若干未实现项 | Synapse（对齐质量更高） |

### 12.2 核心发现

1. **性能是 synapse-rust 最显著的优势**：Rust 无 GIL、无 GC，单进程可利用多核，内存利用率预期提升 5-10 倍。这对于大型联邦房间（如 60,000 成员的 #matrix:matrix.org）场景尤为关键。

2. **安全是 synapse-rust 的结构性改进**：Rust 编译时内存安全保证 + Argon2 密码哈希 + `zeroize` 敏感数据清理，从语言层面消除了整类内存安全漏洞。但 vodozemac 的密码学审计历史短于 libolm。

3. **Synapse 的核心优势在于成熟度**：多年的生产验证、安全审计、社区运维经验、以及与 Element 客户端的深度集成，是 synapse-rust 短期内无法复制的。

4. **synapse-rust 的业务扩展能力是独有价值**：好友系统、阅后即焚、信标位置、内容扫描、阿里云短信等私有扩展功能，使该项目不仅仅是协议重写，而是面向特定业务场景的增强实现。

5. **开发效率是 synapse-rust 的主要代价**：Rust 学习曲线陡峭、编译时间长、社区贡献门槛高。Python 的快速迭代能力在原型开发和社区贡献方面仍有优势。

6. **（v1.3 新增）协议正确性缺陷比"功能缺失"更值得优先处理**：本仓默认创建房间版本 11，
   但撤回事件仍按 v1–v10 的顶层 `redacts` 格式生成（`handlers/room/events.rs:959-982`），
   而 v11 消费方从 `content.redacts` 读取 → 本服务端发出的撤回可能在合规实现上不生效。
   同类还有：E2EE SAS 派生未用 HKDF、QR 验证为桩、`leak_detection` 为未编译死代码。
   这些是"已经声称支持、实际不符合规范"的项，风险高于"尚未实现"的 MSC4242/MSC4512（上游均实验性）。

7. **（v1.3 新增）数据一致性存在已知窗口**：`create_event_with_graph` 在无事务时先写 `events`
   再于事务外写 `event_edges`（`synapse-storage/src/event/create.rs:112-142`），联邦入库/补洞/backfill 走该路径；
   消息发送的 txn 去重标记在事件提交之后写入（`messages.rs:288-305`），标记失败时客户端重试可能产生重复事件。

8. **（v1.3 新增）"文档声称"与"代码实现"之间的漂移需要机制约束**：本次更新出现
   `api-reference.md`（不存在的文件）被当作来源、MSC4140 被标为 v1.161 新增、
   `include_redundant_members` 被当成 MSC4222 修复证据等。建议按 §12.5 D 类建立
   "数字必须可复现 / MSC 编号必须在语义表登记"的守卫。

### 12.3 适用场景建议

| 场景 | 推荐 | 原因 |
|------|------|------|
| **大规模生产部署** | synapse-rust | 性能和资源效率优势显著 |
| **快速原型开发** | Synapse | Python 迭代速度快 |
| **安全敏感场景** | synapse-rust | Rust 内存安全 + Argon2 |
| **资源受限环境** | synapse-rust | 内存占用低，Docker 镜像小 |
| **业务定制需求** | synapse-rust | 好友/阅后即焚/信标等扩展功能 |
| **社区贡献项目** | Synapse | 贡献门槛低，文档完善 |
| **合规审计要求** | Synapse | 安全审计历史长，CVE 记录完整 |
| **跨平台部署** | synapse-rust | 单二进制 + distroless 容器；⚠️ 依赖 glibc 动态库，**不是**静态链接，跨平台需按目标平台重新编译 |

### 12.4 风险提示

> v1.3 起，风险条目只保留**实测存在**或**明确未验证**的项；已证伪的旧条目直接删除（不保留"可能"表述）。

- synapse-rust 的性能数据多为**预期值**（§4/§10 中"预期"字样均指未实测），缺乏大规模生产实测验证；
  Docker 镜像与二进制体积本轮**未实测**（无可用产物）。
- **vodozemac 已升级**：`Cargo.toml` 要求 `>=0.10.0`，`Cargo.lock` 实锁 **0.11.0**。
  2026-02 Soatok 披露的 DH 贡献性问题（Olm 接受恒等元导致共享密钥全零）已在 0.10.0 修复；
  相关历史公告可引 `GHSA-c3hm-hxwf-g5c6`（CVE-2024-34063）与 `GHSA-j8cm-g7r6-hfpq`（CVE-2024-40640）。
  —— 上一版本写作"CVE-2026-XXXX 系列"属占位符，不应出现在正式报告。
- SDK 封装层存在已知 Bug（URL 重复前缀、batch 接口不存在等）。
- Worker 拓扑验证仍在建设中，水平扩展方案成熟度待验证。
- **协议正确性风险（v1.4 代码取证重排，优先级最高）**：
  - **【新 P0】联邦 `/send_join` 响应缺必需字段**：`federation/membership/join.rs:183-185`（v1）与 `:297-300`（v2）仅返回 `event_id`/`room_id`，**缺 `state` 与 `auth_chain`** → 合规远端拿不到房间状态，无法完成入房。（2026-09-23 独立复核：属实 —— 两处响应体确仅此二字段。）
  - **【新 P0】OIDC 回调提权**：`routes/oidc/sso.rs:215-232` 仅按 `localpart` 命中本地用户即签发令牌，未校验 OIDC subject 绑定；可接管任意同名账号（含 admin）。同仓 `routes/oidc/provider.rs:187-201` 已有正确检查。（2026-09-23 独立复核：属实，两条路径语义不一致。）
  - ~~**【新 P0】`soft_failed` 读路径无过滤**~~ → **已修（2026-09-23，commit `53c43a48` + `7d968f6d`）**。`53c43a48` 只补了 `/messages` 的**无游标**分支；`7d968f6d` 补齐全部面向客户端的读取面：`get_room_events_paginated_cursor` 的**两个带游标分支**（`/messages` 真实生产路径，回归用例证明修复前会返回 loser）、`get_room_events_after_stream_ordering`（sliding sync）、`find_event_by_timestamp`/`find_event_id_by_timestamp`（MSC3030）、`get_room_events_batch_inner`（`/sync`，谓词置于 ROW_NUMBER 之前）、`has_room_events_since`、`get_room_message_counts_batch`、四个 `search_*`、`get_unread_counts(_batch)`、sliding sync bump_stamp、`count_sent_messages`。**行为回归**用例 `test_soft_failed_events_hidden_from_all_consumer_read_paths`（`synapse-storage/src/event/db_tests.rs`）：写入 winner(loser) 后逐个读取方法断言 loser 不出现、winner 仍在；先红后绿。**未覆盖（有意）**：DAG/prev_events/auth/state-resolution/联邦/redaction 目标查找等**内部**读取面——它们必须看到该行；`friend_room` 与 relations 读的是 state/`event_relations`，不属于 soft-fail 事件类型（仅 `send_message_with_txn` 调用 `mark_event_soft_failed`）。**顺带修掉** `search_postgres_messages` 的 `ts_rank`(real) → `f64` 解码缺陷（生产 postgres provider 会 `ColumnDecode` 失败）。
  - **E2EE**：SAS 仍 4 处偏离规范（info 串缺公钥+顺序错 / emoji 仅 6 个且 decimal 被丢弃 / MAC 非 `hkdf-hmac-sha256.v2` / commitment 非 SHA-256）、SSSS 用 AES-256-GCM（规范要求 CTR + HMAC）、QR 为显式 fail-closed 不支持、`leak_detection` 模块已删除（详见 §7.2）。
  - **MSC4140**：无 EDU/联邦。
  - ~~撤回格式 × 房间版本~~ → **Phase 1 已修**（服务层按房间版本注入 `content.redacts`，PDU 不再重复写顶层）；关系性级联撤回仍未实现（见 §11.1 MSC3912 行）。
  - ~~Dehydrated device `/events` 仅 POST~~ → **Phase 2 已修**（GET + query 参数，`next_batch` 空页返回 `null`）。
- **数据一致性（2026-09-23 逐条复核后的状态）**：
  - ~~`create_event_with_graph` 半写窗口~~ → **Phase 1 已修**：无调用方事务时改用本地事务包裹 `events` + `event_edges`（`event/create.rs:111-148`，代码注释标 B8）。
  - **同一窗口的"另一半"**：`create_state_event_with_dag` 在无调用方事务时同样是先落 `events`、再于事务外插 `event_edges`（B8 因两处插入逻辑重复而漏掉）→ **2026-09-23 已修**（改本地事务）；红证明 `create_state_event_with_dag_rolls_back_event_when_edges_insert_fails`（先证明修复前确有孤立行，再证明修复后整笔回滚）。⚠️ 两处插入逻辑仍是重复实现，建议抽成单一 helper（铁律 2）。
  - txn 去重标记仍在事件提交之后（`room/messaging/messages.rs`）；Phase 2 的补偿（标记失败即 soft-fail 已提交事件）**因上一条 P0（读路径不过滤 `soft_failed`）而实际无效** —— 本项保持"未解决"。
  - ~~生产路径吞 DB 错误：`messages.rs:32` 的 `unwrap_or(0)`、`federation/transaction.rs:358-362` 的 `.ok().flatten()`、`membership/federation.rs:191-216/251-268` 的 warn-and-drop~~ → **2026-09-23 逐条复核确认均已修**：前两处全仓 0 命中；第三处改为 `return Err` + 事务回滚（`membership/federation.rs:219`）。v1.4 复核文本称这三处"仍存在"属**过时**，此处按实测更正。
- **未实现/未装配风险**：
  - **Content Scanner 整模块孤儿**（无存储、无 config、无构造点），非"仅缺存储层"。
  - **App Service 登录整体缺失**（无 `m.login.application_service` / `M_APPSERVICE_LOGIN_UNSUPPORTED`），
    以及 pushers、设备管理、虚拟用户调用 C-S、MSC4512 代理缺失。
  - ~~**`rc_reports` 专项限流缺失**~~ → **Phase 2 已修**（handler 内 per-user 桶 + 可配规则 + 守卫测试）；**LiveKit `ws_url` 仍为死配置**；稳定 `/_matrix/client/v3/profile/{userId}/{keyName}` 未注册。
- **未验证/待决策**：v12/v13 房间能否从"可 join/联邦"推进到"可创建"；`MSC4186/4262/4502/2409` 等代码中已出现的编号
  其语义是否与官方一致（须查 `MSC_SEMANTICS.md`，当前未登记）。
- **对齐基准更新建议**：当前最新稳定版为 v1.161.0（2026-09-15）。后续审查须记录 tag + CHANGES 链接 + 复核日期；
  **安全版本（如 v1.157.2 的 12 条 ELEMENTSEC 公告）必须逐条纳入 §7**，不得省略。

### 12.5 优化建议（v1.3 重写）

> **口径变更**：上一版本按 P0/P1/P2 + **人日估算**排列，且与仓库既有权威清单
> （`docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md`、`docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md`）
> **没有任何交叉引用**，构成第二份 backlog（违反 AGENTS.md 铁律 2/6 的精神）；同时把上游**实验性**的
> MSC4242/MSC4512 列为"P0 阻断性"，而漏掉了本次实测发现的协议正确性缺陷。
> v1.3 起改为：**只描述差异 + 指向权威清单**，每条给出证据与验收判据，不再给人日估算（估算无依据会掩盖不确定性）。
> 完整版本见 `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md` §7。

**分层原则**：A 类（门禁与事实，已完成）→ B 类（协议正确性，本轮实测发现，优先）→ C 类（功能补齐与收敛，按上游成熟度定级）→ D 类（把文档可信度变成守卫）。

**B 类：协议正确性（建议优先于任何新功能）**

| 编号 | 现象与证据 | 动作 | 验收判据 | 上游关联 |
|------|------------|------|----------|----------|
| B1 | 默认 v11，撤回仍写顶层 `redacts`（`room_versions.rs:89,113` vs `handlers/room/events.rs:959-982`） | 撤回创建按房间版本分支写 `content.redacts`；修正过期注释 | v11 房间 `content.redacts` == 目标 id；v10 仍在顶层；互操作冒烟通过 | v1.161 #19782 |
| B2 | 联邦 `make_*`/`send_*` 的 `membership` 校验：**`send_*` 已校验**（`federation/membership/mod.rs:128-137` 按 `expected_membership` 比对），`make_join` 为无 body 的 GET | 按上游 diff 复核 `make_knock`/`make_leave` 是否同源；同源则补测 | `send_leave` 带 `membership=join` → 400 | v1.161 #20189（作用面 `[未验证]`） |
| B3 | 举报端点无 `rc_reports` 专项限流（`directory_reporting.rs:226-266`） | 补限流桶（客户端举报端点同理） | 429 + `retry-after` 有测试 | v1.161 #20036 |
| B4 | Dehydrated `/events` 仅 POST + body 游标（`assembly.rs:216-217`） | 改为 GET + query；`next_batch` 末页可为 null | 契约测试断言 GET 与 null | v1.157 #19896 |
| B5 | `create_event_with_graph` 半写窗口（`event/create.rs:112-142`） | 并入同一事务或给出补偿 | 注入 `event_edges` 失败后无孤立 `events` 行 | 新增 |
| B6 | txn 去重标记在事件事务外（`messages.rs:288-305`） | 标记与事件同事务，或"标记先行 + 幂等回填" | 注入标记失败后重试，房内仅 1 条事件 | 新增 |
| B7 | 4 处吞 DB 错误 | 改错误传播/fail-closed | 每点有失败用例证明返回错误 | CLAUDE.md 已知坑 |
| B8 | 搜索索引死代码 + 在线仅 `m.room.message`（`search_index.rs` 无调用者；`event/search.rs:170,210,311`） | 接线或删除；统一索引事件类型集合 | 主题可默认搜到，或明确声明不支持 | v1.161 #20119 |
| B9 | Profile：停用用户自定义字段 404、稳定路由缺失、非对象 500 | 分开"存在/停用"判定；注册稳定路由或声明不支持；非对象→400 | 三条各自断言状态码 | v1.161 #20149/#20172 |
| B10 | v1.157.2 的 12 条 ELEMENTSEC 公告未做同类性判定 | 逐条产出"受影响/不受影响 + 证据" | 对照表 + 结论 | v1.157.2 |

**C 类：功能补齐与收敛（重新定级）**

| 项 | 原定级 | 新定级 | 理由 |
|----|--------|--------|------|
| E2EE SAS 对齐 HKDF + 真实 MAC 校验 | 未列（误判 ✅ 完整） | **高** | 影响客户端验证互操作；接受任意 MAC 是安全弱化 |
| E2EE QR 实现或声明未实现 | 未列 | **高** | 当前为桩（复用公钥 + 空签名），文档称"完整"属误报 |
| `leak_detection` 接入或删除 | 误判"已实现" | **高** | 未编译 + 启用即编译失败 + 桩计数 + schema 列缺失（铁律 1） |
| OIDC `validate_id_token_claims` 接线 | 未列 | **高（安全）** | 死代码，声明校验未生效 |
| Content Scanner 装配或删除 | P0"缺存储层" | **中（决策项）** | 现状是孤儿模块；先决定是否上线该功能 |
| MSC4140 联邦（EDU） | ✅ 已对齐 | **中** | 单机真实，联邦缺失，需按草案确认是否必须 |
| 死字段清理（3 处 "Reserved/constructor parity"） | 未列 | **中** | 违反铁律 1 |
| MSC4242 State DAG | P0 阻断性 | **低（观察项）** | 上游实验性 + 无房间版本启用；先修正 `dag.rs` 不实注释 |
| MSC4512 AS 代理 | P0 阻断性 | **低（观察项）** | 上游实验性、opt-in |
| SMS 厂商专用实现（Twilio 等） | P1 | **低** | 已有 `SmsProvider` trait + 通用 HTTP provider |
| v12/v13 创建支持 | 未列 | **待决策** | 影响与"默认 v11 + v12 房间已在联邦中存在"的兼容边界 |

**D 类：把"文档可信度"变成守卫**

1. 文档中的端点/文件/模块数必须来自 `ROUTE_CONTRACT.md` 或可复现命令，并用**故意写错的数字**证明检查会变红（铁律 8）。
2. 文档中的 `MSC\d+` 必须出现在 `MSC_SEMANTICS.md`；新增编号须同时登记语义。
3. 引用人工文档（如 `API_COVERAGE_REPORT.md`）必须带时间戳；过期条目不得作为"缺失"证据。
4. 每条 ✅ 必须带 `路径:行号` 或测试名；无法证实的写 `[未验证]` 并说明阻塞原因。

**实施建议**

1. **先 B 后 C**：B 类中 B1/B2/B5/B6/B7 属正确性与数据一致性，优先于任何新 MSC 开发。
2. 每个修复附**能变红的测试**（注入失败、版本分支、篡改 MAC 等），避免"报通过的门禁未必在工作"。
3. 修复后同步更新本文档 §11/§12.4 与 `MSC_SEMANTICS.md`（若涉及编号语义）。
4. 持续对齐：每季度复核一次，**必须包含上游安全版本**；记录 tag + CHANGES 链接 + 复核日期。

---

## 13. v1.3 复核修正记录（2026-09-22）

| 类别 | 修正内容 |
|------|----------|
| 门禁 | 修复 docs-quality-gate 拼写回归（`.aspell.ignore.txt` +10 词；修复前 `check_doc_spelling.sh` exit 1，修复后 exit 0） |
| 伪造引用 | 删除 `docs/synapse-rust/api-reference.md`（文件不存在）与 "656 端点 / 48 模块"；改挂 `ROUTE_CONTRACT.md`（1,151 条 / 66 模块） |
| 计数 | .rs 1020 / 439,130 行；workspace members 8 + 根 = 9；common 72、federation 20、web 162（routes 144）、根 `src/` 38；tests 230（106/113/3/5）；benches 5；docs 221；docker 212；migrations 1 |
| 版本 | tokio 实锁 1.53.1；vodozemac 实锁 0.11.0；insta 实锁 1.48.0；quickcheck 仅用于输入校验 |
| 事实 | 删除"sqlx 编译时验证"（static 61 / dynamic 2147 ≈ 2.8%）；删除"静态链接 + musl"（gnu + distroless 动态库）；修正 `server` 特性（已删除）、`web/routes/e2ee/backup.rs` 路径 |
| §7.2/§11.1 | E2EE 由 "✅ 完整" 降为 PARTIAL（SAS 非 HKDF、QR 桩、泄漏检测未编译）；MSC4140 由 "✅ 已对齐" 降为 PARTIAL（无联邦/EDU）；MSC3912 行由"待验证"改为明确的 v11 撤回格式缺陷 |
| §11.2/§11.3 | Content Scanner 由"缺存储层"改为"未装配孤儿模块"；SMS 补 `HttpSmsProvider`；privacy 路径纠正；Dehydrated `/events` 标注端点漂移；Search/Relations/Admin/App Service 降级为部分对齐 |
| §11.1 v1.161 表 | 8 项"待核查"全部实测判定；其中 #20169 的旧 ✅ 属误判（机制不同）；#20036、#20180 确认缺失 |
| §12.4 | 风险改为只保留实测/明确未验证项；CVE 占位符替换为真实 GHSA/CVE 编号 |
| §12.5 | 重写为 A/B/C/D 分层，删除人日估算，改为指向权威清单并按验收判据验收 |
| **代码修复（Phase 1）** | **B1** v11+ 撤回目标写入 `content.redacts`（服务层唯一写入口）+ PDU 不再重复写顶层 `redacts`；**B8** `create_event_with_graph` 无事务分支改单事务；**B10a** `send_message` 传播 `origin_server_ts` 读取错误；**B5** 修正过期房间版本注释。验证证据见 `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md` §10（原计划文件位于 gitignored 的 `docs/superpowers/plans/`，不作为持久引用） |
| **代码修复（Phase 2）** | **B10b** 联邦 gap-fill 查重吞错；**B11** 默认搜索面纳入 `m.room.name`/`m.room.topic`；**B3** 举报端点 per-user `rc_reports` 限流（可配 + 守卫）；**B9** 事务去重标记失败时 soft-fail 已提交事件。证据见 `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md` §11–§12（原计划文件位于 gitignored 目录，不作为持久引用）。**Phase 2 全部完成**：B3/B4/B9/B10b/B10c/B11/B13；遗留缺陷另记：`scripts/api_test/scan_handler_schemas.py` 的 `ROOT` 为硬编码绝对路径（会写错工作树）、ledger `query_params` 字段无消费方 |
| **代码修复（Phase 3，安全）** | **C1** SAS 规范对齐：`derive_sas` 改用 HKDF-SHA256（`synapse-e2ee/src/verification/service.rs:106`，带已知答案测试 `derive_sas_matches_hkdf_sha256_known_answer`）、`confirm_sas` 以 `synapse_common::crypto::secure_compare` 校验 HMAC（`:367`）、删掉随机 SAS 兜底（fail-closed）；**C2** QR 不再伪造载荷，改为 `ApiError::unsupported`（`:409,423`）；**C3** 删除从未编译的 `synapse-e2ee/src/leak_detection/` 死模块；**C9** OIDC `id_token` 校验失败改 fail-closed（`ApiError::unauthorized`）+ 授权 URL 携带 `nonce` + 删除绕过校验的死函数（`validate_id_token_claims` 全仓 0 命中）。提交 `a6f797f3` / `fda1317f` / `db570918` / `c010135c`；复核记录见 `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md` §13 |
| **v1.4 代码取证复核（2026-09-22）** | **推翻本文档 5 处旧结论**（详见 §14.1）：① SAS 由"SHA256 派生 + 接受任意非空 MAC"→ 实为 `derive_sas` 已 HKDF（`verification/service.rs:105-113`）、`confirm_sas` 已校验 MAC（`:316-375`），但**仍 4 处偏离规范**；② QR 由"桩"→ 实为**显式 fail-closed** `M_UNSUPPORTED`（`:403-424`）；③ `leak_detection` 由"未编译死代码"→ 实为**目录已删除**；④ `create_event_with_graph` 半写窗口由"仍存在"→ 实为 **Phase 1 已修**（本地事务，`event/create.rs:111-148`）；⑤ `validate_id_token_claims` 由"从未被调用"→ 实为**已接线**，真实缺陷在 `routes/oidc/sso.rs:215-232`。**新增 3 条 P0**（§14.2）；§11.1 联邦协议由 ✅ 降为 PARTIAL；§7.2 SSSS 新增非合规判定。<br>（2026-09-23 交叉复核：①②③④ 与 3 条新 P0 均**独立复现属实**；**⑤ 已过时** —— Phase 3 C9 已把该函数整体删除，当前树 `validate_id_token_claims` 0 命中，`exchange_code` 改为 fail-closed；但其指出的 `routes/oidc/sso.rs:215-232` 提权面**仍然存在**。） |
| **本轮（2026-09-23）** | **文档可信度变成门禁**：新增 `tests/unit/doc_credibility_guard_tests.rs` —— 本文档（1）引用的仓库相对路径必须存在、不得引用 gitignored 的 `docs/superpowers/plans/`；（2）声明的 route 条目数与模块数必须等于 `docs/synapse-rust/ROUTE_CONTRACT.md`；两个纯谓词各有红证明（把 v1.2 的"伪造引用/过期计数"原文喂进去必须被判违规）。据此前置修正：§13 两处 gitignored 计划引用改为指向复核报告；`docs/*.md` 这类 glob 与"文档有意记录某文件不存在"的否定陈述分别按过滤/白名单处理。**代码优化**：删除 LiveKit `ws_url` 死配置（`synapse-common/src/config/voip.rs`，从未被读取）；关闭 `create_state_event_with_dag` 的半写窗口（B8 因两处插入逻辑重复而漏掉的并行路径，改为本地事务，红→绿证明见 `create_state_event_with_dag_rolls_back_event_when_edges_insert_fails`）；修 `synapse-storage/src/server_notification/repository.rs` 5 处 `created_by` 缺 `Some(..)`（**main 上 CI lib 批次仍因此红**）。**更正本报告自身的过度声明**：上一版把 E2EE SAS 写成"已按规范对齐"不成立 —— 经逐行复核（info 串缺公钥且 txn 占位、emoji 仅 6 个且 decimal 被丢弃、MAC 非 `hkdf-hmac-sha256.v2`、commitment 用全零密钥 HMAC），仍 **4 处偏离规范**，§7.2/§11.1 已按 v1.4 复核改写。**计数重测（口径改为可复现的 `git ls-files`）**：`.rs` **1,019 个 / 442,451 行**；`docs` **188**（其中 `*.md` 155）；`docker` **58**；`tests` **292**（unit 132 / integration 128 / e2e 5 / performance 5 / 其余 22）；`benches` 5；`migrations` 3；per-crate 未变（common 72 / federation 20 / web 162、routes 144 / 根 `src/` 38）。⚠️ 旧版对 `docs`/`docker`/`tests` 用 `find` 统计，会随 gitignore 与本地产物漂移（同一提交在不同 worktree 得出 221/212/230 与 188/58/292 两套数）—— 故统一改为 `git ls-files` |

---

## 14. v1.4 复核保留摘要 + v1.5 修复进度（2026-09-23 续）

### 14.1 v1.4 复核的 5 处旧结论更正（保留摘要）

| 旧结论（v1.3 及更早） | 实测结论 | 证据 |
|----------------------|----------|------|
| SAS 用 `SHA256(secret‖info)` 派生，非 HKDF | **已更正**：`derive_sas` 实为 HKDF-SHA256（无 salt、info 为上下文串、取 6 字节） | `synapse-e2ee/src/verification/service.rs` |
| `confirm_sas` 接受任意非空 MAC | **已更正**：校验 MAC 非空、要求 `keys`/`peer_pubkey`、`secure_compare` 比对，不符 403 | 同上 |
| QR 验证为桩（复用同一公钥 + 空 `signature`） | **已更正**：显式 fail-closed，返回 `M_UNSUPPORTED` | 同上 |
| `leak_detection` 是"未在 `lib.rs` 声明"的死代码 | **已更正**：整个目录**已删除**，即该能力不存在 | `glob 'synapse-e2ee/src/leak_detection/**'` 无结果 |
| OIDC `validate_id_token_claims` 从未被调用（死代码） | **已更正**：该函数已被调用；真实缺陷在回调侧未校验 subject 绑定。**2026-09-23 再更正**：Phase 3 C9 已删除该函数，`exchange_code` 改为 fail-closed | `synapse-web/src/routes/oidc/sso.rs` |

### 14.2 v1.4 新增 3 条 P0 的当前状态

| # | 问题 | 状态（2026-09-23） |
|---|------|--------------------|
| P0-1 | 联邦 `/send_join` 响应缺 `state`/`auth_chain` | ❌ **未修（本轮取证后确认工作量为整条写入管线，见 §14.4）** |
| P0-2 | OIDC 回调提权（按 localpart 签发令牌，无 subject 绑定） | ✅ **已修**：回调路径写入并复用 OIDC 绑定（`fe35fb0a`），账号接管判定抽成纯函数并补判定表用例（`0a633b89`） |
| P0-3 | `soft_failed` 无任何读路径过滤 | ✅ **已修**：`53c43a48`（时间线无游标分支）+ `7d968f6d`（其余全部消费者读取面 + 行为回归用例），详见 §12.4 |

### 14.3 本轮（2026-09-23 续）已完成并验证

| 项 | 内容 | 提交 | 验证 |
|----|------|------|------|
| 1 | `soft_failed` 其余消费者读取面（pagination 带游标分支 / batch(`/sync`) / search 四条路径 / unread / sliding sync / 消息计数）+ 行为回归用例；顺带修 `ts_rank` 的 `real`→`f64` 解码缺陷 | `53c43a48`、`7d968f6d` | `cargo nextest run -p synapse-storage --lib -E 'test(soft_failed_events_hidden)'` 先红（cursor 分支返回 loser）后绿 |
| 3（一半） | **SSSS 对齐 `m.secret_storage.v1.aes-hmac-sha2`**：HKDF（salt=32×0，info=空串/secret name）、AES-256-CTR（128 位大端计数器）、encrypt-then-MAC、16 字节 IV 且 bit 63 清零、无填充 base64、`decrypt_secret` 先验 MAC；删除不可解的 MSC2697 curve25519 路径（改为 fail-closed 400）；`create_key`/`encrypt_secret`/`decrypt_secret` 改关联函数 | `a2375743` | `synapse-e2ee --lib` SSSS 27 项全绿，含 **NIST SP 800-38A F.5.5 CTR-AES256 已知向量**、篡改密文→403、错误 secret name→403、非 32 字节密钥拒绝 |
| 5（B2） | **签名前校验 `make_join`/`make_leave` 模板**（上游 #20189 / 规范 PR #2284）：新增纯函数 + join/leave 接线，失败 400 且不取签名密钥、不调 `send_*` | `cd7b97bb`（被并发写入者合并） | 8 项表驱动单测 + 端到端：mock 返回 `membership:"ban"` ⇒ 400 且 `send_join_call_count()==0` |
| 5（B9c） | MSC4133 非对象文档：写入口加形状守卫（400）、读取面 `internal`→`bad_request`；data type 常量收敛到一处 | `7fc49e06` | 3 项单测 + 集成 `test_msc4133_non_object_document_is_bad_request_not_server_error` 实跑通过（21.7s） |

### 14.4 遗留阻塞与后续计划

**item 2（P0-1 `/send_join` 的 `state`/`auth_chain`）—— 根因与最小正确实现**

本轮取证发现该缺陷比"补两个响应字段"深：本仓**本地创建的事件从来不落 PDU 图元数据**。

- 房间创建（`synapse-services/src/room/lifecycle/create.rs` 的 `m.room.create`/`power_levels`/`join_rules`/`member` 等）走**普通 `create_event`**：`depth`/`prev_events`/`auth_events` 为 `NULL`，且**完全没有签名/哈希**（该文件不调用 `sign_and_broadcast_event`）。
- `sign_and_broadcast_event`（`room/messaging/service.rs`）虽然把 `prev_events` 放进被签名的 PDU，但只回写 `signatures`/`hashes`，**不回写** `depth`/`prev_events`/`auth_events`。
- 因此 `state` 数组里的多数事件既无 `auth_events` 也无 `depth`，而 `hashes.sha256` 覆盖这些字段——**事后补造会作废哈希与远端签名**，不能只在响应侧修。
- 存储里也没有 auth 边表：`event_edges` 的 `is_state` 区分的是"房间 DAG vs MSC4242 状态 DAG"，**不是 auth vs prev**；auth 边只存在于 `events.auth_events` JSONB。

最小正确实现（建议单独分支/里程碑）：① 为本地事件补齐创建期 PDU 管线——选 `auth_events`（按房间版本的 auth 事件选择规则）+ 算 `depth` + 签名/哈希 + 用 `create_event_with_graph` 落库，覆盖房间生命周期、状态、消息、成员等**全部**本地写入口；② 新增 storage 全 PDU 读取（`depth/prev_events/auth_events/hashes/signatures/unsigned`）与 auth 链闭包查询；③ 统一 `serialize_full_pdu`；④ `/send_join` v1 返回 `[200, {...}]`（v1 **必须**是二元素数组）、v2 返回裸对象，二者均含 `state`/`auth_chain`/`event`/`members_omitted:false`；⑤ 顺带修 `make_join` 模板（补 `origin`/`origin_server_ts`）与 `SendJoinResponse.origin` 改为 `Option`（v1.14 起规范已删除响应中的 `origin`，当前客户端结构体把它当必填，解析真实 Synapse 响应会失败）。

**item 3（SAS 4 处偏离）—— 规范修法被私有 API 形状阻塞**

`/keys/device_signing/verify_*` 是本仓私有的非规范 REST 面（`synapse-web/src/routes/verification_routes.rs`），其 `mac` 是单个字符串、`keys` 是 `key_id → 公钥值` 的映射，且**没有算法字段**；规范 `hkdf-hmac-sha256.v2` 的 key-list MAC 需要对"排序后逗号分隔的 `{algorithm}:{keyId}` 列表"做 MAC。因此偏离③（MAC）与④（commitment = `SHA-256(公钥‖start content 规范 JSON)`，且当前**从不校验** commitment）无法在现有 API 形状下正确实现，需先重构该私有面为规范形状（或明确声明不支持 SAS）。偏离①（info 串缺双方公钥且 txn 位置错）与②（emoji 6 个且分组错、decimal 被丢弃、**emoji 表本身与规范表不一致**）可在现有形状内修，但需与 API 重构一起验收。

**item 5 其余子项**

- **B8（搜索死代码）**：`synapse-storage/src/search_index.rs`（`SearchIndexStorage` 等）**无任何生产调用者**，`search_index` 表永远为空；`search_postgres_messages`/`search_room_postgres_messages` 仅经 `search_messages` 可达，而 `search_messages` 只有测试调用者。建议按铁律 1 删除模块 + 新增前向迁移 drop 表。**未做**（涉及 schema 变更，需单独决策/迁移）。
- **B9(a)/(b)**：停用用户在 MSC4133 上返回 404 而标准 `/v3/profile` 返回 200（两面对"存在 vs 停用"判定不一致）；稳定 `/_matrix/client/v3/profile/{userId}/{keyName}` 未注册而 capability 已声明 `m.profile_fields`。**未做**（(b) 需重生成 ledger/快照/契约 fixture，或改为不再声明该 capability）。
- **B10**：见 §14.5 对照表。
- **C 类死字段**：实测为 **7 处**（不是 3 处）——`room/lifecycle/service.rs`、`room/state/service.rs`、`room/membership/service.rs`、`room/service.rs`（`event_writer`，仅 `burn-after-read` 特性下有 1 个读取点）、`friend_room_service/models.rs`、`admin_registration_service.rs`、`admin_security_service.rs` 的 `user_service`/`event_writer`，均为 `#[allow(dead_code)]` + "Reserved/constructor parity"。另见 `user_service.rs` 的 `event_reader` + `set_event_reader`（0 调用者）。**未做**（需删除字段 + 构造参数 + wiring + 测试构造点，并新增可红的守卫测试）。

### 14.5 B10：Synapse v1.157.2 ELEMENTSEC 公告对照（2026-07-28，共 **11** 条而非 12）

> 计数三处交叉验证一致（GitHub Releases API 正文、tag `v1.157.2` 的 `CHANGES.md`、仓库 advisory 列表 `patched_versions: ["1.157.2"]`）。
> 均无 CVE 编号。"12"的来源疑为 ELEMENTSEC-2026-1740 描述中交叉引用的旧公告 `GHSA-rfq8-j7rh-8hf2` 被一并计数。

| # | 公告 | 组件 | 本仓判定 | 主要证据 |
|---|------|------|----------|----------|
| 1 | ELEMENTSEC-2026-1071 / GHSA-fp53-rw9v-hcf9 | push rules 数量/体积无上界 → 磁盘/内存耗尽 | ⚠️ **可能受影响/待深查** | push rule 走独立表（`client_push_service.rs` → `synapse-storage/src/push/mod.rs`），未发现 per-user 条数上限；唯一的 64KB 限制在 account-data 路径，pushrules 路由不经过 |
| 2 | ELEMENTSEC-2024-1520 / GHSA-rgv2-84w7-5j9p | to-device EDU 发送方伪造 | ✅ 不受影响 | `synapse-web/src/federation/edu.rs` 要求 `user_matches_origin(sender, origin)`，否则丢弃 EDU |
| 3 | ELEMENTSEC-2026-1717 / GHSA-27p5-4f45-gx76 | `/get_missing_events` 跨房泄露 | ✅ 不受影响（有纵深防御备注） | 路由先 `validate_federation_origin_can_observe_room`；storage 最终查询 `WHERE room_id = $1 AND event_id = ANY($2)`（CTE 本身未按房间限定，建议补注释） |
| 4 | ELEMENTSEC-2026-1721 / GHSA-95fh-hv8c-chvq | 联邦错误回传导致客户端销毁加密状态 | ✅ 不受影响 | `From<FederationClientError> for ApiError` 一律映射为 500 `M_UNKNOWN`，远端状态码被丢弃 |
| 5 | ELEMENTSEC-2026-1729 / GHSA-cjh7-rcpx-xpf8 | 房间别名重定向 | ⚠️ **本地可劫持（确认）** | `synapse-storage/src/room/mod.rs` 的 `ON CONFLICT (room_alias) DO UPDATE SET room_id = EXCLUDED.room_id` 会静默改指；路由无"别名已被占用"预检。联邦向量待深查 |
| 6 | ELEMENTSEC-2026-1740 / GHSA-6wjm-9p2x-gvpm | `multipart/form-data` Content-Type 大小写绕过 DoS 缓解 | ⚠️ **待深查** | 本仓无手写 Content-Type 检查（multipart 仅经 axum `Multipart`，`routes/voice.rs`）；有全局 body 上限，但解析器内部行为属上游 |
| 7 | ELEMENTSEC-2026-1714 / GHSA-qcjr-46gf-7f4r | `/get_event_auth` 缺已入房校验 | ✅ 不受影响 | 先 `validate_federation_origin_can_observe_room`，再按 `room_id` 限定取事件 |
| 8 | ELEMENTSEC-2026-1718 / GHSA-r66v-qhwx-8rg4 | `/timestamp_to_event` 缺成员校验 | ✅ 不受影响 | 格式校验后调 `validate_federation_origin_can_observe_room` |
| 9 | ELEMENTSEC-2026-1751 / GHSA-jhcg-5392-5mjw | Sliding Sync 畸形响应（非法房间名/头像/资料） | ✅ 不受影响（结构上不可能） | 相关字段均为 `Option<String>`，类型系统保证不会输出错误 JSON 类型 |
| 10 | ELEMENTSEC-2026-1703 / GHSA-vh4c-pqh4-w3wq | 多余路径段被忽略 → 限流绕过 | ✅ 应用层不受影响 / ⚠️ 代理层待查 | axum 0.8 精确 `{param}` 匹配、无通配路由，fallback 404 `M_UNRECOGNIZED` |
| 11 | ELEMENTSEC-2026-1760 / GHSA-hgcg-p9gx-fq5f | 尾随后缀被接受 → nginx 规范化代理绕过 | ⚠️ **待深查（代理配置）** | `docker/deploy/nginx/` 使用前缀 `location`，无 `merge_slashes`；需运维侧复核，Rust 代码内不可证 |

**结论**：需要动作或明确决策的是 #1（push rule 上限）与 #5（别名劫持）；需要代理/基础设施复核的是 #6 与 #11；其余 7 条本仓已有对应守卫。

### 14.6 并发写入者事故记录（流程）

本轮作业期间，另一个 agent 会话（`.workbuddy`，Phase 3 closeout / SIGTERM 内存门禁 / E2EE v2 优化）在**同一工作树、同一分支**上持续 `git add` 提交，把本轮在途改动至少 3 次扫进其提交：`bc1501f9`（"format db_tests.rs"，含本轮的 soft_failed 回归用例）、`d6c55ed8`（"T2EE-001 cross-signing keys"，含本轮 SSSS 的 `models.rs`/`Cargo.toml`/`Cargo.lock`）、`cd7b97bb`（"Sync: 处理并发会话遗留的 federation 相关变更"，含本轮 B2 校验）。内容未丢，但**提交信息与内容不符**，违反 AGENTS.md 铁律 9（同一工作树同一时刻只允许一个写者）。本轮已通过"每完成一项立即逐路径 `git add` + 提交"把暴露窗口压到最小；后续并行作业必须改用独立 `git worktree`。

---
> **声明**: 本报告基于 synapse-rust v6.2.0 工作树（`HEAD cd7b97bb` + 未提交改动）与 Synapse v1.161.0
> （2026-09-15，`release-v1.161` CHANGES.md）编写。**性能与资源数据凡标"预期/未实测"者均未经过生产验证**，
> 不得作为容量规划依据。所有结论遵循"代码优先"原则；凡未实测项显式标注，不以"需确认"充当结论。
> **审查方法**：以可复现命令与 `路径:行号` 为唯一证据形式——
> 计数与版本来自 `git ls-files` / `Cargo.toml` / `Cargo.lock`；
> 路由与模块口径来自 `docs/synapse-rust/ROUTE_CONTRACT.md`；
> MSC 语义以 `docs/synapse-rust/MSC_SEMANTICS.md` 为准；
> 上游条目来自 `element-hq/synapse` `release-v1.161` CHANGES.md；
> 逐条证据与命令清单见 `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md`。
