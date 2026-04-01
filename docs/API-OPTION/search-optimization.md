# Search 模块优化方案

## 一、当前实现

`src/web/routes/search.rs` 当前作为薄包装层，实际路由实现位于 `src/web/routes/handlers/search.rs`，整体分成三类：

1. `v3` / `r0` 共享的搜索端点
2. `v1` / `v3` 分别暴露的上下文与层级端点
3. 已迁移到 `thread` 模块的线程兼容入口

实际路由如下：

```rust
/_matrix/client/v3/search
/_matrix/client/r0/search
/_matrix/client/v3/search_recipients
/_matrix/client/r0/search_recipients
/_matrix/client/v3/search_rooms
/_matrix/client/r0/search_rooms
/_matrix/client/v1/rooms/{room_id}/hierarchy
/_matrix/client/v3/rooms/{room_id}/hierarchy
/_matrix/client/v1/rooms/{room_id}/timestamp_to_event
/_matrix/client/v1/rooms/{room_id}/context/{event_id}
/_matrix/client/v3/rooms/{room_id}/context/{event_id}
```

---

## 二、核心判断

### 2.1 搜索端点适合做子路由复用

`search`、`search_recipients`、`search_rooms` 在 `v3` 与 `r0` 下完全复用同一 handler。
这一部分适合参考 `space.rs` / `key_backup.rs` 的写法，用 `nest()` 复用子路由。

### 2.2 `context` / `hierarchy` 不适合直接参数化

当前 `context` 是 `v1 + v3`，`hierarchy` 也是 `v1 + v3`，而 `timestamp_to_event` 只有 `v1`。
因此把全部改成 `/_matrix/client/{version}/...` 并不准确，也会让不支持的版本看起来像是“理论上可用”。

### 2.3 `threads` 已与 thread 模块边界对齐

历史上的 `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads` 已改由 `thread` 模块承接，避免与独立 thread 模块职责重叠。
当前 search 与 thread 的职责边界已经对齐。

---

## 三、推荐优化方案

### 3.1 只抽取真正同构的搜索子路由

```rust
pub fn create_search_router(state: AppState) -> Router<AppState> {
    let compat_router = Router::new()
        .route("/search", post(search))
        .route("/search_recipients", post(search_recipients))
        .route("/search_rooms", post(search_rooms));

    Router::new()
        .nest("/_matrix/client/v3", compat_router.clone())
        .nest("/_matrix/client/r0", compat_router)
        .route(
            "/_matrix/client/v1/rooms/{room_id}/hierarchy",
            get(get_room_hierarchy),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/hierarchy",
            get(get_room_hierarchy_v3),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/context/{event_id}",
            get(get_event_context),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/context/{event_id}",
            get(get_event_context),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/timestamp_to_event",
            get(timestamp_to_event),
        )
        .with_state(state)
}
```

这个方案只对真正重复的 `v3/r0` 搜索接口做收敛，不会误伤 `v1` 特有路径。

### 3.2 `threads` 模块归属已完成

已完成动作：

- `search` 路由实现中已不再注册 `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads`
- 线程查询入口已统一收口到 `thread` 模块
- 兼容入口由 `thread` 模块内部复用 service 承接

---

## 四、模块边界建议

| 能力 | 建议归属 |
|------|----------|
| `search` / `search_recipients` / `search_rooms` | `search.rs` |
| `rooms/.../context/{event_id}` | `search.rs` |
| `rooms/.../hierarchy` | `search.rs` |
| `rooms/.../timestamp_to_event` | `search.rs` |
| `user/.../threads` | `thread.rs` |

这样可以避免“搜索模块同时承载线程能力”的职责混乱。

---

## 五、不建议的方案

- 不建议把全部搜索相关路径统一成 `/{version}/...`
- 不建议通过“内部转发到 v3”来描述 r0 或 v1 兼容
- 不建议把 `v1/context`、`v1/hierarchy` 写成“旧版重定向到 v3”

更准确的表达应是：**这些路径当前就分别存在，并共享部分 handler。**

---

## 六、实施建议

| 项目 | 建议 |
|------|------|
| 搜索三件套 | 提取 `v3/r0` 共享子路由 |
| `context` | 保持 `v1/v3` 显式注册，继续共享 handler |
| `hierarchy` | 保持 `v1/v3` 显式注册，注意 `v3` 使用独立 handler |
| `timestamp_to_event` | 保留 `v1` 独立路由 |
| `threads` | 已从 `search.rs` 移出，并入 `thread.rs` |

---

## 七、最终结论

Search 模块真正应该做的不是“全量版本参数化”，而是：

1. **仅对 `v3/r0` 完全同构的搜索端点使用 `nest()` 复用**
2. **保留 `v1/v3` 的上下文与层级显式路径**
3. **已将 `threads` 从 search 模块中剥离出去**
4. **用模块职责清晰化替代简单的路由数量压缩**
