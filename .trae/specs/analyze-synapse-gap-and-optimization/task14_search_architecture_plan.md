# Task 14 - 搜索链路架构方案

## 1. 当前问题

当前仓库内至少存在三套搜索实现口径：

- 路由层直查 `events/users/rooms` 的 handler 搜索。
- `SearchService` 内部的 Postgres / Elasticsearch provider 抽象。
- `SearchIndexStorage` 基于 `search_index` 表的另一套检索实现。

这意味着 provider、权限、分页、排序和高亮语义并未形成单一事实源。

## 2. 目标架构

```text
Client Request
    -> Route Adapter
    -> SearchQuery DSL
    -> SearchCoordinator
    -> SearchProvider
       -> PostgresFtsProvider
       -> ElasticsearchProvider
    -> SearchResult Mapper
    -> Matrix Response
```

## 3. 分层职责

- Route Adapter：只做 Matrix 请求解析、兼容字段映射、错误映射。
- SearchQuery DSL：统一关键词、分页、过滤、排序、高亮、范围约束表达。
- SearchCoordinator：选择 provider，执行权限范围裁剪，统一结果分页 token。
- SearchProvider：执行具体检索，不感知 Matrix 路由细节。
- Result Mapper：把 provider 结果转换成 Matrix `search_categories.room_events` 等结构。

## 4. 最小落地方案

- `M1`：统一所有 `/search` 主入口到 `SearchCoordinator + PostgresFtsProvider`。
- `M2`：将 room 内搜索和用户/房间补全类搜索也改走统一 coordinator。
- `M3`：保留 `ElasticsearchProvider` 扩展位，仅在配置开启时替换执行层。

## 5. 兼容要求

- Matrix / Synapse 现有请求结构保持不变。
- 权限边界至少不弱于当前 handler 直查逻辑。
- route handler 内不再直接写大段搜索 SQL。
- `SearchIndexStorage` 若继续保留，只能作为 provider 内部实现，不再作为平行入口。
