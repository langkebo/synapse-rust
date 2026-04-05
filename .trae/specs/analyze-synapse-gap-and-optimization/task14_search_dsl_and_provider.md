# Task 14 - 搜索 DSL 与 Provider 设计

## 1. 统一 DSL

```rust
struct SearchQuery {
    scope: SearchScope,
    term: String,
    room_ids: Vec<String>,
    sender_ids: Vec<String>,
    event_types: Vec<String>,
    limit: u32,
    offset: Option<u32>,
    next_token: Option<String>,
    sort: SearchSort,
    order: SortOrder,
    include_highlight: bool,
    include_context: bool,
    time_range: Option<TimeRange>,
}
```

## 2. 语义要求

- `scope` 至少覆盖 `RoomEvents`、`RoomDirectory`、`UserDirectory`。
- `next_token` 统一由 coordinator 生成，provider 不直接暴露底层 offset/scroll 语义。
- `sort` 至少支持 `Rank`、`Recent`。
- `include_highlight` 不支持时显式降级为空，而不是伪高亮。

## 3. Matrix/Synapse 请求 -> DSL 映射（room_events）

| Matrix/Synapse 字段（抽象） | DSL 字段 | 口径 |
| --- | --- | --- |
| `search_term` | `term` | 统一做 trim；空字符串直接 `M_INVALID_PARAM` |
| `filter.rooms` | `room_ids` | 仅 room_events scope 允许；为空表示不限制 |
| `filter.senders` | `sender_ids` | 多值 OR；provider 不支持时返回能力限制错误 |
| `filter.types` | `event_types` | 事件 type 过滤；与 `type`（内部分类）区分 |
| `order_by` | `sort` + `order` | `rank` -> `Rank`；`recent` -> `Recent` |
| `limit` | `limit` | coordinator 强制最大值上限，避免深分页压垮 DB |
| `next_batch` / `next_token` | `next_token` | 只允许 coordinator 生成的 token；provider 不直出 |
| `event_context.before_limit/after_limit` | `include_context` +（context 参数） | DSL 只声明是否需要 context；具体上下文窗口由 adapter 解析 |
| `include_highlight` | `include_highlight` | provider 不支持时允许降级为“无高亮但正常搜索结果” |

## 3. Provider Trait

```rust
trait SearchProvider {
    async fn search(&self, query: &SearchQuery, authz: &SearchAuthzScope)
        -> Result<SearchPage, SearchError>;
    fn capabilities(&self) -> SearchCapabilities;
}
```

## 4. Provider 能力矩阵

| 能力 | Postgres FTS | Elasticsearch | 降级策略 |
| --- | --- | --- | --- |
| 全文检索 | 支持 | 支持 | n/a |
| 排名排序 | 支持 | 支持 | n/a |
| 时间范围过滤 | 支持 | 支持 | n/a |
| 复杂 highlight | 基础支持 | 强支持 | Postgres 返回简化片段 |
| 多字段 boost | 有限支持 | 强支持 | 约束为公共子集 |
| 深分页 | 有限支持 | 较强支持 | 超阈值时返回能力限制错误或游标化分页 |

## 5. 错误语义

- 参数非法：`M_INVALID_PARAM`
- provider 不可用：`M_UNKNOWN`
- 能力不支持但端点存在：返回明确错误，不允许静默返回空结果
- 搜索结果为空：允许 200 + 空结果，但必须是真实查询后的空结果

## 6. 分页 token 规范（coordinator 生成）

### 6.1 Token 编码

- 使用 `base64url(json)`，json 结构必须带版本号：

```json
{
  "v": 1,
  "scope": "room_events",
  "sort": "recent",
  "order": "desc",
  "cursor": {
    "origin_server_ts": 1710000000000,
    "event_id": "$event",
    "rank": null
  }
}
```

### 6.2 稳定性约束

- `Recent` 排序必须至少使用 `(origin_server_ts, event_id)` 作为稳定游标，禁止只用单字段时间戳导致重复/漏项。
- `Rank` 排序必须定义确定性 tie-break（例如 `(rank, origin_server_ts, event_id)`），并把 tie-break 字段纳入 cursor。
- provider 不得自行解释 token 内部字段；token 仅由 coordinator 解析与生成。
