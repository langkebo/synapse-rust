-- =============================================================================
-- Synapse-Rust Schema Fix Migration
-- Version: 20260301000000
-- Created: 2026-03-01
-- Description: Fix schema inconsistencies and add missing tables
-- =============================================================================

-- =============================================================================
-- Part 1: Fix device_keys table - Add key_data column for compatibility
-- =============================================================================
-- The code expects a key_data TEXT column, but the unified schema uses separate columns
-- We need to add a computed column or trigger to maintain compatibility

-- First, check if key_data column exists, if not add it
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'key_data'
    ) THEN
        -- Add key_data column as TEXT
        ALTER TABLE device_keys ADD COLUMN key_data TEXT;
        
        -- Populate key_data from existing columns
        UPDATE device_keys SET key_data = json_build_object(
            'algorithm', algorithm,
            'key_id', key_id,
            'public_key', public_key,
            'signatures', COALESCE(signatures, '{}'::jsonb),
            'display_name', display_name
        )::text;
        
        RAISE NOTICE 'Added key_data column to device_keys table';
    END IF;
END $$;

-- Create trigger to auto-populate key_data on insert/update
CREATE OR REPLACE FUNCTION update_device_keys_key_data()
RETURNS TRIGGER AS $$
BEGIN
    NEW.key_data = json_build_object(
        'algorithm', NEW.algorithm,
        'key_id', NEW.key_id,
        'public_key', NEW.public_key,
        'signatures', COALESCE(NEW.signatures, '{}'::jsonb),
        'display_name', NEW.display_name
    )::text;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Drop trigger if exists, then create
DROP TRIGGER IF EXISTS trg_update_device_keys_key_data ON device_keys;
CREATE TRIGGER trg_update_device_keys_key_data
    BEFORE INSERT OR UPDATE ON device_keys
    FOR EACH ROW
    EXECUTE FUNCTION update_device_keys_key_data();

-- =============================================================================
-- Part 2: Create read_markers table
-- =============================================================================
CREATE TABLE IF NOT EXISTS read_markers (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    marker_type VARCHAR(50) NOT NULL DEFAULT 'm.read',
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    CONSTRAINT read_markers_room_user_type_unique UNIQUE(room_id, user_id, marker_type),
    CONSTRAINT read_markers_marker_type_check CHECK (marker_type IN ('m.read', 'm.fully_read', 'm.read.private'))
);

CREATE INDEX IF NOT EXISTS idx_read_markers_room ON read_markers(room_id);
CREATE INDEX IF NOT EXISTS idx_read_markers_user ON read_markers(user_id);
CREATE INDEX IF NOT EXISTS idx_read_markers_room_user ON read_markers(room_id, user_id);

-- =============================================================================
-- Part 3: Create event_receipts table
-- =============================================================================
CREATE TABLE IF NOT EXISTS event_receipts (
    id BIGSERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    receipt_type VARCHAR(50) NOT NULL DEFAULT 'm.read',
    ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    CONSTRAINT event_receipts_event_room_user_type_unique UNIQUE(event_id, room_id, user_id, receipt_type),
    CONSTRAINT event_receipts_receipt_type_check CHECK (receipt_type IN ('m.read', 'm.read.private'))
);

CREATE INDEX IF NOT EXISTS idx_event_receipts_event ON event_receipts(event_id);
CREATE INDEX IF NOT EXISTS idx_event_receipts_room ON event_receipts(room_id);
CREATE INDEX IF NOT EXISTS idx_event_receipts_user ON event_receipts(user_id);
CREATE INDEX IF NOT EXISTS idx_event_receipts_room_user ON event_receipts(room_id, user_id);
CREATE INDEX IF NOT EXISTS idx_event_receipts_room_type ON event_receipts(room_id, receipt_type);

-- =============================================================================
-- Part 4: Create receipts table (alias for compatibility)
-- =============================================================================
CREATE TABLE IF NOT EXISTS receipts (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    receipt_type VARCHAR(50) NOT NULL DEFAULT 'm.read',
    ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    CONSTRAINT receipts_room_event_user_type_unique UNIQUE(room_id, event_id, user_id, receipt_type)
);

CREATE INDEX IF NOT EXISTS idx_receipts_room ON receipts(room_id);
CREATE INDEX IF NOT EXISTS idx_receipts_user ON receipts(user_id);

-- =============================================================================
-- Part 5: Add foreign key constraints
-- =============================================================================
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_read_markers_room_id') THEN
        ALTER TABLE read_markers ADD CONSTRAINT fk_read_markers_room_id 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_read_markers_user_id') THEN
        ALTER TABLE read_markers ADD CONSTRAINT fk_read_markers_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_event_receipts_room_id') THEN
        ALTER TABLE event_receipts ADD CONSTRAINT fk_event_receipts_room_id 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_event_receipts_user_id') THEN
        ALTER TABLE event_receipts ADD CONSTRAINT fk_event_receipts_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_receipts_room_id') THEN
        ALTER TABLE receipts ADD CONSTRAINT fk_receipts_room_id 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_receipts_user_id') THEN
        ALTER TABLE receipts ADD CONSTRAINT fk_receipts_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- =============================================================================
-- Part 6: Add missing columns to existing tables if needed
-- =============================================================================

-- Add history_visibility to rooms if not exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'rooms' AND column_name = 'history_visibility'
    ) THEN
        ALTER TABLE rooms ADD COLUMN history_visibility VARCHAR(50) DEFAULT 'shared';
    END IF;
END $$;

-- Add guest_access to rooms if not exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'rooms' AND column_name = 'guest_access'
    ) THEN
        ALTER TABLE rooms ADD COLUMN guest_access VARCHAR(50) DEFAULT 'forbidden';
    END IF;
END $$;

-- =============================================================================
-- Part 7: Verify migration
-- =============================================================================
DO $$
DECLARE
    table_count INTEGER;
    column_exists BOOLEAN;
BEGIN
    -- Verify key_data column exists in device_keys
    SELECT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'key_data'
    ) INTO column_exists;
    
    IF NOT column_exists THEN
        RAISE EXCEPTION 'Migration failed: key_data column not found in device_keys';
    END IF;
    
    -- Verify new tables exist
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_schema = 'public'
    AND table_name IN ('read_markers', 'event_receipts', 'receipts');
    
    IF table_count < 3 THEN
        RAISE EXCEPTION 'Migration failed: Missing tables';
    END IF;
    
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'Schema Fix Migration Completed!';
    RAISE NOTICE 'Added key_data column to device_keys';
    RAISE NOTICE 'Created read_markers table';
    RAISE NOTICE 'Created event_receipts table';
    RAISE NOTICE 'Created receipts table';
    RAISE NOTICE '==========================================';
END $$;

-- =============================================================================
-- Record migration
-- =============================================================================
INSERT INTO schema_migrations (version, description, success)
VALUES ('20260301000000', 'Fix schema inconsistencies and add missing tables', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();
