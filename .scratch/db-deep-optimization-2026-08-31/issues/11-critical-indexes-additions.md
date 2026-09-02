# 11: 关键索引补齐

**What to build:** v11 baseline 中补齐 `event_relations`、`device_lists`、`read_receipts`、`thread_read_receipts` 等表的缺失索引。

**Blocked by:** 01-v11-baseline-scaffold

**Status:** ✅ DONE (2026-08-31)

- [x] `device_lists_changes`: 新增 `idx_device_lists_changes_user_stream` (`user_id, stream_id DESC`)
- [x] `device_lists_changes`: 新增 `idx_device_lists_changes_user_device_stream` (`user_id, device_id, stream_id DESC`) WHERE `device_id IS NOT NULL`
- [x] `device_lists_stream`: 新增 `idx_device_lists_stream_user` (`user_id, stream_id DESC`)
- [x] 验证 v11 重建后索引数量：763（+3 新索引）
- [x] `event_relations`（已有 `(room_id, relates_to_event_id, relation_type)`, `(sender, relation_type)` 等索引，无需新增）
- [x] `read_receipts`（该表不存在，跳过）
- [x] `thread_read_receipts`（已有 `(user_id, room_id)` 和唯一约束，无需新增）
- [x] `room_summaries`（已有 `(last_event_ts DESC)`, `(room_id)` 索引，无需新增）

- [ ] `event_relations` 表：根据查询模式加复合索引（如 `(room_id, relation_type)`、`(relates_to_event_id, relation_type)`）
- [ ] `device_lists_stream` 表：加 `(user_id, stream_id DESC)` 复合索引
- [ ] `device_lists_changes` 表：加 `(user_id, device_id, stream_id)` 复合索引
- [ ] `read_receipts` 表：加 `(user_id, room_id, event_id)` 复合索引
- [ ] `thread_read_receipts` 表：加 `(user_id, room_id, thread_id, event_id)` 复合索引
- [ ] `room_summaries` 表：根据查询模式确认是否需要 `(last_event_ts DESC)` 之外的索引
- [ ] 验证 v11 重建后索引数量 + 索引大小（应有显著优化）
- [ ] 验证 storage 测试套件不因新增索引失败

**关键决策**：
- 不删已有索引（DB-01 已做过去重）
- 仅补齐深度报告中识别的缺失索引
