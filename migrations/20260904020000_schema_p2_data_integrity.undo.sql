-- Migration undo: 20260904020000_schema_p2_data_integrity.sql

-- P2-1: device_keys UQ（保留旧 UQ）
ALTER TABLE device_keys DROP CONSTRAINT IF EXISTS uq_device_keys_user_device_algorithm_keyid;

-- P2-2: e2ee_audit_log 索引
DROP INDEX CONCURRENTLY IF EXISTS idx_e2ee_audit_log_device;
DROP INDEX CONCURRENTLY IF EXISTS idx_e2ee_audit_log_room_event;

-- P2-3: cross_signing_keys FK VALIDATE（无 undo，直接重新设为 NOT VALID）
-- ALTER TABLE cross_signing_keys ALTER CONSTRAINT fk_cross_signing_keys_user NOT VALID;
-- （不建议回滚 VALIDATE，这里仅作占位）

-- P2-4: events CHECK 约束
ALTER TABLE events DROP CONSTRAINT IF EXISTS ck_events_depth_nonneg;
ALTER TABLE events DROP CONSTRAINT IF EXISTS ck_events_not_before_nonneg;
