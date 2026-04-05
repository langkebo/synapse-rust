# Task 13 - 房间权限守卫矩阵

## 1. 目标

统一房间相关路由中重复的 `room_exists`、成员校验、管理员校验和 owner/creator 校验，收敛为有限个高频 guard 模型。

## 2. 高频场景矩阵

| 场景 | 典型端点 | 当前重复模式 | 目标 guard | 对象不存在 | 无权限 | 审计字段 |
| --- | --- | --- | --- | --- | --- | --- |
| 房间只读查询 | `/rooms/{room_id}/members`, `/messages`, `/context` | `room_exists + is_member/admin` | `RoomMemberOrAdmin` | `404 + M_NOT_FOUND` | `403 + M_FORBIDDEN` | `user_id`, `room_id`, `guard=member_or_admin` |
| 房间成员操作 | join/leave/forget | `room_exists + membership checks` | `RoomMembershipTransition` | `404 + M_NOT_FOUND` | `403 + M_FORBIDDEN` | `user_id`, `room_id`, `transition` |
| 邀请/踢/封/解封 | invite/kick/ban/unban | `room_exists + is_member + power/admin` | `RoomAdminOnly` | `404 + M_NOT_FOUND` | `403 + M_FORBIDDEN` | `actor`, `target`, `room_id` |
| 状态事件写入 | `send_state_event`, `put_state_event` | `room_exists + member/admin + event type checks` | `RoomStateEditor` | `404 + M_NOT_FOUND` | `403 + M_FORBIDDEN` | `user_id`, `room_id`, `event_type` |
| creator/owner 操作 | metadata / owner-only actions | `room_exists + creator/owner` | `RoomOwnerOnly` | `404 + M_NOT_FOUND` | `403 + M_FORBIDDEN` | `user_id`, `room_id`, `owner_check` |
| E2EE 房间级接口 | room key request / distribution | `room_exists + member` | `RoomE2eeMemberOnly` | `404 + M_NOT_FOUND` | `403 + M_FORBIDDEN` | `user_id`, `room_id`, `e2ee_action` |
| 仅需对象存在 | metadata / summary 只读路径 | `room_exists` | `RoomMustExist` | `404 + M_NOT_FOUND` | n/a | `room_id`, `guard=must_exist` |

## 3. 推荐 guard 集合

- `RoomMustExist`
- `RoomMemberOnly`
- `RoomMemberOrAdmin`
- `RoomAdminOnly`
- `RoomOwnerOnly`
- `RoomStateEditor`
- `RoomMembershipTransition`
- `RoomE2eeMemberOnly`

## 4. 统一错误语义

- 对象不存在优先返回 `404 + M_NOT_FOUND`，禁止把“不存在”吞成 `403`。
- 对象存在但用户无权访问时返回 `403 + M_FORBIDDEN`。
- 未支持能力单独走 `M_UNRECOGNIZED`，不放进 guard 决策。
- 参数格式错误仍由 handler 或 extractor 返回 `400 + M_INVALID_PARAM` / `M_BAD_JSON`。
