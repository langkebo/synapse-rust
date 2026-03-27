# DM 模块优化方案

## 一、当前实现

`src/web/routes/dm.rs` 当前只有 5 个路由，结构如下：

```rust
/_matrix/client/r0/create_dm
/_matrix/client/v3/direct
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
3. 如果未来需要补 v3 的创建入口，优先做**新增别名 + 内部共享 handler**

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

### 3.2 可选增强方案

如果后续确认客户端需要 v3 版创建接口，可以考虑增加别名：

```rust
Router::new()
    .route("/_matrix/client/r0/create_dm", post(create_dm_room))
    .route("/_matrix/client/v3/create_dm", post(create_dm_room))
```

注意：

- 这应视为**新增兼容能力**
- 不能直接删除 r0 入口
- 需要确认客户端是否真的会使用该 v3 别名

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
| 若后续扩展 | 新增 v3 创建别名，但保留 r0 |

---

## 六、最终结论

DM 模块当前最优策略不是“合并所有版本”，而是：

1. **保留 r0 创建入口**
2. **保留 v3 查询入口**
3. **在代码内部做轻量整理**
4. **后续按实际客户端需求再决定是否补 v3 创建别名**
