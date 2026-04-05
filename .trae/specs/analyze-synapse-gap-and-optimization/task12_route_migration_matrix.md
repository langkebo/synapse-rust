# Task 12 - 路由迁移矩阵

## 1. 用法

本矩阵用于指导拆分时的“旧文件 -> 新模块”迁移。每个批次都必须保持对外行为不变。

## 2. 迁移矩阵

| 当前文件/函数簇 | 目标模块 | 批次 | 示例端点列表（抽样） | 旧入口 -> 新入口（可追踪） | 前置依赖 | 主要风险 | 回归重点 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `room.rs` 兼容版本路由 | `room/router_compat.rs`, `room/router_v3.rs` | Batch 4 | `/_matrix/client/r0/rooms/...`、`/_matrix/client/v3/rooms/...` | `room.rs::create_room_router` -> `room/router_*.rs::create_*_router` | handler 已稳定拆出 | 装配顺序漂移 | 版本 URL 不回退 |
| `handlers/room.rs` join/leave/invite/kick/ban/unban/forget | `room/membership.rs` | Batch 3.1 | join/leave/invite/kick/ban/unban/forget | `handlers/room.rs::{join,leave,invite,...}` -> `room/membership.rs::{join,leave,invite,...}` | `room/access.rs` 原型可用 | 403/404 漂移 | 成员态 API 测试 |
| `handlers/room.rs` state get/send/put/power levels | `room/state.rs` | Batch 3.2 | `GET/PUT /state/...`、power levels | `handlers/room.rs::{get_state,put_state,...}` -> `room/state.rs::{get_state,put_state,...}` | 事件构造 helper 收口 | state_key 语义变化 | state event 回归 |
| `handlers/room.rs` messages/context/search/unread/sync | `room/query.rs` | Batch 3.3 | `/messages`、`/context`、`/search`、unread | `handlers/room.rs::{messages,context,search,...}` -> `room/query.rs::{messages,context,search,...}` | query helper 收口 | 分页 token 漂移 | timeline/search/sync |
| `handlers/room.rs` room key/query/claim/forward | `room/keys.rs` | Batch 3.4 | room keys、claim、forward | `handlers/room.rs::{room_keys_*,claim,forward}` -> `room/keys.rs::{...}` | E2EE 公共类型抽出 | 与 `e2ee` 交叉依赖 | E2EE 契约测试 |
| `handlers/room.rs` hierarchy/space 相关端点 | `room/spaces.rs` | Batch 3.5 | hierarchy/space summary | `handlers/room.rs::{hierarchy,...}` -> `room/spaces.rs::{hierarchy,...}` | space 依赖梳理 | space 与 room 交叉 | hierarchy/summary |
| `handlers/room.rs` preview/metadata/render 读取 | `room/rendering.rs` | Batch 3.6 | preview/metadata/render | `handlers/room.rs::{preview,render,...}` -> `room/rendering.rs::{...}` | DTO 收口 | 响应字段缺失 | 响应结构断言 |
| `handlers/room.rs` 未支持/试验接口 | `room/experimental.rs` | Batch 3.7 | 明确不支持端点集合 | `handlers/room.rs::{...}` -> `room/experimental.rs::{...}` | unsupported 清单稳定 | 漏掉 feature gate | `M_UNRECOGNIZED` |
| `middleware.rs` CORS 与 origin 校验 | `middleware/cors.rs` | Batch 1.1 | 全站预检与跨域 header | `middleware.rs::{cors_*}` -> `middleware/cors.rs::{...}` | 挂载顺序核对 | 与 `server.rs` 双层 CORS 冲突 | 预检/跨域 smoke |
| `middleware.rs` CSRF 管理 | `middleware/csrf.rs` | Batch 1.2 | 写路径 CSRF | `middleware.rs::{csrf_*}` -> `middleware/csrf.rs::{...}` | state 访问稳定 | 写路径 CSRF 漂移 | 登录/写接口回归 |
| `middleware.rs` 安全头、request id、panic/timeout | `middleware/security.rs` | Batch 1.3 | 安全头、timeout | `middleware.rs::{security_*}` -> `middleware/security.rs::{...}` | server 装配复核 | header 或 timeout 变化 | header/timeout smoke |
| `middleware.rs` auth/replication auth | `middleware/auth.rs` | Batch 1.4 | 需要鉴权的全部端点 | `middleware.rs::{auth_*}` -> `middleware/auth.rs::{...}` | extractor 设计草案 | token 错误码变化 | 鉴权测试 |
| `middleware.rs` federation 签名链 | `middleware/federation_auth.rs` | Batch 1.5 | 联邦入口 | `middleware.rs::{federation_*}` -> `middleware/federation_auth.rs::{...}` | key fetch/cache 依赖梳理 | 联邦签名回退 | federation auth |
| `middleware.rs` 限流逻辑 | `middleware/rate_limit.rs` | Batch 1.6 | 受限流的端点集合 | `middleware.rs::{rate_limit_*}` -> `middleware/rate_limit.rs::{...}` | 配置读取稳定 | 阈值或 header 变化 | rate limit smoke |
| `e2ee_routes.rs` upload/query/claim/signatures | `e2ee/keys.rs` | Batch 2.1 | `/keys/upload`、`/keys/query`、`/keys/claim` | `e2ee_routes.rs::{keys_*}` -> `e2ee/keys.rs::{...}` | 共享 JSON helper | body 解析语义变化 | key upload/query/claim |
| `e2ee_routes.rs` sendToDevice/device list stream | `e2ee/device_lists.rs` | Batch 2.2 | `/sendToDevice`、device list stream | `e2ee_routes.rs::{to_device,device_lists_*}` -> `e2ee/device_lists.rs::{...}` | stream helper | 流水位置漂移 | to-device / changes |
| `e2ee_routes.rs` room key requests | `e2ee/key_requests.rs` | Batch 2.3 | room key requests CRUD | `e2ee_routes.rs::{room_key_request_*}` -> `e2ee/key_requests.rs::{...}` | E2EE access guard | request 状态回退 | request CRUD |
| `e2ee_routes.rs` trust / verification / security summary | `e2ee/trust.rs` | Batch 2.4 | verification + trust endpoints | `e2ee_routes.rs::{verification_*}` -> `e2ee/trust.rs::{...}` | 与 `verification_routes.rs` 边界确认 | 双入口继续分叉 | verification 回归 |
| `e2ee_routes.rs` secure backup | `e2ee/backup.rs` | Batch 2.5 | backup endpoints | `e2ee_routes.rs::{backup_*}` -> `e2ee/backup.rs::{...}` | passphrase validator | 备份口径漂移 | backup 测试 |

## 3. 停止条件

- 出现路由装配顺序变化导致 404/405 回退，立即停止并回滚该批次。
- 错误语义从 `404` 变成 `403` 或从 `M_UNRECOGNIZED` 漂移为其他错误码，不得继续后续批次。
- 任一批次新增循环依赖，必须回退并重新设计公共 helper 落点。
