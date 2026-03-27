# Room Summary 模块优化方案

## 一、当前实现

`src/web/routes/room_summary.rs` 目前同时包含三类接口：

1. `/_matrix/client/v3/...` 主体接口
2. `/_matrix/client/r0/...` 只读兼容接口
3. `/_synapse/room_summary/v1/...` 内部管理接口

实际结构并不是“v3 和 r0 全量对称”，而是：

- `v3` 有完整 CRUD、成员管理、状态管理、统计、同步、英雄重算、未读清理
- `r0` 只有只读能力：`summary`、`members`、`state`、`stats`
- `/_synapse/room_summary/v1/*` 是内部接口，不应和 Matrix client API 混在同一版本统一策略里

---

## 二、关键判断

### 2.1 这里只适合做“只读兼容子路由”复用

由于 `r0` 仅覆盖只读子集，下面这种写法并不准确：

```rust
"/_matrix/client/{version}/rooms/{room_id}/summary"
```

因为它会让文档看起来像是 `r0` 也支持：

- `POST /summary`
- `PUT /summary`
- `DELETE /summary`
- `POST /summary/members`
- `PUT /summary/state/...`

而这些在当前实现里并不存在。

### 2.2 `sync_room_summary` 与 `clear_unread` 需要单独评估

这两个接口确实与更通用的同步 / 已读体系存在能力重叠，但当前代码并没有把它们接到主 sync 模块上。
因此文档更适合写成：

- “建议后续评估整合”
- 而不是“已经可以直接转发或立即废弃”

### 2.3 内部 Synapse 路由应保持独立

`/_synapse/room_summary/v1/summaries` 和 `/_synapse/room_summary/v1/updates/process` 是项目内部接口。
它们不应纳入 Matrix client API 的版本统一模板。

---

## 三、推荐优化方案

### 3.1 抽取只读公共子路由

```rust
pub fn create_room_summary_router(state: AppState) -> Router<AppState> {
    let read_only_router = Router::new()
        .route("/rooms/{room_id}/summary", get(get_room_summary))
        .route("/rooms/{room_id}/summary/members", get(get_members))
        .route("/rooms/{room_id}/summary/state", get(get_all_state))
        .route("/rooms/{room_id}/summary/stats", get(get_stats));

    Router::new()
        .nest("/_matrix/client/v3", read_only_router.clone())
        .nest("/_matrix/client/r0", read_only_router)
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary",
            post(create_room_summary).put(update_room_summary).delete(delete_room_summary),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/sync",
            post(sync_room_summary),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/members",
            post(add_member),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/members/{user_id}",
            put(update_member).delete(remove_member),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/state/{event_type}/{state_key}",
            get(get_state).put(update_state),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/stats/recalculate",
            post(recalculate_stats),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/heroes/recalculate",
            post(recalculate_heroes),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/summary/unread/clear",
            post(clear_unread),
        )
        .route(
            "/_synapse/room_summary/v1/summaries",
            get(get_user_summaries).post(create_room_summary),
        )
        .route(
            "/_synapse/room_summary/v1/updates/process",
            post(process_updates),
        )
        .with_state(state)
}
```

这个方案的重点是：

- 只复用 `v3/r0` 真实重叠的只读路径
- 保留 `v3` 独有写接口的显式注册
- 不把内部 `_synapse` 接口混入版本统一逻辑

### 3.2 对重叠功能采用“后续整合”表述

更准确的建议应是：

| 接口 | 当前状态 | 建议 |
|------|----------|------|
| `/summary/sync` | 已实现 | 评估是否并入主 sync 流程 |
| `/summary/unread/clear` | 已实现 | 评估是否与统一未读/已读体系收敛 |
| `/_synapse/room_summary/v1/*` | 已实现 | 保持独立 |

---

## 四、不建议的方案

- 不建议把整个 room_summary 写成 `/{version}` 通用模板
- 不建议假设 `r0` 拥有和 `v3` 一样的写能力
- 不建议把 `/_synapse/*` 内部路由算作 client API 版本兼容的一部分
- 不建议在文档中写入当前不存在的 `compat-r0-summary` 或 `compat-deprecated` feature

---

## 五、实施建议

| 项目 | 建议 |
|------|------|
| `v3/r0` 只读接口 | 抽共享子路由 |
| `v3` 写接口 | 保持显式定义 |
| `/summary/sync` | 暂不删除，先做模块边界评估 |
| `/summary/unread/clear` | 暂不删除，先确认与已读体系的关系 |
| `_synapse` 内部接口 | 保持独立 |

---

## 六、最终结论

Room Summary 模块真正适合的优化方向是：

1. **仅统一 `v3/r0` 的只读路由**
2. **保留 `v3` 独有写接口与维护接口**
3. **把 `sync` / `unread` 视为待整合能力，而不是立即删除的重复项**
4. **明确区分 Matrix client API 与 `_synapse` 内部接口**
