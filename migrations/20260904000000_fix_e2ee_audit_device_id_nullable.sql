-- Fix: e2ee_audit_log.device_id is NOT NULL but CrossSigningVerificationService::verify_all_devices
-- writes a summary event with device_id=None (no specific device involved). This caused
-- "Failed to log key operation" ApiError on every verify_user_devices call for users
-- with at least one device. Fix by making device_id nullable.
ALTER TABLE e2ee_audit_log ALTER COLUMN device_id DROP NOT NULL;
