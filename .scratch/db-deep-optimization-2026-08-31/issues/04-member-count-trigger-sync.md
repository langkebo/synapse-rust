# 04: room_summaries.member_count 触发器同步

**What to build:** 为 `room_memberships` 表建触发器，自动化同步 `room_summaries` 的 `member_count/joined_member_count/invited_member_count` 字段。消除应用层自增逻辑的数据漂移和并发竞态风险。

**Blocked by:** 01-v11-baseline-scaffold

**Status:** ✅ DONE (2026-08-31)

- [x] 在 v11 baseline 中创建 `sync_room_member_count()` 函数（维护 `joined_member_count` / `invited_member_count` / `member_count`）
- [x] 在 `room_memberships` 表上创建 `trg_sync_member_count` 触发器（AFTER INSERT/UPDATE/DELETE）
- [x] 5/5 场景端到端测试全部通过（INSERT join, INSERT invite, UPDATE invite→join, UPDATE join→leave, DELETE）
- [x] `GREATEST(x-1, 0)` 防止负数
- [x] v11 baseline 重新应用 0 错误

- [ ] 创建 `sync_room_member_count()` 函数：
  - INSERT + NEW.membership='join' → `joined_member_count + 1`
  - INSERT + NEW.membership='invite' → `invited_member_count + 1`
  - DELETE + OLD.membership='join' → `GREATEST(joined_member_count-1, 0)`
  - DELETE + OLD.membership='invite' → `GREATEST(invited_member_count-1, 0)`
  - UPDATE：OLD≠'join'/NEW='join' 和 OLD='join'/NEW≠'join' 切换逻辑
- [ ] 创建 `trg_sync_member_count` 触发器：`AFTER INSERT OR UPDATE OR DELETE ON room_memberships FOR EACH ROW`
- [ ] 在 v11 baseline 中将函数和触发器定义内联到 CREATE TABLE 块后
- [ ] 验证：直接 INSERT/DELETE room_memberships，room_summaries 计数自动更新
- [ ] 验证：现有数据跑初始化脚本（触发器不影响已有数据，需单独 init）
- [ ] 与 ticket 05 配合：应用层移除自增逻辑后，触发器接管计数

**关键决策**：
- 触发器是 AFTER 而非 BEFORE（避免与 Rust 应用层更新冲突）
- 应用层迁移到 INSERT/DELETE 后，触发器接管计数逻辑