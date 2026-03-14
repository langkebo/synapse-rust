-- 数据库字段一致性检查脚本
-- 用于检查数据库字段与项目规范的一致性
-- 运行方式: psql -d synapse -f scripts/check_field_consistency.sql

-- ============================================================
-- 1. 检查时间戳字段命名规范
-- ============================================================

-- 检查使用 created_at 而非 created_ts 的字段
SELECT 
    '检查: created_at 字段' as check_name,
    table_name,
    column_name,
    data_type
FROM information_schema.columns 
WHERE table_schema = 'public'
AND column_name IN ('created_at', 'updated_at', 'expires_ts', 'revoked_ts', 'validated_ts')
ORDER BY table_name;

-- ============================================================
-- 2. 检查布尔字段命名规范
-- ============================================================

-- 检查缺少 is_ 前缀的布尔字段
SELECT 
    '检查: 布尔字段命名' as check_name,
    table_name,
    column_name,
    data_type
FROM information_schema.columns 
WHERE table_schema = 'public'
AND data_type = 'boolean'
AND column_name NOT IN ('is_admin', 'is_guest', 'is_enabled', 'is_revoked', 'is_active', 
                        'is_public', 'is_deactivated', 'is_federatable', 'is_spotlight',
                        'is_default', 'is_verified', 'is_shadow_banned', 'has_avatar', 'has_displayname')
AND column_name NOT LIKE 'is_%'
AND column_name NOT LIKE 'has_%'
ORDER BY table_name;

-- ============================================================
-- 3. 检查外键约束
-- ============================================================

-- 检查缺少外键的 user_id 字段
SELECT 
    '检查: user_id 外键' as check_name,
    tc.table_name,
    kcu.column_name
FROM information_schema.table_constraints tc
JOIN information_schema.key_column_usage kcu
    ON tc.constraint_name = kcu.constraint_name
JOIN information_schema.columns c
    ON kcu.table_name = c.table_name AND kcu.column_name = c.column_name
LEFT JOIN information_schema.table_constraints fk
    ON fk.table_name = tc.table_name 
    AND fk.constraint_type = 'FOREIGN KEY'
WHERE tc.constraint_type = 'PRIMARY KEY'
AND kcu.column_name = 'user_id'
AND tc.table_name NOT IN (
    SELECT DISTINCT tc2.table_name
    FROM information_schema.table_constraints tc2
    JOIN information_schema.key_column_usage kcu2
        ON tc2.constraint_name = kcu2.constraint_name
    WHERE tc2.constraint_type = 'FOREIGN KEY'
    AND kcu2.column_name = 'user_id'
)
ORDER BY tc.table_name;

-- 检查缺少外键的 room_id 字段 (排除 rooms 表自身)
SELECT 
    '检查: room_id 外键' as check_name,
    tc.table_name,
    kcu.column_name
FROM information_schema.table_constraints tc
JOIN information_schema.key_column_usage kcu
    ON tc.constraint_name = kcu.constraint_name
WHERE tc.constraint_type = 'PRIMARY KEY'
AND tc.table_name != 'rooms'
AND EXISTS (
    SELECT 1 FROM information_schema.columns c
    WHERE c.table_name = tc.table_name AND c.column_name = 'room_id'
)
AND tc.table_name NOT IN (
    SELECT DISTINCT tc2.table_name
    FROM information_schema.table_constraints tc2
    JOIN information_schema.key_column_usage kcu2
        ON tc2.constraint_name = kcu2.constraint_name
    WHERE tc2.constraint_type = 'FOREIGN KEY'
    AND kcu2.column_name = 'room_id'
)
ORDER BY tc.table_name;

-- ============================================================
-- 4. 检查必需索引
-- ============================================================

-- 检查 devices 表的 user_id 索引
SELECT 
    '检查: devices.user_id 索引' as check_name,
    indexname,
    indexdef
FROM pg_indexes
WHERE tablename = 'devices'
AND indexdef LIKE '%user_id%';

-- 检查 room_memberships 表的复合索引
SELECT 
    '检查: room_memberships 索引' as check_name,
    indexname,
    indexdef
FROM pg_indexes
WHERE tablename = 'room_memberships';

-- 检查 events 表的时间索引
SELECT 
    '检查: events 索引' as check_name,
    indexname,
    indexdef
FROM pg_indexes
WHERE tablename = 'events';

-- ============================================================
-- 5. 检查数据完整性
-- ============================================================

-- 检查孤立设备 (user_id 不存在)
SELECT 
    '检查: 孤立设备' as check_name,
    COUNT(*) as orphan_count
FROM devices d
WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = d.user_id);

-- 检查孤立房间成员 (room_id 或 user_id 不存在)
SELECT 
    '检查: 孤立房间成员' as check_name,
    COUNT(*) as orphan_count
FROM room_memberships rm
WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rm.room_id)
OR NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = rm.user_id);

-- 检查孤立令牌 (user_id 不存在)
SELECT 
    '检查: 孤立令牌' as check_name,
    COUNT(*) as orphan_count
FROM access_tokens at
WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = at.user_id);

-- ============================================================
-- 6. 统计信息汇总
-- ============================================================

SELECT '=== 数据库统计 ===' as info;

SELECT 
    '表总数' as metric,
    COUNT(DISTINCT table_name)::text as value
FROM information_schema.tables 
WHERE table_schema = 'public'
AND table_type = 'BASE TABLE';

SELECT 
    '外键约束总数' as metric,
    COUNT(*)::text as value
FROM information_schema.table_constraints 
WHERE constraint_type = 'FOREIGN KEY'
AND table_schema = 'public';

SELECT 
    '索引总数' as metric,
    COUNT(*)::text as value
FROM pg_indexes 
WHERE schemaname = 'public';

SELECT 
    '用户总数' as metric,
    COUNT(*)::text as value
FROM users;

SELECT 
    '房间总数' as metric,
    COUNT(*)::text as value
FROM rooms;

SELECT 
    '设备总数' as metric,
    COUNT(*)::text as value
FROM devices;

\echo '=== 检查完成 ==='
