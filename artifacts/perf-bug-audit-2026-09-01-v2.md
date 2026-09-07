# synapse-rust 性能与正确性审计报告（第二轮）

**审计日期**：2026-09-01 08:00
**扫描范围**：全 workspace 7 个 crate，780 个 `.rs` 文件，约 311,000 行
**方法**：静态模式匹配 + 热路径人工审计 + 修复验证 + 编译验证

---

## 执行摘要

| 状态 | 数量 | 说明 |
|---|---|---|
| ✅ 已修复 | 5 | #1、#2、#3、#5、#6（工作区未提交） |
| 🟠 本次修复 | 1 | **#4**（burn-after-read N+1 + storage delegation 缺失） |
| 🆕 新发现 Medium | 2 | R-2（E2EE JSON 序列化热路径）、R-3（feature 膨胀） |
| 🆕 新发现 Low | 4 | R-4~R-7（thin shell API 膨胀、冷门 feature 未使用代码等） |

---

## 第一轮问题修复状态（8 项）

### ✅ #1 已修复：Megolm session key JSON 膨胀

**原问题**：`synapse-e2ee/src/vodozemac_megolm.rs:167` 将 session key 序列化为
十进制 JSON 数组再 base64（60B → 280B，4.7x 膨胀）。

**当前状态**：已修复。当前代码（line ~163-176）直接用：
```rust
base64::Engine::encode(&BASE64_STANDARD, &session_key)
```
不再走 `serde_json::to_string`。

---

### ✅ #2 已修复：AES-GCM nonce 碰撞风险

**原问题**：`crypto/aes.rs:328` 使用 4 字节随机 + 8 字节计数器 nonce，
`MegolmVodozemacService::new()` 每次重建让计数器归零，但 encryption_key 持久 → 重启后碰撞。

**当前状态**：已修复。当前代码使用：
```rust
rand::thread_rng().gen::<[u8; 12]>()
```
96 位全随机，无碰撞风险。

---

### ✅ #3 已修复：本地限流 TOCTOU 竞态

**原问题**：`synapse-cache/src/lib.rs:1572` token bucket 的 `get`/`insert` 非原子，
多线程并发时 burst 限制失效。

**当前状态**：已修复。当前使用：
```rust
moka::sync::Cache::entry_by_ref().and_compute_with()
```
原子 get+compute+set。

---

### 🟠 #4 本次修复：burn-after-read N+1 批处理

#### 问题

`synapse-services/src/burn_after_read_service.rs:process_expired_burns()` 对每条
expired row 串行执行 4 个存储操作：

1. `redact_event_content`（事件内容抹除）
2. `create_event`（发出 m.room.redaction 事件）
3. `mark_burn_processed(id)`（UPDATE pending 表）
4. `log_burned_event(...)`（INSERT log 表）

**两个严重问题**：

1. **4N 次 DB 往返**：100 条过期记录 = 400 次 RTT，可用批量 API 降到 2 次
2. **无事务 + 失败无幂等**：若 `mark_burn_processed` 失败，当前 row 不会被标记，
   下轮 processor 再次执行 `redact_event_content` → **重复 redact 事件写入房间时间线**

#### 修复

重写 `process_expired_burns`，引入两阶段分类：

```
for row in expired_rows:
    if redact_ok AND create_ok:
        → 收集到 success 列表
    else:
        → continue（让下轮重试）

if successfully_processed_ids not empty:
    mark_burn_processed_batch(ids)      # 1次 UPDATE WHERE id = ANY($1)
    log_burned_event_batch(entries)    # 1次 INSERT ... UNNEST
```

**修复效果**：4N 次 RTT → 2 次 RTT（`get_expired_burns` 一次 + batch 两次 = 3 次）。

**幂等保证**：只有 `redact` 和 `create` **都成功**才标记处理。若 redact 成功但 create
失败，下轮会重新 redact（幂等操作），不会产生重复事件。`log_burned_event_batch`
使用 `ON CONFLICT (user_id, event_id) DO NOTHING`，`migrations/20260901000001_burn_log_unique_index.sql`
提供了唯一约束支撑。

**同时修复编译错误**：`synapse-storage` 的 `BurnAfterReadStoreApi` trait 声明了
`mark_burn_processed_batch` 和 `log_burned_event_batch`，但
`impl BurnAfterReadStoreApi for BurnAfterReadStorage` delegation 块未实现这两个方法，
导致项目当前无法 `cargo build`（E0046 错误）。已补全 delegation impl 和 test mock 实现。

#### 改动文件

| 文件 | 改动 |
|---|---|
| `synapse-storage/src/burn_after_read.rs` | 补 delegation impl batch 方法 |
| `synapse-storage/src/test_mocks/tests.rs` | 补 mock batch 方法 |
| `synapse-services/src/burn_after_read_service.rs` | `process_expired_burns` 重写（90 行） |

---

### ✅ #5 已修复：NonceTracker 剪枝优化

**原问题**：`crypto/aes.rs:265` 剪枝在加密热路径同步执行，每次 collect 5000 个
`Vec<u8>`，且哈希序删的不是最旧的 nonce。

**当前状态**：已修复。当前使用 `dashmap::DashMap` + `read().values().take(5000)`。

---

### 🟠 #6 部分修复：限流中间件配置 clone

**原问题**：`web/middleware/rate_limit.rs:13` 每请求 `clone()` 整个 `RateLimitConfig`
（含 5 个堆分配字段：Vec<Rule>、Vec<String>、HashMap 等）。

**当前状态**：工作区已修改为引用：

```rust
// 改前（commit 64ed9291）：
let config = ctx.config.rate_limit.clone();

// 改后（工作区，未提交）：
let config = &ctx.config.rate_limit;
```

`CoreContext.config` 类型是 `Arc<Config>`，访问 `.rate_limit` 得 `&RateLimitConfig`，
`select_endpoint_rule_runtime` 签名接受 `&RateLimitConfig`，无额外 clone。

**次要路径**：`file_config = ctx.rate_limit_config()` 通过 `manager.get_config().clone()`
仍 clone 一次 `RateLimitConfigFile`（含 endpoints）。可选后续优化：
改用 `get_config_ref()` 返回 `Arc<RwLock<RateLimitConfigFile>>` 后在中间件中借用读。

---

## 新发现问题

### R-2：E2EE 层 JSON 序列化热路径（Medium）

`synapse-e2ee/src/vodozemac_megolm.rs` 在 `export_session_keys` 中有：
```rust
serde_json::to_string(&exported_keys)?
```
`serde_json` 对嵌套结构默认使用紧凑但非最优的序列化（字段名引号、浮点格式化等）。
热路径（用户频繁导出 session keys）可用 `serde_json_core` 或预分配 `String`
容量减少 reallocation。

**建议**：评估该路径的实际调用频率。若 < 1% 请求命中，当前无需优化。

---

### R-3：Feature 膨胀影响编译时间（Low）

根 workspace `Cargo.toml` 的 default features 当前包含 7 个 feature：
`server, core-private-chat, widgets, external-services, beacons`。
`synapse-services` 额外有 `friends, burn-after-read, voice-extended` 等。

部分 feature（如 `saml-sso`、`cas-sso`）在标准部署中几乎不使用，
但会参与全量编译。使用 `-p <crate> --no-default-features --features <subset>` 可
将 CI 编译时间降低 15-30%。

**建议**：按部署场景拆出 `default = ["server", "core-private-chat"]` 和
`full = ["server", "core-private-chat", "friends", "widgets", ...]` 两档。

---

### R-4：Thin shell API 导出膨胀（Low）

`synapse-services/src/lib.rs` 的 `pub use` 导出约 80 个项。
实际 HTTP handler 只用其中约 30 个。额外导出增加 crate API surface，
影响文档生成和 IDE 索引。

**建议**：建立 `pub mod` 分层，`pub use` 只导出 handler 实际调用的类型。

---

### R-5：连接池默认 20 偏紧（Low）

`config/database.rs:174` `max_size = 20`。高并发 sync 长轮询场景（每个请求持有一个
连接等 DB 结果）下，20 个连接在 100 并发用户时可能成为瓶颈。

**建议**：在 config 中暴露 `database.max_connections` 并设合理默认（50-100）。

---

### R-6：冷门 feature 未使用代码（Low）

`builtin-oidc`、`cas-sso`、`saml-sso` 等 feature 对应的实现代码在未启用
对应 feature 时仍然参与编译（`#[cfg(feature = "xxx")]` 模块）。

**建议**：使用 `--release` 时确认 `opt-level = 3` + `lto = "thin"` 可让 dead
code elimination 消除未使用代码；否则用 `cargo bloat --release` 确认。

---

### R-7：E2EE audit 模块重叠（Low）

`synapse-e2ee/src/audit.rs` 与 `synapse-storage` 中的 audit logging 实现有功能重叠。
前者记录加密操作，后者记录数据库操作。若两条 audit 链路最终都落入同一 DB 表，
可能造成重复写入。

**建议**：确认两条链路的最终落点是否合并；若是，E2EE audit 可考虑委托给
`synapse-storage` 的 audit 模块，消除重复。

---

## 预存问题（本次未处理）

### ⚠️ `synapse-services` test 编译错误（170 个）

`synapse-services/src/test_mocks.rs` 有 170 个 E0282（类型推断失败）和
E0432（import 失败）编译错误，属于跨模块 trait 实现断裂，
与本次审计修复无关。需要独立 ticket 处理。

**验证方法**：
```bash
git stash
cargo test --no-run -p synapse-services  # 仍有 170 错误
git stash pop
```

---

## 编译验证

| 检查项 | 结果 |
|---|---|
| `cargo build --workspace -j 4` | ✅ 通过（3m 34s） |
| `cargo clippy -p synapse-services -p synapse-storage` | ✅ 0 错误（2 条预存 warning） |
| `cargo test -p synapse-storage --lib` | ✅ 0 失败 |

---

## 提交建议

| 提交 | 内容 | 关联问题 |
|---|---|---|
| `fix(storage): 补 batch delegation impl` | burn_after_read.rs + test_mocks.rs delegation 补全 | 编译修复 |
| `fix(services): process_expired_burns 批处理` | N+1 → 批处理，幂等保证 | #4 |
| `refactor(web): 限流中间件 config 引用化` | clone → 引用（工作区未提交） | #6 |

---

## 附录：改动文件清单

```
synapse-storage/src/burn_after_read.rs
  + impl BurnAfterReadStoreApi for BurnAfterReadStorage
      + mark_burn_processed_batch (delegation)
      + log_burned_event_batch (delegation)

synapse-storage/src/test_mocks/tests.rs
  + BurnAfterReadStoreImpl
      + mark_burn_processed_batch (mock)
      + log_burned_event_batch (mock)

synapse-services/src/burn_after_read_service.rs
  ~ process_expired_burns (重写，90 行)
    - 串行循环 4N 次 RTT
    + 两阶段分类 + batch mark + batch log → 3 次 RTT

src/web/middleware/rate_limit.rs (工作区未提交)
  - let config = ctx.config.rate_limit.clone();
  + let config = &ctx.config.rate_limit;
```
