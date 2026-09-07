# synapse-rust 性能瓶颈与潜在 Bug 审计报告

**扫描日期**：2026-09-01  
**范围**：全 workspace 7 个 crate，780 个 `.rs` 文件，约 311,000 行  
**方法**：静态模式匹配 + 热路径人工审计 + 关键假设实证验证

| Crate              | 行数      | 职责             |
| ------------------ | ------- | -------------- |
| synapse-storage    | 104,886 | PostgreSQL 持久层 |
| synapse-services   | 80,863  | 业务逻辑           |
| src（HTTP 层）        | 70,613  | Axum 路由 / 中间件  |
| synapse-common     | 21,880  | 共享配置 / 工具      |
| synapse-e2ee       | 19,568  | 端到端加密          |
| synapse-federation | 9,316   | 联邦传输           |
| synapse-cache      | 5,284   | Redis / 本地缓存   |

---


## 结论速览

发现 **2 个高危 + 3 个中危 + 3 个低危** 问题。整体代码质量高于同类项目平均水平——多数经典陷阱（无界缓存、OFFSET 分页、SQL 注入、跨 await 持锁、缺失索引）已在此前多轮审查中修复。剩余问题集中在 **E2EE 双写路径** 和 **写路径的批处理缺失** 两处。

| # | 严重度       | 问题                                                         | 位置                                                     |
| - | --------- | ---------------------------------------------------------- | ------------------------------------------------------ |
| 1 | 🟠 High   | Megolm session key 双写 `Vec<u8>` 走 JSON 数组序列化，**4.7x 存储膨胀** | `synapse-e2ee/src/vodozemac_megolm.rs:167`             |
| 2 | 🟠 High   | AES-GCM nonce 计数器在实例重建时归零，**持久密钥下存在 nonce 重用风险**           | `synapse-e2ee/src/crypto/aes.rs:328`                   |
| 3 | 🟠 High   | 本地限流 token bucket 的 `get`/`insert` 非原子，**TOCTOU 竞态使限流失效**  | `synapse-cache/src/lib.rs:1572`                        |
| 4 | 🟡 Medium | burn-after-read 逐行 4 次串行 DB 写且无事务包裹                        | `synapse-services/src/burn_after_read_service.rs:208`  |
| 5 | 🟡 Medium | NonceTracker 剪枝在加密热路径同步执行，周期性延迟尖峰                          | `synapse-e2ee/src/crypto/aes.rs:265`                   |
| 6 | 🟡 Medium | 限流中间件每请求深拷贝整个 `RateLimitConfig`                            | `src/web/middleware/rate_limit.rs:13`                  |
| 7 | 🟢 Low    | sliding sync 缓存失效串行删除 N 个 key                              | `synapse-services/src/sliding_sync_service/mod.rs:913` |
| 8 | 🟢 Low    | 连接池 `max_size` 默认 20，sync 长轮询场景偏紧                          | `synapse-common/src/config/database.rs:174`            |

---

## 🟠 High


### 1. Megolm session key 双写：`Vec<u8>` 被序列化成 JSON 十进制数组，存储膨胀 4.7 倍

**位置**：`synapse-e2ee/src/vodozemac_megolm.rs:159-170`

```rust
fn dual_write_legacy_session_key(&self, raw_session_key: &[u8]) -> Option<String> {
    if !is_dual_write_enabled() { return None; }
    let key = self.encryption_key?;
    let cipher_key = Aes256GcmKey::from_bytes(key);
    let encrypted = self.aes_cipher.encrypt_with_nonce(&cipher_key, raw_session_key).ok()?;
    let json = serde_json::to_string(&encrypted).ok()?;   // ← Vec<u8> → "[200,207,214,...]"
    Some(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, json.as_bytes()))
}
```

**问题**：`encrypted` 是 `Vec<u8>`（nonce ‖ ciphertext）。`serde_json` 对 `Vec<u8>` 的默认行为是序列化成 **JSON 十进制数组**，而不是紧凑字节串。得到 `"[200,207,214,221,...]"` 之后又做了一次 base64 —— 等于二进制 → ASCII 十进制 → base64，双重冗余编码。

**实证验证**（60 字节输入 = 12 字节 nonce + 32 字节密文 + 16 字节 GCM tag）：

```
原始字节数 : 60
JSON 长度  : 209  (膨胀 3.5x)
base64 长度: 280  (总膨胀 4.7x)
JSON 前 60 字符: [200,207,214,221,228,235,242,249,0,7,14,21,28,35,42,49,56,63
```

**影响**：

- 每个 megolm session 的 `session_key` 列从 ~60 字节膨胀到 ~280 字节，**存储与 WAL 放大 4.7 倍**
- 每次写入多两次全量遍历（serde 逐元素序列化 + base64），CPU 浪费
- 该列被频繁读写（每条加密消息都要取 session），放大效应直接压到热路径

**修复**：去掉 JSON 环节，直接 base64 原始字节。

```rust
let encrypted = self.aes_cipher.encrypt_with_nonce(&cipher_key, raw_session_key).ok()?;
// 直接编码字节，不要再 serde_json
Some(base64::engine::general_purpose::STANDARD.encode(&encrypted))
```

> 若确实需要 JSON 结构（比如要分开存 nonce），改用 `serde_bytes` 或显式 `[serde(with = "serde_bytes")]`，让 `Vec<u8>` 走紧凑字符串。

---


### 2. AES-GCM nonce：计数器在实例重建时归零，持久密钥下存在 nonce 重用风险

**位置**：`synapse-e2ee/src/crypto/aes.rs:328-342`（构造）+ `synapse-e2ee/src/vodozemac_megolm.rs:127-139`（使用）

```rust
pub fn generate_aes_gcm_nonce(&self) -> Result<Aes256GcmNonce, CryptoError> {
    let counter = self.counter.fetch_add(1, Ordering::SeqCst);   // 从 0 开始
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes[0..4]);               // 仅 4 字节随机
    nonce_bytes[4..12].copy_from_slice(&counter.to_be_bytes());   // 8 字节计数器
    self.tracker.check_and_record(&nonce_bytes)?;
    Ok(Aes256GcmNonce { bytes: nonce_bytes })
}
```

而使用方持有一个**配置里读出来的持久服务器密钥**：

```rust
// vodozemac_megolm.rs:127
encryption_key: Option<[u8; 32]>,        // 持久密钥
// vodozemac_megolm.rs:139
aes_cipher: crate::crypto::Aes256GcmCipher::default(),   // 每次 new() 都是新的 tracker，counter 归零
```

**问题**：nonce 唯一性 = **4 字节随机前缀 + 8 字节计数器**。同一实例内计数器单调递增，不会撞；但 `MegolmVodozemacService::new()` 每次重建都会让计数器**归零**，而密钥不变。此时唯一性只剩下 32 位随机前缀。

碰撞概率（N = 实例重建次数，K = 每次重建后的加密条数）：

$P \approx \frac{N^2}{2} \cdot K \cdot 2^{-32}$

| N（重建次数） | K（每次条数） | 碰撞概率        |
| ------- | ------- | ----------- |
| 100     | 1,000   | 0.12%       |
| 1,000   | 1,000   | **11.6%**   |
| 1,000   | 10,000  | **趋近 100%** |

**影响**：AES-GCM 在同一密钥下重用 nonce 是**灾难性**的——会直接泄露认证子密钥，攻击者可伪造任意密文。而 `NonceTracker` 只在进程内工作，跨重启完全失效，防不住这个场景。

**触发条件**：`E2EE_DUAL_WRITE=true`（该路径的开关）+ 服务反复重启 / k8s Pod 频繁重建。目前双写默认关闭，所以是**潜伏风险**，但代码里已明确支持开启（注释写着"当 `E2EE_DUAL_WRITE=true` 时必须设置"），一旦上生产就会暴露。

**修复**（任选其一，推荐 A）：

```rust
// 方案 A：直接用 96 位全随机 nonce —— 符合 NIST SP 800-38D 对 GCM 的标准做法
// 在 2^32 条消息内碰撞概率可忽略，且不依赖任何跨重启状态
pub fn generate_aes_gcm_nonce(&self) -> Result<Aes256GcmNonce, CryptoError> {
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);
    self.tracker.check_and_record(&nonce_bytes)?;
    Ok(Aes256GcmNonce { bytes: nonce_bytes })
}

// 方案 B：保留计数器方案，但把计数器持久化到 DB / Redis，进程重启后继续递增
// 方案 C：tracker 提为进程级全局单例（OnceLock），至少覆盖单进程内多实例场景
```

---


### 3. 本地限流 token bucket 的 TOCTOU 竞态——限流可被并发绕过

**位置**：`synapse-cache/src/lib.rs:1572-1590`

```rust
let state = self.rate_limit_local.get(key)
    .unwrap_or(LocalRateLimitState { tokens: burst_size as f64, last_ms: now_ms });

let delta_ms = now_ms.saturating_sub(state.last_ms);
let refill = (delta_ms as f64 / 1000.0) * (rate_per_second as f64);
let mut tokens = (state.tokens + refill).min(burst_size as f64);
let allowed = tokens >= 1.0;
// ...
if allowed { tokens -= 1.0; }

self.rate_limit_local.insert(key.to_string(), LocalRateLimitState { tokens, last_ms: now_ms });
```

**问题**：`get` 与 `insert` 是两个独立操作，中间没有原子性保证。`moka::sync::Cache` 保证单个操作线程安全，但**不保证 read-modify-write 序列**。

Tokio 多线程 runtime 下，同一 IP 的并发请求会跑在不同 worker 线程上：N 个请求可以同时读到 `tokens = 1.0`，全部判定 `allowed = true`，然后全部插入。结果是 **burst 限制被放大到接近并发数**。

补充一点：`get` 和 `insert` 之间没有 `.await`，所以单个 task 内部不会被挂起——但**不同 worker 线程之间是真正并行的**，指令级交错无法靠"没有 await"来排除。

**影响**：

- Redis 不可用时（降级到本地桶）限流形同虚设，这正是最需要限流生效的时刻（后端已经不健康）
- 突发流量下实际放行量远超 `burst_size`
- 缓存容量已用 moka 做了有界处理（100k 条 / 300s TTL，审查 #6 已修），所以不是 OOM 问题，是**正确性问题**

**修复**：改用 moka 的原子 entry API（项目用的 moka 0.12 支持）：

```rust
use moka::sync::sync_ops::compute::Op;

let decision = self
    .rate_limit_local
    .entry_by_ref(key)
    .and_compute_with(|existing| {
        let prev = existing.unwrap_or(LocalRateLimitState { tokens: burst_size as f64, last_ms: now_ms });
        let delta_ms = now_ms.saturating_sub(prev.last_ms);
        let refill = (delta_ms as f64 / 1000.0) * (rate_per_second as f64);
        let mut tokens = (prev.tokens + refill).min(burst_size as f64);
        let allowed = tokens >= 1.0;
        if allowed { tokens -= 1.0; }
        let retry_after_seconds = if allowed || rate_per_second == 0 { 0 }
            else { ((1.0 - tokens) / rate_per_second as f64).ceil().max(1.0) as u64 };
        (Op::Put(LocalRateLimitState { tokens, last_ms: now_ms }),
         RateLimitDecision { allowed, retry_after_seconds, remaining: tokens.floor().max(0.0) as u32 })
    });

// Redis 路径同样需要检查 token_bucket_take 是否用了 Lua 脚本保证原子性
```

> 顺带确认：Redis 后端走的是 `redis.token_bucket_take(...)`，需要核对其实现是否为 Lua 脚本（`EVAL`）。若也是 GET + SET 分离，同样有此竞态，只是窗口更小。

---

## 🟡 Medium


### 4. burn-after-read 逐行 4 次串行 DB 写，且无事务包裹

**位置**：`synapse-services/src/burn_after_read_service.rs:208-270`

```rust
for row in &expired_rows {
    self.event_writer.redact_event_content(&row.event_id, Some(&row.user_id)).await      // 1
    self.event_writer.create_event(...).await                                            // 2
    self.storage.mark_burn_processed(row.id).await                                       // 3
    self.storage.log_burned_event(&row.user_id, &row.room_id, &row.event_id, now).await  // 4
}
```

**影响**：

- **性能**：N 条过期记录 = 4N 次串行数据库往返。1,000 条积压 → 4,000 次往返，按 1ms RTT 算就是 4 秒独占 worker。这是后台任务，但会拖慢整个队列消费。
- **正确性（更严重）**：四步没有事务。若 `redact_event_content` 和 `create_event` 成功但 `mark_burn_processed` 失败，该行仍标记为未处理，**下一轮会对同一 event 再生成一条 redaction 事件**——重复 redaction 会污染房间时间线。错误只是 `tracing::warn!` 记了日志，没有任何重试上限或死信隔离。

**修复**：

1. 单条记录的四步包进一个事务；
2. 整批尽量合并——`redact_event_content` 与 `create_event` 可按 room 批量化，`mark_burn_processed` 改成 `WHERE id = ANY($1)` 一次更新；
3. 给失败行加重试计数，超过阈值转入死信，避免无限重复 redact。

```rust
let mut tx = pool.begin().await?;
for row in &expired_rows {
    // 同一事务内完成该行的全部写操作
}
// 批量标记
sqlx::query("UPDATE burn_after_read SET processed = true WHERE id = ANY($1)")
    .bind(&processed_ids).execute(&mut *tx).await?;
tx.commit().await?;
```

---


### 5. NonceTracker 剪枝在加密热路径同步执行，造成周期性延迟尖峰

**位置**：`synapse-e2ee/src/crypto/aes.rs:260-289`

```rust
pub fn check_and_record(&self, nonce: &[u8]) -> Result<(), CryptoError> {
    if !self.used_nonces.insert(nonce.to_vec()) { return Err(CryptoError::NonceReuseDetected); }
    if self.used_nonces.len() >= self.max_history_size { self.prune_old_nonces(); }  // ← 同步剪枝
    self.counter.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

fn prune_old_nonces(&self) {
    let to_remove = current_size - self.max_history_size / 2;         // 默认 5000
    let keys_to_remove: Vec<Vec<u8>> = self.used_nonces.iter()
        .take(to_remove).map(|entry| entry.clone()).collect();        // ← 5000 次堆分配
    for key in keys_to_remove { self.used_nonces.remove(&key); }      // ← 5000 次 remove
}
```

**影响**：

- **延迟尖峰**：每 10,000 次加密触发一次，一次性 collect 5,000 个 `Vec<u8>`（每个一次堆分配）+ 5,000 次 remove，同时长时间持有 DashSet 分片锁。加密路径上出现周期性的毫秒级卡顿，其他并发加密全部被阻塞。
- **语义缺陷**：`DashSet` 的迭代顺序是哈希序，**不是插入序**。所以"剪枝"删掉的是随机 5,000 条，不是最旧的。被删的 nonce 之后可以被无检测地重用——削弱了 nonce 重用检测的保障力度。
- **每次加密的堆分配**：`DashSet<Vec<u8>>` 的 key 是 `Vec<u8>`，每条消息一次堆分配 + 一次哈希。`[u8; 12]` 是 `Copy` 的栈类型，完全没必要用 `Vec`。

**修复**：

```rust
// 1) key 改为定长数组，消灭每次加密的堆分配
pub struct NonceTracker { used_nonces: DashSet<[u8; 12]>, /* ... */ }

// 2) 改用有界 FIFO 队列做剪枝，保证删的是最旧的；或把剪枝挪到后台任务
//    简单做法：追加一个 VecDeque 记录插入顺序，剪枝时从队头 pop
```

---

### 6. 限流中间件每请求深拷贝整个 `RateLimitConfig`

**位置**：`src/web/middleware/rate_limit.rs:13`

```rust
pub async fn rate_limit_middleware(State(ctx): State<CoreContext>, request: Request<Body>, next: Next) -> Response {
    let config = ctx.config.rate_limit.clone();   // ← 每个请求一次深拷贝
```

`RateLimitConfig` 含 5 个堆分配字段：

```rust
pub endpoints: Vec<RateLimitEndpointRule>,
pub ip_header_priority: Vec<String>,
pub exempt_paths: Vec<String>,
pub exempt_path_prefixes: Vec<String>,
pub endpoint_aliases: HashMap<String, String>,
```

**影响**：每个 HTTP 请求都要做 5 次堆分配 + 全部字符串深拷贝。这是全站最高频的代码路径之一（所有请求都要过），纯属无谓开销。而且紧接着第 14 行 `ctx.rate_limit_config()` 又取了一份，两处配置来源并存。

**修复**：改成引用。配置是启动时加载、运行期只读的，没有任何理由 clone。

```rust
let config = &ctx.config.rate_limit;
// 后续 file_config.as_ref().map_or(config.enabled, |c| c.enabled) 等用法不变
```

---

## 🟢 Low

### 7. sliding sync 缓存失效串行删除 N 个 key

**位置**：`synapse-services/src/sliding_sync_service/mod.rs:913-935`

```rust
for prefix in prefixes {
    let keys = self.cache.get_keys_with_prefix(&prefix);
    for key in keys {
        self.cache.delete(&key).await;    // 逐个 await，N 次 Redis 往返
    }
}
for key in exact_keys {
    self.cache.delete(&key).await;
}
```

**影响**：位于写路径（连接状态变更时触发）。每个 key 一次 Redis 往返，串行执行。key 数量随 list/room 数量增长，房间多的账号单次失效可能几十次往返。

**修复**：改用批量删除（Redis `DEL key1 key2 ...` 或 pipeline）。`CacheManager` 里补一个 `delete_batch(&[String])` 方法，L1 走 `invalidate_entries` / Redis 走单次 `DEL`。

---

### 8. 连接池 `max_size` 默认 20，sync 长轮询场景偏紧

**位置**：`synapse-common/src/config/database.rs:174`

```rust
max_size: 20,
min_idle: None,        // 运行时兜底为 5
connection_timeout: 30,
```

`src/server/database.rs` 的池配置本身写得很好（`after_connect` 设了 `statement_timeout=30s`、`lock_timeout=10s`、`idle_in_transaction_session_timeout=60s`，且 `test_before_acquire(false)` 省掉每次取的 ping）。但默认 20 条连接对 Matrix homeserver 偏保守——sync v2 长轮询会长时间持有连接，房间数多的账号一次 sync 可能占住连接数秒。

**建议**：根据实际并发调优，一般建议 `max_size ≈ CPU 核数 × 4`，高并发部署提到 50-100；并监控 `pg_stat_activity` 的连接等待。这条属于配置项而非代码缺陷，优先级最低。

---

## 已验证为「健康」的部分

审计中发现这些常见陷阱**已被正确处理**，记录在此避免重复排查：

| 检查项         | 结论                                                                                      |
| ----------- | --------------------------------------------------------------------------------------- |
| 限流本地桶容量     | ✅ 已用 moka 有界（100k 条 / 300s TTL），非无界 HashMap                                             |
| 限流与认证顺序     | ✅ 限流在认证之前，符合防护意图（防未认证洪水）                                                                |
| CORS / CSRF | ✅ `is_local_bind_address` 正确排除 `0.0.0.0`/`::` 通配地址；`TRUST_FORWARDED_HEADERS` 开启时打 warn  |
| SQL 注入      | ✅ 全量使用 `bind()` 参数化，未发现字符串拼接 SQL                                                        |
| 分页          | ✅ 全库无 `OFFSET` 分页，均用 keyset pagination                                                  |
| 索引覆盖        | ✅ migrations 中 773 条 `CREATE INDEX`，覆盖充分                                                |
| 连接池配置       | ✅ `after_connect` 设置了三级超时，`test_before_acquire(false)`                                  |
| sync 热路径    | ✅ 已做批量化（`get_room_state_events_batch`）、presence dedup 缓存、device list 批量查询，注释明确标注了规避 N+1 |
| 密钥零化        | ✅ `Aes256GcmKey` 实现 `Zeroize` + `ZeroizeOnDrop`                                         |
| 跨 await 持锁  | ✅ 全库 `std::sync::Mutex` 仅 14 处，且均在测试代码或无 await 的短临界区                                    |
| 优雅停机        | ✅ telemetry guard 在 drop 时 flush                                                        |



---

## 建议的下一步

**立即处理（本周）**

1. 修 #1（JSON 膨胀）——改动 3 行，收益明确，无兼容性风险
2. 修 #3（限流 TOCTOU）——Redis 不可用时限流失效是安全相关缺陷
3. 修 #6（每请求 clone）——改动 1 行

**排期处理（本迭代）**  
4\. 修 #2（nonce 方案）——建议直接切 96 位全随机 nonce，彻底消除跨重启状态依赖  
5\. 修 #4（burn-after-read 批处理 + 事务）——重复 redaction 会污染时间线，属于数据正确性问题  
6\. 修 #5（NonceTracker 剪枝）——改 `[u8;12]` key + 后台剪枝

**验证手段**

- #1：跑一遍双写路径，对比 `session_key` 列长度，预期从 ~280 降到 ~60
- #3：并发压测单一 IP（`wrk -t16 -c100`），确认放行量不超过 `burst_size`
- #4：造 1,000 条过期记录，对比优化前后耗时，预期从数秒降到百毫秒级

**尚未覆盖**  
本次为静态审计，未做运行时 profiling。若需要实测热路径，建议补：

- `cargo bench --bench performance_api_benchmarks`
- 生产环境 jemalloc heap profiling（`MALLOC_CONF=prof:true`，`main.rs` 已接入 tikv-jemallocator）
- 慢查询日志分析（`log_min_duration_statement`）
