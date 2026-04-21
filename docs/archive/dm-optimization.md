# DM 模块优化方案

## 一、当前实现

`src/web/routes/dm.rs` 当前有 7 个路由，结构如下：

```rust
/_matrix/client/r0/create_dm
/_matrix/client/v3/create_dm      // 新增 v3 创建别名
/_matrix/client/r0/direct         // 兼容路由
/_matrix/client/v3/direct
/_matrix/client/r0/direct/{room_id}  // 兼容路由
/_matrix/client/v3/direct/{room_id}
/_matrix/client/v3/rooms/{room_id}/dm
/_matrix/client/v3/rooms/{room_id}/dm/partner
```

对应判断：

- `create_dm` 仍是 r0 路径
- 其余查询与管理接口都在 v3
- 这不是“大量重复路由”问题，而是**版本演进不完全统一**

---

## 二、结合 Matrix 规范的结论

DM 在 Matrix 中更多是基于房间、账户数据和成员关系表达，不像 `account_data`、`device` 那样天然存在大批同构路由。
因此这里不适合套用“把所有版本写成 `{version}`”的模板。

更合理的目标是：

1. 保留当前 r0 `create_dm` 兼容入口
2. 保持 v3 的查询接口不变
3. 当前已经补齐 `v3` 的创建别名，继续保持**新增别名 + 内部共享 handler** 的兼容策略

---

## 三、可行优化方案

### 3.1 保守方案

当前最推荐的方案其实是**不改外部路径，只整理代码表达**：

```rust
pub fn create_dm_router(state: AppState) -> Router<AppState> {
    let v3_router = Router::new()
        .route("/direct", get(get_dm_rooms))
        .route("/direct/{room_id}", put(update_dm_room))
        .route("/rooms/{room_id}/dm", get(check_room_dm))
        .route("/rooms/{room_id}/dm/partner", get(get_dm_partner_route));

    Router::new()
        .route("/_matrix/client/r0/create_dm", post(create_dm_room))
        .nest("/_matrix/client/v3", v3_router)
        .with_state(state)
}
```

收益：

- 路由意图更清晰
- 保持现有行为不变
- 不引入新的 Matrix 兼容风险

### 3.2 已落地的兼容增强

当前代码已经提供 v3 版创建别名：

```rust
Router::new()
    .route("/_matrix/client/r0/create_dm", post(create_dm_room))
    .route("/_matrix/client/v3/create_dm", post(create_dm_room))
```

当前约束：
- 这属于**新增兼容能力**
- 不能直接删除 r0 入口
- 仍应保留 r0 与 v3 共享同一 handler 的实现方式

### 3.3 增强的 fallback 机制

当用户的 `m.direct` account data 为空时，代码会通过 `build_direct_map_from_memberships` 函数自动从用户的房间成员关系构建 DM 列表：

```rust
async fn build_direct_map_from_memberships(
    state: &AppState,
    user_id: &str,
) -> Result<Map<String, Value>, ApiError> {
    // 遍历用户所有房间
    // 筛选成员数为 2 的房间（1个自己 + 1个对方）
    // 自动识别 DM 伙伴并构建 m.direct 映射
}
```

此机制解决了以下问题：
- ✅ 用户直接加入的 DM 房间（不在 m.direct 中）也能被正确识别
- ✅ `get_dm_rooms`、`check_room_dm`、`get_dm_partner_route` 均支持 fallback

### 3.4 单元测试覆盖

[dm.rs:394-434](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/dm.rs#L394-L434) 包含单元测试：
- `test_direct_map_helpers_preserve_matrix_shape` - 验证 m.direct 数据结构正确性
- `test_parse_dm_users_requires_string_array` - 验证用户解析逻辑

---

## 四、不建议的方案

- 不建议改成 `/_matrix/client/{version}/create_dm`
- 不建议把 r0 入口做 HTTP 重定向到 v3
- 不建议把 DM 文档写成“必须统一成单版本”

原因是当前模块路由数量很少，强行抽象反而会降低可读性。

---

## 五、实施建议

| 项目 | 建议 |
|------|------|
| 代码调整优先级 | 低 |
| 是否需要立即重构 | 否 |
| 推荐动作 | 仅整理 v3 子路由结构 |
| 当前兼容状态 | 已新增 v3 创建别名，并保留 r0 |

---

## 六、最终结论

DM 模块当前最优策略不是“合并所有版本”，而是：

1. **保留 r0 创建入口**
2. **保留 v3 查询入口**
3. **在代码内部做轻量整理**
4. **继续保留 r0/v3 双入口与共享 handler**
