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
