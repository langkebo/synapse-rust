# DB Schema 审计报告
**项目**: synapse-rust (Matrix homeserver)
**审计时间**: 2026-09-04
**范围**: `migrations/00000000_unified_schema_v11.sql` + 27 个增量迁移
**规模**: 253 表 · 6685 行 SQL · 366 索引

---

## 一、总体指标

| 指标 | 数值 |
|------|------|
| 总表数 | 253 |
| 增量迁移 | 27 对（SQL + undo） |
| 总索引数 | 366 |
| 被 FK 引用过的表 | 19（顶层实体表） |
| CHECK 约束总数 | 约 15（主要在 rooms 表） |
| Matrix Spec v1.11 对齐度 | 基本合规 ⚠️ |

---

## 二、🔴 阻塞级问题（高优先级，需修复）

### 2.1 `device_signatures` — 全表 0 索引，联邦热点

```
当前状态：仅有 PK (user_id, device_id, target_user_id, target_device_id, algorithm, signature)
```

**问题**：`/keys/query` 联邦查询 `WHERE user_id = ? AND device_id = ?` 全表扫。在大型 homeserver 上可累积数十万行，每次联邦设备同步触发全表扫描。

**修复建议**：
```sql
-- 必须添加的索引（联邦热点查询）
CREATE INDEX idx_device_signatures_user_device
    ON device_signatures(user_id, device_id);

CREATE INDEX idx_device_signatures_target
    ON device_signatures(target_user_id, target_device_id);

-- 建议添加的外键（数据完整性）
ALTER TABLE device_signatures
    ADD CONSTRAINT fk_device_signatures_user
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    ADD CONSTRAINT fk_device_signatures_device
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE,
    ADD CONSTRAINT fk_device_signatures_target_user
    FOREIGN KEY (target_user_id) REFERENCES users(user_id) ON DELETE CASCADE;
```

---

### 2.2 `room_memberships` — `membership` 无 CHECK 约束

```
membership TEXT NOT NULL  -- 可写入任意字符串，无校验
```

**问题**：Matrix Spec §7.2 要求 membership 值必须是 `invite | join | knock | leave | ban`。当前无 CHECK，可写入脏数据（如 `JOIN`、`active`）。

**修复建议**：
```sql
ALTER TABLE room_memberships
    ADD CONSTRAINT ck_room_memberships_valid
    CHECK (membership IN ('invite', 'join', 'knock', 'leave', 'ban'));
```

> 注：Synapse 原生也缺失此约束，属 Matrix 实现共同问题。

---

### 2.3 `event_edges` — `prev_event_id` 无 FK 约束

```
prev_event_id TEXT NOT NULL  -- 无 FK，孤儿 prev_event_id 可写入
```

**问题**：不引用 `events(event_id)`，无法保证 DAG 完整性。恶意/损坏数据可写入不存在的 `prev_event_id`，破坏事件图遍历。

**修复建议**：
```sql
-- 新增外键（需要兼容已存在孤儿数据，先 NO ACTION）
ALTER TABLE event_edges
    ADD CONSTRAINT fk_event_edges_prev
    FOREIGN KEY (prev_event_id) REFERENCES events(event_id)
    ON DELETE SET NULL;  -- 或 NO ACTION + Rust 层清理

-- 补充索引（向后遍历事件图）
CREATE INDEX idx_event_edges_prev_room
    ON event_edges(prev_event_id, event_id);
```

---

### 2.4 `events` 表 — 孤儿 `redacted_by` 无约束

```
redacted_by TEXT  -- 引用 event_id 但无 FK
```

**问题**：被删除的 `event_id` 留下孤儿 `redacted_by` 引用。

**修复建议**：
```sql
ALTER TABLE events
    ADD CONSTRAINT fk_events_redacted_by
    FOREIGN KEY (redacted_by) REFERENCES events(event_id)
    ON DELETE SET NULL;
```

---

## 三、🟠 中优先级问题

### 3.1 `device_keys` — Unique Key 漏 `algorithm`，与 Spec 不一致

```
当前: UNIQUE (user_id, device_id, key_id)
Spec: 应包含 algorithm
```

Matrix Spec §10.2：`/keys/upload` 的 key_id 格式为 `<algorithm>:<key_id>`，但 UQ 约束不含 algorithm，导致同一设备同一 key_id 不同 algorithm 的重名 key 可冲突写入。

**修复建议**：
```sql
-- 迁移步骤（先建新约束，验证后再删旧）
ALTER TABLE device_keys
    ADD CONSTRAINT uq_device_keys_user_device_algorithm_keyid
    UNIQUE (user_id, device_id, algorithm, key_id);

-- 验证后
ALTER TABLE device_keys
    DROP CONSTRAINT uq_device_keys_user_device_keyid;  -- 旧约束
```

---

### 3.2 `e2ee_audit_log` — 缺关键索引

```
已有: user_id, created_ts, action, (user_id, created_ts)
缺失: device_id, room_id, event_id 单独/组合索引
```

**修复建议**：
```sql
CREATE INDEX idx_e2ee_audit_log_device
    ON e2ee_audit_log(device_id) WHERE device_id IS NOT NULL;

CREATE INDEX idx_e2ee_audit_log_room_event
    ON e2ee_audit_log(room_id, event_id)
    WHERE room_id IS NOT NULL AND event_id IS NOT NULL;
```

> `device_id NOT NULL` 约束已于 2026-09-04 (commit 本次 sprint) 修复为 nullable，但索引仍建议保留 partial 以减少膨胀。

---

### 3.3 `e2ee_audit_log.device_id` — 刚修复为 nullable

**2026-09-04 发现并修复**：原为 `device_id TEXT NOT NULL`，`verify_all_devices` summary 日志传入 `None` 时触发 NOT NULL 约束错误，导致整个设备验证 500。

✅ **已修复**（commit `16a6db5f`）：迁移 `ALTER TABLE e2ee_audit_log ALTER COLUMN device_id DROP NOT NULL`。

---

### 3.4 `key_backups` — 双 ID 设计混乱

```
backup_id BIGSERIAL PRIMARY KEY,
backup_id_text TEXT UNIQUE  -- 冗余双主键
```

`backup_id_text` 为 Matrix spec field，是业务主键；`backup_id BIGSERIAL` 为内部 surrogate key。无到 `users.user_id` 的 FK。

**修复建议**：确认 `backup_id_text` 是业务主键，考虑删除 `BIGSERIAL` surrogate key，改用 UUID 或直接用 `backup_id_text` 作为 PK。

---

### 3.5 `backup_keys` — 缺 Unique + FK

```
无: (backup_id, room_id, session_id) UNIQUE
无: room_id FK → rooms(room_id)
```

**修复建议**：
```sql
ALTER TABLE backup_keys
    ADD CONSTRAINT uq_backup_keys_room_session
    UNIQUE (backup_id, room_id, session_id);

ALTER TABLE backup_keys
    ADD CONSTRAINT fk_backup_keys_room
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
```

---

### 3.6 E2EE 表 FK 缺失（Rust 层兜底，但无 DB 级保护）

以下关键业务表**没有 DB 级级联删除保护**，删除用户/设备时依赖 Rust 层手动清理：

| 表 | 关联实体 | 风险 |
|---|---|---|
| `device_keys` | users / devices | 删用户后孤儿 key |
| `olm_sessions` | users / devices | 删用户后孤儿 session |
| `olm_accounts` | users / devices | 删用户后孤儿 account |
| `one_time_keys` | users / devices | 删用户后孤儿 otk |
| `cross_signing_keys` | users | 删用户后孤儿 CSK |
| `cross_signing_trust` | users | 删用户后孤儿 trust |
| `megolm_sessions` | users / rooms | 删用户/room 后孤儿 |
| `key_signatures` | users | 删用户后孤儿签名 |
| `device_lists_outbound_pokes` | users | 删用户后孤儿 poke |

> 注：这是 Synapse 原生架构的通用模式，不算 bug，但需要注意 Rust 层实现完整性。

---

### 3.7 `cross_signing_keys` — FK 未 VALIDATE

迁移文件中有 `ADD CONSTRAINT ... NOT VALID`，生产需跑 `VALIDATE CONSTRAINT`。

**修复建议**：
```sql
ALTER TABLE cross_signing_keys VALIDATE CONSTRAINT fk_cross_signing_keys_user;
```

---

## 四、⚠️ 建议级问题

### 4.1 `rooms` 表 — CHECK 约束注释缺失

**现有约束**：4 个 CHECK (`join_rules`, `history_visibility`, `visibility`, `room_version`, `timestamps`) 设计良好。

**问题**：`room_version` CHECK 只允许 1-11，部分 Matrix Spec 新版 room_version（v12+）会拒绝写入。

**修复建议**：扩展 room_version CHECK 或将约束改为 `CHECK (room_version ~ '^\d+$')` 以支持未来版本。

---

### 4.2 重复索引定义

```
idx_rooms_name_trgm         -- 创建两次
idx_rooms_canonical_alias_trgm  -- 创建两次
```

同一索引在 unified schema 中定义了两次（第二次覆盖第一次，无害但冗余）。

**修复建议**：清理重复 `CREATE INDEX` 语句。

---

### 4.3 `reference_image TEXT` — 用途不明

`events` 表的 `reference_image TEXT` 字段无注释。需确认是否为：
- 未使用的废弃字段（应删除）
- Matrix MSC3489 `m.reference` 类型（应有注释）

---

### 4.4 `rooms.is_federated` — 缺索引

`is_federated BOOLEAN DEFAULT TRUE` 常用于联邦目录查询，无索引。

**修复建议**：
```sql
CREATE INDEX idx_rooms_federated ON rooms(is_federated) WHERE is_federated = TRUE;
```

---

### 4.5 `push_notification_queue` — 缺热点查询索引

```
无: (user_id, is_processed, priority, created_ts)  -- 推送 worker 取待发通知
无: (status, next_attempt_at)  -- 调度重试
```

**修复建议**：
```sql
CREATE INDEX idx_push_queue_user_pending
    ON push_notification_queue(user_id, priority DESC, created_ts)
    WHERE is_processed = FALSE AND status = 'pending';

CREATE INDEX idx_push_queue_retry
    ON push_notification_queue(next_attempt_at)
    WHERE is_processed = FALSE AND status = 'retry';
```

---

### 4.6 `federation_queue` — 无发送顺序索引

```
无: (destination, created_ts)  -- 按 destination 先进先出发送
无: (destination, status, retry_count)  -- 重试策略
```

**修复建议**：
```sql
CREATE INDEX idx_federation_queue_dest_created
    ON federation_queue(destination, created_ts)
    WHERE status = 'pending';

CREATE INDEX idx_federation_queue_retry
    ON federation_queue(destination, status, retry_count, created_ts)
    WHERE status = 'pending';
```

---

## 五、✅ 设计良好

以下表设计优秀，值得借鉴：

| 表 | 亮点 |
|---|---|
| `rooms` | 4 个 CHECK 约束 + room_version enum + timestamp 非负校验 |
| `events` | 17 个索引覆盖所有热点查询路径 + MSC4242 state DAG JSONB 列 + GIN |
| `room_memberships` | `(room_id, user_id)` UQ + 10 个索引覆盖所有 membership 查询 |
| `state_groups/state_group_state` | FK 链完整 (event→state_group→state)，DAG edges FK 完整 |
| `receipts_linearized` | stream_id BIGSERIAL 主键 + FK room/user |
| `user_locks` | UQ + partial index 保证每用户同时 1 个 active lock |
| `refresh_token_families` | UUID family_id + FK 链完整 |
| `destination_retry_timings` | `(destination)` PK，无冗余 surrogate key |

---

## 六、Schema 优化补丁（按优先级）

### 补丁 P1（阻塞级，Federation 性能）

```sql
-- P1-1: device_signatures 索引（最紧急，联邦热点）
CREATE INDEX IF NOT EXISTS idx_device_signatures_user_device
    ON device_signatures(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_device_signatures_target
    ON device_signatures(target_user_id, target_device_id);

-- P1-2: room_memberships CHECK
ALTER TABLE room_memberships
    ADD CONSTRAINT ck_room_memberships_valid
    CHECK (membership IN ('invite', 'join', 'knock', 'leave', 'ban'));

-- P1-3: event_edges prev_event_id FK
ALTER TABLE event_edges
    ADD CONSTRAINT fk_event_edges_prev
    FOREIGN KEY (prev_event_id) REFERENCES events(event_id)
    ON DELETE SET NULL;
```

### 补丁 P2（中优先级，数据完整性）

```sql
-- P2-1: device_keys UQ 补 algorithm
ALTER TABLE device_keys
    ADD CONSTRAINT uq_device_keys_user_device_algorithm_keyid
    UNIQUE (user_id, device_id, algorithm, key_id);

-- P2-2: e2ee_audit_log 索引
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_log_device
    ON e2ee_audit_log(device_id) WHERE device_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_log_room_event
    ON e2ee_audit_log(room_id, event_id)
    WHERE room_id IS NOT NULL AND event_id IS NOT NULL;

-- P2-3: events.redacted_by FK
ALTER TABLE events
    ADD CONSTRAINT fk_events_redacted_by
    FOREIGN KEY (redacted_by) REFERENCES events(event_id)
    ON DELETE SET NULL;

-- P2-4: cross_signing_keys VALIDATE
ALTER TABLE cross_signing_keys VALIDATE CONSTRAINT fk_cross_signing_keys_user;
```

### 补丁 P3（建议级，性能优化）

```sql
-- P3-1: push_notification_queue 索引
CREATE INDEX IF NOT EXISTS idx_push_queue_user_pending
    ON push_notification_queue(user_id, priority DESC, created_ts)
    WHERE is_processed = FALSE AND status = 'pending';

-- P3-2: federation_queue 索引
CREATE INDEX IF NOT EXISTS idx_federation_queue_dest_created
    ON federation_queue(destination, created_ts)
    WHERE status = 'pending';

-- P3-3: backup_keys 约束
ALTER TABLE backup_keys
    ADD CONSTRAINT uq_backup_keys_room_session
    UNIQUE (backup_id, room_id, session_id);
ALTER TABLE backup_keys
    ADD CONSTRAINT fk_backup_keys_room
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

-- P3-4: 清理重复索引
-- 需先查询确认无冲突后再 DROP DUPLICATE INDEX
```

---

## 七、后续迁移注意事项

1. **幂等原则**：所有 `ALTER TABLE` 前加 `IF NOT EXISTS` 或 `IF EXISTS`（PostgreSQL 14+ 支持）
2. **`NOT VALID` FK**：对于已存在数据的表，先 `ADD CONSTRAINT ... NOT VALID`，生产低峰期 `VALIDATE`
3. **索引顺序**：先加索引（只锁表，不锁写），再加 CHECK/FK（可能重写整表）
4. **`events` 表 DDL**：该表生产数据量大，所有 ALTER 建议安排在低峰窗口，用 `CREATE INDEX CONCURRENTLY`

---

## 八、审计清单

| # | 检查项 | 状态 | 备注 |
|---|---|---|---|
| 1 | `events` 表索引覆盖 | ✅ 17 indexes | 全部关键字段已索引 |
| 2 | `events.origin_server_ts` 类型 | ✅ BIGINT NOT NULL | 毫秒 Unix 时间戳合规 |
| 3 | `room_memberships.membership` CHECK | 🔴 缺失 | 需新增 |
| 4 | `event_edges.prev_event_id` FK | 🔴 缺失 | 需新增 |
| 5 | `device_signatures` 索引 | 🔴 0 indexes | 需新增 2 个索引 |
| 6 | `device_keys` UQ 含 algorithm | 🟠 不完整 | 需补 algorithm |
| 7 | `e2ee_audit_log` 索引 | 🟠 缺 device/room/event | 需补 2 个索引 |
| 8 | `events.redacted_by` FK | 🟠 缺失 | 需新增 |
| 9 | `cross_signing_keys` FK VALIDATE | 🟠 待生产验证 | 已 NOT VALID |
| 10 | `e2ee_audit_log.device_id` NOT NULL | ✅ 已修复 | commit 16a6db5f |
| 11 | `rooms` CHECK 约束 | ✅ 4 个 CHECK | join_rules/visibility/room_version |
| 12 | `state_groups` FK 链 | ✅ 完整 | event→sg→state 链 |
| 13 | 重复索引 | ⚠️ 2 对 | name_trgm / canonical_alias_trgm |
| 14 | `reference_image` 字段 | ⚠️ 用途不明 | 需确认是否为死代码 |
| 15 | E2EE 表无级联 FK | 🟠 设计已知 | Rust 层手动清理 |
| 16 | `events.room_id` DELETE 行为 | ✅ 已正确 | NO ACTION + Rust 批删 |
| 17 | 增量迁移可回滚 | ✅ 27 对 | 全有 undo.sql |
| 18 | Schema v11 是最新 | ✅ | unified baseline |
