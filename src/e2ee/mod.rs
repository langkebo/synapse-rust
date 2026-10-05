//! Thin re-export facade routing E2EE consumers to the `synapse-e2ee` crate.
//!
//! The per-feature submodules (`device_keys`, `key_rotation`, `megolm`,
//! `ssss`) were formerly local one-line shells that only glob-re-exported
//! from `synapse_e2ee`; they are now re-exported directly from the crate.

pub use synapse_e2ee::backup;
pub use synapse_e2ee::cross_signing;
pub use synapse_e2ee::crypto;
pub use synapse_e2ee::device_keys;
pub use synapse_e2ee::key_request;
pub use synapse_e2ee::key_rotation;
pub use synapse_e2ee::megolm;
pub use synapse_e2ee::olm;
pub use synapse_e2ee::secure_backup;
pub use synapse_e2ee::signed_json;
pub use synapse_e2ee::ssss;
pub use synapse_e2ee::to_device;
pub use synapse_e2ee::vodozemac_megolm;

// Explicit exports for backup module
pub use backup::models::{
    BackupKeyInfo, BackupKeyUpload, BackupKeyUploadRequest, BackupUploadRequest, BackupUploadResponse,
    BackupVerificationRequest, BackupVerificationResponse, BackupVersion, BatchRecoveryRequest, BatchRecoveryResponse,
    KeyBackup, RecoveryProgress, RecoveryRequest, RecoveryResponse, RecoverySession,
};
pub use backup::service::KeyBackupService;

// Explicit exports for secure_backup (Phase 3, client-side encryption only)
pub use secure_backup::models::{
    BackupVersion as SecureBackupVersion, EncryptedSessionKey, RestoreResponse, RestoreSecureBackupRequest,
    SecureBackupAuthData, SecureBackupInfo, SecureBackupResponse, SessionKeyData,
};
pub use secure_backup::service::SecureBackupService;
// Explicit exports to avoid ambiguous glob re-exports
pub use cross_signing::models::CrossSigningKey;
pub use cross_signing::models::CrossSigningKeys;
pub use cross_signing::models::DeviceKeyVerificationResult;
pub use cross_signing::models::VerifiedDevicesMap;
pub use cross_signing::service::CrossSigningService;
pub use cross_signing::storage::CrossSigningStorage;
pub use device_keys::models::*;
pub use device_keys::service::DeviceKeyService;
pub use key_request::{KeyRequestInfo, KeyRequestService};
pub use megolm::models::{EncryptedEvent, MegolmSession};
pub use megolm::service::MegolmProvider;
pub use olm::models::*;
pub use olm::OlmService;
pub use ssss::SecretStorage;
pub use ssss::SecretStorageService;
