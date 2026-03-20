-- Verification tables for SAS and QR verification
-- Created: 2026-03-15

-- Verification requests table
CREATE TABLE IF NOT EXISTS verification_requests (
    transaction_id VARCHAR(255) PRIMARY KEY,
    from_user VARCHAR(255) NOT NULL,
    from_device VARCHAR(255) NOT NULL,
    to_user VARCHAR(255) NOT NULL,
    to_device VARCHAR(255),
    method VARCHAR(50) NOT NULL DEFAULT 'sas',
    state VARCHAR(50) NOT NULL DEFAULT 'requested',
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_verification_from_user ON verification_requests(from_user);
CREATE INDEX IF NOT EXISTS idx_verification_to_user ON verification_requests(to_user);
CREATE INDEX IF NOT EXISTS idx_verification_state ON verification_requests(state);

-- SAS verification state
CREATE TABLE IF NOT EXISTS verification_sas (
    tx_id VARCHAR(255) PRIMARY KEY,
    from_device VARCHAR(255) NOT NULL,
    to_device VARCHAR(255),
    method VARCHAR(50) NOT NULL,
    state VARCHAR(50) NOT NULL,
    exchange_hashes JSONB DEFAULT '[]',
    commitment TEXT,
    pubkey TEXT,
    sas_bytes BYTEA,
    mac TEXT,
    FOREIGN KEY (tx_id) REFERENCES verification_requests(transaction_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_verification_sas_from_device ON verification_sas(from_device);

-- QR verification state
CREATE TABLE IF NOT EXISTS verification_qr (
    tx_id VARCHAR(255) PRIMARY KEY,
    from_device VARCHAR(255) NOT NULL,
    to_device VARCHAR(255),
    state VARCHAR(50) NOT NULL,
    qr_code_data TEXT,
    scanned_data TEXT,
    FOREIGN KEY (tx_id) REFERENCES verification_requests(transaction_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_verification_qr_from_device ON verification_qr(from_device);
