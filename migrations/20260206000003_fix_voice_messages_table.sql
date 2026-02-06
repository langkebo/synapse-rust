-- 修复 voice_messages 表结构以匹配代码期望
-- 执行时间: 2026-02-06
-- 问题: 代码期望的列与数据库表结构不匹配

-- 1. 添加缺失的列
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS session_id VARCHAR(255);
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS waveform_data TEXT;
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS transcribe_text TEXT;

-- 2. 重命名列以匹配代码期望
ALTER TABLE voice_messages RENAME COLUMN size_bytes TO file_size;
ALTER TABLE voice_messages RENAME COLUMN created_at TO created_ts;

-- 3. 修改列类型以匹配代码期望
ALTER TABLE voice_messages ALTER COLUMN content_type TYPE VARCHAR(100);
ALTER TABLE voice_messages ALTER COLUMN duration_ms TYPE INT;
