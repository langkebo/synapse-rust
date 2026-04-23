pub mod backup;
pub mod cross_signing;
pub mod crypto;
pub mod device_keys;
pub mod device_trust;
pub mod key_request;
pub mod megolm;
pub mod olm;
pub mod secure_backup;
pub mod signature;
pub mod signed_json;
pub mod ssss;
pub mod to_device;

// Explicit exports for backup module
pub use backup::models::{
    BackupKeyInfo, BackupKeyUpload, BackupKeyUploadRequest, BackupUploadRequest,
    BackupUploadResponse, BackupVerificationRequest, BackupVerificationResponse, BackupVersion,
    BatchRecoveryRequest, BatchRecoveryResponse, KeyBackup, RecoveryProgress, RecoveryRequest,
    RecoveryResponse, RecoverySession,
};
pub use backup::service::KeyBackupService;

// Explicit exports for secure_backup (Phase 3)
pub use secure_backup::models::{
    BackupVersion as SecureBackupVersion, CreateSecureBackupRequest, KeyDerivationParams,
    RestoreResponse, RestoreSecureBackupRequest, SecureBackupAuthData, SecureBackupInfo,
    SecureBackupResponse, SessionKeyData, VerifyPassphraseRequest, VerifyPassphraseResponse,
};
pub use secure_backup::service::SecureBackupService;
// Explicit exports to avoid ambiguous glob re-exports
pub use cross_signing::models::CrossSigningKey;
pub use cross_signing::models::CrossSigningKeys;
pub use cross_signing::service::CrossSigningService;
pub use cross_signing::storage::CrossSigningStorage;
pub use crypto::aes::*;
pub use crypto::argon2::*;
pub use crypto::ed25519::*;
pub use crypto::x25519::*;
pub use crypto::CryptoError;
pub use device_keys::models::*;
pub use device_keys::service::DeviceKeyService;
// Explicit exports for device_trust
pub use device_trust::models::{
    DeviceTrustLevel, DeviceTrustStatus, DeviceVerificationRequest, E2eeSecurityEvent,
    KeyRotationLog, SecuritySummary, VerificationMethod, VerificationRequestStatus,
};
pub use device_trust::service::DeviceTrustService;
pub use device_trust::storage::DeviceTrustStorage;
pub use key_request::{KeyRequestInfo, KeyRequestService};
pub use megolm::models::{EncryptedEvent, MegolmSession};
pub use megolm::service::MegolmService;
pub use olm::models::*;
pub use olm::OlmService;
pub use signature::EventSignature;
pub use signature::SignatureService;
pub use ssss::SecretStorage;
pub use ssss::SecretStorageService;
pub use verification::{
    QrCodeData, QrState, SasData, SasRepresentation, SasResult, SasState,
    VerificationMethod as VerifMethod, VerificationState,
};

pub mod verification;
