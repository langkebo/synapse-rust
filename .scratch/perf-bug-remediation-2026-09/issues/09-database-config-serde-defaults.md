# 09: 数据库配置补齐序列化缺省值

**What to build:** 数据库配置结构使用派生 `Default` 且关键字段没有序列化缺省值，因此不经过配置文件、以程序化方式构造出来的配置会静默得到「最大连接数 0 / 超时 0」，而不是一个可用的连接池。当前生产配置显式写明了这些值所以没有暴露，但这属于靠约定兜底而非靠类型兜底。本票给连接池相关字段补上序列化缺省值，取值对齐生产配置，让省略配置或程序化构造都落在安全值上。

**Blocked by:** None (can start immediately)

**Status:** done — implemented by commit `20101dcf` (2026-09-07)

**审计条目：** #8（🟢 Low）— 原报告结论已修正

## 修复（2026-09-07 13:00 GMT+8）

**变更（`synapse-common/src/config/database.rs`）**：
- `port`: `#[serde(default = "default_database_port")]` → 5432
- `pool_size`: `#[serde(default)]` → 0（语义：废弃字段）
- `max_size`: `#[serde(default = "default_database_max_size")]` → 50（对齐 Synapse）
- `min_idle`: `#[serde(default)]` → None（Option 默认值）
- `connection_timeout`: `#[serde(default = "default_database_connection_timeout_secs")]` → 60s

**新增单测**（`synapse-common/src/config/database.rs` 内 `tests` 模块）：
- `database_config_serde_defaults_apply_when_fields_missing`：YAML 缺字段时 9 个字段取正确默认值
- `database_config_default_and_serde_defaults_match`：Default impl 与 serde 反序列化一致性（防漂移）

**验证**：
- `cargo test -p synapse-common --lib --features test-utils config::database` → **6 passed, 0 failed**
- `cargo build --workspace --all-features` → 0 errors
- `cargo doc -p synapse-common --no-deps` → 0 missing documentation
- `cargo clippy -p synapse-common --all-targets --features test-utils -- -D warnings` → 5 pre-existing errors（位于 `server.rs:47/62/77/91` + `rate_limit_config.rs:100` doc list 缩进，与本任务无关）

## 现状（2026-09-07）

**已修复**：`max_lifetime_secs` / `idle_timeout_secs` / `statement_timeout_secs` / `lock_timeout_secs` / `idle_in_transaction_timeout_secs` 全部有 `#[serde(default = "...")]` ✅

**仍缺**（`DatabaseConfig` line 50-101）：
- `max_size: u32` — 无 `#[serde(default)]`，`Default` 是 `0`（生产通过 config file 有值，程序化构造会得 0 连接池）
- `connection_timeout: u64` — 无 `#[serde(default)]`，`Default` 是 `0`（连接永不超时）
- `min_idle: Option<u32>` — `None` 是合理默认值 ✅
- `host/port/username/password/name` — 有默认值（空字符串或 5432）✅

**修复方案**：给 `max_size` 加 `#[serde(default = "default_database_max_size")]` → 返回 50（对齐 Synapse）；给 `connection_timeout` 加 `#[serde(default = "default_database_connection_timeout")]` → 返回 30（秒）

**对原审计结论的修正：** 原报告称「连接池 max_size 默认 20」，该数值取自测试夹具而非常规默认值。已核实生产配置实际使用最大连接数 50、最小空闲 10、超时 60 秒，并不存在「默认 20 偏紧」的问题。真实隐患是**缺少缺省值兜底**——程序化构造会得到 0。本票按修正后的结论处理。

- [ ] 最大连接数、最小空闲、连接超时三个字段均有序列化缺省值，取值与生产配置一致
- [ ] 完全省略这些字段的配置仍能解析出可用且非零的连接池参数
- [ ] 显式配置的值优先，不被缺省值覆盖
- [ ] 密码等敏感字段的调试输出脱敏行为不受影响（既有测试仍通过）
- [ ] 相关 crate 的既有测试全绿，`cargo clippy -- -D warnings` 无告警
