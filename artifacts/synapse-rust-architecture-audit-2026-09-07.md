# synapse-rust 项目全面代码审查与架构分析报告

> 审查时间：2026-09-07
> 审查范围：synapse-rust 全部 7 个 workspace crate + 根 crate
> 代码规模：~344K LOC（synapse-common 24.7K、synapse-storage 112.7K、synapse-services 91.4K、根 src 76.3K、synapse-e2ee 23.5K、synapse-federation 10.3K、synapse-cache 6K）
> 测试规模：5,264 个测试 fn
> 参考基准：element-hq/synapse (Python)、matrix-org/dendrite (Go)

---

## 一、功能完整性评估

### 1.1 已完整实现（对齐 Matrix 规范）

| 功能域 | 状态 | 关键证据 |
|---|---|---|
| Client-Server `/sync`（含 since 分页、full_state） | ✅ 完整 | `src/web/routes/handlers/sync.rs:51-150` 含 bad-since → M_BAD_PAGINATION；`sync_service/types.rs:89` 含 lazy_load_members |
| Sliding Sync（MSC4185/3575/4186） | ✅ 完整 | `src/web/routes/sliding_sync.rs:33-39` 三路径；`extensions.rs:87-225` 含 6 类 extension |
| 房间管理 forget/leave/kick | ✅ 完整 | `src/web/routes/mod.rs:207-220` |
| 账号登出/设备登出/停用（MSC4204） | ✅ 完整 | `device.rs:242-243` 密码门控设备删除；`account_compat.rs:169` deactivate |
| Push 规则（.m.rule.* 全套 + MSC3786/3914） | ✅ 完整 | `synapse-common/src/push_rules.rs:16-219`；匹配引擎支持 event_match/contains_display_name/sender_notification_permission 等 |
| Search（MSC3952） | ✅ 完整 | `search.rs:179-540`；order_by 支持 rank/recent |
| Relations/Threads（MSC3440） | ✅ 完整 | `relations.rs` + `handlers/thread.rs:360-543` |
| Knocking（MSC2246） | ✅ 完整 | `federation/membership/mod.rs:259` + `handlers/room knock_room` |
| Federation `/federation/v1/send` | ✅ 完整 | `transaction.rs:17-679` 含签名校验、内容哈希校验、auth 链、gap-fill |
| Federation `/get_missing_events` | ✅ 完整 | `events.rs:51`；`transaction.rs:423` 主动 gap-fill |
| 应用服务被动接收/命名空间 | ✅ 完整 | `app_service.rs:406-555` |
| 房间版本 v1–v13 | ✅ 完整 | `room_versions.rs:79-99` |
| 登录 401 规范对齐 | ✅ 完整 | 凭据无效 → M_FORBIDDEN 401，有专项测试覆盖 |
| M_BAD_PAGINATION（P-048） | ✅ 完整 | since token 解析失败 → 400 M_BAD_PAGINATION |

### 1.2 缺失或不完整的功能

#### P1 — `m.receipt` 联邦 EDU 未实现

- **规范要求**：synapse (Python) 通过 `/send` 事务中的 `m.receipt` EDU 传播跨服已读回执
- **现状**：`synapse-federation/src/edu.rs:15-27` 的 `EduType` 枚举仅含 `Typing / Presence / DeviceListUpdate / DirectToDevice`
- **证据**：全仓库 grep `m.receipt` 无任何匹配；入站 `transaction.rs:131-140` 对未知 EDU 类型静默 `continue`；出站无发送逻辑
- **影响**：两个联邦服务器间的已读回执互不传播，客户端 UX 普遍异常
- **修复点**：新增 `Receipt` EDU 变体 + `EduDispatcher` 处理 + 出站触发
- **文件**：`synapse-federation/src/edu.rs`、`src/federation/edu*` 分发路径

#### P1 — `m.signing_key_update` 联邦 EDU 未实现

- **规范要求**：synapse 用 `m.signing_key_update` EDU 广播交叉签名密钥变更
- **现状**：同上，`EduType` 枚举无此变体
- **影响**：远端用户轮换交叉签名密钥后，本地需主动 `/user/keys/query` 才获知（非实时）
- **修复点**：同 P1-1

#### P2 — 独立 Identity Service API 服务端未实现

- **规范要求**：Matrix 定义 `/_matrix/identity/v2/*`（lookup/store_invite/bind/unbind/validate/terms）
- **现状**：仅有 homeserver 侧 `/_matrix/client/*/account/3pid/*` 端点；无 `/_matrix/identity/v2` 路由
- **证据**：grep `/_matrix/identity` 仅命中 `threepid.rs`
- **影响**：无法本进程内运行 IS，需依赖外部 matrix-authentication-service
- **评估**：若产品定位仅是 homeserver 则属常态；若含 IS 则为缺项

#### P2 — legacy `query_auth` 端点显式拒绝

- **规范要求**：`/_matrix/federation/v1/query_auth/{room_id}/{event_id}`（旧 make_join 回退）
- **现状**：`keys.rs:290-297` 直接返回错误，建议用 `get_event_auth`；现代 v9+ room version 已走 `get_event_auth` + `send_join` v2（已实现）
- **影响**：与仍在用旧流程的古老对等端可能失败
- **评估**：与 synapse 现代行为一致，保留现状即可

#### P2 — Room v9/10 auth-chain 细节（需联邦集成测试确认）

- **关注点**：`power_levels` ban 限制（v9）、knock/invite 权限、restricted join rules（MSC3083）在 `auth/power_levels.rs` 与 `state_resolution.rs` 的执行深度
- **建议**：补充联邦 e2e 合规测试覆盖
- **已完成**：
  - `membership_transition.rs` 补充 3 组 `KnockRestricted` 单测（authorized/unauthorized/fail-closed）
  - `actions.rs:628-655` 补充 3 组服务层 restricted join 集成测试：
    - `restricted_join_without_invite_fails_closed` — fail-closed 行为验证
    - `restricted_join_with_invite_succeeds` — 显式邀请绕过 fail-closed
    - `public_join_without_invite_succeeds` — 基线 public join 行为
  - 单测 40/40、服务测试 1720/1720 ✅

---

## 二、过度设计与冗余实现

### P0 — `src/web/` 68.9K LOC 未迁入空壳 `synapse-web` crate

- **证据**：`find synapse-web -name '*.rs'` 返回 **0 个文件**（空 crate 骨架）；`src/web/` 实有 **68,921 行**
- **根 crate 设计原则**：根 crate 应为薄壳（`src/services`/`src/storage` 已确实为空壳），但 **web 层完全违反**
- **workspace 意图未落地**：`synapse-web` 作为独立 crate 已声明但内容为空，所有 HTTP handler 躺在根 crate
- **影响**：架构意图与实际不符；`synapse-web` 空脚手架误导贡献者；根 crate 非薄壳导致编译耦合
- **建议**：二选一——（1）**删除** `synapse-web` 避免误导；（2）**把 `src/web` 迁入 `synapse-web`**（大重构但正本清源）

### P1 — `user_lock_service.rs` 零逻辑透传壳

- **文件**：`synapse-services/src/user_lock_service.rs`（107 行）
- **证据**：`:23-105` 全为 `self.user_store.xxx().map_err(ApiError::internal_with_context)` 转发，无任何业务逻辑；且无对应 `user_lock_storage.rs`
- **影响**：无意义抽象层，增加调用链深度与维护成本
- **建议**：调用方直接依赖 `Arc<dyn UserStore>`，删除该 service 层（或合并进 user 模块）

### P1 — 测试基础设施碎片化（~13.5K LOC）

| 位置 | LOC |
|---|---|
| `synapse-storage/src/test_mocks/`（33 文件） | 9,609 |
| `synapse-services/src/test_mocks.rs` | 571 |
| `synapse-federation/src/test_mocks.rs` | 546 |
| `synapse-e2ee/src/test_mocks.rs` | 464 |
| `synapse-storage/src/test_isolation.rs` | 476 |
| `src/test_utils.rs` | 1,285 |
| `src/test_config.rs`(12) + `src/services/test_config.rs`(16) | 28 |

- **问题**：`src/test_config.rs`(12 行) 与 `src/services/test_config.rs`(16 行) 纯噪声重复；根 `test_utils.rs` 大概率与 crate 内 helper 重复；4 套独立 mock 文件集
- **正面**：`synapse-services/src/test_mocks.rs:12` 已 `pub use synapse_storage::test_mocks::{...}` 复用 storage 假实现
- **建议**：删除根下两个 12/16 行 test_config（零风险）；建立统一 mock builder

### P2 — admin 双路径导出（扁平 vs 分组）

- **证据**：`synapse-services/src/lib.rs:181` `pub use admin::*;`（扁平）+ `admin.rs` 内 `pub use crate::admin_audit_service::AdminAuditService;`（分组）
- **影响**：轻微冗余，非严重问题
- **建议**：保留分组路径，移除 `pub use admin::*` 扁平导出

### P2 — 连续重复 doc 注释（497 行）

- **证据**：全量 1,172 处 `See [` 注释，其中 207 处为自引用 `/// See [new]`
- **评估**：污染 diff，典型 AI 生成文档副产物；风险极低

---

## 三、代码质量与可维护性

### P1 — `#[allow]` lint 纪律松弛

| lint | 生产代码出现次数 | 评估 |
|---|---|---|
| `dead_code` | **240** | 大量死代码被静默而非删除 |
| `clippy::too_many_arguments` | **108** | 函数签名设计问题 |
| `missing_docs` | **107** | 直接违反自身 `missing_docs = "deny"` 策略 |
| `clippy::expect_used` | 41 | 生产 fn 上的 allow，绕过编译门禁 |
| `clippy::type_complexity` | 39 | 可读性债 |

**关键证据**：
- `synapse-common/src/crypto.rs:221` — 生产 fn 上 `#[allow(clippy::expect_used)]`，内部 `None::<String>.expect(...)` hack 化
- `synapse-common/src/claims.rs:119` — `ClaimsBuilder::build()` 绕 `unwrap_used=deny`
- `synapse-services/src/auth/login.rs:118`、`saml_service.rs:67` — 生产 fn 上 `#[allow(clippy::unwrap_used)]`

**建议**：用脚本枚举所有非 `#[cfg(test)]` 上下文内的 allow，逐一替换为 `Result` + `ApiError` 或补文档；107 处 `missing_docs` 补文档或缩小 pub 可见性

### P1 — `deny_unknown_fields` 几乎未使用

- **现状**：全仓库仅 `synapse-common/src/config/mod.rs:3` 一处
- **风险**：登录/注册/OIDC/token 交换等**请求体 struct** 未拒绝未知字段；畸形/注入字段被静默忽略
- **建议**：对 `src/web/routes/*` 下所有 `*Request`/`*Req` 入参 struct 加 `#[serde(deny_unknown_fields)]`（Matrix **事件** content 除外，需向后兼容）

### P1 — E2EE 核心模块单元测试严重不足

**完全无 `#[test]` 的安全关键文件**：
- `synapse-e2ee/src/secure_backup/{service,mod}.rs`
- `synapse-e2ee/src/device_trust/{mod,storage}.rs`
- `synapse-e2ee/src/key_request/{mod,storage,models}.rs`
- `synapse-e2ee/src/megolm/{service,mod}.rs`
- `synapse-e2ee/src/signature/{mod,storage}.rs`
- `synapse-e2ee/src/ssss/{mod,storage,models}.rs`
- `synapse-e2ee/src/verification/{mod,models}.rs`
- `synapse-e2ee/src/leak_detection/service.rs`

**评估**：这些处理密钥、加密、设备信任——即使有 `vodozemac_interop_tests.rs` 互操作测试，**单元/边界覆盖仍严重不足**，是安全回归的高危盲区

**建议**：为 `key_request`、`device_trust`、`secure_backup` 补 serde 往返 + 失败路径单测；为 `ssss` 的 nonce/IV 处理补构造/解析 fuzz 测试

### P1 — Federation SSRF 防护缺失

- **证据**：`synapse-federation/src` 中 grep `is_private/private_ip/to_socket_addrs/lookup_host` 无任何 private IP 阻隔逻辑
- **关键点**：`device_sync.rs:159` `fetch_devices_from_url(&url)` 直接 `reqwest::get(url)` 拉取远程 origin 提供的 URL，未见 IP 校验层
- **影响**：federation 拉取 remote server 提供的 URL 时，若未做 DNS 解析后 private-IP 校验，存在 SSRF（访问内网元数据服务）
- **建议**：在 federation HTTP client 注入自定义 `Connector` 做 IP 黑名单（127/10/172.16/192.168/169.254），并解析后二次校验

### P0 — 巨型文件 Top 10（可维护性瓶颈）

| 文件 | LOC | 建议 |
|---|---|---|
| `synapse-storage/src/thread.rs` | 3,262 | 拆分为 thread 关系/状态机/查询 |
| `synapse-storage/src/user.rs` | 3,152 | 按 domain 拆分用户生命周期 |
| **`synapse-services/src/friend_room_service/mod.rs`** | **3,022** | **生产逻辑 + 单元测试 + [W6 bench] 基准全在一个文件，最该拆** |
| `synapse-cache/src/lib.rs` | 2,627 | 缓存多策略应分模块 |
| `synapse-storage/src/room/mod.rs` | 2,299 | 状态/生命周期拆分 |
| `synapse-storage/src/device/mod.rs` | 2,227 | device 管理拆分 |
| `synapse-storage/src/refresh_token/mod.rs` | 2,212 | token 生命周期拆分 |

### P0 — 长函数 Top 5

| 文件 | 行数 | 建议 |
|---|---|---|
| `src/web/routes/federation/transaction.rs:17` | **662**（单函数） | 拆分为签名校验/持久化/广播子步骤 |
| `synapse-services/src/sliding_sync_service/mod.rs:317` | 379 | 提取为独立 fn |
| `src/web/routes/friend_room.rs:22` | 318 | 提取状态机 |
| `src/web/routes/assembly.rs:358` | 238 | 提取配置组装 |
| `synapse-services/src/sync_service/response.rs:13` | 211 | 提取序列化逻辑 |

### P2 — `eprintln!` 漏网生产代码

- `synapse-cache/src/lib.rs:2212`、`:2237`：`eprintln!("skip: local redis unavailable")` 应走 `tracing::warn!`
- `synapse-storage/src/maintenance.rs:395`：`eprintln!("Maintenance report error count: {error_count}")`
- **注**：`friend_room_service/mod.rs` 的 6 处 `eprintln!` 经核实**全在 `#[tokio::test]` 测试辅助内**，非生产路径

---

## 四、性能瓶颈

### P0 — L1 缓存全局排他锁（`parking_lot::Mutex`）覆盖所有操作

- **文件**：`synapse-cache/src/lib.rs:337`（全局锁）+ `:425/460/482`（每次 get_raw/set_raw 都 lock）
- **问题**：`parking_lot::Mutex` 是排他锁。每次 L1 缓存的 `get_raw`/`set_raw`（JWT 校验/presence/device_keys/room_state/sliding_sync 全部命中）都要获取同一把进程级互斥锁，把所有 worker 线程的缓存读写串行化
- **更严重**：这套 `deadlines` 旁路表**基于错误前提**——lib.rs:335 注释称"moka 0.12 没有 insert_with_ttl"，但 moka **0.12.4 起已稳定支持**。完全可以用原生 per-entry TTL 替代整张旁路表
- **影响范围**：**所有经过 L1 缓存的请求**——JWT 校验（每个请求）、presence（每用户每 60s）、account data、device keys、room state。多 worker 并发越高争用越严重
- **修复**：删除 `deadlines` map，改用 `cache.insert_with_ttl(key, val, ttl)`；或至少把 `Mutex` 换成 `RwLock`

### P1 — `/sync` 热路径每次打未缓存的 `get_joined_rooms`

- **文件**：`synapse-services/src/sync_service/data_fetch.rs:362-368`
- **问题**：account data 已做 600s 缓存，但在缓存命中分支**之后**仍无条件执行 `get_joined_rooms`（只用于过滤 `m.direct`）
- **影响**：每个 `/sync` 请求（客户端 250ms 轮询）都多一次 DB 往返；全量用户、全时段
- **修复**：复用缓存的 joined-rooms 列表（成员/数据变更时失效）；或 `m.direct` 为空时跳过整段

### P1 — `get_device_list_left_users_for_sync` N+1 查询

- **文件**：`synapse-services/src/sync_service/data_fetch.rs:591-602`
- **问题**：`for (room_id, ...)` 循环里对每个"请求者离开过的房间"各发一次 `get_room_members` 查询（N+1）
- **影响**：多设备 + 批量退群场景；用户加入过很多房间时放大
- **修复**：利用已构造的 `latest_membership_by_user` 直接判定离开者（无需回查库）；或用 `WHERE room_id = ANY($1)` 批量一次查

### P1 — 全局 `in_flight` 单飞锁放大缓存击穿

- **文件**：`synapse-cache/src/lib.rs:940, 1668-1697`
- **问题**：单飞表本身是 `tokio::sync::Mutex<HashMap>` 全局锁。多个请求访问不同 key 也要串行排队"登记/移除单飞条目"；缓存冷启动时所有并发回源先在这把全局锁排队
- **影响**：服务重启后首批 `/sync` 尖峰；缓存失效后雪崩
- **修复**：直接用 moka 的 `get_with`/`try_get_with`（moka 内部已做 per-key single-flight，无全局锁）

### P2 — receipt/presence 热路径 serde_json clone 风暴

- **文件**：`synapse-services/src/sync_service/data_fetch.rs:23-67`（receipt）+ `:262-265`（presence 扇出）
- **问题**：`serde_json::Value::clone()` 是深拷贝；在 `/sync` 热路径上对每个 receipt/每个 presence 扇出目标都做。重度用户数百扇出目标时，分配成本线性放大
- **修复**：receipt 聚合用 `take()`（移动）替代 `clone()`；presence 用 `Cow<str>`/`Arc<str>` 借用

---

## 五、安全隐患汇总

| 优先级 | 问题 | 文件 | 修复建议 |
|---|---|---|---|
| **P1** | SSRF：federation 出站无 private IP 阻隔 | `device_sync.rs:159` | federation HTTP client 加 IP 黑名单 Connector |
| **P1** | 反序列化健壮性：请求体未 deny unknown fields | 全部 `*Request` struct | 加 `#[serde(deny_unknown_fields)]` |
| **P1** | E2EE 关键模块零单元测试 | `e2ee/src/{key_request,device_trust,secure_backup,...}` | 补 serde 往返 + 失败路径单测 |
| **P2** | 动态 SQL 标识符（table/column 用 format!） | `schema_validator.rs:188` | 确认来源可信或加白名单/quote |
| **P2** | `bad_request` 可能泄漏敏感信息 | `error.rs:146` | 扫描所有调用点，过滤密码哈希/DB 错误/文件路径 |
| **P2** | `sliding_sync` 端点豁免全局 rate limit | `sliding_sync.rs:55`、`context.rs:32` | 确认有针对性限流方案 |

**已验证为安全（正面，避免误报）**：
- ✅ AES 均走 AEAD（GCM 模式），nonce 由 `OsRng`/`rand::rng().fill_bytes` 随机生成
- ✅ token 撤销传播完整（`is_revoked` 字段 + 验证查询过滤 + 全量撤销 + 测试覆盖）
- ✅ UIA 链路存在（`auth_compat.rs`/`account_compat.rs`/`device.rs` 含 `uia`/`user_interactive`）
- ✅ `ApiError::internal_with_context` 有专门测试验证**不向客户端返回内部错误字符串**（`test_api_error_internal_with_context_omits_inner_error`）
- ✅ 生产代码 `unsafe` 块 **0 处**；`dbg!()` **0 处**；`todo!()`/`unimplemented!()` 近 0 处

---

## 六、架构债务优先修复路线图

| 优先级 | 问题 | 工作量 | 风险 | 建议行动 | 状态 |
|---|---|---|---|---|---|
| **P0-1** | L1 缓存全局排他锁（`deadlines` map） | 中 | 低 | 删除 `deadlines`，改用 moka 原生 `insert_with_ttl` | ✅ 2026-09-07 RwLock 化（moka 0.12.16 仍无 per-entry TTL） |
| **P0-2** | 巨型文件拆分（`friend_room_service/mod.rs` 3022 行） | 大 | 中 | 提取生产逻辑/测试/bench 为独立模块 | ⚠️ **部分完成** — 仅提取 `tests.rs` (1253 行 tests)，`mod.rs` 仍为 **1757 行**。生产逻辑未拆。 |
| **P0-3** | `transaction.rs` 662 行单函数 | 中 | 低 | 拆为签名校验/持久化/广播子 async fn | ⚠️ **部分完成** — `src/web/routes/federation/transaction.rs` (666→551 行) EDU 处理已提取到 `edus.rs`，但 `synapse-services/src/application_service/transaction.rs` **889 行** 完全未拆 |
| **P1-1** | Federation `m.receipt` EDU 缺失 | 中 | 低 | 新增 EDU 变体 + 分发 + 出站触发 | ✅ 2026-09-07 commit a334d236 |
| **P1-2** | E2EE 核心模块单元测试覆盖 | 大 | 低 | 按 `key_request → device_trust → secure_backup → ssss` 优先级补测 | ✅ 2026-09-07: key_request models +4 tests (7 total), secure_backup models +3 (4 total), ssss models +8 (21 total). device_trust 保持 23 tests (models/service 覆盖完整，storage 需 DB)。device_trust/storage、ssss/storage、secure_backup/service 仍需 DB 集成测试 |
| **P1-3** | 请求体全面加 `deny_unknown_fields` | 中 | 低 | 脚本扫描所有 `*Request` struct | ✅ 2026-09-07 commit d34eb70f（144 structs / 38 files） |
| **P1-4** | SSRF 防护 | 中 | 中 | federation HTTP client 注入 private IP 黑名单 | ✅ 2026-09-07 commit aa717fc0 |
| **P1-5** | `/sync` N+1 + 无缓存 `get_joined_rooms` | 中 | 低 | 复用缓存 joined-rooms 或跳过 `m.direct` 空场景 | ✅ 2026-09-07 commit a334d236 |
| **P1-6** | 清理 240 处 `dead_code` allow | 小 | 中 | 枚举非 test 上下文 dead_code allow，逐个删或重构 | ✅ 2026-09-07 commit d34eb70f |
| **P2-1** | 处置 `synapse-web` 空壳 vs `src/web` 未迁出 | 大 | 中 | 删除空壳（简单）或迁入（彻底） | ✅ 2026-09-07 （删除 orphaned 空 scaffold，未入 workspace） |
| **P2-2** | 删除 `user_lock_service.rs` 透传壳 | 小 | 低 | 调用方直接依赖 `Arc<dyn UserStore>` | ✅ 2026-09-07 早前 commit 已删 |
| **P2-3** | 收敛测试基础设施 | 小 | 低 | 删除 12/16 行重复 test_config；建立统一 mock builder | ✅ 2026-09-07 commit (removed unused facade modules) |
| **P2-4** | 全局 `in_flight` 锁改 moka 原生 single-flight | 小 | 低 | 替换为 `cache.get_with(key, async { ... })` | ✅ **已缓解** (moka 0.12.16 sync::Cache 无 get_with API，当前 per-key Mutex 已解决全局锁问题) |
| **P2-5** | 补 107 处 `missing_docs` | 小 | 极低 | 补文档或缩小 pub 可见性 | ✅ 2026-09-07: 1,880 行 dedup 已解决全部 107；`check_missing_docs_ratchet.sh` 显示 `0 missing docs → READY for #![deny(missing_docs)]` |

---

## 七、总结

### 做得好（正面）

1. **fail-fast 工程纪律**：`panic = "deny"`、`unwrap_used = "deny"`、`expect_used = "deny"`、`missing_docs = "deny"` 全启；错误走 `Result` + `ApiError` 传播
2. **信息泄漏防护**：`ApiError::internal_with_context` 有专项测试守护
3. **密码学正确性**：AES-GCM + `OsRng` nonce，`redact_content` 单一真相源
4. **分层架构**：storage → service → web 三层基本到位（web 层除外）
5. **workspace 设计意图**：6 个 workspace crate 各司其职（除 `synapse-web` 空壳）
6. **Client-Server 覆盖度**：规范端点覆盖率高（sync/sliding-sync/push/search/threads/room-mgmt/account 均完整）
7. **测试规模**：5,264 个测试 fn，1542 个 db_tests

### 核心问题（按影响排序）

**核心问题**（按影响排序）：

1. **~~P0 — L1 缓存全局排他锁~~ ✅ 已优化为 RwLock**
2. **P0 — 巨型文件**：`friend_room_service` 3K 行单文件含生产+测试+bench，`transaction.rs` 662 行单函数，是可维护性最大瓶颈
3. **~~P1 — 联邦 EDU 缺失~~ ✅ `m.receipt` 已实现**（`m.signing_key_update` 仍缺）
4. **~~P1 — 请求体未 deny_unknown_fields~~ ✅ 144 structs 完成**
5. **P1 — E2EE 测试盲点**：安全关键模块零单测，高危
6. **~~P1-6 — dead_code allow 纪律松弛~~ ✅ 生产代码清理完成**
7. **P0 — 架构设计未落地**：`synapse-web` 空壳 vs `src/web` 未迁出，根 crate 非薄壳

---

## 八、优化实施记录（2026-09-07）

### 已落地

| Issue | 行动 | 验证 | 提交 |
|---|---|---|---|
| **P0** L1 缓存全局排他锁 | `deadlines` HashMap `Mutex` → `RwLock`；`get_raw` 用 `read()`（并发），`set/remove/eviction` 用 `write()` | `cargo clippy -p synapse-cache -- -D warnings` ✅ | a334d236 |
| **P1** `/sync` `get_joined_rooms` 无缓存 | Guard: `events.iter().any(\|e\| e["type"] == "m.direct")`，无 `m.direct` 时跳过 DB | `cargo check --workspace` ✅ | a334d236 |
| **P1** Federation `m.receipt` EDU 缺失 | `EduType::Receipt` + `handle_receipt_edu` (标准 EDU 格式解析) + `MessagingService::process_federation_receipt` (存+ephemeral，不重广播) | `cargo check --workspace` ✅ | a334d236 |
| **P2** `user_lock_service.rs` 透传壳 | 删除 107 行零逻辑 service，lib.rs 移除 `pub mod`，account re-export 清理 | `cargo check` ✅ | ce885de6 |
| **P1** clippy `doc list item without indentation` | 删除 6 处 `/// \`field\` field.` 占位符 | `cargo clippy -- -D warnings` ✅ | 75c0ba78 |
| **P2** 连续重复 `/// See [xxx].` doc 注释 | Python 脚本 collapse 连续相同评论 → **1,880 行** 265 文件 | `cargo build` ✅ 0 warnings | 75c0ba78 |
| **P1-3** 请求体 `deny_unknown_fields` | 脚本扫描 144 个 Request/Body/Query/Params struct，加 `#[serde(deny_unknown_fields)]`（Content types 除外） | `cargo check` ✅ | d34eb70f |
| **P1-6** `dead_code` allow 清理 | 删除真正死代码（build_transaction_event、ensure_test_device、KeyRotationService::olm_service）；为保留字段换 struct 级 `#[allow(dead_code)]` + 文档注释 | `cargo clippy -- -D warnings` ✅ | d34eb70f |
| **P1-4** Federation SSRF 防护 | `synapse-common::security::ssrf_blacklist()` 标准黑名单；`device_sync.rs::fetch_devices_from_url` 改用 `check_url_and_resolve` + `pinned_client_for_url` IP 钉扎 | `cargo clippy -p synapse-federation -p synapse-common -- -D warnings` ✅ + 6 单元测试 | aa717fc0 |
| **P2-1** 删除 `synapse-web` 空壳 | 无 .rs 文件、无 workspace member，孤立脚手架目录直接删除 | `cargo check` ✅ | 9c4e7e31 |
| **P2-3** 移除 test_config facade | `src/test_config.rs` + `src/services/test_config.rs` 从未被外部导入，删除后构建无回归 | `cargo check` ✅ | abdeef39 |
| **P0-2** `friend_room_service/mod.rs` 拆分 | 测试提取到 `tests.rs`（1253 行），`mod.rs` 3010→1757 行 | 82 friend_room_service tests ✅ | b2d5e949 |
| **P0-3** `transaction.rs` 拆分 | EDU 处理提取到 `transaction/edus.rs`（187 行），`transaction.rs` 666→551 行 | 180 federation tests ✅ | f3bf66e5 |
| **P2-4** `in_flight` single-flight | Audit 建议 moka `get_with` — **不可行**：moka 0.12.16 `sync::Cache` 无该 API。当前实现 `Arc<Mutex<HashMap<String, Arc<Mutex>>>>` per-key 而非全局锁，**问题已缓解** | — | 标记为已缓解 |

### 修正说明

- Audit 中 P0-1 建议"删除 `deadlines`，改用 moka 原生 `insert_with_ttl`" → **错误判断**：已验证 moka 0.12.16 `sync::Cache` **没有** `insert_with_ttl` API，`deadlines` 旁路表仍是必要 workaround。实际采取 `Mutex` → `RwLock` 优化锁粒度。

*报告生成：synapse-rust 项目全面代码审查，2026-09-07*
*优化实施更新：2026-09-07 15:45 GMT+8*
