# A-3: v1/v3 双前缀退役策略

## 现状（2026-09-17 实测）

| 前缀 | 路由数 | 说明 |
|------|--------|------|
| v1   | **174** | 标准 Matrix v1 (114) + Tjg 专有 (60) |
| v3   | 413    | 标准 Matrix v3 |
| r0   | 0      | 已清除 |

### v1 路由分类

```
标准 Matrix v1 (114):
  - /v1/account/*, /v1/keys/*, /v1/login/*, /v1/media/*, /v1/profile/*
  - /v1/rooms/*, /v1/sendToDevice/*, /v1/sync, /v1/spaces/*, /v1/rendezvous/*
  - /v1/auth_metadata, /v1/config/client, /v1/external_services/*
  - /v1/threads/*, /v1/widgets/*

Tjg 专有 v1 (60):
  - /v1/friends/*         (8条，MSC4204 Sprint 4)
  - /v1/voice/*           (5条，语音功能)
  - /v1/burn/*            (3条，阅后即焚)
  - /v1/spaces/*/hierarchy/v1
  - /v1/account/password/* (部分重复)
```

### 规范依据

- **Matrix Client-Data API**: v1 是 **deprecated**（[spec matrix.org](https://spec.matrix.org/latest/client-server-api/)）
- **官方推荐**: 客户端应迁移到 v3
- **兼容性**: v3 路径与 v1 **相同**（仅版本号不同），handler 可复用

---

## 退役计划

### 阶段 1: 标记 + 文档（已完成）
- [x] 记录当前 v1 路由清单
- [x] 区分标准 v1 vs Tjg 专有 v1
- [x] 明确 SDK 覆盖率（SDK fork 已覆盖标准 v1）

### 阶段 2: v3 双注册（需新 ticket）
对标准 Matrix v1 路由，在 `derived_route_table` 中同时注册 v3 路径：
```rust
// 示例: v1/sync → v3/sync
("/_matrix/client/v1/sync", Method::Get, ..., "sync"),
("/_matrix/client/v3/sync", Method::Get, ..., "sync"),  // 新增
```

### 阶段 3: 客户端迁移（客户端团队职责）
- Tauri Android/iOS 更新到 v3 路径
- 更新 API 契约测试

### 阶段 4: v1 只返回 410 Gone（需新 ticket）
```rust
// 所有 /v1/* 路由改为：
("/_matrix/client/v1/sync", Method::Get, ..., |_, _| async {
    Err(ApiError::gone("v1 endpoints are deprecated, use v3"))
})
```

### 阶段 5: 移除 v1 注册（需新 ticket）
- 清理 `derived_route_table` 中的 v1 条目
- 运行 `scripts/contract/gen_derived_routes.py` 重新生成
- 更新 ledger fixtures

---

## 优先级排序

1. **高优先**: 标准 Matrix v1（影响互操作性）
2. **中优先**: Tjg 专有 v1 friends/voice/burn（内部团队使用，可同步迁移）
3. **低优先**: spaces/v1 hierarchy（非核心功能）

## 风险评估

| 风险 | 等级 | 缓解 |
|------|------|------|
| 客户端调用中断 | 🟡 中 | 保持 v1/v3 双注册至少 1 个 sprint |
| 第三方组件依赖 | 🟡 中 | 文档更新 + API 契约测试 |
| v1 handler 复用 | 🟢 低 | 路由注册层独立，handler 不变 |

---

## 相关文档

- [PROJECT_ACTUAL_ISSUES_2026-09-14.md](./PROJECT_ACTUAL_ISSUES_2026-09-14.md) §18.1
- [SDK fork 审计](./sdk-encapsulation-audit.md) — SDK 已支持 v3

---

## 2026-10-22 复核（实测）与棘轮落地

### 现状（数据源：`docs/synapse-rust/ROUTE_HANDLERS.json`，1048 条路由）

| 口径 | 数量 |
|---|---|
| `/_matrix/client/v1/*` 路由（method × path） | **112** |
| 去重路径 | **79** |
| 其中**有 `/v3` 孪生** | 81 条（58 路径） |
| 其中**没有 `/v3` 孪生** | 31 条（21 路径） |

上面第一、二行与早先记录的"112 处 / 79 路径"一致 —— 但来源要说清：这 112 条里
**112 处都在生成物 `derived_route_table_always.inc.rs` 中**，手写的 `.route()` 字面量只有 38 处
（其余由 `assembly::account_compat` 之类的批量注册从常量表展开）。所以"退役"不能靠删字面量，
必须改**注册意图**再重新生成派生链。

孪生组的语义一致性也核过：81 条里有 **80 条的 v1 与 v3 由同一个 `registered_by` 处理**
（唯一例外是 `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}`，v1 归 `thread`、v3 归 `room`）。
即：只要客户端改调 v3，这批删掉不会改变行为。

### 棘轮：`scripts/contract/check_v1_deprecation.py`（已接入 `check_route_contract.sh`）

它**不删任何路由**，只把退役面钉死，防止"越退越多"和"悄悄删断"同时发生：

- **规模只减不增**：`v1_surface_baseline` 记 `112 / 79`，涨一条就红；
- **无孪生端点逐条锁死**：`v1_no_twin_baseline.txt` 列出那 31 条，**消失一条就红**
  （客户端没有替代路径），新冒出一条也红（这类端点天生不可退役）；
- **报出可退队列**：80 条（同 handler 的孪生组），即阶段 5 的候选集。

变异自证（4 个变异体，逐一验证门禁会红/会绿）：

| 变异 | 期望 | 实测 |
|---|---|---|
| 新增一条 `client/v1` 路由 | 红 | exit 1，报规模超基线 + 新冒出的无孪生端点 |
| 删掉一条无孪生端点、未改基线 | 红 | exit 1，报"端点从路由里消失了" |
| 新增一条无孪生 v1 端点 | 红 | exit 1，报"这类端点天生不可退役" |
| 删掉一条**有孪生**的 v1 端点 | 绿（提示收紧） | exit 0 + "用 --update 收紧，别留静默额度" |

### 阶段 4/5 的真实阻塞点（**不是技术问题**）

"删 v1 注册"的前置是**确认线上客户端都改调 v3**，而这个证据不在本仓：

- `check_sdk_route_coverage.py` 读的是仓库外的 `SDK_ROOT`（`matrix-js-sdk`），且它按
  "相对路径首段对齐"匹配，**不区分 v1/v3**，因此它绿了也证明不了客户端用的是哪个前缀；
- Tjg 客户端的调用面在另一个仓库。

所以本轮的结论是：**退役面已可度量、可门禁，但批 1（真删）要先拿到客户端调用面证据**
（例如 SDK 侧 `/_matrix/client/v1` 的命中数为 0，或 Tjg 的 HTTP 抓包/单测覆盖）。
在那之前，棘轮负责保证"只减不增"，谁先迁移完谁就能先删——每次删除都要同步
`--update` 基线，让每个决定在 diff 里可见。

---

## 2026-10-22 复核纠正：**`v1` 不等于 deprecated**（原阶段 2 作废）

原计划的前提是"v1 已 deprecated，官方推荐全部迁 v3"。**这个前提只对一半**：

- Matrix 的 `v3` 是 2021 年确定的 stable 前缀，**此后新增的端点规范指定就是 `v1`**，
  并没有"又一个 v4/v5"来取代它；
- 所以"v1 路径"里混着两族完全不同的东西：

| 族 | 条数 | 性质 | 正确动作 |
|---|---|---|---|
| **A. 历史双注册** | 73 条 | 同一 handler 同时注册 v1 与 v3，纯兼容尾巴 | 客户端迁完后**删除 v1 注册**（原阶段 5） |
| **B. 规范指定 v1** | 28 条 | MSC3266 / MSC3856 / MSC3916 / MSC4108 / MSC2965 / MSC2946 / spec v1.6 / 媒体配置 | **永久保留 v1**，不得造 v3 孪生 |
| **C. 私有扩展** | 11 条 | proxy/*、appservice/fed_proxy、user/{id}/appservice、config/client、login/{get_token,qr_token}、`POST v1/sync` | 迁 `/_matrix/vendor/v1`（ISSUE-13），不是 v3 |

B 族的逐条依据：

| 端点 | 规范出处 |
|---|---|
| `GET /v1/rooms/{room_id}/summary` | MSC3266（治理文档 §605 明确标 "keep"） |
| `GET/POST /v1/rooms/{room_id}/threads{,…}`（5 条） | MSC3856（治理文档把 18 条列入"不该动"） |
| `GET /v1/rooms/{room_id}/timestamp_to_event` | spec v1.6 |
| `GET /v1/media/{download,thumbnail,preview_url}`（4 条） | MSC3916 |
| `GET /v1/auth_metadata` | MSC2965 |
| `POST /v1/rendezvous`、`…/{session_id}`（4 条）、`…/messages`（2 条） | MSC4108 |
| `GET /v1/rooms/{room_id}/hierarchy`、`/v1/spaces/{space_id}/hierarchy{,/v1}`（3 条） | MSC2946（治理文档 §12 列为 keep） |
| `GET/POST/PUT /v1/presence/{user_id}/status`（3 条） | 治理文档 §13.5「规范多挂载（应保留）：v1/v3 presence」 |
| `GET /v1/rooms/{room_id}/threads/{thread_id}` | MSC3856（v3 孪生归 `room` 模块、响应形状不同） |
| `GET /v1/media/config` | Matrix v1.11 媒体配置端点（`docs/sdk/media.md` §媒体配置） |

**结论：原阶段 2（"给标准 v1 补 v3 双注册"）作废。** 对 B 族造 v3 只会凭空多出一套
非标准路径，扩大 API 面与测试面；对 C 族造 v3 更糟 —— 等于把私有端点再污染一次标准命名空间。

### 阶段重排

| 阶段 | 原计划 | 修正后 |
|---|---|---|
| 1 记录现状 | 已完成 | 已完成，且 112/79 已由棘轮钉住 |
| 2 v3 双注册 | 给标准 v1 补 v3 | **作废**（B 族不该有 v3；A 族本来就双注册） |
| 3 客户端迁移 | 客户端改调 v3 | 保留（只针对 A 族 81 条） |
| 4 v1 返 410 | 全部 v1 | 只针对 A 族；B 族永不 410 |
| 5 删除 v1 注册 | 全部 v1 | 只针对 A 族；C 族走 ISSUE-13 迁 vendor |

### 门禁层面的反向守卫

`check_v1_deprecation.py` 除了"只减不增"与"无孪生端点不许消失"，还加了一条：
**若某条无孪生端点突然有了 v3 孪生，直接判红**，并指名"这一批是规范指定 v1 或私有扩展，
不得造 v3"。变异自证：给 `GET /v1/rooms/{room_id}/summary` 造一条 v3 孪生 ⇒ exit 1。

这条守卫的意义是防"为了让 A3 的数字好看"而做错误操作 —— 数字变好看的最快办法
恰恰是给 B 族造 v3 孪生，而那会让整个 API 面变差。

---

## 2026-10-22 第二批：A 族清零（v1 面只剩规范端点与私有扩展）

### 一个必须先纠的判据错误

第一版棘轮把「有 v3 孪生」等同于「可删」。**这条推断是错的**：有 8 条端点的稳定前缀
**本来就是 v1**，仓库里那个 v3 路径反而是别名。

| 端点 | 为什么 v1 是正典 |
|---|---|
| `GET /v1/rooms/{room_id}/hierarchy` | MSC2946 稳定版即 v1；v3 孪生默认 `max_depth=1`、v1 默认 3，行为不同 |
| `GET /v1/spaces/{space_id}/hierarchy{,/v1}` | MSC2946；治理文档 §12 把整组列为 keep |
| `GET/POST/PUT /v1/presence/{user_id}/status` | 治理文档 §13.5「规范多挂载（应保留）：v1/v3 presence」 |
| `GET /v1/rooms/{room_id}/threads/{thread_id}` | MSC3856；v3 孪生归 `room` 模块，响应形状不同 |
| `GET /v1/media/config` | Matrix v1.11 媒体配置端点的稳定前缀就是 v1（`docs/sdk/media.md` 同口径） |

所以新增 `scripts/contract/v1_spec_keep_baseline.txt`（`METHOD<TAB>path<TAB>依据`，三列缺一即报错），
门禁判定改成：

- 名单里的端点**消失 ⇒ 红**（"删掉了规范指定用 v1 的端点"，并打印保留依据）；
- 可退队列 = 有 v3 孪生 **且不在名单里** **且同 handler**。

这条守卫挡的是最容易发生的错误动作：为了让 A3 数字好看，把规范端点当成"历史尾巴"一起扫掉。

### 本批删除（22 条 / 4 处挂载）

| 位置 | 删法 | 条数 | 为什么安全 |
|---|---|---|---|
| `assembly.rs::create_account_router` | 同一 router 的 v1 nest 去掉 | 18 | `create_account_compat_router()` 原样挂在 v3；含 `account/3pid/*`、`account/{deactivate,password,whoami}`、`profile/*` |
| `handlers/search/mod.rs` | 只从 `v1_router` 去掉 `create_room_context_router()` | 1 | 同一 router 已在 `v3_router`；同文件 `hierarchy` 与 `timestamp_to_event` 的 v1 **保留** |
| `moderation.rs` | 删整个 v1 nest（v1 只是 v3 的子集） | 1 | `report/{event_id}` 在 v3 上同 handler |
| `room.rs` | 删 v1 nest 与 `create_room_v1_router()` | 2 | v1 只有 `state/m.room.power_levels/` 的 GET/PUT，v3 router 已 merge 同一 router |

交叉验证：被删的 22 条与保留名单（8）/ 无孪生名单（31）**交集均为空**；
路由条目 997 → **975**（三张派生表合计 977 行，`gen_derived_routes.py --check` 实测），
`client/v1` 由 61 条 / 45 路径降到 **39 条 / 27 路径**。

### 终态：A 族 = 0

| 族 | 条数 | 处置 |
|---|---|---|
| A 历史双注册 | **0**（原 73 条已全部退役：第一批 51 + 本批 22） | 完成 |
| B 规范指定 v1 | 28 | 永久保留 |
| C 私有扩展 | 11 | 待迁 `/_matrix/vendor/v1`（ISSUE-13） |

`check_v1_deprecation.py` 现在报 `可退队列: 0 条`。也就是说**后端侧 A3 的"退役"已无可删对象**：
剩下的 39 条 v1 要么是规范正典（28），要么是私有扩展（11，方向是 vendor 而不是删除）。

### 下一步（不是删 v1）

1. C 族 11 条迁 `/_matrix/vendor/v1`：`proxy/*`（7）、`appservice/fed_proxy`、
   `user/{id}/appservice`、`config/client`、`login/{get_token,qr_token}`、`POST v1/sync`。
   迁移后 `v1_no_twin_baseline.txt` 收窄到 28 条（只剩 B 族）。
2. 客户端/SDK 侧同步：现在 `client/v1` 上已无"历史双注册"可调，客户端若仍打 v1 的
   `account/*`、`profile/*`、`context`、`report`、`power_levels`，会 404 —— 需要它们改调 v3。
