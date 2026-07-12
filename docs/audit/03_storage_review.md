# synapse-rust 存储层结构性代码审查

- 日期：2026-07-10
- 范围：`synapse-storage/src/`（真实实现，196 个文件，sqlx 直连 PostgreSQL）。`src/storage/` 仅 re-export facade，不含逻辑。
- 依据规则：`.trae/rules/project_rules.md`（已核对原文，非凭关键词猜测）
- 方法：机械扫描（format! / 禁用字段 / 索引 / 类型）+ 2 个 Explore agent 深挖 N+1 与事务边界，均带 file:line 证据
- 一句话结论：**没有 SQL 注入、没有连接泄漏、必需索引基本齐全——真正的债是两类:①约 20 个 struct 用裸 `i64` 映射 NULLABLE 列(NULL 时 sqlx 运行时报错，P1)；②多处多步写操作(建设备/删房级联/房间目录)未包事务，中途失败留孤儿数据(P1)。**

## 总览(按严重度)

| 级别 | 数量 | 类别 |
|------|------|------|
| P0 | 0 | 无 |
| P1 | 3 类 | 类型映射 NULL 崩溃、多步写缺事务、device 批量 N+1 |
| P2 | 4 类 | 其他 N+1、字段命名、索引列名漂移、maintenance 标识符插值 |
| ✅ 合规 | 3 项 | SQL 注入防护、连接池、必需索引(3/4) |

---

## 1. N+1 查询(关注点 1)

| 文件:行号 | 级别 | 问题 | 修复建议 |
|-----------|------|------|---------|
| `device/mod.rs:643-646` `delete_devices_batch` | **P1** | 循环内每台设备 3 次 DB 往返(DELETE + INSERT stream + INSERT changes)。删 100 台 = 300 次往返。**批量版本已存在但没用**:`delete_lazy_loaded_members_for_devices_batch`(:339)、`record_device_list_changes_batch`(:192) | 改调已有的 batch 方法,循环只收集 id,循环外一次性 `= ANY($1)` |
| `user.rs:944-951` `update_displayname_batch` | P2 | 循环内逐个 `UPDATE users`,名为 batch 实为 N 次 | `UPDATE users SET displayname=v.name FROM (VALUES ...) v WHERE ...` 单条 |
| `space/repository.rs:731-735` `get_space_hierarchy_paginated` | P2 | 每个 child 调 `build_hierarchy_room` = 3 查询(2 state + 1 member count) | 按 child_id 批量取 state / member count |
| `space/repository.rs:835-839` `get_parent_spaces` | P2 | 每个 child 一次 `get_space` | `WHERE space_id = ANY($1)` 批量 |
| `media/chunked_upload.rs:283-290` `cleanup_expired` | P2 | 循环内每个过期上传各开一个事务(后台任务) | 单事务批量删,或 `DELETE ... WHERE upload_id = ANY($1)` |
| `state_groups.rs:398-432` `resolve_state_for_group` | P2(信息) | DAG 遍历每节点 2 查询(深度上限 100,树形固有) | 固有结构,可留;若热点可加 state group 缓存 |

## 2. SQL 注入(关注点 2 / 规则 11.3)—— ✅ 合规

结论:**未发现真注入**。所有 `sqlx::query(&format!(...))` 分两类,均安全:
- **常量列名插值**(`state_groups.rs`、`event/*.rs`、`event_report/repository.rs`):插的是 `STATE_GROUP_COLS`、`REPORT_RATE_LIMIT_SELECT` 等编译期常量列清单,真实参数仍走 `$1` 绑定。安全。
- **硬编码标识符白名单**(`maintenance.rs`):`REINDEX INDEX {index}` 的 `indexes` 是硬编码 `vec![...]`(:118-127),安全。
- ⚠ **唯一需确认项** `maintenance.rs:86` `VACUUM ANALYZE {table}`(P2 信息):`tables` 是函数入参。DDL 无法用 `$1` 绑定标识符,当前先查 `pg_stat_user_tables` 存在性(非注入防护)。**修复建议**:确认 `tables` 调用方只传内部常量;稳妥起见加一个 `[a-z_]+` 正则/allowlist 校验后再插值。

## 3. 必需索引(关注点 3 / 规则 4.3)—— 3/4 合规

| 规则要求索引 | 实际 | 状态 |
|--------------|------|------|
| events (room_id, origin_server_ts DESC) | `idx_events_room_time`(v10.sql:3525) | ✅ |
| room_memberships (user_id, membership) | `idx_room_memberships_user_membership`(:3511) | ✅ |
| access_tokens (user_id, is_revoked) | `idx_access_tokens_user_revoked`(:3487,`WHERE is_revoked=FALSE` 部分索引) | ✅ |
| presence_subscriptions (user_id, observed_user_id) | 表实际列是 `subscriber_id, target_id`,PK=(subscriber_id,target_id)(v10.sql:2864 / INDEXES.md:235) | ⚠ **P2 列名漂移** |

**presence_subscriptions**:语义等价索引存在(作为主键),但**列名与规则不符**(规则写 `user_id/observed_user_id`,实际 `subscriber_id/target_id`)。要么规则文档过时、要么表定义偏离。修复建议:统一二者——若表是对的,更新规则 4.3;若规则是对的,建迁移改列名(代价大,建议前者)。

## 4. 事务边界(关注点 4)—— 多处 P1

现状:196 文件仅 15 处 `.begin()`/15 `.commit()`。已用事务的 13 处生产代码 begin/commit 配平,提前 return 分支有显式 rollback(`feature_flags.rs:197`、`dehydrated_device.rs:285`),**无悬挂连接**。问题在**该用没用**:

| 文件:行号 | 级别 | 多步写 | 中途失败后果 |
|-----------|------|--------|-------------|
| `device/mod.rs:378-404` `create_device` | **P1** | INSERT device + DELETE lazy_loaded + record change(2 写) | 设备已建但 device_list_change 丢失 → 对端同步不到新设备 |
| `device/mod.rs:570-588` `delete_user_device` | **P1** | DELETE device + DELETE lazy + record change(2 写) | 设备删了但变更流没记 → E2EE 对端仍认为设备在 |
| `server_notification/repository.rs:744-756` `delete_room_cascade` | **P1** | DELETE memberships+summaries+summary_members+events+rooms(5 写) | 级联删一半 → 孤儿 events/memberships |
| `room/admin.rs:408-428` `set_room_public_with_directory` | P1 | UPDATE rooms + INSERT room_directory | is_public=true 但目录无条目 → 房间搜不到 |
| `room/admin.rs:430-443` `set_room_private_with_directory` | P1 | UPDATE rooms + DELETE room_directory | 私有了但目录残留 → 泄漏可见性 |
| `room/admin.rs:33-96` `cleanup_abnormal_data` | P1 | DELETE orphan rooms+events+memberships+state(4 写) | 清理一半 → 新孤儿 |

修复建议:统一包 `let mut tx = pool.begin().await?; ... tx.commit().await?;`,所有写走 `&mut *tx`。`delete_devices_batch`(见 1.)整个循环也应在单事务内。

## 5. 连接池泄漏 / 未消费 Stream(关注点 5)—— ✅ 合规

未发现 `.fetch()`(返回 Stream 需手动消费)的用法;全部用 `fetch_one`/`fetch_optional`/`fetch_all`(即时消费)。begin/commit 配平,提前 return 有 rollback。无泄漏。

## 6. 禁用字段名(关注点 6 / 规则 2.3)

规则禁用清单:`invalidated`/`invalidated_ts`/`created_at`/`updated_at`/`expires_ts`/`revoked_ts`/`enabled`。

| 文件:行号 | 级别 | 问题 | 修复建议 |
|-----------|------|------|---------|
| `url_preview_storage.rs:17,43,45,60,73,96` + 迁移 `v10.sql:4608` `url_preview_cache.expires_ts BIGINT NOT NULL` | **P2** | 真 DB 列名 `expires_ts`,规则要求 `expires_at` | 建迁移 `ALTER TABLE url_preview_cache RENAME COLUMN expires_ts TO expires_at`,同步 struct/SQL/索引 `idx_url_preview_cache_expires` |
| `media/models.rs:19,36` `created_at: DateTime<Utc>` | P2(信息) | **非 DB 列**——DB 实际用 `created_ts`(`chunked_upload.rs`/`quarantine_stream.rs` 均 `created_ts`)。这是 API JSON 序列化字段 | 属 API 契约字段(JSON 惯用 created_at)。规则 2.3 针对 DB 列,严格说不违规;若要统一,改 serde rename 但会破坏客户端契约,**建议保留 + 文档注明** |
| `saml/repository.rs:93,105` `status='invalidated'` / `captcha.rs:277` 日志 | ✅ 非违规 | `invalidated` 是 status **枚举值**、日志字符串,不是字段名 | 无需改 |

## 7. 类型映射 NULLABLE(关注点 7 / 规则 3.1)—— P1

规则 3.1:`BIGINT (NULLABLE)` → `Option<i64>`。已核实 schema 中 `updated_ts BIGINT`、`expires_at BIGINT` 均**无 NOT NULL(即 NULLABLE)**。以下 struct 用裸 `i64`,**该列取到 NULL 时 sqlx `query_as` 运行时报 `UnexpectedNullError`**——不是风格问题,是潜在崩溃:

| 文件:行号 | 字段 | 级别 |
|-----------|------|------|
| `oidc_session_storage.rs:32,61` | `expires_at: i64` | P1 |
| `matrixrtc.rs:15,31` | `updated_ts: i64` | P1 |
| `feature_flags.rs:22,44` | `updated_ts: i64` | P1 |
| `federation_blacklist.rs:37,71,85` | `updated_ts: i64` | P1 |
| `threepid.rs:43` | `expires_at: i64` | P1 |
| `qr_login.rs:146` / `rendezvous.rs:18` | `expires_at: i64` | P1 |
| `moderation.rs:16`、`dehydrated_device.rs:15`、`thread.rs:49,61,93,109`、`sticky_event.rs:189`、`privacy.rs:16` | `updated_ts: i64` | P1 |

修复建议:逐个 struct 对照其**确切表**的列可空性(上面是模式匹配,需 per-struct 确认表名),NULLABLE 的改 `Option<i64>`。凡是 `INSERT` 时该列可能不填的,一定是 NULLABLE。这是一次值得做的 schema-vs-struct 可空性全量对账。**置信度 7/10**:已证实 schema 有 NULLABLE 的 `updated_ts`/`expires_at` 列且 struct 用裸 i64,但每个 struct 到具体表的映射需落实确认。

---

## 修复优先级建议

1. **P1 事务边界**(§4)——`create_device`/`delete_user_device`/`delete_room_cascade` 先做,这几个失败会造成 E2EE 设备不同步和孤儿数据,用户可感知。
2. **P1 类型映射**(§7)——做一次 struct↔表 可空性对账,把 NULLABLE 列的裸 i64 全改 `Option<i64>`,消除运行时 NULL 崩溃。
3. **P1 device N+1**(§1 第 1 行)——batch 方法现成,改动小收益大。
4. P2 其余(字段改名迁移、索引列名对账、其他 N+1)可排后。

## 合规亮点(避免过度否定)

- ✅ 无 SQL 注入:参数化到位,format! 只插常量/白名单。
- ✅ 无连接泄漏:无裸 Stream,事务 begin/commit 配平,提前 return 有 rollback。
- ✅ 必需索引 3/4 齐全,room_memberships/events 甚至有多个覆盖索引。

产出:docs/audit/03_storage_review.md
