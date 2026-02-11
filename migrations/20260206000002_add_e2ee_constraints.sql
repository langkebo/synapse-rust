-- 修复 E2EE 相关表结构以匹配代码期望
-- 执行时间: 2026-02-06
-- 问题: 外键约束的列顺序必须与被引用表的主键顺序一致

-- 1. 修复 device_keys 表的外键约束
ALTER TABLE device_keys DROP CONSTRAINT IF EXISTS device_keys_user_id_device_id_fkey;
ALTER TABLE device_keys ADD CONSTRAINT device_keys_user_id_device_id_fkey 
    FOREIGN KEY (device_id, user_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE;

-- 2. 修复 to_device_messages 表的外键约束
ALTER TABLE to_device_messages DROP CONSTRAINT IF EXISTS to_device_messages_user_id_device_id_fkey;
ALTER TABLE to_device_messages ADD CONSTRAINT to_device_messages_user_id_device_id_fkey 
    FOREIGN KEY (device_id, user_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE;

-- 3. 修复 one_time_keys 表的外键约束
ALTER TABLE one_time_keys DROP CONSTRAINT IF EXISTS one_time_keys_user_id_device_id_fkey;
ALTER TABLE one_time_keys ADD CONSTRAINT one_time_keys_user_id_device_id_fkey 
    FOREIGN KEY (device_id, user_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE;

-- 4. 修复 refresh_tokens 表的外键约束
ALTER TABLE refresh_tokens DROP CONSTRAINT IF EXISTS refresh_tokens_device_id_user_id_fkey;
ALTER TABLE refresh_tokens ADD CONSTRAINT refresh_tokens_device_id_user_id_fkey 
    FOREIGN KEY (device_id, user_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE;
