# S 系列问题核查与优化方案（presence / sync / 限流 / MSC4156）

> **日期**: 2026-09-11
> **核查提交**: `30c90bc0`（核查时工作树状态）
> **核查方法**: 逐项**代码取证 + 运行时实测**，不采信问题清单本身。
> 每项均给出 `file:line` 引用与可复现命令；与原清单不符处**明确推翻**。

---

## 0. 结论总表

| 原编号 | 原描述 | 核查结论 | 真问题 |
|---|---|---|---|
| S-2 | v2 sync presence 只下发自身、忽略 since、每次全量、无订阅扇出 | ❌ **推翻**（4 个子论断全不成立） | 扇出目标列表缓存 60s；无 presence stream 游标（用 1800s 去重缓存） |
| S-6 | v2 sync 250ms DB 轮询，未接 EventNotifier | ❌ **推翻**（事件驱动已接入） | 仅无 notifier 接线时（测试/基准）才走 250ms 降级 |
| S-7 | `EventNotifier::with_redis()` 全仓无人调用 | ❌ **推翻**（生产有调用 + 订阅端） | 无 |
| S-8 | presence 去重缓存读写不对称 | ❌ **推翻**（生产零 `.get_raw()` 调用） | 同步 `get_raw()` 仍是 L1-only，属未来踩坑点 |
| S-9 | 限流三件套 + 429 与长轮询互为掩护 | ⚠️ **部分成立，且比原描述更严重** | ✅ **`/sync` 完全无限流**（实测 120/120 全 200） |
| — | MSC4156 server_name→via：join 从 body 读、knock 忽略 via、无 msc4156 路由 | ⚠️ **部分成立**（三处均有偏差） | ✅ join 读的是非规范键 `via_servers`；✅ knock 忽略 `via`；`msc4156` 路由存在但**语义错标** |

**总结**：S-2/S-6/S-7/S-8 描述的是 **2026-08-10 之前**的状态，已由提交
`ecca8751`（presence 扇出）与 `a77e8b22` 修复，且带测试。**真正需要修的是 S-9 与 MSC4156**。

---

## 1. 被推翻的四项（附反证）

### 1.1 S-2 — presence 扇出**已实现**，非"只下发自身"

反证（`synapse-services/src/sync_service/data_fetch.rs`）：

```rust
// :198-230  presence_fanout_targets()
let mut set: HashSet<String> = match self.member_storage.get_shared_room_users(user_id).await { ... };
match self.presence_storage.get_subscriptions(user_id).await { Ok(subs) => set.extend(subs), ... }
set.insert(user_id.to_string());
```

* 共享房间成员：`synapse-storage/src/membership/mod.rs:333-347`（`room_memberships m1/m2` 双 join，均 `membership='join'`）
* 显式订阅：`synapse-storage/src/presence/mod.rs:496-506`（`presence_subscriptions`）；好友会写入该表
  （`friend_room_service/mod.rs:521,715`）

`since` **并非被忽略**——它决定增量/全量（`:265-291`）：

```rust
let changed_senders: Option<HashSet<String>> = if since.is_some() {
    let prev = self.cache.get(&dedup_cache_key).await...;
    // 只发状态变化或新增目标
    if changed.is_empty() { return Ok(Vec::new()); }   // :281-283
} else {
    // 初始 sync：全量 + 播种去重缓存
};
```

**残留真问题（低）**：

| 项 | 说明 |
|---|---|
| 扇出目标列表缓存 60s（`:199-202,225`） | 成员关系变化最坏 60s 后才反映到 presence 扇出 |
| 无 presence stream 游标 | 去重靠 1800s TTL 缓存；冷缓存（重启/驱逐）时回退为**全量扇出** |
| best-effort 降级 | 成员查询失败时降级为"只发自己"（记录 warn），即旧行为 |

> 这三条是自觉的设计取舍（代码注释已说明），不是原清单描述的"缺失"。

### 1.2 S-6 — 长轮询**是事件驱动的**

`synapse-services/src/sync_service/event_fetch.rs:172-193`：

```rust
if long_poll_waiters.is_empty() {
    let poll_interval = self.sync_poll_interval();
    tokio::time::sleep(poll_interval.min(remaining)).await;   // 仅此分支轮询
} else {
    tokio::select! {
        _ = futures::future::select_all(long_poll_waiters.iter_mut()) => { /* 被事件唤醒 */ }
        _ = tokio::time::sleep(remaining) => return Ok(IncrementalUpdate::Timeout),
    }
}
```

生产接线：`synapse-services/src/wiring/rooms.rs:160` `event_notifier: Some(...)`；
且 `EventNotifier::slots_for` 至少返回 1 个 slot（`event_notifier.rs:160-167`），
故 `long_poll_waiters` **不可能为空**，轮询分支在生产不可达。250ms 默认值
（`synapse-common/src/config/performance.rs:49-50`）只服务测试/基准。

### 1.3 S-7 — `with_redis()` **有生产调用**

```console
$ grep -rn "with_redis\|start_redis_subscriber" --include='*.rs' src/ synapse-*/src | grep -v CacheManager
synapse-services/src/event_notifier.rs:102   pub fn with_redis(...)          # 定义
synapse-services/src/event_notifier.rs:305   pub fn start_redis_subscriber(..)# 定义
synapse-services/src/container.rs:357                .with_redis(pool, redis_url)   # 生产调用
synapse-services/src/container.rs:359            notifier.start_redis_subscriber(...) # 生产调用
```

订阅端实现完整（`event_notifier.rs:305-436`，含自我回声跳过 `:416`），
发布端 `notify_room/notify_user → publish_redis`（`:233-243, 446-511`）均活。
测试证据：`s8_handle_redis_message_*` 三个用例（`:787,813,841`）。

### 1.4 S-8 — presence 去重**读写对称**

`get_raw()` 确实是 L1-only（`synapse-cache/src/manager.rs:408-410`），但**生产代码零调用**：

```console
$ grep -rn "\.get_raw(" --include='*.rs' src/ synapse-*/src | grep -v get_raw_shared
synapse-cache/src/manager.rs:229,409,423,518,546,588,801   # 缓存自身内部
synapse-services/src/auth/token.rs:399,429                 # 测试断言
```

两条 presence 去重路径都对称：

| 路径 | 读 | 写 |
|---|---|---|
| v2 sync | `cache.get()`（L1→L2，`manager.rs:514-537`） | `cache.set(..., 1800)`（L1+L2，`:627-642`） |
| sliding sync | `get_raw_shared()`（L1→L2 并回填） | `set_raw()`（`extensions.rs:246,250`） |

**残留（低）**：同步 `get_raw()` 仍是 L1-only 的公开 API，未来调用者可能重新引入跨实例 bug。建议加 `#[deprecated]` 或改名。

---

## 2. ✅ 真问题一：`/sync` 完全无限流（P1）

### 2.1 机制

1. `/sync` 与 sliding sync 在路由 ledger 中被标记 `rate_limit_exempt`
   （`src/web/routes/sync.rs:71,81`；`sliding_sync.rs:55`），由 `assembly.rs:421-433`
   收集后注入中间件，**绕过通用 IP 限流**（`rate_limit.rs:35`）。
2. `/sync` 的专用限流在 handler 内（`handlers/sync.rs:119-150`），但它由
   `rate_limit.yaml` 的 `sync.enabled` 控制（`:39-49`）。
3. 两份 `rate_limit.yaml` 原先都是 `sync.enabled: false`，而两份
   `homeserver.yaml` 写的是 `sync.enabled: true` —— 因为文件配置**整体替换**
   `rate_limit:` 段（`rate_limit.rs:59-68`，`map_or` 逐字段），**文件赢**。

⇒ 结果：`/sync` 既不在通用 IP 限流内，专用限流又被关闭 → **零限流**。

### 2.2 实测（修复前）

```console
$ # 自建临时用户取 token，然后：
$ for i in $(seq 1 120); do curl -s -o /dev/null -w "%{http_code}\n" \
    -H "Authorization: Bearer $TOKEN" \
    "http://localhost:8008/_matrix/client/v3/sync?timeout=0"; done | sort | uniq -c
    120 200                      # ← 零限流

$ # 对照：同样 120 次 /versions（通用 IP 限流生效）
    39 200
    81 429
```

**影响**：一个已认证用户用 `timeout=0` 紧循环即可制造无上限的 DB 读放大
（每次 `/sync` 至少触发一次事件可见性查询）。这不是"配置冗余"，是
**可复现的 DoS 防护缺口**。

### 2.3 修复

`docker/deploy/config/rate_limit.yaml`（生产）：`sync.enabled: false → true`。
`docker/config/rate_limit.yaml`（本地开发）：保留 `false` 但**注明原因与非生产属性**。

### 2.4 修复后实测

```console
$ # 修复后，同一探测用户 + 120 次初始 sync
    15 200
   105 429                      # M_LIMIT_EXCEEDED
$ curl ... /sync?timeout=0
{"errcode":"M_LIMIT_EXCEEDED","error":"Rate limited","retry_after_ms":1000}

$ # 正常增量轮询不被误伤：40 次（1/2s 节奏 10 次 + 连续 30 次）
   40 200
$ # 10 次 1/2s 慢节奏逐条
poll1..poll10 全部 200
```

> 初始 sync 规则 `per_second: 5, burst_size: 10` 会限制紧循环；
> 增量规则 `per_second: 50, burst_size: 100` 对正常长轮询客户端足够宽松
> （每分钟 50 次增量的客户端恰好命中 1 次、随时可用）。

### 2.5 派生的次要问题（本次未修，已记录）

| # | 问题 | 证据 |
|---|---|---|
| a | `homeserver.yaml` 的整个 `rate_limit:` 段运行时**永不生效**（文件缺失时也用硬编码默认值，`server/mod.rs:211-243` 两分支都返回 `Some`） | `rate_limit.rs:59-68` 的 `None` 分支是死代码 |
| b | `RateLimitConfigAdapter` 声明并导出但**从未构造**（`rate_limit_config.rs:422-466`，`lib.rs:215`） | 死表面 |
| c | `RateLimitConfig` 无 `backend` / `reload_interval_seconds` 字段且未 `deny_unknown_fields` → 写成 `rate_limit.backend:` 会被静默忽略 | `config/rate_limit.rs:21-70` |
| d | 至少 **4 套** HTTP 限流机制（通用 IP、联邦 per-origin、sync、sliding sync）+ ad-hoc 桶（`friend_room.rs:632`、`search.rs:294,363`）+ 登录锁定（`auth_compat.rs:377`），原清单说"三件套"是低估 | 见上 |

> (a) 已在 `docker/deploy/config/homeserver.yaml:39-42` 由前序修复加注说明；
> 本次为 `docker/config/homeserver.yaml` 补上同类注释，并**删除其中已失效的
> 4 条认证端点覆盖**（避免读者以为 `burst_size: 3` 生效——实际生效的是
> `rate_limit.yaml` 的 `per_second: 5, burst_size: 50`，宽松 16 倍）。

---

## 3. ✅ 真问题二：join/knock 的 `via` 处理不合约（P2）

### 3.1 规范依据

Matrix `POST /_matrix/client/v3/join/{roomIdOrAlias}` 与
`/knock/{roomIdOrAlias}` 的 `via` 是**重复 query 参数**
（`?via=srv1&via=srv2`），不是 JSON body 字段——见 ruma
`join_room_by_id_or_alias` 与 [MSC4156 "Migrate server_name to via"](https://github.com/matrix-org/matrix-spec-proposals/pull/4156)。

### 3.2 实测缺陷（修复前）

| 端点 | 行为 | 后果 |
|---|---|---|
| `join` | 从 **body 读 `via_servers`**（`members.rs:86-91`，修复前） | 标准客户端 `?via=` 与 `{"via":[...]}` **全被静默忽略**，联邦 join 退化为按 room_id 域名选路（`room/membership/actions.rs:38-44`） |
| `knock` | 只读 `body.reason`（`members.rs:178`），`via` **完全忽略** | 同上；且 `knock_room` 服务签名无 via 参数（`membership/moderation.rs:140`） |

> 有意思的是同一字段在别处**确实**用 `via`：`room/space/children.rs:44`
> `"via": child.via_servers`。所以 `via_servers` 是局部不一致。

### 3.3 修复

新增可测 helper `extract_via_servers(query, legacy_body)`（`members.rs`）：

* **优先**读规范重复 query 参数 `via`（含 `%XX` 百分号解码，`+` 视为空格）
* **回退**读遗留 body 键 `via_servers`（保持已发布客户端可用）
* 畸形百分号序列**原样透传**，不因坏 via 提示把 join 变成 400

`join` 与 `knock` 均加 `Query<Vec<(String, String)>>` 提取器（axum 0.8 的
`Query` 在无 query 时返回空表，`unwrap_or_default()`，不引入破坏性变更）。

**关于 knock**：`knock_room` 目前是**纯本地状态迁移**（无联邦分支），
`via` 暂无可达的使用点。本次让 handler **接受并记录**该参数（不再静默丢弃），
待实现联邦 knock 时再向下传递。这个边界在代码注释中明确标注，避免被误读为"已支持"。

### 3.4 测试

仓内 13 个用例（`members.rs::via_servers_tests`）：重复参数、百分号编码、
空值丢弃、`via` 优先于遗留键、遗留键非数组/含非字符串项、无提示、
无关参数不受影响、畸形百分号透传。

---

## 4. ⚪ MSC4156 路由的语义错标（低，已修注释）

`src/web/routes/handlers/thread.rs:158,242` 声明了
`/_matrix/client/unstable/org.matrix.msc4156/threads/subscribed`，并注释为
"MSC4155 / MSC4156 unstable compat stubs"。

**MSC4156 = "Migrate server_name to via"（join/knock 的 `via`）**，与线程无关；
线程订阅是用户私有态（account_data），不跨服务器同步。该路由是**语义错标**。

处理：**保留路径**（已发布客户端的兼容性），但改写注释明确它不是 MSC4156 表面，
并指向 `members.rs::extract_via_servers`。同时**未**在
`unstable_features` 中声明 `org.matrix.msc4156` —— 因为本服务器的 via 处理
只是部分实现（knock 仍无联邦路径），按项目的"声明纪律"不应过早声明。

> 参考：`capability_governance.rs:107-139, 243-275, 451-465` 三处声明列表均无 msc4156。
> 相关 MSC 编号语义见 AGENTS.md / CLAUDE.md §3（MSC4155=邀请过滤、
> MSC4156=server_name→via）。

---

## 5. 对照上游 element-hq/synapse

| 主题 | Synapse 做法 | 本项目状态 |
|---|---|---|
| 长轮询唤醒 | `synapse/notifier.py` 的 `Notifier` 用 per-user/per-room 的等待器 + replication 通知，`wait_for_events` 由事件唤醒 | ✅ 已对齐（`EventNotifier` + Redis 扇出） |
| 多实例扇出 | `synapse/replication/` 通过 replication stream 把通知转发到各 worker | ✅ 已对齐（Redis pub/sub，`synapse:events:notify`） |
| presence 扇出 | `synapse/handlers/presence.py` 基于共享房间 + `presence_stream` 游标增量 | ⚠️ 部分对齐：共享房间扇出有，**无 stream 游标**（用 30min 去重缓存替代） |
| `/sync` 限流 | Synapse 对 `/sync` 走 `ratelimiter`（`rc_*` 配置），并配合 `SyncHandler` 的 per-device 逻辑 | ❌ 修复前零限流（本次已修） |
| join/knock `via` | 按规范读 query 参数 `via`，用于联邦选路 | ⚠️ join 读错键、knock 忽略（本次已修 join，knock 仅接受） |

> Synapse 参考：[notifier.py](https://github.com/matrix-org/synapse/blob/develop/synapse/notifier.py)、
> [handlers/presence.py](https://git.tilera.org/MirrorHub/synapse/src/commit/d40878451c1f76f10cfa1bb6befc9627fc13a104/synapse/handlers/presence.py)、
> [storage/databases/main/presence.py](https://github.com/matrix-org/synapse/blob/d323cdcdb3481201521b2648bc89975376059495/synapse/storage/databases/main/presence.py)、
> [MSC4156 PR](https://github.com/matrix-org/matrix-spec-proposals/pull/4156)

---

## 6. 优化方案（按优先级）

### P1（已在本次修复）

| # | 项 | 状态 |
|---|---|---|
| 1 | 生产配置恢复 `/sync` 限流 | ✅ 已修（`docker/deploy/config/rate_limit.yaml`）+ 回归测试 |

### P2（建议下一轮）

| # | 项 | 理由 | 落点 |
|---|---|---|---|
| 2 | **给 presence 加 stream 游标** | 去掉 1800s 去重缓存与冷缓存全量回退；对齐 Synapse 的 `presence_stream` | `sync_service/data_fetch.rs:239-291`；新增 `presence_stream` 表与游标 |
| 3 | **联邦 knock + via 选路** | 当前 knock 纯本地，标准客户端带 `via` 无效 | `membership/moderation.rs:140` 加 via 参数 + 联邦分支 |
| 4 | **配置单一真相源** | `docker/config/` 与 `docker/deploy/config/` 靠手工 `cp` 同步、无 CI 检查（与已根治的迁移双副本同型） | 挂载共用目录，或加 `check_config_consistency.py` ✅ **已完成**（提交 `99bb8a09`，见 `docs/audit/P5_config_consistency_gate_2026-09-11.md`） |
| 5 | **让 `homeserver.yaml` 的 `rate_limit:` 段要么生效要么删除** | 现状是"写了但不生效"的陷阱；文件缺失时静默用硬编码默认值（无 metric/health 信号） | `src/server/mod.rs:211-243` 仅在文件真加载成功时 attach manager |

### P3（技术债）

| # | 项 |
|---|---|
| 6 | 给 `CacheManager::get_raw()` 加 `#[deprecated]`（L1-only，易被误用为跨实例读） |
| 7 | 清理 `RateLimitConfigAdapter` 死表面；给 `RateLimitConfig` 补 `backend`/`reload_interval_seconds` 或加 `deny_unknown_fields` |
| 8 | 统一 4 套限流机制 + ad-hoc 桶的配置入口 |
| 9 | `tests/unit/sliding_sync_perf_gate_tests.rs` 类的"镜像逻辑"测试应改为真实子进程执行（参考 `pagination_gate_tests.rs`） |

---

## 7. 复现命令

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 1) 被推翻的四项：直接读代码
sed -n '198,230p;239,291p' synapse-services/src/sync_service/data_fetch.rs   # S-2 扇出 + 去重
sed -n '125,195p'          synapse-services/src/sync_service/event_fetch.rs # S-6 事件驱动
grep -rn "with_redis\|start_redis_subscriber" --include='*.rs' src/ synapse-*/src | grep -v CacheManager  # S-7
grep -rn "\.get_raw(" --include='*.rs' src/ synapse-*/src | grep -v get_raw_shared                         # S-8

# 2) /sync 零限流实测（需 token；注意用完删除临时用户）
TOKEN=$(curl -s -X POST http://localhost:8008/_matrix/client/v3/register \
  -H 'Content-Type: application/json' \
  -d '{"username":"probe","password":"<pwd>","device_id":"P","auth":{"type":"m.login.dummy"}}' \
  | python3 -c 'import json,sys;print(json.load(sys.stdin)["access_token"])')
for i in $(seq 1 120); do curl -s -o /dev/null -w "%{http_code}\n" \
  -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8008/_matrix/client/v3/sync?timeout=0"; done | sort | uniq -c

# 3) 配置矛盾
grep -n -A 3 "^sync:" docker/deploy/config/rate_limit.yaml    # enabled: true（已修）
grep -n -A 3 "^  sync:" docker/deploy/config/homeserver.yaml  # enabled: true（不生效）

# 4) via 处理
sed -n '86,105p' src/web/routes/handlers/room/members.rs      # extract_via_servers
grep -n "via_servers" src/web/routes/handlers/room/members.rs

# 5) MSC4156 语义
grep -rn "msc4156" --include='*.rs' src/ synapse-services/src/

# 6) 回归测试
cargo nextest run --profile test --features test-utils --test unit \
  -E 'test(/sync_rate_limit_config_tests|via_servers_tests/)'
```

---

## 8. 本次副作用与恢复

| 项 | 状态 |
|---|---|
| 临时用户 `@rlprobe:matrix.test`、`@rlprobe2:matrix.test`（用于限流实测） | ✅ 均已在事务中删除（含 `access_tokens`/`refresh_tokens`/`devices`）；`SELECT count(*) FROM users` = 0 |
| `docker/deploy/config/rate_limit.yaml` 语义变更 | ⚠️ **有意的修复**（`sync.enabled: false → true`），需提交 |
| 容器重启 | ✅ healthy；启动日志确认"限流配置加载完成"，reload 错误 0 |
