-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260213000001
-- 描述: 修复语音消息表缺失列问题
-- 问题: voice_messages 表缺少 processed, processed_at, duration_seconds 等列
-- =============================================================================

BEGIN;

-- =============================================================================
-- 第一部分: 添加缺失列
-- =============================================================================

-- 添加 processed 列（标记语音消息是否已处理）
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS processed BOOLEAN DEFAULT FALSE;

-- 添加 processed_at 列（处理时间戳）
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS processed_at BIGINT;

-- 添加 duration_seconds 列（秒为单位的时长）
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS duration_seconds INTEGER;

-- 添加 sample_rate 列（采样率）
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS sample_rate INTEGER DEFAULT 44100;

-- 添加 channels 列（声道数）
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS channels INTEGER DEFAULT 1;

-- 添加 bitrate 列（比特率）
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS bitrate INTEGER;

-- 添加 format 列（音频格式）
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS format VARCHAR(20) DEFAULT 'ogg';

-- 添加 sender_id 列（发送者ID，兼容旧数据）
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS sender_id VARCHAR(255);

-- 添加 created_at 列（如果不存在）
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW();

-- =============================================================================
-- 第二部分: 数据迁移和修复
-- =============================================================================

-- 从 user_id 复制到 sender_id（如果 sender_id 为空）
UPDATE voice_messages 
SET sender_id = user_id 
WHERE sender_id IS NULL;

-- 从 duration_ms 计算 duration_seconds
UPDATE voice_messages 
SET duration_seconds = duration_ms / 1000 
WHERE duration_seconds IS NULL AND duration_ms IS NOT NULL;

-- 设置已存在记录的 processed 为 TRUE
UPDATE voice_messages 
SET 
    processed = TRUE,
    processed_at = created_ts
WHERE processed = FALSE 
AND created_ts IS NOT NULL;

-- 修复 created_at 时间戳
UPDATE voice_messages 
SET created_at = TO_TIMESTAMP(created_ts / 1000.0) 
WHERE created_at IS NULL AND created_ts IS NOT NULL;

-- =============================================================================
-- 第三部分: 添加索引
-- =============================================================================

-- 创建处理状态索引
CREATE INDEX IF NOT EXISTS idx_voice_processed ON voice_messages(processed);

-- 创建用户处理状态复合索引
CREATE INDEX IF NOT EXISTS idx_voice_user_processed ON voice_messages(user_id, processed);

-- 创建房间处理状态复合索引
CREATE INDEX IF NOT EXISTS idx_voice_room_processed ON voice_messages(room_id, processed) WHERE room_id IS NOT NULL;

-- 创建发送者索引
CREATE INDEX IF NOT EXISTS idx_voice_sender ON voice_messages(sender_id) WHERE sender_id IS NOT NULL;

-- =============================================================================
-- 第四部分: 添加约束
-- =============================================================================

-- 确保 sender_id 有值
ALTER TABLE voice_messages 
DROP CONSTRAINT IF EXISTS chk_voice_sender_exists;

ALTER TABLE voice_messages 
ADD CONSTRAINT chk_voice_sender_exists 
CHECK (sender_id IS NOT NULL OR user_id IS NOT NULL);

-- =============================================================================
-- 第五部分: 验证迁移
-- =============================================================================

DO $$
DECLARE
    column_count INTEGER;
BEGIN
    -- 验证新列已添加
    SELECT COUNT(*) INTO column_count
    FROM information_schema.columns 
    WHERE table_name = 'voice_messages'
    AND column_name IN ('processed', 'processed_at', 'duration_seconds', 'sample_rate', 'channels', 'bitrate', 'format', 'sender_id');
    
    IF column_count < 8 THEN
        RAISE EXCEPTION 'Migration failed: Expected 8 new columns, found %', column_count;
    END IF;
    
    RAISE NOTICE 'Migration completed successfully. Added % new columns', column_count;
END $$;

COMMIT;

-- =============================================================================
-- 迁移完成
-- =============================================================================
-- 预期效果:
-- 1. voice_messages 表添加 8 个新列
-- 2. 现有数据自动迁移和修复
-- 3. 添加性能优化索引
-- 4. 添加数据完整性约束
-- =============================================================================
