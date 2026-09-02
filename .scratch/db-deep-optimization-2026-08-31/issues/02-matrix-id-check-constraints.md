# 02: Matrix ID 字段 CHECK 约束

**What to build:** 在 v11 baseline 中为 user_id、room_id、event_id、sender、mxc:// 字段添加正则 CHECK 约束，防止非法 Matrix 标识符写入数据库。

**Blocked by:** 01-v11-baseline-scaffold

**Status:** ready-for-agent

- [ ] 在 `users` 表：`user_id TEXT NOT NULL` → 加 `CHECK (user_id ~ '^@[a-zA-Z0-9._=-]+:[a-zA-Z0-9.-]+$')`
- [ ] 在 `rooms` 表：`room_id TEXT NOT NULL` → 加 `CHECK (room_id ~ '^![a-zA-Z0-9._=-]+:[a-zA-Z0-9.-]+$')`
- [ ] 在 `events` 表：`event_id TEXT NOT NULL` → 加 `CHECK (event_id ~ '^\$[a-zA-Z0-9._=-]+:[a-zA-Z0-9.-]+$')`
- [ ] 在 `events` 表：`sender TEXT NOT NULL` → 加与 user_id 相同的 CHECK
- [ ] 在 `event_relations`、`room_memberships` 等关联表：加对应 ID 字段的 CHECK
- [ ] 在 `device_lists`、`push_rules` 等表：加 user_id CHECK
- [ ] 在 `mxc://` 媒体字段（`content_url`、`avatar_url`）加：`CHECK (url ~ '^mxc://[^/]+/[^/]+$')`
- [ ] 验证：构造非法 ID 字符串尝试写入，确认被约束拒绝
- [ ] 验证：合法 Matrix ID 写入不受影响

**约束命名规范**: `ck_<table>_<column>_matrix_format`
