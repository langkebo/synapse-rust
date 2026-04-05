# Task 13 - guard / extractor 设计

## 1. 设计目标

将房间访问控制从“路由内重复 helper 调用”演进为“可复用 extractor + guard 组合”，同时避免显著增加数据库往返。

## 2. 推荐结构

```text
src/web/
├── extractors/
│   ├── authenticated_user.rs
│   ├── room_context.rs
│   └── room_permission.rs
└── guards/
    ├── room_exists.rs
    ├── room_member.rs
    ├── room_admin.rs
    ├── room_owner.rs
    └── room_state_editor.rs
```

## 3. 抽象分层

- Extractor
  - 负责把 `room_id`、`user_id`、可能的 membership/power level 上下文提取到统一类型。
- Guard
  - 负责对 extractor 产物做权限判定与错误映射。
- Helper
  - 保留给低频或迁移中间态场景，避免一开始过度抽象。

## 4. 建议核心类型

```rust
struct RoomContext {
    room_id: String,
    user_id: String,
    room_exists: bool,
    membership: Option<String>,
    is_admin: bool,
    is_owner: bool,
}

enum GuardDecision {
    Allow(RoomContext),
    NotFound,
    Forbidden(&'static str),
}
```

## 5. 统一错误映射（GuardDecision -> HTTP + Matrix errcode）

| GuardDecision | HTTP | Matrix errcode | error 文案模板 | 备注 |
| --- | --- | --- | --- | --- |
| `Allow(_)` | 2xx | n/a | n/a | handler 继续返回具体业务响应 |
| `NotFound` | 404 | `M_NOT_FOUND` | `Not found` | 禁止把“不存在”吞成 `403` |
| `Forbidden(reason)` | 403 | `M_FORBIDDEN` | `Forbidden: {reason}` | `{reason}` 必须是稳定枚举值，避免自由文本漂移 |

## 6. 审计/日志字段规范（最小必填）

| 字段 | 来源 | 说明 |
| --- | --- | --- |
| `request_id` | request context / tracing span | 必填，串联路由与 guard |
| `user_id` | `AuthenticatedUser` | 必填 |
| `room_id` | path param | 房间域必填 |
| `guard` | guard 名称 | 例如 `RoomMemberOrAdmin` |
| `decision` | `allow/not_found/forbidden` | 必填 |
| `deny_reason` | `Forbidden(reason)` | 仅拒绝时填写，使用稳定枚举 |
| `membership` | extractor 缓存 | 可选，避免二次查库 |
| `is_admin` | extractor 缓存 | 可选 |

约束：
- 禁止记录 access token、device private key、未脱敏的第三方凭证。
- `deny_reason` 只能从受控枚举生成，例如 `not_member`、`not_admin`、`not_owner`、`power_level_insufficient`。

## 5. 查询优化原则

- 优先合并 `room_exists + membership + admin` 为单次上下文查询。
- 高热路径只允许在 extractor 内一次性构建 `RoomContext`，guard 不再单独二次查库。
- `RoomAdminOnly` / `RoomStateEditor` 需要 power level 时，可按需延迟补充，但必须在同一 helper 内集中处理。

## 7. 典型用例覆盖（Success / NotFound / Forbidden）

| Guard | Success 样例 | NotFound 样例 | Forbidden 样例 |
| --- | --- | --- | --- |
| `RoomMustExist` | room 存在 | room 不存在 | n/a |
| `RoomMemberOnly` | membership=`join` | room 不存在 | room 存在但 membership!=`join` |
| `RoomMemberOrAdmin` | member 或 `is_admin=true` | room 不存在 | 非 member 且非 admin |
| `RoomAdminOnly` | `is_admin=true` 或 power level 满足 | room 不存在 | room 存在但权限不足 |
| `RoomStateEditor` | 满足 state 编辑权限 | room 不存在 | 无权限编辑指定 event_type/state_key |

## 8. 首批试点接口

- `GET /rooms/{room_id}/members`
- `GET /rooms/{room_id}/messages`
- `PUT /rooms/{room_id}/state/{event_type}/{state_key}`
- `POST /rooms/{room_id}/invite`
- `POST /rooms/{room_id}/ban`
