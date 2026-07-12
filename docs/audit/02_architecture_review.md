# synapse-rust 工程架构审查报告

- 日期：2026-07-10
- 范围：全代码库架构审查（cargo workspace，约 285K 行 Rust）
- 分支：feat/architecture-optimization-round2
- 方法：ServiceContainer / 各 crate Cargo.toml / storage traits / sync 服务 / 联邦签名 / 状态机 / 失败模式，逐项读源码取证（file:line 为证）
- 一句话结论：**分层骨架是健康的（crate 依赖单向、DI 无循环、worker at-least-once、启动有 schema 门禁），真正的债集中在三处——存储层规则 7.1 被明确违反、token 刷新的非事务竞态会误杀整个 token 家族、worker 无死信/无幂等。这三条是"必须修复"。**

先纠正一个规模认知：项目不是 100K 行。根 crate `synapse-rust`（src/ 66K，其中 src/web 60K 是 HTTP 层）+ 6 个 workspace crate（synapse-storage 100K、synapse-services 69K、synapse-common 20K、synapse-e2ee 18K、synapse-federation 7.6K、synapse-cache 3.8K）合计约 285K 行。src/storage、src/services 现在只是 re-export facade，真实实现已迁到同名 crate——这是一次基本完成的"单体→workspace"迁移，方向正确。

---

## 一、Component 图（crate 依赖 + ServiceContainer 装配）

```
                      ┌─────────────────────────────────────────────┐
                      │           synapse-rust (root crate)          │
                      │  src/web (60K, HTTP/axum) · src/server ·      │
                      │  src/bin/synapse_worker · src/tasks           │
                      └───────────────┬─────────────────────────────┘
                                      │ depends on ↓ (单向)
       ┌──────────────┬──────────────┼───────────────┬──────────────┐
       ▼              ▼              ▼               ▼              ▼
 synapse-services  synapse-e2ee  synapse-federation  (root 直接依赖全部 crate)
       │              │              │
       │ depends ↓    │ depends ↓    │ depends ↓
       ├──────────────┴──────────────┤
       ▼                             ▼
 synapse-storage ───────────► synapse-cache ───► synapse-common
       │                             │                 ▲
       └─────────────────────────────┴─────────────────┘
        (storage/cache/common 从不反向依赖 service/web —— 依赖方向干净)

  ✅ synapse-storage/Cargo.toml:34-35  仅依赖 common + cache，零反向依赖
  ✅ synapse-services/Cargo.toml:38-43 无 axum/http/tower 生产依赖（HTTP 概念不泄漏进 service）

ServiceContainer (synapse-services/src/container.rs:30-42) —— 8 字段，分阶段 DAG 装配
  Phase1 Infra ─► Phase2 Storage ─► Phase3 Domain ─► Phase4 Extensions
  ┌── 4 域装配组 ──────────────┐   ┌── 4 横切组 ──────────────────┐
  │ e2ee    (12 字段, 边界清)  │   │ core      (14, 横切基础设施)  │
  │ rooms   (16+, 边界清)      │   │ account   (10, 边界清)        │
  │ federation (6, 边界清)     │   │ sso       (4-6, 边界清)       │
  │ admin   (5 子组~30 字段)   │   │ extensions(18+, ⚠ 杂物抽屉)   │
  └────────────────────────────┘   └───────────────────────────────┘
  ✅ 无 OnceCell/Weak/RwLock<Option> 打破循环的 hack；注释宣称的 DAG 属实
  ⚠ extensions 组 与 admin.module 子组 是两个"杂物抽屉"（功能门控杂项堆放）
```

**Q1 结论**：ServiceContainer **不是上帝对象**——直接扇出仅 8，分阶段线性装配，无循环依赖 hack（wiring/ 里零 `OnceCell`/`Weak`/`RwLock<Option>` 用于打破循环；oidc_service.rs:105 的 `RwLock<Option>` 是 discovery 文档缓存，不是破环）。真正的"上帝上下文"在 web 层的 `RoomContext`（src/web/routes/context.rs:42-82，25+ 字段，从 8 个组任意抽取）。

**Q2 结论**：核心分层在 **crate 依赖级别干净**（storage/services 从不反向依赖）。但 **web 层存在分层违规**：`RoomContext`（context.rs:60,66,79）直接携带 `Arc<dyn UserStore>` / `Arc<dyn RoomStoreApi>` / `StickyEventStoreApi` 等原始存储引用，多个 handler（app_service.rs:14、event_report.rs:13、space.rs:33、push_notification.rs:11、widget.rs:79、threepid.rs:103 等）`use synapse_storage` 绕过 service 直接读写。

---

## 二、Data-flow 图（sync 冗余 + 联邦签名热路径）

```
【Q4】 sync_service 与 sliding_sync_service —— 底层 storage trait 共享，编排层完全重复

  客户端 /sync (SyncToken: stream_id+since)      客户端 /org.matrix.msc3575/sync (pos+ranges)
        │                                                │
        ▼ sync_service/                                  ▼ sliding_sync_service/
  ┌──────────────────────┐                        ┌──────────────────────┐
  │ data_fetch.rs        │                        │ timeline.rs/state.rs │
  │ event_fetch.rs       │   都各写各的编排循环     │ extensions.rs        │
  │ response.rs          │  ◄── 重复 ──►           │ filters.rs           │
  └──────────┬───────────┘                        └──────────┬───────────┘
             │        ┌───── 共享同一批 Arc<dyn ...> storage trait ─────┐
             └────────┤ EventStoreApi · DeviceListStoreApi · ToDevice ·  ├────────┘
                      │ PresenceStoreApi · MemberStoreApi · AccountData  │
                      └──────────────────────────────────────────────────┘
   重复获取的数据类别：timeline / state / global+room account_data /
   to-device / device_lists / presence / unread / typing / receipts（各取一遍）
   分叉（阻止整体合并）：SyncToken(since) vs pos+ranges(MSC3575 窗口语义)；
   lazy-load-members 仅 sync 有；连接跟踪 LRU/GC 仅 sliding sync 有

【Q5】 联邦签名热路径

  出站 PDU:  event ──► canonical_json(内容哈希) ──► canonical_json(sign_json) ──► ed25519 sign
            └─ signing.rs:180 每个出站 PDU 做【两次】canonical_json，每次重新分配+排序 map
  出站请求:  build_auth_header (client.rs:221) ──► canonical_federation_request_bytes(每请求新建 Map)
  入站验签:  verify_event_content_hash (signing.rs:81) ──► 需对端 verify_keys
            ├─ key_cache 有 TTL=3600s (client.rs:176) ✅
            └─ ⚠ query_server_keys (client.rs:424) 不走缓存；build_auth_header 不查 key_cache
               ⇒ 入站事件验签可能每次触发远程 HTTP 拉公钥
  ⚠ 全 federation/common 生产代码无 spawn_blocking：ed25519 微秒级可接受，
     但大事件 canonical_json（同步 CPU）在 async 线程上、且重复多次，是潜在瓶颈
```

---

## 三、State 图（房间成员状态机 + Token 生命周期）

```
【Q6】房间成员状态机（合法转换 = Matrix 规范）

        (null)
         │ invite          ┌──────────── ban ────────────┐
         ▼                 │                             ▼
      invite ── join ──► join ── leave/kick ──► leave ◄── ban(可从任意态)
         │ (拒邀=leave)      │                    │ join(重入,须先 unban)
         │                  └── ban ──► ban ── unban(leave) ──┘
         ▼ knock(若 join_rule=knock)
      knock ──?──► (⚠ 缺 knock 接受/拒绝 handler)

  ✅ 已覆盖：null→invite/join、invite→join、join→leave、leave→join、join→ban、
     ban→leave(unban)、leave→ban、join→kick；PL 检查见 auth/power_levels.rs（
     can_kick:333 / can_ban:393 / can_unban:454，均含 actor>target + 保护 creator）
  ⚠ 缺口1：knock→invite/join/leave 撤回敲门无显式 handler（membership/moderation.rs:82 只到 knock）
  ⚠ 缺口2：联邦【入站】m.room.member 无 check_membership_transition —— 只查 is_auth_event
           (federation/src/event_auth/mod.rs:26)，非法转换靠 auth chain 间接兜，无显式转换表
  ⚠ 缺口3：can_invite_user(power_levels.rs:494) 不检查邀请目标是否已被 ban
  ⚠ 缺口4：unban_user 无 is_banned 前置检查；kick_user 无 target 是否在房间的前置检查（幂等但静默）

【Q7】access/refresh token 生命周期

  refresh_token（有状态，DB）：
    validate_token ──[非原子窗口]──► revoke_token_cas ──► 建新 token + rotation 记录
     (service:107)                    (service:163, CAS: UPDATE..WHERE is_revoked=FALSE)
    ⚠ 竞态：validate 与 CAS 不在同一事务。并发两个 refresh 用同一旧 token：
        第二个 CAS rows_affected=0 ─► 判定 replay ─► 【撤销整个 token 家族 + 该用户所有 token】
        (refresh_token_service.rs:169-188)  合法客户端网络重试即被误杀 → 用户被强制登出

  access_token（无状态 JWT + 黑名单撤销）：
    validate: is_in_blacklist ─► is_token_revoked ─► decode JWT ─► logout_all 缓存标记 ─► 缓存活跃
     (auth/token.rs:14→24→34→44→54)  顺序检查，非原子
    ⚠ TOCTOU：token.rs:54 命中缓存后绕过 DB 检查直到 TTL。/logout 会 cache.delete_token()，
        但【外部直接改 DB 撤销】不会失效缓存 ⇒ 悬空缓存条目在 TTL 内仍放行
```

---

## 四、Sequence 图（失败模式：Redis 降级 / Worker 崩溃 / 迁移失败）

```
【Q8】Redis 不可用降级（synapse-cache/src/lib.rs）
  启动: CacheManager::with_redis (646) ─► RedisCache::new 失败 ─► warn + use_redis=false(一次性)
  运行: get() ─► circuit breaker(374-424) Open ─► is_call_allowed=false ─► 不碰网络
        ─► CacheError ─► CacheManager.get 返回 None(静默 L1 miss)  ✅ 不会每次超时拖慢
  ⚠ L2 写静默丢失；⚠ rate_limit(1125-1180) fallback 内存后【多 worker 全局限流失效】(代码自注释)
  ⚠ cache invalidation 依赖 Redis Pub/Sub，Redis 挂后跨实例一致性广播无 fallback

【Q9】Worker 崩溃（Redis Streams consumer group，synapse-common/src/task_queue.rs）
  produce: XADD mq:tasks:default (243)
  consume: XREADGROUP GROUP synapse_workers (271) ─► 进 PENDING
  success: XACK (290)  ◄── 崩溃发生在此之前 ⇒ 消息留 PENDING，其他 worker XPENDING+XCLAIM 认领
  ✅ at-least-once，崩溃不丢
  ⚠ 无死信队列(295 空注释) ⚠ 无重试(Err 只 log) ⚠ BackgroundJob 无 idempotency_key
     ⇒ 失败任务无限滞留 PENDING；重投重复副作用(SendEmail/RedactEvent 会重复执行)

【Q10】DB 迁移失败（docker/db_migrate.sh，自定义 psql，非 sqlx migrate）
  apply_pending_migrations(182-220) ─► 逐个 apply_sql_file
  ⚠ 迁移文件 --no-transaction(v10.sql:46)，文件内多 DDL 中途失败 ⇒ 部分已提交，不回滚
  ⚠ 无 down/rollback（migrations/ 零 .undo.sql，脚本无 rollback 命令）
  ✅ 幂等：CREATE TABLE/INDEX IF NOT EXISTS、ADD COLUMN 用 DO$$ IF NOT EXISTS ⇒ 可重跑
  启动门禁: server/database.rs:70 run_schema_health_check
    ✅ 缺表/缺列 ─► return Err(94) 阻止启动
    ⚠ 但 health check 自身报错 ─► "非致命，继续启动"(104-107) ⇒ 无法验证时带病启动
    ⚠ 只验"表/列存在"，不验"迁移是否半应用/数据一致" ⇒ 结构在但内容半迁移可蒙混过关
```

---

## 五、问题清单（三档）

### 🔴 必须修复（正确性 / 数据安全 / 用户可感知故障）

| # | 问题 | 证据 | 影响 | 建议 |
|---|------|------|------|------|
| M1 | **规则 7.1 被违反**：RoomStorage 直接查 `events` 表读事件内容 | synapse-storage/src/room/mod.rs:778-801 `copy_room_state`、1002-1043 `get_unread_counts`、1045-1102 `get_unread_counts_batch`（后两者 parse `ev.content::text` 做 mention 检测） | 存储职责边界崩坏，events schema 变更会同时波及 RoomStorage，未读计数逻辑与事件解析耦合 | 把这三处的事件访问下沉到 EventStorage，RoomStorage 通过 EventStoreApi 调用；未读/mention 计算移到 event 域 |
| M2 | **token refresh 非事务竞态 → 误杀 token 家族** | refresh_token_service.rs:107 vs 163（validate 与 CAS 不同事务）；169-188 replay 判定撤销全家族；auth/session.rs:103-127 vs 168 同病 | 合法客户端并发/网络重试同一 refresh_token → 第二个被判 replay → 用户所有 token 被撤销、强制登出 | 把 validate + revoke_token_cas 包进单个 DB 事务（`SELECT ... FOR UPDATE` 或让 CAS 的 rows_affected=0 走"幂等返回已生成的新 token"而非"判定攻击"） |
| M3 | **Worker 无死信 / 无重试 / 无幂等** | task_queue.rs:295 空的失败处理注释；BackgroundJob(background_job.rs:4-10) 无 idempotency_key | 失败任务无限滞留 PENDING（静默积压）；XCLAIM 重投时 SendEmail/RedactEvent 等副作用重复执行 | 加死信队列 + 有限重试计数；给 BackgroundJob 加 idempotency_key，消费侧去重（Redis SET NX 或 DB 唯一键） |
| M4 | **schema health check 自身出错时带病启动** | server/database.rs:104-107 "非致命错误，继续启动" | DB 抖动/权限问题导致 health check 无法执行时，服务器跳过校验直接起，可能对着半迁移 schema 提供服务 | health check 执行失败应视为致命（return Err），或加显式 `SYNAPSE_SCHEMA_CHECK_STRICT` 默认严格 |

### 🟡 建议修复（架构卫生 / 性能 / 边缘正确性）

| # | 问题 | 证据 | 建议 |
|---|------|------|------|
| S1 | **web 层绕过 service 直接访问 storage**（分层违规集中在 RoomContext） | context.rs:60,66,79 携带原始 storage；多 handler `use synapse_storage`（app_service.rs:14 等） | RoomContext 只暴露 service API，把直接 storage 调用收进对应 service 方法 |
| S2 | **联邦验签公钥缓存不全** | client.rs:424 `query_server_keys` 不走缓存；build_auth_header 不查 key_cache | 入站验签统一走带 TTL 的 key_cache，避免每事件触发远程 HTTP 拉公钥 |
| S3 | **canonical JSON 热路径**：每出站 PDU 两次 canonicalize、每次重分配排序、无 spawn_blocking | signing.rs:180；canonical_json.rs:30-49 | 单次 canonicalize 复用结果；大事件的 canonical JSON 考虑 `spawn_blocking` 或复用缓冲区 |
| S4 | **Redis 降级后分布式语义丢失且静默** | lib.rs:1125-1180 rate limit 内存 fallback 多 worker 失效（自注释）；invalidation Pub/Sub 无 fallback | 降级时发醒目告警/指标；多 worker 部署应把 Redis 视为硬依赖（限流/失效广播） |
| S5 | **状态机缺口** | knock 接受/拒绝 handler 缺失；联邦入站 m.room.member 无 check_membership_transition（event_auth/mod.rs:26 只查 is_auth_event）；can_invite 不查目标是否被 ban（power_levels.rs:494） | 补 knock 生命周期；对入站成员事件应用显式转换表；invite 前置 ban 检查 |
| S6 | **JWT 缓存 TOCTOU** | token.rs:54 命中缓存绕过 DB；外部改 DB 撤销不失效缓存 | 缩短 token 缓存 TTL 或撤销走统一入口强制 cache.delete；管理员撤销路径也失效缓存 |

### 🟢 可选优化（长期演进 / 可读性）

| # | 优化 | 证据 / 说明 |
|---|------|------|
| O1 | **sync/sliding_sync 只抽共享 data-fetch helpers，不整体合并** | 底层 storage trait 已共享，重复的是编排层。抽取 response-assembly helpers（timeline/state/ephemeral/account_data/to-device/device_lists）为共享 `SyncResponseBuilder`。工作量 M（~2-3 周）。**不建议全量合并**：SyncToken(since) vs pos+ranges(MSC3575 窗口)语义分叉、sliding sync 独有连接 LRU/GC，整体合并复杂度不值（规则 7.3 的"合并"应降级为"共享数据层"） |
| O2 | **拆"杂物抽屉"** | ExtensionServices(18+ 字段) 与 admin.module 子组按功能域再分，边界更清 |
| O3 | **拆 RoomContext 上帝上下文** | context.rs 25+ 字段，按域拆成多个小 context，配合 S1 一起做 |
| O4 | **迁移原子性** | 关键迁移文件去掉 `--no-transaction`（或明确标注为何需要）；考虑给不可逆迁移写 down 脚本或前滚补偿脚本 |

---

## 六、做对了的地方（避免过度自我否定）

- **迁移方向正确**：单体 src/ → workspace crate 基本完成，src/storage、src/services 已是 thin facade。
- **crate 依赖单向干净**：storage/cache/common 从不反向依赖 service/web；service 层不泄漏 axum/http（仅测试代码有）。
- **DI 无循环**：ServiceContainer 分阶段线性 DAG 装配，无 OnceCell/Weak 破环 hack。
- **Worker at-least-once**：Redis Streams consumer group + XACK + XPENDING/XCLAIM，崩溃不丢任务。
- **Redis 降级有熔断**：circuit breaker + 连接/命令 500ms 超时，不会每次调用超时拖垮请求。
- **迁移幂等可重跑**、**启动对缺表/缺列有硬门禁**（return Err 阻止启动）。
- **refresh token 有 CAS + replay 检测**（家族撤销）——机制在，只是 M2 的事务边界要收紧。

---

## GSTACK REVIEW REPORT

| Run | 内容 | 状态 |
|-----|------|------|
| 代码取证 | 4 个并行 Explore agent 覆盖 Q1-Q10，全部带 file:line 证据 | ✅ 完成 |
| 关键事实核实 | server/database.rs:70-108 亲自复核 schema 启动门禁（修正 agent 对 Q10 的误判） | ✅ 完成 |
| 交叉验证 | 规则 7.1 违规、token 竞态、worker 语义 均有直接源码行号支撑 | ✅ 完成 |

**Findings：必须修复 4（M1 存储越界 / M2 token 家族误杀 / M3 worker 无死信无幂等 / M4 带病启动）；建议修复 6（S1-S6）；可选优化 4（O1-O4）。**

**VERDICT**：架构骨架健康，债集中且可定位。优先级：M2 > M1 > M3 > M4（M2 用户可感知、M1 边界腐蚀最快扩散）。规则 7.3 建议从"合并 sync 服务"降级为"共享 data-fetch 层"（O1），全量合并不划算。

产出：docs/audit/02_architecture_review.md（含 component / data-flow / state / sequence 四类 ASCII 设计图）。

NO UNRESOLVED DECISIONS
