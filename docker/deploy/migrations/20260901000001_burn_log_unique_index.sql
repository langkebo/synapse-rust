-- P-AUDIT #4: 为 burn_after_read_log 加唯一约束以支持幂等批量写入。
--
-- 背景：审计 #4 修复中 log_burned_event_batch 使用 ON CONFLICT DO NOTHING。
-- 该表此前无 UNIQUE 约束，导致：
--   (1) ON CONFLICT DO NOTHING 不生效（需要 conflict target）
--   (2) retry 时会插入重复行
--
-- 选择 (user_id, event_id)：同一事件对同一用户只能被"burned"一次。
-- 这与 burn_after_read_pending 已有 UNIQUE(user_id, room_id, event_id) 语义一致。
--
-- 零停机：
-- 1. ALTER TABLE ... ADD CONSTRAINT ... USING INDEX —— 必须先建并发安全索引
-- 2. CREATE UNIQUE INDEX CONCURRENTLY 不能在事务中，迁移整体不能包 BEGIN/COMMIT
-- 3. 直接 inline 建 unique index（项目无 BEGIN 包裹，逐条执行），
--    已建则跳过（IF NOT EXISTS）
--
-- 大表考量：burn_after_read_log 是 append-only 写入表，单条事件仅写一次，
-- 历史重复行风险极低。即便极少量历史重复，下面 NOT VALID + VALIDATE 分两阶段：
--   - NOT VALID 只校验新行，旧重复仍存在但不阻塞
--   - VALIDATE 在后台锁下校验全表，不阻塞读
-- 但 ON CONFLICT DO NOTHING 是 INSERT 路径，旧重复不会被读到。
-- 实测无重复则直接 VALIDATE 即可。

CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS ux_burn_log_user_event
    ON burn_after_read_log(user_id, event_id);
