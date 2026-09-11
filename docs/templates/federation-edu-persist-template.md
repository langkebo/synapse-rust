# 联邦状态落库与广播模板（基于 MSC4262 提炼）

> 用途：为「跨实例状态变更需要 ① 本地落库 ② 通知同房间本地客户端 ③ 广播到远端」的场景提供统一实现骨架。
> 参考实现：`m.profile_update`（MSC4262）。适用前提见 §6：仅「房间可见的公开态」才走本模板；「用户私有态」不应套本模板。

---

## 一、整体链路（三层对称）

```
Service 层写操作
  ├─ (落库) storage.upsert_xxx()            # 本地持久化
  ├─ (通知本地) notify_xxx() → event_notifier # 唤醒同房间本地客户端 / sync 流
  └─ (广播远端) broadcast_xxx_edu()           # 经 EDU 推给共享房间的远端 server

远端 server 收到 EDU
  ├─ 解析 + 校验 origin
  ├─ (落库) apply_xxx_from_federation()      # UPDATE-only，返回 bool
  ├─ (更新缓存) cache.set/delete
  └─ (递增 stream) insert_device_list_change() # 让本端客户端也感知
```

关键点：**写入端广播、接收端落库 + 递增 stream**，两端对称，避免「远端改了、本地 sync 永远看不到」。

---

## 二、五个触点清单（新增一种联邦状态时逐项打勾）

| # | 文件 | 改动 | 参考（MSC4262 已落地） |
|---|------|------|------------------------|
| 1 | `synapse-federation/src/edu.rs` | `EduType` 新增变体 + `FromStr` 分支 + Display | `ProfileUpdate` / `"m.profile_update"` |
| 2 | `synapse-services/src/<域>_service.rs` | 写操作里调用 `broadcast_xxx_edu()`（best-effort，不 `?`） | `update_profile()` → `broadcast_profile_update_edu()` |
| 3 | `src/federation/edu.rs` | `dispatch` 表新增 `EduType::Xxx => handle_xxx_edu`；实现 handler | `EduType::ProfileUpdate => handle_profile_update_edu` |
| 4 | `synapse-storage/src/<域>/storage.rs` | `apply_xxx_from_federation() -> Result<bool>`：**trait 声明 + Fake impl + 单测 mock** 三处同步。**trait 方法必须带文档注释说明 `bool` 语义**（见 §3.1）——实现者与 Fake 只看 trait 签名，语义不写在 trait 上就会漂移 | trait 声明 `user/storage.rs:148-157` / 具体实现 `:763` / Fake `user_store_fake.rs:286` |
| 5 | `synapse-services/src/container.rs` | 注入 `federation_broadcaster` / `server_name`（若新服务未接） | `set_federation_broadcaster()` |

---

## 三、落库函数骨架（storage 层）

**UPDATE-only 语义**（不要为未知远端实体 INSERT 占位行）——理由：
1. `users.username` 有全局 UNIQUE 约束，不同 domain 的 localpart 可能冲突；
2. MSC 语义是「刷新缓存」，不是「物化未知账号」；
3. 返回 `bool` 让 handler 区分「已落库→递增 stream」与「未知→仅失效缓存」。

```rust
/// 接收远端 <状态> 变更并刷新本地 <表>。
/// 返回 true 表示命中已存在的行并已更新；false 表示本地从未见过该实体
/// （不物化未知远端实体，交由调用方决定是否失效缓存）。
pub async fn apply_<state>_from_federation(
    &self,
    <key>: &str,
    <field_a>: Option<&str>,
    <field_b>: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let now = synapse_common::current_timestamp_millis();
    // COALESCE：只覆盖本次携带的字段，缺省字段保留原值
    let result = sqlx::query(
        r"UPDATE <table>
            SET field_a = COALESCE($1, field_a),
                field_b = COALESCE($2, field_b),
                updated_ts = $3
          WHERE <key> = $4",
    )
    .bind(field_a).bind(field_b).bind(now).bind(<key>)
    .execute(&*self.pool).await?;

    if result.rows_affected() == 0 {
        return Ok(false);
    }
    // 读写对称：写完立即回填 L1+L2 缓存（set 是异步共享写）
    if let Some(row) = self.get_<entity>(<key>).await? {
        let key = format!("<entity>:<state>:{<key>}");
        if let Err(e) = self.cache.set(&key, &row, CACHE_TTL).await {
            ::tracing::warn!(target: "cache", error = %e, "回填缓存失败");
        }
    }
    Ok(true)
}
```

> 注意 Cache 读写对称铁律：这里用 `cache.set`（异步写 L1+L2）。跨实例读必须 `get_raw_shared().await`，不能用同步 `get_raw`。

### 3.1 `apply_xxx_from_federation` 的 `bool` 返回值契约（trait 侧必须写明）

`Result<bool, _>` 的 `bool` 是**接收端 handler 的分支依据**，且它同时存在于三层，语义一旦漂移会导致「未知实体 → 仍递增 stream」或「已知实体 → 只失效缓存」两类错误。因此 **trait 声明处写死契约**，不要只写在具体 impl 上（实现者/后续 Fake 只看 trait）：

```rust
/// 接收远端 <状态> 变更并刷新本地 <表>（UPDATE-only，绝不 INSERT 占位行）。
///
/// # 返回值
/// - `Ok(true)`：命中已存在的行并已更新（调用方应递增 device-list/stream）
/// - `Ok(false)`：本地从未见过该实体（调用方应仅失效缓存，不得递增 stream）
/// - `Err(e)`：DB 错误（handler 计 errored 并触发 backoff）
async fn apply_<state>_from_federation(
    &self,
    <key>: &str,
    <field_a>: Option<&str>,
    <field_b>: Option<&str>,
) -> Result<bool, sqlx::Error>;
```

MSC4262 落地参考：trait 文档见 `synapse-storage/src/user/storage.rs:148-157`，三处签名（trait `:152` / 具体 `:763` / Fake `user_store_fake.rs:286`）语义一致。

---

## 四、EDU handler 骨架（接收端）

```rust
async fn handle_<state>_edu(ctx: &FederationContext, origin: &str, edu: &Value, _remaining: usize) -> EduProcessResult {
    let content = match edu.get("content") {
        Some(c) => c,
        None => { increment_counter(ctx, "federation_inbound_<state>_dropped_total");
                  return EduProcessResult { dropped: 1, ..Default::default() }; }
    };
    let <key> = match content.get("<key>").and_then(|v| v.as_str()) {
        Some(k) => k,
        None => { increment_counter(ctx, "federation_inbound_<state>_dropped_total");
                  return EduProcessResult { dropped: 1, ..Default::default() }; }
    };
    // 安全：校验 user/entity 属于声明的 origin，否则丢弃（防伪造）
    if !user_matches_origin(<key>, origin) {
        increment_counter(ctx, "federation_inbound_<state>_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let updated = match ctx.<service>.apply_<state>_from_federation(<key>, field_a, field_b).await {
        Ok(u) => u,
        Err(e) => {
            ::tracing::warn!(error = %e, "落库失败");
            increment_counter(ctx, "federation_inbound_<state>_error_total");
            return EduProcessResult { errored: 1, ..Default::default() };
        }
    };

    if !updated {
        // 未知实体：仅失效（可能存在的）负缓存，仍计 processed，绝不 INSERT
        let _ = ctx.cache.delete(&format!("<entity>:<state>:{<key>}")).await;
        increment_counter(ctx, "federation_inbound_<state>_processed_total");
        return EduProcessResult { processed: 1, ..Default::default() };
    }

    // 命中并已落库：递增 stream，让本端共享房间的客户端下次 sync 感知
    let stream_id = current_timestamp_millis();
    if let Err(e) = ctx.device_storage
        .insert_device_list_change(<key>, None, "<state-kind>", stream_id).await {
        // best-effort：profile 已落库，stream bump 失败仅告警
        ::tracing::warn!(error = %e, "stream 递增失败");
    }
    increment_counter(ctx, "federation_inbound_<state>_processed_total");
    EduProcessResult { processed: 1, ..Default::default() }
}
```

四个 Prometheus 计数器命名规范：`federation_inbound_<state>_{processed,dropped,error}_total`。

---

## 五、广播函数骨架（发起端）

```rust
async fn broadcast_<state>_edu(&self, <key>: &str) {
    let broadcaster = match self.federation_broadcaster.read().unwrap().as_ref() {
        Some(b) => b.clone(),
        None => return, // federation 未启用/未注入：静默跳过
    };
    let server_name = self.server_name.read().unwrap().clone();
    if server_name.is_empty() { return; }

    // 读取「落库后」的最新值放进 EDU（不是入参，避免并发下的旧值覆盖）
    let (a, b) = match self.storage.get_<entity>(<key>).await {
        Ok(Some(e)) => (e.field_a, e.field_b), _ => (None, None),
    };
    let edu = serde_json::json!({
        "edu_type": "m.<state>",
        "content": { "<key>": <key>, "field_a": a, "field_b": b,
                     "origin_server_ts": current_timestamp_millis() }
    });

    // destinations = 共享房间里的远端 server（去重、排除自己）
    // 用 broadcast_edu(destination, &edu, &server_name)
    for destination in destinations {
        if let Err(e) = broadcaster.broadcast_edu(&destination, &edu, &server_name).await {
            ::tracing::warn!(%e, %destination, "广播失败"); // best-effort
        }
    }
}
```

**best-effort 原则**：广播失败绝不能回滚或 `?` 传播本地写——状态一致性靠「下一次 EDU + TTL 缓存过期」最终收敛。

---

## 六、适用性判定：状态可见性决定「要不要 EDU」

> **套用本模板前必须先判断：这个状态是「房间可见的公开态」还是「用户私有态」。**
> 只有公开态（如 profile 变更需要共享房间的成员感知）才需要 EDU 跨服务器广播。

### 6.1 ⚠️ MSC 编号语义漂移（已核正，勿再误用）

早期路线图文档曾把「线程订阅/退订跨服务器同步」标注为 **MSC4155/4156**。经与
[matrix-spec-proposals](https://github.com/matrix-org/matrix-spec-proposals) 核对：

| 编号 | 官方真实标题 | 与本仓 compat stub 路径的关系 |
|------|-------------|------------------------------|
| MSC4155 | **Invite filtering**（Johennes, 2024-06） | 与本仓 `org.matrix.msc4155/rooms/{room_id}/threads` 路径名**语义不符** |
| MSC4156 | **Migrate server_name to via**（Johennes, 2024-08 merged） | 与本仓 `org.matrix.msc4156/threads/subscribed` 路径名**语义不符** |
| MSC3773 | **Notifications for threads**（clokep, 2022-09 merged） | 才是线程通知/未读计数的官方来源 |

**结论**：本仓 `unstable/org.matrix.msc4155|4156/...` 两个 compat stub 路径是**客户端本地读接口**，借用了未占用的 MSC 号段命名，**不是**联邦状态，也不对应官方 MSC4155/4156 的语义。这是「编号-语义分裂」的已知实例（参见 SDK fork 同类问题）。

### 6.2 线程订阅**不应**套用本模板（设计裁定，非待办）

`thread_subscriptions`（`notification_level`/`is_muted`/`is_pinned`/`subscribed_ts`）是**用户私有态**：
- 订阅/退订只影响**订阅者自己**的通知路由，对房间内其他成员不可见；
- Matrix 中私有态经 **CS `/sync` 的 account_data** 下发到用户**自己的**多设备，account_data **本就不跨服务器同步**（用户隶属于单一 homeserver）；
- 因此**不存在**「远端 server 收到他人线程订阅变更」的语义 —— 不需要 `EduType::ThreadSubscription`、不需要 `apply_thread_subscription_from_federation`、不需要 `broadcast_thread_subscription_edu`。

> ✅ **裁定**：路线图 W6「MSC4155/4156 线程订阅跨服务器同步」**关闭**，标记为「经核正：非联邦态，不实现」。当前 `subscribe`/`unsubscribe`/`mute_thread` 的纯 DB + 本地生效实现是正确的。
>
> 若未来确需唤醒该用户**其它设备**，沿用既有 device-list stream（`insert_device_list_change(user_id, None, ...)`）即可，仍不需要 EDU 广播。

---

## 七、验证清单

```bash
cargo check -p synapse-storage -p synapse-services -p synapse-federation --lib --features test-utils
cargo clippy -p synapse-storage -p synapse-services -p synapse-federation --lib --features test-utils
cargo test  -p synapse-services -p synapse-federation --lib --features test-utils
# db_tests 需正确 DB（见 docs/ci-db-testing.md 与 manual-db-test.yml）：
TEST_DATABASE_URL="postgres://synapse:<pw>@<host>:<port>/synapse_test" \
  cargo test -p synapse-storage --lib --features test-utils -- <域>::db_tests
```

---

## 八、drop 路径的「纯校验函数」抽提模式（防御性单测）

`FederationContext` 有 30+ 字段，直接给 handler 写单测成本高。约定：**每个 handler 的结构校验/防伪逻辑都抽成同模块内的纯函数**（`fn(&Value, &str) -> Option<...>`，无 `async`、无 `ctx`、无 DB），handler 的每条 drop 分支都委托它——这样：

1. 单测只测纯函数，无需构造 `FederationContext`；
2. handler 与纯函数共享同一份校验逻辑，**不会出现「测试绿、线上 drop 逻辑漂移」**（这也是为什么不用 `#[allow(dead_code)]` 的独立校验函数——它会变成第二事实来源）。

参考实现：`src/federation/edu.rs` 的 `validate_presence_update` / `extract_typing_room_id` / `filter_typing_user_ids` / `validate_device_list_update_content` / `validate_direct_to_device_content` / `parse_receipt_content` / `validate_signing_key_type` / `parse_signing_key_content` / `validate_profile_update_content`，配套单测见同文件 `#[cfg(test)] mod tests`（覆盖每条 drop 路径：缺字段 / origin 伪造 / localpart-only / 类型错误）。
