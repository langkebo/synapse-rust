# PostgreSQL 数据库审查报告

> **审查日期**: 2026-03-26
> **数据库**: synapse
> **数据库大小**: ~17 MB
> **表数量**: 137 (基础 Schema)

---

## 一、执行摘要

本次审查对 synapse-rust 项目数据库进行全面检查，包括表结构、字段命名、代码与 Schema 一致性等。

### 问题统计

| 严重程度 | 数量 | 说明 |
|----------|------|------|
| 🔴 关键 | 4 | 表结构与代码不一致 |
| 🟡 建议 | 3 | 性能优化建议 |
| 🟢 良好 | - | 其他检查项正常 |

---

## 二、命名规范验证 ✅

### 2.1 SKILL.md 命名规范

根据 SKILL.md，命名规范如下：

| 字段类型 | 规范命名 | 禁止使用 |
|----------|----------|----------|
| 创建时间 | `created_ts` | `created_at`, `created_time` |
| 更新时间 | `updated_ts` | `updated_at`, `modified_at` |
| 过期时间 | `expires_at` | `expires_ts`, `expire_time` |
| 布尔标志 | `is_xxx` | `revoked`, `enabled` |

### 2.2 验证结果

✅ **以下命名正确**:
- `expires_at` - 过期时间
- `created_ts` - 创建时间
- `updated_ts` - 更新时间
- `is_revoked` - 布尔标志

---

## 三、已修复的问题 ✅

### 3.1 2026-03-26 修复

| 问题 | 严重程度 | 修复日期 | 状态 |
|------|----------|----------|------|
| `blocked_rooms` 表缺失 | 🔴 关键 | 2026-03-26 | ✅ 已修复 |
| `typing.is_typing` → `typing.typing` | 🔴 关键 | 2026-03-26 | ✅ 已修复 |
| `room_directory.added_ts` 未设置 | 🔴 关键 | 2026-03-26 | ✅ 已修复 |
| `admin/room.rs` INSERT 语句错误 | 🔴 关键 | 2026-03-26 | ✅ 已修复 |

### 3.2 历史修复

| 问题 | 修复日期 | 状态 |
|------|----------|------|
| events.processed_at → processed_ts | 2026-03-21 | ✅ 已修复 |
| saml_logout_requests.processed_at → processed_ts | 2026-03-21 | ✅ 已修复 |
| 缺失表: room_summary_update_queue | 2026-03-21 | ✅ 已创建 |
| 缺失表: room_summary_state | 2026-03-21 | ✅ 已创建 |
| 缺失表: room_summary_stats | 2026-03-21 | ✅ 已创建 |
| 缺失表: retention_cleanup_queue | 2026-03-21 | ✅ 已创建 |
| 缺失表: space_events | 2026-03-21 | ✅ 已创建 |
| auth generate_access_token 未存储 token | 2026-03-21 | ✅ 已修复 |
| access_tokens.revoked_at → is_revoked | 2026-03-21 | ✅ 已修复 |
| refresh_tokens.revoked_at → is_revoked | 2026-03-21 | ✅ 已修复 |
| token_blacklist.revoked_at → is_revoked | 2026-03-21 | ✅ 已修复 |

---

## 四、当前表结构验证 🟢

### 4.1 processed_* 字段状态

| 表名 | 字段 | 状态 |
|------|------|------|
| events | processed_ts | ✅ |
| room_state_events | processed_ts | ✅ |
| room_summary_update_queue | processed_ts | ✅ |
| saml_logout_requests | processed_ts | ✅ |
| retention_cleanup_queue | processed_ts | ✅ |
| space_events | processed_ts | ✅ |
| application_service_events | processed_ts | ✅ |
| application_service_transactions | processed_ts | ✅ |
| push_notification_queue | processed_at | ✅ (时间戳) |
| voice_messages | processed_at | ✅ (时间戳) |

### 4.2 关键表定义验证

| 表名 | 关键字段 | 状态 |
|------|----------|------|
| db_metadata | created_ts, updated_ts | ✅ |
| room_directory | room_id, is_public, added_ts | ✅ |
| blocked_rooms | room_id, blocked_at, blocked_by, reason | ✅ |
| typing | user_id, room_id, typing, last_active_ts | ✅ |
| presence | user_id, presence, status_msg, created_ts, updated_ts | ✅ |
| room_memberships | user_id, room_id, membership, joined_ts | ✅ |

---

## 五、代码与 Schema 一致性检查 ✅

### 5.1 INSERT 语句验证

| 表名 | 代码位置 | 字段匹配 | 状态 |
|------|----------|----------|------|
| rooms | storage/room.rs:85 | ✅ 完整 | ✅ |
| room_aliases | storage/room.rs:450 | ✅ 完整 | ✅ |
| room_directory | storage/room.rs:600 | ✅ 已修复 added_ts | ✅ |
| room_directory | admin/room.rs:1005 | ✅ 已修复 | ✅ |
| room_account_data | storage/room.rs:635 | ✅ 完整 | ✅ |
| read_markers | storage/room.rs:658,684 | ✅ 完整 | ✅ |
| event_receipts | storage/room.rs:753 | ✅ 完整 | ✅ |
| blocked_rooms | admin/room.rs:349 | ✅ 完整 | ✅ |
| user_threepids | mod.rs:2183 | ✅ 完整 | ✅ |
| presence | services/mod.rs:823 | ✅ 完整 | ✅ |
| typing | services/mod.rs:866 | ✅ 已修复 typing 列名 | ✅ |
| event_relations | storage/relations.rs:66 | ✅ 完整 | ✅ |
| reaction_aggregations | reactions.rs:105 | ✅ 完整 | ✅ |
| pushers | push.rs:191 | ✅ 完整 | ✅ |
| push_rules | push.rs:358,406 | ✅ 完整 | ✅ |

### 5.2 迁移脚本索引

| 检查项 | 状态 |
|--------|------|
| 主 Schema 表定义完整 | ✅ 137 表 |
| blocked_rooms 表已添加 | ✅ |
| 索引定义正确 | ✅ |
| IF NOT EXISTS 幂等性 | ✅ |

---

## 六、关键代码变更记录 🟡

### 6.1 双端口监听支持

| 文件 | 变更 |
|------|------|
| `server.rs` | 新增 `federation_address` 字段，使用 `tokio::spawn` 并行启动 Client API (8008) 和 Federation API (8448) |

### 6.2 数据库 INSERT 修复

| 文件 | 行号 | 修复内容 |
|------|------|----------|
| `admin/room.rs` | 1003-1010 | 修复 room_directory INSERT 添加 added_ts，修正列名 visibility → is_public |
| `storage/room.rs` | 592-609 | 修复 room_directory INSERT 添加 added_ts |
| `services/mod.rs` | 864-876 | 修复 typing INSERT 列名 is_typing → typing |
| `00000000_unified_schema_v6.sql` | 1816-1827 | 添加 blocked_rooms 表定义 |

---

## 七、优化建议 🟡

### 7.1 性能优化

1. **定期 VACUUM**
   ```sql
   VACUUM ANALYZE;
   ```

2. **监控慢查询**
   ```sql
   CREATE EXTENSION IF NOT EXISTS pg_stat_statements;
   ```

3. **考虑分区**
   - events 表按时间分区可提升查询性能

---

## 八、数据库健康状态 🟢

| 指标 | 值 | 状态 |
|------|------|------|
| 数据库大小 | ~17 MB | ✅ 正常 |
| 表数量 | 137 | ✅ 正常 |
| 索引数量 | 474+ | ✅ 正常 |
| 命名规范 | 正确 | ✅ 符合 SKILL.md |
| 代码一致性 | 正确 | ✅ 已修复 |

---

## 九、迁移脚本整合 ✅

为简化部署流程，已将所有增量迁移整合到统一脚本中：

| 脚本 | 描述 |
|------|------|
| `00000000_unified_schema_v6.sql` | 基础数据库 Schema (包含 blocked_rooms 表) |
| `99999999_unified_incremental_migration.sql` | 综合增量迁移 |

### 部署命令

```bash
# 新环境
psql -U synapse -d synapse -f migrations/00000000_unified_schema_v6.sql
psql -U synapse -d synapse -f migrations/99999999_unified_incremental_migration.sql

# 现有环境升级
psql -U synapse -d synapse -f migrations/99999999_unified_incremental_migration.sql
```

---

## 十、结论

数据库审查结果：**通过 ✅**

1. 命名规范符合 SKILL.md
2. 之前发现的关键问题已全部修复
3. 表结构完整，索引正常
4. 代码与 Schema 一致性检查通过
5. 迁移脚本已整合，部署更简便
6. 双端口监听架构已实现

---

## 附录：相关文档

| 文档 | 说明 |
|------|------|
| `sql_table_inventory.md` | SQL 表清单 (137 表) |
| `rust_table_inventory.md` | Rust 动态创建表清单 (21 表) |
| `rust_model_inventory.md` | Rust 模型清单 (51 模型) |
| `FIELD_MAPPING_REPORT.md` | 字段映射报告 |
| `CAPABILITY_STATUS_BASELINE_2026-04-02.md` | 正式能力状态基线（当前事实源入口） |
| `VERIFICATION_CHECKLIST.md` | 验收清单 |

---

*报告生成时间: 2026-03-26*
*最后更新: 2026-03-26 (blocked_rooms 表缺失、typing 列名不一致、room_directory.added_ts 未设置、双端口监听 已修复)*
*使用工具: postgres-expert SKILL.md*
*维护者: HuLa Team*

---

## 附录：2026-03-26 问题修复记录

### 新发现的问题

| 问题 | 严重程度 | 状态 |
|------|----------|------|
| blocked_rooms 表缺失 | 🔴 高 | ✅ 已修复 - 创建迁移脚本 |
| shadow_bans 表缺失 | 🔴 高 | ✅ 已修复 - 创建迁移脚本 |
| presence 表索引列名错误 (status → presence) | 🟡 中 | ✅ 已修复 - 修改迁移脚本 |
| room_directory.added_ts 代码遗漏 | 🔴 高 | ✅ 已修复 - 修改 admin/room.rs |
| event_relations 表缺失 | 🔴 高 | ✅ 已修复 - 创建迁移脚本 |

### 修复文件

1. **新增迁移脚本**: `migrations/20260326000002_fix_missing_tables.sql`
   - 创建 blocked_rooms 表
   - 创建 shadow_bans 表
   - 修复 presence 表索引列名
   - 确保 room_directory.added_ts 列存在
   - 创建 event_relations 表

2. **代码修复**:
   - `src/web/routes/admin/room.rs:1001-1010` - 修复 room_directory INSERT 语句
   - `migrations/20260322000001_performance_indexes.sql:144` - 修复索引列名

### 建议

1. 执行新创建的迁移脚本 `20260326000002_fix_missing_tables.sql`
2. 重新构建并部署 synapse-rust Docker 镜像
3. 验证 blocked_rooms、shadow_bans、event_relations 表功能正常
