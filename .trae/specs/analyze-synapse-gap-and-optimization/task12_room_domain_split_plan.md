# Task 12 - 房间域拆分蓝图

## 1. 目标

将 `src/web/routes/room.rs`、`src/web/routes/handlers/room.rs`、`src/web/middleware.rs`、`src/web/routes/e2ee_routes.rs` 从“按文件聚合”收敛为“按领域聚合”，降低单文件复杂度，同时保持 URL、HTTP 方法、错误码与现有测试入口不回退。

## 2. 当前状态

- `room.rs` 主要承担版本路由装配，但一次性聚合大量房间 handler。
- `handlers/room.rs` 同时承载创建房间、成员关系、状态事件、消息查询、搜索、同步、房间 key 与未支持端点。
- `middleware.rs` 同时包含 CORS、CSRF、安全头、限流、认证、联邦签名、复制鉴权、超时等横切逻辑。
- `e2ee_routes.rs` 混合了路由装配、handler、DTO、手写 SQL 和响应拼装，与 `verification_routes.rs` 有边界重叠风险。

### 2.1 现状职责快照（可追踪映射）

| 领域职责簇 | 现状主要落点（文件） | 目标模块 | 代表性端点/行为（样例） |
| --- | --- | --- | --- |
| 房间成员关系 | `src/web/routes/handlers/room.rs` | `src/web/routes/room/membership.rs` | join/leave/invite/kick/ban/unban/forget |
| 房间状态事件 | `src/web/routes/handlers/room.rs` | `src/web/routes/room/state.rs` | get/send/put state、power levels |
| Timeline/上下文/成员列表 | `src/web/routes/handlers/room.rs` | `src/web/routes/room/query.rs` | messages/context/members/unread |
| 房间内搜索（room scope） | `src/web/routes/handlers/room.rs`（直查或旁路） | `src/web/routes/room/query.rs`（Route Adapter） + `src/services/search/*`（coordinator/provider） | room events search、分页 token |
| 房间级 E2EE keys | `src/web/routes/handlers/room.rs` + `src/web/routes/e2ee_routes.rs` | `src/web/routes/room/keys.rs` + `src/web/routes/e2ee/keys.rs` | room keys、claim/forward、distribution |
| Space（room 视角） | `src/web/routes/handlers/room.rs` | `src/web/routes/room/spaces.rs` | hierarchy、space 关系查询 |
| 房间渲染/元信息读取 | `src/web/routes/handlers/room.rs` | `src/web/routes/room/rendering.rs` | preview/metadata/render |
| 未支持/试验路径 | 多处分散（包含 `handlers/room.rs`） | `src/web/routes/room/experimental.rs` | 明确 `M_UNRECOGNIZED` 或 feature gate |
| 版本路由装配 | `src/web/routes/room.rs` | `src/web/routes/room/router_v3.rs`, `router_compat.rs` | `/_matrix/client/r0`/`v3` 的稳定装配 |

### 2.2 `middleware.rs` 收敛边界（guard/extractor vs 横切关注点）

| 类别 | 现状落点 | 目标落点 | 处理口径 |
| --- | --- | --- | --- |
| CORS/origin | `middleware.rs` | `src/web/middleware/cors.rs` | 继续作为 middleware；仅做跨域与预检，不引入业务语义 |
| CSRF | `middleware.rs` | `src/web/middleware/csrf.rs` | 继续作为 middleware；只覆盖写路径与 session 相关入口 |
| 安全头/request id/timeout | `middleware.rs` | `src/web/middleware/security.rs` | 继续作为 middleware；对 handler 透明 |
| 认证（token） | `middleware.rs` | `src/web/middleware/auth.rs` + `src/web/extractors/authenticated_user.rs` | “身份识别”走 extractor；“全局鉴权/拒绝”仍作为 middleware |
| 复制鉴权（replication auth） | `middleware.rs` | `src/web/middleware/auth.rs` | 继续作为 middleware，保持错误语义与 header 约定不变 |
| 联邦签名与联邦鉴权 | `middleware.rs` | `src/web/middleware/federation_auth.rs` | 继续作为 middleware；不下沉到 room 领域模块 |
| 房间存在性/成员/管理员等访问控制 | 当前分散在各 handler/helper | `src/web/extractors/room_context.rs` + `src/web/guards/*` | 从 middleware 中剥离，统一由 Task 13 的 guard/extractor 收敛 |
| 限流 | `middleware.rs` | `src/web/middleware/rate_limit.rs` | 继续作为 middleware；与业务 guard 分离 |

## 3. 拆分原则

- 以 bounded context 拆分，不按文件行数平均切块。
- 装配层稳定优先，先拆 handler 与 helper，再瘦身聚合路由。
- 所有新模块都必须声明职责、输入输出、依赖方向和禁止跨界访问项。
- `M_UNRECOGNIZED`、`M_NOT_FOUND`、`M_FORBIDDEN` 等错误语义保持不变。

## 4. 目标模块树

```text
src/web/routes/
├── room.rs
├── room/
│   ├── router_compat.rs
│   ├── router_v3.rs
│   ├── membership.rs
│   ├── state.rs
│   ├── query.rs
│   ├── keys.rs
│   ├── spaces.rs
│   ├── rendering.rs
│   ├── experimental.rs
│   └── access.rs
├── e2ee/
│   ├── router_compat.rs
│   ├── router_v3.rs
│   ├── keys.rs
│   ├── device_lists.rs
│   ├── key_requests.rs
│   ├── trust.rs
│   └── backup.rs
└── middleware/
    ├── cors.rs
    ├── csrf.rs
    ├── security.rs
    ├── auth.rs
    ├── federation_auth.rs
    └── rate_limit.rs
```

## 5. 模块职责

| 模块 | 收口职责 | 禁止跨界访问 |
| --- | --- | --- |
| `room/membership.rs` | join/leave/invite/kick/ban/unban/forget | 直接拼装 room summary |
| `room/state.rs` | state get/send/put、power levels | 直接处理 membership side effects |
| `room/query.rs` | members/messages/context/search/unread/sync | 直接创建事件或修改 membership |
| `room/keys.rs` | 房间级 key 查询、claim、forward | 直接装配设备验证路由 |
| `room/spaces.rs` | hierarchy、space 关系查询 | 修改普通房间状态 |
| `room/experimental.rs` | 显式未支持/受 feature gate 约束端点 | 混入稳定路径 |
| `e2ee/trust.rs` | 设备验证、信任状态、安全摘要 | 继续维持 verification 双入口 |
| `middleware/auth.rs` | token 认证、复制鉴权 | 混入 CORS / CSRF |

## 6. 实施顺序

1. 先拆 `middleware.rs`
2. 再拆 `e2ee_routes.rs`
3. 再拆 `handlers/room.rs`
4. 最后瘦身 `room.rs`

## 7. 文件规模目标

- `room.rs` 控制在 250 行以内。
- 单个 room 子模块控制在 400-600 行，超过 800 行继续拆分。
- `middleware` 任一子模块不超过 350 行，联邦签名模块可放宽到 500 行。
- `e2ee_routes.rs` 完成拆分后只保留路由入口，不再承载大段 SQL。

## 8. 验收口径

- `room.rs`、`middleware.rs`、`e2ee_routes.rs` 的职责边界清晰可解释。
- 旧 handler 到新模块均可唯一映射，并能在“现状职责快照”和“路由迁移矩阵”中定位到归属模块。
- 路由 URL、HTTP method、错误语义与现有回归测试入口保持兼容。
