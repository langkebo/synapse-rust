// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err/panic per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::unwrap_err_used, clippy::panic))]

pub mod backup;
pub mod cross_signing;
pub mod crypto;
pub mod device_keys;
pub mod device_trust;
pub mod key_request;
pub mod key_rotation;
pub mod megolm;
pub mod olm;
pub mod secure_backup;
pub mod signature;
pub mod signed_json;
pub mod ssss;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_mocks;
pub mod to_device;
pub mod vodozemac_megolm;

// 跨客户端互操作测试（Phase 3）。所有 case 需 `E2EE_INTEROP=1` 显式启用，
// 不会拖慢默认 `cargo test` 速度；本地 Element 互操作通过
// `.github/workflows/e2ee-interop.yml` 跑。
#[cfg(test)]
mod vodozemac_interop_tests;

// Explicit exports for backup module
pub use backup::models::{
    BackupKeyInfo, BackupKeyUpload, BackupKeyUploadRequest, BackupUploadRequest, BackupUploadResponse,
    BackupVerificationRequest, BackupVerificationResponse, BackupVersion, BatchRecoveryRequest, BatchRecoveryResponse,
    KeyBackup, RecoveryProgress, RecoveryRequest, RecoveryResponse, RecoverySession,
};
pub use backup::service::KeyBackupService;

// Explicit exports for secure_backup (Phase 3)
pub use secure_backup::models::{
    BackupVersion as SecureBackupVersion, CreateSecureBackupRequest, KeyDerivationParams, RestoreResponse,
    RestoreSecureBackupRequest, SecureBackupAuthData, SecureBackupInfo, SecureBackupResponse, SessionKeyData,
    VerifyPassphraseRequest, VerifyPassphraseResponse,
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
// Explicit exports for device_trust
pub use device_trust::models::{
    DeviceTrustLevel, DeviceTrustStatus, DeviceVerificationRequest, E2eeSecurityEvent, KeyRotationLog, SecuritySummary,
    VerificationMethod, VerificationRequestStatus,
};
pub use device_trust::service::DeviceTrustService;
pub use device_trust::storage::DeviceTrustStorage;
pub use key_request::{KeyRequestInfo, KeyRequestService};
pub use megolm::models::{EncryptedEvent, MegolmSession};
pub use megolm::service::MegolmProvider;
pub use olm::models::*;
pub use olm::OlmService;
pub use signature::EventSignature;
pub use signature::SignatureService;
pub use ssss::SecretStorage;
pub use ssss::SecretStorageService;
pub use verification::{
    QrCodeData, QrState, SasData, SasRepresentation, SasResult, SasState, VerificationMethod as VerifMethod,
    VerificationState,
};

pub mod verification;
