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
