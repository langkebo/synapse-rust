# synapse-rust API 优化文档总览

> 版本: 2.0
> 日期: 2026-03-27
> 基线: 以当前 `src/web/routes` 实现和最新 Matrix 规范为准

---

## 一、这组文档解决什么问题

本目录不再假设“所有旧版本 API 都应该通过 HTTP 重定向或 `{version}` 路径参数合并”。
根据当前后端实现，真正可行的优化方向是：

1. **优先复用处理函数和子路由**，而不是改动公开路径语义
2. **优先采用 `Router::nest()` 挂载多版本前缀**，这与现有 `space.rs`、`key_backup.rs` 的实现一致
3. **不轻易删除旧版本入口**，尤其是 r0 / v1 仍被客户端兼容层使用的场景
4. **只在行为完全一致时做路由收敛**，语义不同的接口保留独立说明

---

## 二、统一结论

### 2.1 当前代码库最适合的版本策略

```rust
let router = Router::new()
    .route("/keys/upload", post(upload_keys))
    .route("/keys/query", post(query_keys));

Router::new()
    .nest("/_matrix/client/r0", router.clone())
    .nest("/_matrix/client/v3", router)
    .with_state(state)
```

适用原因：

- 当前代码库已经存在成熟示例
- 不需要给处理函数额外注入 `version` 路径参数
- 更符合 Matrix API 已公开版本路径的稳定性要求
- 能保留现有 URL，同时减少重复注册代码

### 2.2 当前文档统一约束

| 主题 | 统一结论 |
|------|----------|
| HTTP 30x 重定向 | 不推荐用于 Matrix Client-Server API 的常规兼容处理 |
| `{version}` 路径参数 | 仅能作为讨论示意，不作为当前项目首选方案 |
| Feature Flag | 当前 `Cargo.toml` 未提供兼容专用 feature，不应写成既有实现 |
| 向后兼容 | 优先复用 handler / service / subrouter，保留外部路径 |
| Matrix 规范对齐 | 优先保留规范已有版本路径，不随意改成自定义 URL |

---

## 三、各文档范围

| 文件 | 覆盖范围 | 当前结论 |
|------|----------|----------|
| `synapse-rust-api-optimization-plan.md` | 总体策略与分阶段建议 | 已改为保守、可落地方案 |
| `synapse-rust-api-optimization-friend-room.md` | friend_room，以及 account_data / device / media / search 的复用模式 | 已改为基于实际路由差异的实现建议 |
| `dm-optimization.md` | DM 模块 | 以“保持混用版本、最小化改造”为主 |
| `e2ee-optimization.md` | E2EE / keys / sendToDevice | 以子路由复用代替 `{version}` |
| `media-optimization.md` | Media API | 改为 `/_matrix/media/*` 真实路径与内部复用方案 |
| `room_summary-optimization.md` | Room Summary | 改为保守收敛，不再假设可直接删除 sync/unread |
| `search-optimization.md` | Search / hierarchy / context / threads | 改为“先抽 service，再评估收口” |

---

## 四、模块优先级

### 4.1 低风险

- `account_data`
- `device`
- `e2ee_routes` 中 r0/v3 完全同构的 keys 端点
- `media` 中已经共享 handler 的上传、下载、配置端点

### 4.2 中风险

- `friend_room`，因为存在 `friends` 与 `friendships` 的路径别名差异
- `search`，因为 threads 与 thread 模块仅“语义重叠”，并非完全同一接口

### 4.3 高风险

- `room_summary`，因为 `summary/sync` 和 `unread/clear` 目前仍走独立 service 逻辑
- 管理端点版本整理，因 `admin v2` 当前只覆盖有限用户接口，不能简单宣称取代 v1

---

## 五、文档使用方式

- 先看总方案文档，确认统一原则
- 再按模块文档逐一评估“哪些路由能合并、哪些只能共享实现”
- 进入代码实现时，优先对照 `space.rs` 与 `key_backup.rs` 的 `nest()` 写法
- 若后续真的增加兼容配置，再补充运行时配置文档，而不是先写成已存在 feature

---

## 六、后续落地建议

1. 先处理“路径不变、内部复用”的模块
2. 再处理“有重叠但语义不完全相同”的模块
3. 最后再评估是否需要统一兼容日志、版本访问统计、运行时开关

进一步建议（基于最近一次重构经验）：

1. 继续把 `routes/mod.rs` 的剩余 handler 推进到 `handlers/*.rs`（优先级：Room / Moderation 等仍较集中部分）
2. 继续在文档与代码中保持 `search` / `thread` 的职责边界一致；本轮已完成路由归属收口与状态归档

---

## 七、最近完成（2026-03-29）

### 7.1 Room 模块下沉（God File Split）

- 将房间主逻辑集中到 `src/web/routes/handlers/room.rs`，覆盖：房间基础查询与消息流、join/leave/upgrade/forget、state/receipt/read markers、moderation（kick/ban/unban/redact）、createRoom/room visibility/membership history
- `src/web/routes/mod.rs` 原本的 Room handler 已移除，保留为更薄的聚合与导出层
- 补回 `ApiError` 的导出兼容，避免其他路由模块引用断裂

### 7.2 验证

- 已执行 `cargo fmt`
- 已执行 `cargo test -q`
- 测试汇总：1654 passed; 0 failed; 1 ignored；204 passed; 0 failed；762 passed; 0 failed；doc tests 通过
- 运行测试时可能出现媒体目录创建失败的错误日志（只读文件系统），但不影响测试结果

### 7.3 Sync / Presence 下沉（God File Split）

- `src/web/routes/sync.rs` / `src/web/routes/presence.rs` 改为直接引用 `handlers::sync` 与 `handlers::presence`
- `src/web/routes/mod.rs` 移除 Sync / Presence handler 的 re-export，进一步压薄聚合层

### 7.4 Search / Thread 归属收口（2026-03-30）

- `src/web/routes/search.rs` / `src/web/routes/thread.rs` 改为薄路由包装层，实际实现下沉到 `src/web/routes/handlers/search.rs` 与 `src/web/routes/handlers/thread.rs`
- `search` 模块只保留搜索、context、hierarchy、timestamp_to_event 相关职责；旧的 `user/.../threads` 兼容入口改由 `thread` 模块承接
- `docs/API-OPTION/search-optimization.md` 与 `docs/API-OPTION/dm-optimization.md` 已同步最新实现状态
- `docs/API-OPTION/task-done/` 已补充评审记录、验证脚本、测试与交付归档
