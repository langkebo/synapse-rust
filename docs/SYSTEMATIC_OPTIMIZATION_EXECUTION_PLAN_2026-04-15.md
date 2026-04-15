# synapse-rust 系统性优化执行计划

> 日期: 2026-04-15
> 基线: `element-hq/synapse` 最佳实践、`docs/CODE_QUALITY_IMPROVEMENTS_2026-04-04.md`
> 原则: 最小可行、先止血后统一、先删假能力再补真能力、所有收敛都要能被测试和文档证明

## 1. 本轮已执行的收敛动作

- 联邦只读接口 `get_event` / `get_room_event` 已对齐事件检索响应外壳，统一返回 `origin`、`origin_server_ts`、单元素 `pdus`。
- 联邦只读接口 `get_state` / `get_state_ids` / `backfill` 已继续收口为更贴近规范的外壳与最小披露输出：
  - `get_state` 返回 `pdus` + `auth_chain`
  - `get_state_ids` 返回 `pdu_ids` + `auth_chain_ids`
  - `backfill` 改为 `GET` 查询参数解析，支持重复 `v=` 与 `limit`
- 上述接口补齐稳定排序，避免同时间戳下返回顺序漂移。
- 上述接口去除伪造或多余字段，不再暴露 `prev_events`、非规范 `limit`、`unsigned`、`depth` 等无必要数据。
- 相关回归测试已通过：
  - `cargo test --test integration test_federation_state_and_backfill_endpoints_return_spec_shaped_minimal_payloads -- --nocapture`
  - `cargo test --test integration test_federation_backfill_rejects_unjoined_server_for_private_room -- --nocapture`
- `sync_service` / `sliding_sync` 已完成超长签名收敛：
  - 内部引入参数对象，收敛 `too_many_arguments`
  - 对外保留兼容入口，避免扩大测试与调用面的机械改动
  - 相关验证已通过 `cargo check`、`cargo clippy --all-features --locked -- -D warnings`
  - 定向测试已通过：
    - `cargo test --all-features --locked --test unit sync_service -- --nocapture`
    - `cargo test --all-features --locked --test unit sliding_sync -- --nocapture`
    - `cargo test --all-features --locked --test integration sliding_sync -- --nocapture`

## 2. MVP 功能清单

### P0: 必须继续完成，禁止扩 scope

- 统一联邦入站安全边界，确保所有已挂载联邦读写入口都复用同一套鉴权、`origin` 绑定和最小披露规则。
- 清除“模拟成功”“空实现”“占位兼容”接口，避免继续向客户端或远端服务器暴露假能力。
- 保持 `cargo test`、关键集成测试和联邦安全回归持续通过。

### P1: 在 P0 稳定后推进

- 统一分支基线、依赖版本、构建入口和代码质量门槛。
- 修复 `clippy` 当前仍阻塞 CI 等级收敛的存量问题，先处理低风险机械性问题，再处理函数签名过宽等结构问题。
- 扫描并删除未引用模块、重复包装文件、废弃配置和冗余依赖。

### P2: 仅在 P0/P1 收口后推进

- 做覆盖率补洞与性能 smoke 对比。
- 对 `sync`、`sliding_sync`、E2EE、管理接口等高风险路径开展第二轮结构性收敛。
- 在不新增平行框架的前提下，把 Synapse 风格的核心授权/事件链逻辑继续下沉到统一服务层。

### 明确不做

- 不新起第二套联邦框架。
- 不为未落地能力继续保留“200 + 空结果”的伪实现。
- 不在主干收敛完成前引入新的大型功能分支。

## 3. 分支与技术分叉盘点

### 3.1 远端分支现状

- `origin/main`: 当前主基线。
- `origin/master`: 与 `origin/main` 分叉明显，`git rev-list --left-right --count origin/main...origin/master` 结果为 `240 2`。
  - 解释: `main` 独有 240 个提交，`master` 独有 2 个提交。
  - 结论: `master` 不应再作为并行开发主线，只适合做遗留提交回收。
- `origin/feature/api-merge-updates-20260327`: `git rev-list --left-right --count origin/main...origin/feature/api-merge-updates-20260327` 结果为 `91 0`。
  - 解释: 该分支没有任何领先于 `main` 的提交，反而落后 `main` 91 个提交。
  - 结论: 这是“已被主干吸收但未清理”的陈旧功能分支，应转入归档/删除流程。

### 3.2 合并计划

- 阶段 A: 冻结 `origin/master` 和 `origin/feature/api-merge-updates-20260327`，禁止继续向两者直接提交。
- 阶段 B: 仅对 `origin/master` 独有的 2 个提交做内容级复核。
  - 若为 bugfix 且主干尚无等价修复，则按提交粒度 cherry-pick 到 `main`。
  - 若为历史兼容或构建残片，则记录后废弃，不强行回并。
- 当前复核结论:
  - `edaa76e` 为初始提交，不具备单独回收价值。
  - `e676cef` 中提到的 `Dockerfile` 修补在主干现状中已基本覆盖；当前 `docker/Dockerfile` 已包含 `benches`，并继续显式保留 `--locked`，因此暂不建议直接回并。
- 阶段 C: `feature/api-merge-updates-20260327` 只保留审计记录，不再发起合并。
- 阶段 D: 统一文档、CI、发布流程只认 `main`。

## 4. 依赖、构建脚本与规范差异

### 4.1 已确认的不一致

- `Cargo.toml` 固定 `rust-version = "1.93.0"`，且 `rust-toolchain.toml` 已存在；此前 CI 仍使用泛化 `stable`，存在“本地/CI 工具链漂移”风险。
- `Makefile` 的 `lint` 目标此前为 `cargo clippy ... || true`，会吞掉失败；CI 中 `cargo clippy -- -D warnings` 会真实失败，门槛曾不一致。
- `Makefile` 的 `test-coverage` 此前与 CI `coverage` job 参数不完全一致，导致覆盖率结果不可直接对比。
- 当前仓库仍保留多份 worktree 分支，说明修复工作已高度并行，但缺少统一收口入口。

### 4.2 统一建议

- 已将 CI 的 Rust 安装步骤显式切到 `1.93.0`，与 `Cargo.toml` / `rust-toolchain.toml` 保持一致。
- 已将本地 `make lint` 调整为与 CI 同级别失败策略，不再 `|| true`。
- 已统一覆盖率命令参数，并修正为当前 `cargo-tarpaulin 0.35.2` 可执行的 `--include-tests --locked`。
- 统一“增量整改文档入口”，优先写入本计划和 `CODE_QUALITY_IMPROVEMENTS_2026-04-04.md`，避免多个相近文档并行漂移。

## 5. 静态分析与依赖扫描基线

### 5.1 `clippy` 当前基线

- 已执行: `cargo clippy --all-features --locked -- -D warnings`
- 当前结果: 已清零，`clippy -D warnings` 通过。
- 本轮已完成的静态分析收敛:
  - `src/services/external_service_integration.rs`: 修复 `manual_contains`
  - `src/web/routes/handlers/room.rs`: 去除两处 `unnecessary_cast`
  - `src/e2ee/verification/storage.rs`: 修正数据库字段类型与代码类型不匹配
  - `src/services/sync_service.rs`: 以参数对象收敛内部超长签名，同时保留兼容入口
  - `src/storage/sliding_sync.rs`: 为 `get_rooms_for_list()` 引入参数对象
- 下一批重点:
  - 从“修 clippy 阻塞项”切换到“覆盖率、死代码、冗余依赖和脚本统一”

### 5.2 重复依赖信号

- 已执行: `cargo tree -d`
- 已验证: 本机 `cargo-tarpaulin` 版本为 `0.35.2`，旧的 `--scope` / `--ignore-tests false` 组合不可用，已修正本地与 CI 命令。
- 当前可见的重复版本包括但不限于：
  - `base64` `0.21` / `0.22`
  - `deadpool` `0.10` / `0.12`
  - `thiserror` `1` / `2`
  - `darling` `0.20` / `0.23`
  - `toml_edit` `0.22` / `0.25`
  - `whoami` `1` / `2`
  - `core-foundation` `0.9` / `0.10`
- 说明: 这些不一定都能直接删除，很多是上游传递依赖；但应先识别“我们自己直接选型导致”的重复，再决定升级或替换。
- 本轮已完成的直接依赖复核:
  - 已确认 `deadpool = "0.12"` 为未使用的直接依赖，源码实际只使用 `deadpool-redis`，因此已从 `Cargo.toml` 删除。
  - 当前 `deadpool 0.10 / 0.12` 的重复主要来自 `deadpool-postgres` 与 `deadpool-redis` 的上游链路差异，不属于“已声明但未使用”问题。
  - 当前 `base64 0.21 / 0.22` 的重复主要来自 `config -> ron` 与项目其余依赖链，不宜在未确认配置格式需求前强行收敛。
- 当前判断:
  - 可立即处理: 未使用的直接依赖、无调用点的包装层、空实现占位路由。
  - 需单独评估后再动: `deadpool-postgres` 升级、`config` 特性裁剪、观测链相关依赖统一。

## 6. 清理批次

### 批次 1: 质量门槛统一

- 修 `clippy` 机械性错误。
- 让本地 `Makefile` 与 CI 失败语义一致。
- 固定 Rust 工具链版本。

### 批次 2: 分支与脚本统一

- 回收 `master` 独有提交。
- 归档陈旧 feature 分支。
- 统一覆盖率、测试、lint 命令入口。

### 批次 3: 死代码和冗余依赖清理

- 继续清理未引用模块、重复包装文件、废弃联邦/管理占位路由。
- 对 `cargo tree -d` 中由本项目直接引入的重复版本做收敛。
- 每批删除后必须跑单元测试和对应集成测试。

## 7. 性能与覆盖率证据要求

- 覆盖率基线命令:
  - `cargo tarpaulin --out Xml --out Json --out Html --scope Unit --scope Integration --ignore-tests false`
- 性能 smoke 基线命令:
  - `bash scripts/test/perf/run_tests.sh smoke`
- 对比输出要求:
  - 删除前/删除后测试通过率
  - 删除前/删除后 tarpaulin 总覆盖率
  - 删除前/删除后 perf smoke 时延摘要
- 当前状态:
  - 联邦回归已通过
  - `cargo check --all-features --locked` 已通过
  - `cargo clippy --all-features --locked -- -D warnings` 已通过
  - `sync_service` / `sliding_sync` 定向单测与集成测试已通过
  - 全仓覆盖率和 perf smoke 基线尚未重跑，进入下一批执行

## 8. 下一步执行顺序

1. 统一 `Makefile` / CI 的 lint 与覆盖率参数，消除本地与 CI 门槛漂移。
2. 跑全仓覆盖率和性能 smoke，产出第一版清理前基线。
3. 复核 `master` 独有 2 个提交，决定 cherry-pick 或废弃。
4. 归档 `feature/api-merge-updates-20260327`。
5. 开始删除未引用模块、重复包装文件、废弃配置与冗余依赖，并为每批删除补证据。
