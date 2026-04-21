# synapse-rust 系统性审查与优化主方案（2026-04-12）

> 文档定位：参考 `matrix-js-sdk` 的系统性审查主方案结构，基于当前 `synapse-rust` 仓库实际代码、配置、工作流与迁移资产重写  
> 适用范围：`src/`、`tests/`、`migrations/`、`docker/`、`.github/workflows/`、`docs/`  
> 目标：把“零散问题清单”升级为“可执行、可量化、可验收”的治理路线图

---

## 1. 审查目标与方法

### 1.1 目标

- 对 `synapse-rust` 进行系统化诊断，覆盖架构、代码质量、数据库迁移、安全、测试门禁、部署与文档治理。
- 纠正上一版 `OPTIMIZATION.md` 中与仓库现实不完全一致的结论，避免继续放大误判。
- 形成“问题清单 + 风险矩阵 + 分阶段优化路线图”，为后续迭代提供统一基线。

### 1.2 本次证据来源

- 项目元信息：`Cargo.toml`、`README.md`、`TESTING.md`
- 持续集成：`.github/workflows/ci.yml`、`.github/workflows/db-migration-gate.yml`、`.github/workflows/docs-quality-gate.yml`
- 部署配置：`docker/docker-compose.yml`、`docker/docker-compose.prod.yml`、`docker/docker-compose.dev-host-access.yml`、`docker/config/homeserver.yaml`、`docker/config/homeserver.local.yaml`、`homeserver.yaml.example`
- 迁移资产：`migrations/00000000_unified_schema_v6.sql`、`migrations/20260409090000_to_device_stream_id_seq.sql`
- 高风险代码：`src/federation/key_rotation.rs`
- 安全门禁配置：`cargo-audit.toml`
- 回归验证：`tests/integration/database_integrity_tests.rs`
- 仓库统计：基于当前磁盘代码对 `src/`、`tests/`、`docs/`、`migrations/` 的实际扫描

### 1.3 范围说明

- 本文是“系统性优化主方案”，不是一次性完成所有重构的交付清单。
- 风险优先级采用 `P(概率) × I(影响)` 的方式表达，并映射到 `P0/P1/P2/P3`。
- 结论以当前仓库可见证据为准，不直接继承容器镜像外部环境中的推断。

### 1.4 阶段 0 已落地进展（截至 2026-04-12）

- `Q2` 已完成首轮止血：`src/federation/key_rotation.rs` 已改为按 `SigningKey` 完整字段加载现存联邦签名密钥，并补充了现存记录加载回归测试。
- `D2` 已完成专项修复：`migrations/20260409090000_to_device_stream_id_seq.sql` 已改为基于 `current_schema()` 执行，并修正空表与重复执行场景下的 `setval` 语义。
- `D2` 已完成专项补测：`tests/integration/database_integrity_tests.rs` 已补充空表、已有数据、重复执行三类回归测试入口。
- `S1/S2` 已完成默认部署收敛：主 `docker/` 与 `docker/deploy/` 两套 compose 入口都不再默认暴露 PostgreSQL/Redis 到宿主机，Redis 已改为强制密码认证；主机访问迁移到显式的 `docker-compose.dev-host-access.yml` override。
- `S3` 已完成门禁升级：CI 中已通过 `scripts/run_cargo_audit.sh` 执行阻断型安全审计，`cargo-audit.toml` 负责集中管理豁免条目，并额外阻止新增 `rand::rng()` 用法绕过 `RUSTSEC-2026-0097` 的临时缓解边界。
- `T2` 已完成根级主文档覆盖：`OPTIMIZATION.md` 已纳入文档质量门禁。
- 阶段 0 剩余重点已收敛到阶段结果固化、后续架构拆分与安全收敛深化。

---

## 2. 当前基线（量化）

### 2.1 规模基线

| 指标 | 当前值 | 说明 |
| :-- | :-- | :-- |
| `src/**/*.rs` | 417 | 主代码规模已进入大型 Rust 服务区间 |
| `tests/**/*.rs` | 173 | 测试资产丰富，但需要继续核对实际接线路径 |
| `benches/**/*.rs` | 9 | 已有专项性能基准入口 |
| `migrations/*.sql` | 38 | 迁移资产较多，治理复杂度高 |
| `.github/workflows/*.yml` | 8 | 已具备较细粒度治理能力 |
| `docs/**/*.md` | 134 | 文档资产丰富，但单一事实源压力很大 |

### 2.2 体量与复杂度信号

当前仓库中最大的几个 Rust 文件如下：

| 文件 | 行数 | 主要信号 |
| :-- | --: | :-- |
| `src/web/routes/handlers/room.rs` | 4256 | 路由与房间领域逻辑聚合过重 |
| `src/common/config/mod.rs` | 4126 | 配置模型与兼容层累积明显 |
| `src/web/routes/federation.rs` | 2474 | 联邦路由入口过厚 |
| `src/web/middleware.rs` | 2199 | 中间件职责集中 |
| `src/web/routes/admin/room.rs` | 1826 | 管理接口按领域仍可继续拆分 |
| `src/auth/mod.rs` | 1789 | 认证逻辑聚合偏重 |
| `src/services/database_initializer.rs` | 1715 | 数据库初始化兼容逻辑偏多 |
| `src/services/sync_service.rs` | 1639 | 同步服务持续膨胀 |
| `src/common/error.rs` | 1592 | 错误模型正在演化为大型中心模块 |
| `src/services/room_service.rs` | 1584 | 房间服务边界仍偏宽 |

### 2.3 质量信号

基于当前仓库扫描：

| 指标 | `src/` | `tests/` | 含义 |
| :-- | --: | --: | :-- |
| `.unwrap(` | 688 | 2920 | 生产代码中的显式崩溃点数量仍高 |
| `.expect(` | 91 | 842 | 生产路径里仍有较多假设式断言 |
| `panic!(` | 24 | 52 | 部分路径仍依赖直接中止 |

补充判断：

- `TODO/FIXME/HACK/XXX` 在 Rust 代码中的显式标记并不多，但这并不代表技术债低，更多说明技术债是通过结构膨胀和运行时假设体现出来的。
- 当前最值得优先治理的，不是“注释标记数”，而是“高体量模块 + 高崩溃式错误处理 + 多入口治理分散”。

### 2.4 测试与门禁基线

从 `ci.yml` 与 `TESTING.md` 可见，当前主门禁已经包含：

- `cargo fmt --all -- --check`
- `cargo clippy --all-features --locked -- -D warnings`
- `cargo test --doc --locked`
- `bash scripts/run_ci_tests.sh`
- 仓库治理脚本，如 `check_no_placeholder_strings.sh`、`detect_shell_routes.sh`
- 静态 schema contract / table coverage 检查

当前仍存在的门禁特点：

- 安全审计已改为经 `scripts/run_cargo_audit.sh` 阻断执行，仓库内的 `cargo-audit.toml` 继续作为豁免条目的集中配置；同时新增了 `rand::rng()` 的门禁检查，避免临时豁免失控。
- 根级 `OPTIMIZATION.md` 已纳入文档质量门禁，避免主方案继续游离于自动校验之外。
- 数据库迁移治理较强，单独拥有 `db-migration-gate.yml`，但“迁移存在”不等于“线上迁移场景已闭环”。

### 2.5 部署与配置基线

从 `docker/docker-compose.yml`、`docker/config/homeserver.yaml` 与 `homeserver.yaml.example` 看：

- 配置已普遍改为环境变量插值，不应再笼统下结论为“数据库密码硬编码在正式配置中”。
- `server.name`、`server.server_name`、`federation.server_name` 确实多处出现，但当前默认都绑定同一环境变量 `SERVER_NAME`，并非上一版文档描述的静态冲突。
- 默认 compose 已不再将 PostgreSQL 与 Redis 端口直接暴露到宿主机，开发态主机访问改为显式的 override 文件。
- Redis 访问已切换为强制密码认证，应用侧 `REDIS_URL` 与健康检查都要求认证信息。
- `docker/config/homeserver.yaml` 仍同时存在 `signing_key_path` 与 `federation.signing_key` 两套签名密钥表达方式，说明联邦签名密钥来源尚未完成收敛。
- 示例配置中仍有非占位型敏感示例，如 `turn_shared_secret: "test_turn_shared_secret"`，不适合作为长期基线。

进一步落到可执行证据：

- `docker/docker-compose.yml`、`docker/docker-compose.prod.yml` 与 `docker/deploy/docker-compose.yml` 已移除 `db` / `redis` 的默认宿主机端口映射。
- 开发态主机访问已下沉到 `docker/docker-compose.dev-host-access.yml` 与 `docker/deploy/docker-compose.dev-host-access.yml`，需要显式叠加这些文件才会暴露 `5432/6379`。
- `docker/docker-compose.yml`、`docker/docker-compose.prod.yml` 与 `docker/deploy/docker-compose.yml` 的 `REDIS_URL` 已切换为带密码格式，Redis 启动命令包含 `--requirepass`，健康检查改为认证后 `PING`。
- `.github/workflows/ci.yml` 的 `security-audit` job 已取消 `continue-on-error`，并通过 `scripts/run_cargo_audit.sh` 执行阻断型审计；同时对 `rand::rng()` 建立临时禁用门禁。
- `.github/workflows/docs-quality-gate.yml` 已把根级 `OPTIMIZATION.md` 纳入 Markdown 与链接检查范围。

---

## 3. 对上一版文档的纠偏结论

上一版 `OPTIMIZATION.md` 更像“单次镜像审查报告”，并不完全适合作为当前仓库的总优化主文档。主要偏差如下：

### 3.1 关键问题归因不准确

- 上一版将 `ColumnNotFound("server_name")` 归因到 `key_rotation_history` 缺列。
- 当前仓库代码显示，`src/federation/key_rotation.rs` 中查询的是 `federation_signing_keys`，但随后读取了未选出的 `server_name`、`key_json`、`ts_added_ms`、`ts_valid_until_ms` 字段。
- 这更像“查询列与读取列不一致”的代码缺陷，而不是 `key_rotation_history` schema 缺列。

### 3.2 安全结论需要细化

- 当前 compose 与示例配置已经大量采用 `${VAR}` 或必填环境变量，不宜继续笼统表述为“凭证硬编码普遍存在”。
- 更准确的说法应是：**部署默认值、端口暴露、Redis 无认证、样例密钥残留、审计门禁非阻断** 共同构成安全收敛不充分。

### 3.3 配置冲突结论需要修正

- 当前配置中确实存在多处 `server_name` 配置位，但默认并未出现上一版文档列出的 `localhost` / `cjystx.top` 静态冲突。
- 真正的问题不是“值冲突已发生”，而是“同一语义存在多处入口，缺少统一约束和启动校验”。

### 3.4 文档粒度不适配当前阶段

- 当前项目的主要挑战已经不是“单个镜像有几个问题”，而是“仓库规模增大后，事实源、架构边界、测试语义与安全治理能否持续收敛”。
- 因此 `OPTIMIZATION.md` 应升级为系统性治理蓝图，而不是继续维护一个偏镜像侧、偏单次审计侧的问题列表。

---

## 4. 系统性问题清单

## 4.1 架构设计

- A1. **大文件与大入口持续累积**
  - 现象：`room.rs`、`config/mod.rs`、`federation.rs`、`middleware.rs` 均已超过 2000 行，最大文件超过 4000 行。
  - 风险：评审难、理解难、回归面大、职责漂移严重。
  - 结论：这是当前仓库最显著的长期维护风险之一。

- A2. **服务层与路由层边界仍偏宽**
  - 现象：`sync_service.rs`、`room_service.rs`、`database_initializer.rs` 体量都已接近或超过 1500 行。
  - 风险：单元测试替身注入困难，业务收敛速度下降，模块复用边界不清。

- A3. **配置系统兼容层膨胀**
  - 现象：`src/common/config/mod.rs` 规模达到 4126 行。
  - 风险：配置加载、环境变量覆盖、默认值策略、旧配置兼容逻辑容易交叉污染。

## 4.2 代码质量与稳定性

- Q1. **生产代码中显式崩溃式处理仍然过多**
  - 现象：`src/` 范围内仍有 688 处 `.unwrap(`、91 处 `.expect(`、24 处 `panic!(`。
  - 风险：局部异常容易直接升级为服务中断。

- Q2. **关键路径存在“查询列与读取列不一致”的真实缺陷**
  - 现象：历史上 `src/federation/key_rotation.rs` 的 `load_or_create_key` 查询语句未选出所有后续读取字段。
  - 当前状态：已完成修复，并补充“加载数据库中现有完整记录”的回归测试。
  - 剩余风险：仍需在真实部署启动链路和后续重构中持续防止字段漂移。

- Q3. **错误模型集中，但治理口径还未完全统一**
  - 现象：`src/common/error.rs` 体量很大，说明统一错误模型在推进，但仍可能存在跨模块语义分叉。
  - 风险：调用方重试、日志、指标与 API 映射不一致。

## 4.3 数据库与迁移治理

- D1. **迁移治理框架存在，但关键增量迁移仍需逐条验证**
  - 现象：仓库已拥有统一 schema、增量迁移、迁移审计、coverage gate、manifest 机制。
  - 风险：治理资产越多，越需要确保“真实执行路径”和“文档口径”一致，否则容易形成复杂但脆弱的体系。

- D2. **`20260409090000_to_device_stream_id_seq.sql` 需要专项回归**
  - 现象：该迁移脚本逻辑不复杂，但它涉及 E2EE `to_device` 的序列初始化。
  - 当前状态：已修复 `public` schema 假设与空表 `setval` 语义，并补充空表、已有数据、重复执行三类回归测试。
  - 剩余风险：仍需在 CI 真库链路与升级环境中持续验证迁移执行闭环。

- D3. **统一 schema 与运行时兼容初始化并存**
  - 现象：README 已说明外部迁移脚本是唯一正式入口，但仓库中仍保留运行时兼容初始化路径。
  - 风险：多路径并存若缺少清晰边界，容易让部署者误解“谁才是权威初始化机制”。

## 4.4 安全与部署

- S1. **部署默认暴露面偏大**
  - 现象：历史上 compose 默认暴露 8008、8448、5432、6379。
  - 当前状态：默认 compose 与生产 compose 已移除 PostgreSQL/Redis 的宿主机端口映射，开发态访问需显式叠加 `docker-compose.dev-host-access.yml`。
  - 剩余风险：仍需继续审查其他部署入口，避免旧部署脚本保留同类默认暴露。

- S2. **Redis 缺少认证基线**
  - 现象：历史上 compose 启动命令未配置密码，健康检查也基于匿名 `redis-cli ping`。
  - 当前状态：默认 compose 与生产 compose 已要求 `REDIS_PASSWORD`，Redis 启动命令与健康检查均使用认证。
  - 剩余风险：仍需核对其他部署包与说明文档，确保不存在匿名 Redis 示例路径。

- S3. **安全审计还不是强阻断**
  - 现象：历史上 `cargo audit` 已接入 CI，但为 `continue-on-error`。
  - 当前状态：CI 中的 `cargo audit` 已改为阻断执行，豁免条目由仓库内的 `cargo-audit.toml` 集中维护。
  - 剩余风险：豁免条目仍需按 `Review-by` 日期持续复核，避免 allowlist 老化。

- S4. **样例敏感配置收敛不彻底**
  - 现象：示例配置仍保留测试型 `turn_shared_secret`，联邦签名还存在文件路径与明文配置双轨。
  - 风险：示例配置可能被误用为准生产基线。

## 4.5 测试、质量门禁与文档治理

- T1. **测试资产多，但“验证强度”与“接线状态”仍需持续对齐**
  - 现象：仓库拥有 173 个测试 Rust 文件与多个专项测试入口。
  - 风险：测试数量容易被误读为成熟度，必须持续区分“存在测试”“默认执行”“可支撑正式结论”。

- T2. **文档门禁覆盖面不足**
  - 现象：历史上 `docs-quality-gate.yml` 只校验部分特定文档。
  - 当前状态：根级 `OPTIMIZATION.md` 已纳入 Markdown 与链接检查。
  - 剩余风险：文档门禁仍未覆盖所有高价值根级文档，覆盖面仍可继续扩展。

- T3. **正式结论文档过多，知识体系容易碎片化**
  - 现象：`docs/` 下已有 134 个 Markdown 文件。
  - 风险：如果没有明确的单一事实源，历史结论会反向污染当前判断。

---

## 5. 风险矩阵（优先级）

| ID | 维度 | 风险描述 | P | I | 分值 | 优先级 | 状态 |
| :-- | :-- | --: | --: | --: | --: | :-- | :-- |
| Q2 | 稳定性 | 联邦密钥轮换查询列与读取列不一致，运行时崩溃风险 | 5 | 5 | 25 | P0 | 已修复，待持续观察 |
| A1 | 架构 | 多个核心文件持续膨胀，职责边界失控 | 5 | 4 | 20 | P0 | 未开始 |
| S1 | 安全 | 默认部署暴露 DB/Redis 端口 | 4 | 5 | 20 | P0 | 已修复，待其他部署入口复核 |
| S3 | 安全 | 安全审计非阻断，漏洞可带病进入主线 | 4 | 5 | 20 | P0 | 已修复，待豁免条目复核 |
| Q1 | 质量 | `src/` 中 `unwrap/expect/panic` 过多 | 4 | 4 | 16 | P1 | 未开始 |
| D2 | 迁移 | `to_device` 序列迁移缺少专项闭环证据 | 4 | 4 | 16 | P1 | 已补测，待 CI 真库验证 |
| T1 | 测试 | 测试数量与测试可信度被混淆 | 4 | 4 | 16 | P1 | 进行中 |
| A3 | 架构 | 配置系统兼容层过厚 | 3 | 4 | 12 | P1 | 未开始 |
| S4 | 安全 | 样例敏感配置与签名密钥双轨未收敛 | 3 | 4 | 12 | P1 | 未开始 |
| T2 | 文档 | 总方案文档未被质量门禁覆盖 | 3 | 3 | 9 | P2 | 已修复，待继续扩面 |
| D3 | 迁移 | 统一迁移入口与运行时兼容路径并存 | 3 | 3 | 9 | P2 | 未开始 |
| T3 | 治理 | 文档资产过多，事实源可能继续分散 | 3 | 3 | 9 | P2 | 未开始 |

---

## 6. 分阶段整体优化方案

## 阶段 0（立即执行）：止血与基线纠偏

### 阶段 0 目标

- 修复真实阻断性缺陷。
- 停止继续传播不准确结论。
- 建立本轮治理的正确基线。

### 阶段 0 关键动作

1. 修复 `src/federation/key_rotation.rs` 的查询列与读取列不一致问题。
2. 为联邦签名密钥加载路径补充回归测试与启动前校验。
3. 对 `migrations/20260409090000_to_device_stream_id_seq.sql` 建立专项执行验证，覆盖空表、已有数据、重复执行三类场景。
4. 将 `OPTIMIZATION.md`、README、正式基线文档的口径统一为“证据驱动”。
5. 明确安全审计豁免策略，禁止长期保持 `continue-on-error` 而无到期机制。

### 阶段 0 量化验收

- `key_rotation` 相关启动场景不再因列读取问题崩溃。
- `to_device_stream_id_seq` 迁移具备可重复执行验证证据。
- 根目录总方案文档不再包含已确认与仓库现实不符的结论。

### 阶段 0 当前完成度

- 已完成：`key_rotation` 崩溃根因修复与回归测试补充。
- 已完成：`to_device_stream_id_seq` 迁移脚本修复与三类专项验证入口补充。
- 已完成：默认 compose / 生产 compose 的 DB 与 Redis 暴露面收紧，并为开发态保留显式 override。
- 已完成：Redis 认证基线补齐，应用连接与健康检查均切换到带认证模式。
- 已完成：安全审计从 `continue-on-error` 升级为阻断，并通过 `cargo-audit.toml` 维护豁免条目。
- 已完成：根级 `OPTIMIZATION.md` 已纳入文档质量门禁。
- 进行中：总方案文档按证据回写并同步更新阶段结果。

### 阶段 0 已落地的直接方案

1. **默认部署暴露面收紧**
   - 已将 `docker/docker-compose.yml` 与 `docker/docker-compose.prod.yml` 中 `db` / `redis` 的默认 `ports` 移除。
   - 已新增 `docker/docker-compose.dev-host-access.yml` 作为显式开发态主机访问入口。
   - 验收结果：默认 compose 配置解析下，宿主机不再直接监听 PostgreSQL/Redis 端口。

2. **Redis 认证基线补齐**
   - 已为 compose 增加 `REDIS_PASSWORD` 环境变量与 `--requirepass`。
   - 已同步修正应用侧 `REDIS_URL` 与 Redis 健康检查，使其显式带认证参数。
   - 验收结果：compose 配置已要求认证参数，匿名健康检查路径已移除。

3. **安全审计阻断化**
   - 已取消 `.github/workflows/ci.yml` 中 `cargo audit` 的长期 `continue-on-error`。
   - 已将豁免集中到 `cargo-audit.toml`，并为现有条目补充责任人与 `Review-by` 日期。
   - 验收结果：新增高危依赖漏洞时 CI 默认失败，豁免项具备集中审计轨迹。

4. **根级文档门禁覆盖**
   - 已将 `OPTIMIZATION.md` 纳入现有 Markdown 质量门禁与链接检查入口。
   - 验收结果：主方案文档的格式错误与链接问题已可在 CI 中被发现。

---

## 阶段 1（短期）：架构收敛与模块拆分

### 阶段 1 目标

- 降低超大文件和超大模块的维护成本。
- 把“功能堆叠”转为“边界清晰”的领域模块。

### 阶段 1 关键动作

1. 按能力域拆分 `src/web/routes/handlers/room.rs`。
2. 将 `src/common/config/mod.rs` 拆为：
   - 配置模型
   - 环境变量映射
   - 默认值策略
   - 配置校验
   - 历史兼容层
3. 对 `src/web/routes/federation.rs` 进行按联邦端点类型拆分。
4. 收缩 `database_initializer.rs` 的职责，明确“正式迁移入口”和“兼容初始化逻辑”的分层。
5. 为 `sync_service.rs`、`room_service.rs` 建立子域拆分计划，优先拆事件、成员、摘要、缓存协同逻辑。

### 阶段 1 量化验收

- Top 3 超大文件行数下降 25% 以上，或拆成至少 3 个独立子模块。
- 新增能力默认不再进入总装配大文件，而是进入对应子域模块。
- 配置系统新增字段时，只允许落到明确分层模块中。

---

## 阶段 2（中期）：质量与迁移治理强化

### 阶段 2 目标

- 将“能运行”提升为“更稳、更可预测、可回归”。

### 阶段 2 关键动作

1. 对 `src/` 范围内的 `.unwrap/.expect/panic!` 建立分级台账：
   - 测试专用
   - 启动期不可恢复
   - 运行期必须替换
2. 为关键服务建立统一错误语义映射，收敛到明确的 API / tracing / metrics 输出路径。
3. 将迁移验证从“目录治理”扩展到“关键迁移行为回归”。
4. 对统一 schema、关键增量迁移、运行时兼容初始化建立职责边界文档。
5. 建立联邦与 E2EE 的“可验证能力矩阵”，区分：
   - 已实现并默认验证
   - 已实现但需显式外部条件
   - 仅有代码基础

### 阶段 2 量化验收

- `src/` 中运行期高风险 `unwrap/expect` 数量显著下降，并形成白名单说明。
- 关键迁移具备可重复执行测试。
- 联邦/E2EE 相关文档不再仅以“测试数量”作为成熟度论据。

---

## 阶段 3（中长期）：安全、发布与文档闭环

### 阶段 3 目标

- 形成长期可持续的发布质量体系。

### 阶段 3 关键动作

1. 将 `cargo audit` 从“记录型检查”升级为“有 SLA 的阻断型检查”。
2. 将 DB/Redis 端口暴露改为显式 profile 或开发态开关，生产默认不暴露。
3. 为 Redis 增加认证能力，并同步调整健康检查与配置说明。
4. 收敛联邦签名密钥来源，明确是“文件权威”还是“数据库权威”，禁止双轨长期并存。
5. 将根级主文档纳入 Markdown 质量门禁。
6. 为 `docs/` 建立单一事实源和历史归档规则，避免持续叠加“相似但不同”的正式结论文档。

### 阶段 3 量化验收

- 高危安全问题不能在无审批情况下进入主分支。
- 生产部署默认仅暴露业务必要端口。
- 总方案文档、README、能力基线文档之间不存在关键结论冲突。

---

## 7. 优先执行清单

### P0（必须先做）

1. 修复 `key_rotation` 启动崩溃根因（已完成）。
2. 审核并验证 `20260409090000_to_device_stream_id_seq.sql` 的真实执行闭环（已完成首轮修复与补测）。
3. 收紧默认部署暴露面，至少将 DB/Redis 暴露改成显式开发配置（已完成）。
4. 将安全审计从“非阻断”升级为“有豁免机制的阻断”（已完成）。

### P0 建议执行顺序

1. 阶段 0 的四个 P0 已全部完成，下一步优先切入阶段 1 的超大文件拆分与阶段 2 的运行期崩溃点治理。
2. 安全面后续建议继续复核 `docker/deploy/` 等其他部署入口，确保默认暴露面与 Redis 认证策略全仓统一。
3. 对 `cargo-audit.toml` 中的豁免条目建立周期复核节奏，避免门禁“形式阻断、实质失效”。

### P1（紧随其后）

1. 拆分 `room.rs`、`config/mod.rs`、`federation.rs`。
2. 建立 `src/` 级别的崩溃式错误处理治理台账。
3. 收敛签名密钥配置来源。
4. 将联邦与 E2EE 验证状态做成能力矩阵，而不是散落在多份报告中。

### P2（计划内推进）

1. 补齐文档质量门禁覆盖面。
2. 收拢运行时兼容初始化路径的定位。
3. 制定文档单一事实源和历史归档规则。

### P3（持续优化）

1. 镜像层与构建产物继续瘦身。
2. 多架构镜像与版本标签规范化。
3. 更细粒度的性能预算与仪表盘整合。

---

## 8. 结论

当前 `synapse-rust` 的核心问题已经从“有没有功能”转为“如何把庞大功能资产收敛成可信、可维护、可发布的工程系统”。

与上一版文档相比，更准确的总判断应为：

- 项目代码规模大、能力铺设广，已经具备大型 Matrix homeserver 的雏形。
- 真正的高优先级风险集中在架构膨胀、关键路径稳定性、迁移闭环、安全默认值和文档事实源治理。
- 眼下最重要的不是继续追加零散问题列表，而是把关键缺陷止血、把边界收拢、把门禁升级、把结论统一。
- 截至当前回写，阶段 0 中与联邦密钥加载、`to_device` 序列迁移、部署默认暴露面、安全审计阻断化、根级文档门禁相关的止血工作已经落地。
- 后续优先级应从阶段 0 收尾转向阶段 1 的大文件拆分、阶段 2 的运行期崩溃点治理，以及其他部署入口的安全一致性复核。

因此，本方案将 `OPTIMIZATION.md` 从“镜像审查报告”升级为“系统性治理主方案”，后续所有优化项都应以本文件的风险矩阵、阶段目标与验收口径为准继续推进。
