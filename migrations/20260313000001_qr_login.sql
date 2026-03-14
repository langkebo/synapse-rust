-- QR Login Transactions table - MSC4388
-- Secure out-of-band channel for sign in with QR
-- Following project field naming standards

-- Create table if not exists
CREATE TABLE IF NOT EXISTS qr_login_transactions (
    id BIGSERIAL PRIMARY KEY,
    transaction_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    -- Status: pending, confirmed, expired, failed
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    expires_at BIGINT NOT NULL,
    completed_at BIGINT,
    access_token TEXT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

-- Index for looking up transactions
CREATE INDEX IF NOT EXISTS idx_qr_login_transaction_id 
    ON qr_login_transactions(transaction_id);

-- Index for cleanup (partial index for pending status)
CREATE INDEX IF NOT EXISTS idx_qr_login_expires 
    ON qr_login_transactions(expires_at) WHERE status = 'pending';

-- Add comments for documentation
COMMENT ON TABLE qr_login_transactions IS 'QR Code Login Transactions - MSC4388';
COMMENT ON COLUMN qr_login_transactions.transaction_id IS 'Unique transaction identifier';
COMMENT ON COLUMN qr_login_transactions.user_id IS 'User attempting to login';
COMMENT ON COLUMN qr_login_transactions.device_id IS 'Target device for login';
COMMENT ON COLUMN qr_login_transactions.status IS 'Transaction status: pending, confirmed, expired, failed';
COMMENT ON COLUMN qr_login_transactions.created_ts IS 'Transaction creation timestamp (milliseconds)';
COMMENT ON COLUMN qr_login_transactions.updated_ts IS 'Last update timestamp (milliseconds)';
COMMENT ON COLUMN qr_login_transactions.expires_at IS 'Transaction expiry timestamp (milliseconds)';
