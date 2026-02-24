-- =============================================================================
-- Synapse-Rust 外键约束验证脚本
-- 版本: 20260228000001
-- 描述: 验证外键约束是否正常工作
-- =============================================================================

SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

-- =============================================================================
-- 第一部分: 验证外键约束存在
-- =============================================================================

DO $$
DECLARE
    expected_fk_count INTEGER := 40;
    actual_fk_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO actual_fk_count
    FROM information_schema.table_constraints 
    WHERE constraint_type = 'FOREIGN KEY' 
    AND table_schema = 'public'
    AND constraint_name LIKE 'fk_%';
    
    IF actual_fk_count < expected_fk_count THEN
        RAISE WARNING 'Expected at least % foreign key constraints, found %', expected_fk_count, actual_fk_count;
    ELSE
        RAISE NOTICE 'Foreign key constraints count: % (expected >= %)', actual_fk_count, expected_fk_count;
    END IF;
END $$;

-- =============================================================================
-- 第二部分: 验证外键约束功能
-- =============================================================================

-- 创建临时测试用户
INSERT INTO users (user_id, username, creation_ts)
VALUES ('__test_user_fk__', 'test_user_fk', EXTRACT(EPOCH FROM NOW()) * 1000)
ON CONFLICT (user_id) DO NOTHING;

-- 创建临时测试房间
INSERT INTO rooms (room_id, creator, created_ts)
VALUES ('__test_room_fk__', '__test_user_fk__', EXTRACT(EPOCH FROM NOW()) * 1000)
ON CONFLICT (room_id) DO NOTHING;

-- 验证: 测试插入有效数据应该成功
DO $$
BEGIN
    -- 测试 room_members 外键
    INSERT INTO room_members (room_id, user_id, membership, created_ts)
    VALUES ('__test_room_fk__', '__test_user_fk__', 'join', EXTRACT(EPOCH FROM NOW()) * 1000)
    ON CONFLICT (room_id, user_id) DO NOTHING;
    
    RAISE NOTICE 'Test 1 PASSED: Valid room_members insert succeeded';
EXCEPTION
    WHEN foreign_key_violation THEN
        RAISE WARNING 'Test 1 FAILED: Valid room_members insert failed with FK violation';
END $$;

-- 验证: 测试插入无效数据应该失败
DO $$
BEGIN
    -- 测试 room_members 外键 - 无效的 room_id
    INSERT INTO room_members (room_id, user_id, membership, created_ts)
    VALUES ('__invalid_room__', '__test_user_fk__', 'join', EXTRACT(EPOCH FROM NOW()) * 1000);
    
    RAISE WARNING 'Test 2 FAILED: Invalid room_members insert should have failed';
EXCEPTION
    WHEN foreign_key_violation THEN
        RAISE NOTICE 'Test 2 PASSED: Invalid room_members insert correctly rejected';
END $$;

-- 验证: 测试插入无效 user_id 应该失败
DO $$
BEGIN
    INSERT INTO room_members (room_id, user_id, membership, created_ts)
    VALUES ('__test_room_fk__', '__invalid_user__', 'join', EXTRACT(EPOCH FROM NOW()) * 1000);
    
    RAISE WARNING 'Test 3 FAILED: Invalid room_members insert should have failed';
EXCEPTION
    WHEN foreign_key_violation THEN
        RAISE NOTICE 'Test 3 PASSED: Invalid user_id correctly rejected';
END $$;

-- 验证: 测试 events 外键
DO $$
BEGIN
    INSERT INTO events (event_id, room_id, sender, type, content, origin_server_ts, received_ts)
    VALUES ('__test_event_fk__', '__test_room_fk__', '__test_user_fk__', 'm.room.message', '{}', EXTRACT(EPOCH FROM NOW()) * 1000, EXTRACT(EPOCH FROM NOW()) * 1000)
    ON CONFLICT (event_id) DO NOTHING;
    
    RAISE NOTICE 'Test 4 PASSED: Valid events insert succeeded';
EXCEPTION
    WHEN foreign_key_violation THEN
        RAISE WARNING 'Test 4 FAILED: Valid events insert failed with FK violation';
END $$;

-- 验证: 测试 device_keys 外键
DO $$
BEGIN
    INSERT INTO device_keys (user_id, device_id, algorithm, key_data, added_ts)
    VALUES ('__test_user_fk__', '__test_device__', 'ed25519', 'test_key_data', EXTRACT(EPOCH FROM NOW()) * 1000)
    ON CONFLICT (user_id, device_id, algorithm) DO NOTHING;
    
    RAISE NOTICE 'Test 5 PASSED: Valid device_keys insert succeeded';
EXCEPTION
    WHEN foreign_key_violation THEN
        RAISE WARNING 'Test 5 FAILED: Valid device_keys insert failed with FK violation';
END $$;

-- 验证: 测试 push_device 外键
DO $$
BEGIN
    INSERT INTO push_device (user_id, device_id, push_token, push_type)
    VALUES ('__test_user_fk__', '__test_push_device__', 'test_token', 'fcm')
    ON CONFLICT (user_id, device_id) DO NOTHING;
    
    RAISE NOTICE 'Test 6 PASSED: Valid push_device insert succeeded';
EXCEPTION
    WHEN foreign_key_violation THEN
        RAISE WARNING 'Test 6 FAILED: Valid push_device insert failed with FK violation';
END $$;

-- =============================================================================
-- 第三部分: 验证级联删除
-- =============================================================================

-- 先清理测试数据
DELETE FROM room_members WHERE room_id = '__test_room_fk__';
DELETE FROM events WHERE room_id = '__test_room_fk__';
DELETE FROM device_keys WHERE user_id = '__test_user_fk__';
DELETE FROM push_device WHERE user_id = '__test_user_fk__';

-- 测试删除用户时相关数据应该被级联删除
DO $$
DECLARE
    member_count INTEGER;
    event_count INTEGER;
BEGIN
    -- 重新插入测试数据
    INSERT INTO room_members (room_id, user_id, membership, created_ts)
    VALUES ('__test_room_fk__', '__test_user_fk__', 'join', EXTRACT(EPOCH FROM NOW()) * 1000)
    ON CONFLICT (room_id, user_id) DO NOTHING;
    
    INSERT INTO events (event_id, room_id, sender, type, content, origin_server_ts, received_ts)
    VALUES ('__test_event_fk_2__', '__test_room_fk__', '__test_user_fk__', 'm.room.message', '{}', EXTRACT(EPOCH FROM NOW()) * 1000, EXTRACT(EPOCH FROM NOW()) * 1000)
    ON CONFLICT (event_id) DO NOTHING;
    
    -- 删除房间 (应该级联删除 room_members 和 events)
    DELETE FROM rooms WHERE room_id = '__test_room_fk__';
    
    -- 验证级联删除
    SELECT COUNT(*) INTO member_count FROM room_members WHERE room_id = '__test_room_fk__';
    SELECT COUNT(*) INTO event_count FROM events WHERE room_id = '__test_room_fk__';
    
    IF member_count = 0 AND event_count = 0 THEN
        RAISE NOTICE 'Test 7 PASSED: Cascade delete worked correctly';
    ELSE
        RAISE WARNING 'Test 7 FAILED: Cascade delete did not work (members: %, events: %)', member_count, event_count;
    END IF;
END $$;

-- =============================================================================
-- 第四部分: 清理测试数据
-- =============================================================================

DELETE FROM device_keys WHERE user_id = '__test_user_fk__';
DELETE FROM push_device WHERE user_id = '__test_user_fk__';
DELETE FROM room_members WHERE user_id = '__test_user_fk__';
DELETE FROM events WHERE sender = '__test_user_fk__';
DELETE FROM rooms WHERE creator = '__test_user_fk__';
DELETE FROM users WHERE user_id = '__test_user_fk__';

-- =============================================================================
-- 第五部分: 最终验证报告
-- =============================================================================

DO $$
DECLARE
    fk_count INTEGER;
    tables_with_fk INTEGER;
BEGIN
    SELECT COUNT(*) INTO fk_count
    FROM information_schema.table_constraints 
    WHERE constraint_type = 'FOREIGN KEY' 
    AND table_schema = 'public'
    AND constraint_name LIKE 'fk_%';
    
    SELECT COUNT(DISTINCT table_name) INTO tables_with_fk
    FROM information_schema.table_constraints 
    WHERE constraint_type = 'FOREIGN KEY' 
    AND table_schema = 'public'
    AND constraint_name LIKE 'fk_%';
    
    RAISE NOTICE '';
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'Foreign Key Constraint Verification Report';
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'Total FK constraints: %', fk_count;
    RAISE NOTICE 'Tables with FK: %', tables_with_fk;
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'All tests completed!';
    RAISE NOTICE '==========================================';
END $$;

-- 记录验证版本
INSERT INTO schema_migrations (version, description, success)
VALUES ('20260228000001', 'Verify foreign key constraints', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();
