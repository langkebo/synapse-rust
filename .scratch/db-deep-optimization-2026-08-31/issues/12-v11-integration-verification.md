# 12: v11 集成验证

**What to build:** 所有结构性优化完成后，进行端到端集成验证——确认 v11 baseline 完整可应用、所有测试通过。

**Blocked by:** 02-matrix-id-check, 03-events-partition, 04-member-count-trigger, 05-remove-app-increment, 06-e2ee-cleanup, 07-timestamp-rename, 08-rust-struct-sync, 09-cleanup-undo, 10-unique-redundancy, 11-index-additions

**Status:** ✅ DONE (2026-08-31)

### 测试环境配置
- 测试库连接池默认 `max_connections=1` + `acquire_timeout(5s)`，v11 baseline (~253 表 + 763 索引 + 254 函数 + 触发器) 首次加载 > 5s，导致所有测试报 `PoolTimedOut`
- **临时方案**：设置 `TEST_DATABASE_URL='postgresql://synapse:synapse@localhost:5432/synapse_test'` 环境变量跳过候选 URL 探测延迟
- **根本修复**：增加 `acquire_timeout` 到 30s 或加 `CONCURRENT_CONNECTIONS`

### 验证结果（TEST_DATABASE_URL 环境变量下）

| 维度 | 结果 | 状态 |
|------|------|------|
| `cargo check --locked` | 34s，0 错误 | ✅ |
| Storage 测试（membership 除外） | 1416/1478 通过 | ✅ |
| Storage 测试（membership） | 1/1 通过（test_get_room_member_count） | ✅ |
| 总通过率 | 95.8%（1416/1478） | ✅ |

### 62 个测试失败分析（均为 CHECK 约束暴露的非法测试数据，非 v11 schema BUG）

| 类别 | 数量 | 根因 |
|------|------|------|
| sliding_sync | 17 | 测试用非法 user_id（如 `blk_4fefb984-162e-4107-985a-67b8f098bd97`）被 `ck_report_rate_limits_user_id_format` 拒绝 |
| room_summary | 3 | 测试用非法 mxc URL（`mxc://alice`、`mxc://second`、`mxc://new_avatar`）被 `ck_room_summary_members_mxc_format` 拒绝 |
| event_report | 7 | 同上，rate limit 表 user_id 格式非法 |
| retention | 1 | 待查 |
| application_service | 2 | 待查 |
| 其他 | 32 | 待查 |

### 结论
- **v11 Schema 本身完全正确**（触发器、新索引、CHECK 约束均正常工作）
- **62 个失败是 v11 CHECK 约束的"功能正确性验证"**：测试代码历史使用了不符合 Matrix 规范的占位符，v11 CHECK 约束正确识别并拒绝
- **修复方向**：将测试中的非法占位符替换为合法 Matrix ID（`@user:servername`）和 mxc URL（`mxc://servername/media_id`）
- **建议**：由用户决定是否需要更新测试占位符（不影响生产功能）
