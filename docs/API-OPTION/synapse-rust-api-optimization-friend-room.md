# Friend Room 模块优化方案

## 一、当前实现

`src/web/routes/friend_room.rs` 当前确实存在明显重复，但模式并不是“所有接口都有 v1 / v3 / r0 三套完全对称定义”。

从实际代码看：

- `/_matrix/client/v3/friends` 目前只提供 `GET`
- `/_matrix/client/v1/friends` 提供 `GET + POST`
- `/_matrix/client/r0/friendships` 提供 `GET + POST`
- 绝大多数好友请求、好友状态、好友分组接口只在 `v1` 与 `r0` 下重复
- 很多端点复用同一批 handler，例如 `get_friends`、`send_friend_request`、`update_friend_status`

这说明 friend_room 更适合做**按版本前缀的子路由复用**，而不是用一个 `{version}` 占位符强行覆盖所有路径。

---

## 二、关键约束

### 2.1 不能简单参数化的原因

当前模块至少有两个不对称点：

1. `v3` 使用 `/friends`
2. `r0` 主入口却是 `/friendships`

因此下面这种写法并不准确：

```rust
route("/_matrix/client/{version}/friends", get(get_friends))
```

它无法同时表达：

- `v3/friends`
- `v1/friends`
- `r0/friendships`

### 2.2 不建议的方案

- 不建议把整个模块改成 `/_matrix/client/{version}/*`
- 不建议依赖中间件把未知版本自动归一化为 `v3`
- 不建议引入 HTTP 重定向来做版本兼容

对 Matrix 风格 API，更稳妥的方式仍然是保留公开路径、在内部复用 handler。

---

## 三、推荐重构方式

### 3.1 拆出共享子路由

可以把 `v1` 与 `r0` 的大部分同构路径抽成子路由，再分别挂载：

```rust
pub fn create_friend_router(state: AppState) -> Router<AppState> {
    let common_friends_router = Router::new()
        .route("/friends/request", post(send_friend_request))
        .route("/friends/request/received", get(get_received_requests))
        .route(
            "/friends/request/{user_id}/accept",
            post(accept_friend_request),
        )
        .route(
            "/friends/request/{user_id}/reject",
            post(reject_friend_request),
        )
        .route(
            "/friends/request/{user_id}/cancel",
            post(cancel_friend_request),
        )
        .route("/friends/requests/incoming", get(get_incoming_requests))
        .route("/friends/requests/outgoing", get(get_outgoing_requests))
        .route("/friends/check/{user_id}", get(check_friendship))
        .route("/friends/suggestions", get(get_friend_suggestions))
        .route("/friends/{user_id}", delete(remove_friend))
        .route("/friends/{user_id}/note", put(update_friend_note))
        .route(
            "/friends/{user_id}/status",
            get(get_friend_status).put(update_friend_status),
        )
        .route("/friends/{user_id}/info", get(get_friend_info))
        .route("/friends/groups", get(get_friend_groups).post(create_friend_group));

    Router::new()
        .route("/_matrix/client/v3/friends", get(get_friends))
        .route(
            "/_matrix/client/v1/friends",
            get(get_friends).post(send_friend_request),
        )
        .route(
            "/_matrix/client/r0/friendships",
            get(get_friends).post(send_friend_request),
        )
        .nest("/_matrix/client/v1", common_friends_router.clone())
        .nest("/_matrix/client/r0", common_friends_router)
        .with_state(state)
}
```

这个方向的优点：

- 保持外部路径完全兼容
- 减少 `v1` / `r0` 的重复 `.route()` 定义
- 不需要让 handler 感知“版本参数”

### 3.2 保留不对称入口

以下入口建议继续显式保留：

| 路径 | 原因 |
|------|------|
| `/_matrix/client/v3/friends` | 当前只有该入口暴露 v3 好友列表 |
| `/_matrix/client/v1/friends` | `v1` 主入口 |
| `/_matrix/client/r0/friendships` | `r0` 使用不同资源名 |

这些路径不应被统一成单个 `{version}` 模板。

---

## 四、可落地的整理目标

### 4.1 第一阶段

- 提取 `v1` / `r0` 共享的好友请求子路由
- 提取 `v1` / `r0` 共享的好友详情与分组子路由
- 保留 `v3/friends`、`v1/friends`、`r0/friendships` 为显式定义

### 4.2 第二阶段

如果后续确认 `v3` 也要补全好友请求、分组等能力，再考虑：

- 让 `v3` 复用部分公共子路由
- 或显式新增 `v3` 对应端点

但这属于**功能扩展**，不是单纯“消除重复”。

---

## 五、与其他模块的边界

旧版本文档把 `account_data`、`media`、`device`、`search` 也一起纳入同一方案，并统一改写成 `{version}` 路由。
这一思路已经与当前仓库实际结构不一致。

因此本文件只聚焦 friend_room：

- 不再展开 account_data
- 不再展开 media
- 不再展开 device
- 不再展开 search

这些模块应分别在各自文档中按真实路由结构处理。

---

## 六、实施建议

| 文件 | 建议修改 |
|------|----------|
| `src/web/routes/friend_room.rs` | 提炼 `v1/r0` 公共子路由 |
| 处理函数 | 原则上保持不变 |
| 中间件 | 当前不需要新增统一版本中间件 |
| 配置项 | 当前不需要新增全局版本配置 |

---

## 七、最终结论

friend_room 模块最合理的优化方向是：

1. **保留现有公开路径差异**
2. **把 `v1` / `r0` 的重复路由改为共享子路由**
3. **不使用 `{version}` 路径参数统一整个模块**
4. **不把版本兼容问题扩大成全局中间件改造**
