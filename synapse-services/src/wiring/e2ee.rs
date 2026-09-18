//! E2EE assembly — device keys, cross-signing, megolm, backup, verification.
//!
//! ARCH-07/08 (2026-08-10): The `to_device_storage` field is "backing
//! storage" — it is constructed here, injected into `to_device_service` and
//! `SyncService`/`SlidingSyncService`, and also stored on the struct. The
//! stored copy is not accessed via the container after construction.

use std::sync::Arc;

use synapse_cache::CacheManager;
use synapse_e2ee::backup::KeyBackupService;
use synapse_e2ee::crypto::key_at_rest::KeyAtRest;
use synapse_e2ee::cross_signing::CrossSigningService;
use synapse_e2ee::device_keys::DeviceKeyService;
use synapse_e2ee::device_keys::DeviceKeyStoreApi;
use synapse_e2ee::key_request::KeyRequestService;
use synapse_e2ee::megolm::MegolmProvider;
use synapse_e2ee::ssss::SecretStorageService;
use synapse_e2ee::to_device::ToDeviceService;
use synapse_e2ee::verification::VerificationService;
use synapse_storage::UserStore;

/// The `E2eeServices` struct.
#[derive(Clone)]
pub struct E2eeServices {
    /// The `device_keys_service` field.
    pub device_keys_service: DeviceKeyService,
    /// The `key_request_service` field.
    pub key_request_service: KeyRequestService,
    /// The `megolm_service` field.
    pub megolm_service: MegolmProvider,
    /// The `cross_signing_service` field.
    pub cross_signing_service: CrossSigningService,
    /// The `ssss_service` field.
    pub ssss_service: SecretStorageService,
    /// The `backup_service` field.
    pub backup_service: KeyBackupService,
    /// The `dehydrated_device_service` field.
    pub dehydrated_device_service: crate::dehydrated_device_service::DehydratedDeviceService,
    /// The `secure_backup_service` field.
    pub secure_backup_service: synapse_e2ee::secure_backup::SecureBackupService,
    /// The `to_device_service` field.
    pub to_device_service: ToDeviceService,
    /// The `verification_service` field.
    pub verification_service: VerificationService,
    /// The `device_trust_service` field.
    pub device_trust_service: synapse_e2ee::device_trust::DeviceTrustService,
    /// The `to_device_storage` field.
    pub to_device_storage: synapse_e2ee::to_device::ToDeviceStorage,
}

impl E2eeServices {
    /// See [`new`].
    #[allow(clippy::expect_used)]
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        user_storage: &Arc<dyn UserStore>,
        megolm_encryption_key_path: Option<&str>,
    ) -> Self {
        let device_key_storage = synapse_e2ee::device_keys::DeviceKeyStorage::new(pool);
        let device_key_storage_arc: Arc<dyn DeviceKeyStoreApi> = Arc::new(device_key_storage);
        let cross_signing_storage = synapse_e2ee::cross_signing::CrossSigningStorage::new(pool);
        let cross_signing_storage_arc = Arc::new(cross_signing_storage.clone());
        let dehydrated_device_storage = synapse_storage::DehydratedDeviceStorage::new(pool);
        let dehydrated_device_storage_arc: Arc<dyn synapse_storage::dehydrated_device::DehydratedDeviceStoreApi> =
            Arc::new(dehydrated_device_storage.clone());

        let device_keys_service = DeviceKeyService::new(device_key_storage_arc.clone(), cache.clone())
            .with_cross_signing_storage(cross_signing_storage_arc)
            .with_dehydrated_device_storage(dehydrated_device_storage.clone());

        let megolm_storage = synapse_e2ee::megolm::MegolmSessionStorage::new(pool);
        let at_rest_key = KeyAtRest::load_plaintext(megolm_encryption_key_path.unwrap_or_default())
            .expect("Failed to load megolm encryption key — server cannot start without a valid key file");
        let at_rest = KeyAtRest::new(at_rest_key);
        let megolm_service = MegolmProvider::from_env(megolm_storage, cache.clone(), at_rest);

        let key_request_storage = synapse_e2ee::key_request::KeyRequestStorage::new(pool.as_ref());
        let key_request_service = KeyRequestService::new(key_request_storage, megolm_service.clone());

        let dehydrated_device_service =
            crate::dehydrated_device_service::DehydratedDeviceService::new(dehydrated_device_storage_arc);

        let dehydrated_device_provider: Arc<dyn synapse_common::traits::DehydratedDeviceProvider> =
            Arc::new(dehydrated_device_service.clone());

        let cross_signing_service = CrossSigningService::new(cross_signing_storage)
            .with_device_keys_storage(device_key_storage_arc.clone())
            .with_dehydrated_device_service(dehydrated_device_provider.clone());

        let ssss_storage = synapse_e2ee::ssss::SecretStorage::new(pool);
        let ssss_service = synapse_e2ee::ssss::SecretStorageService::new(ssss_storage)
            .with_dehydrated_device_service(dehydrated_device_provider);

        let key_backup_storage = synapse_e2ee::backup::KeyBackupStorage::new(pool);
        let backup_service = KeyBackupService::new(&key_backup_storage).with_device_key_storage(device_key_storage_arc);

        let secure_backup_service = synapse_e2ee::secure_backup::SecureBackupService::new(pool);

        let to_device_storage = synapse_e2ee::to_device::ToDeviceStorage::new(pool);
        let to_device_service = ToDeviceService::new(std::sync::Arc::new(to_device_storage.clone())
            as std::sync::Arc<dyn synapse_e2ee::to_device::ToDeviceStorageApi>)
        .with_user_storage(user_storage.clone());

        let verification_storage = synapse_e2ee::verification::VerificationStorage::new(pool);
        let verification_service = VerificationService::new(std::sync::Arc::new(verification_storage));

        let device_trust_storage = synapse_e2ee::device_trust::DeviceTrustStorage::new(pool);
        let device_trust_service = synapse_e2ee::device_trust::DeviceTrustService::new(
            std::sync::Arc::new(device_trust_storage),
            std::sync::Arc::new(verification_service.clone()),
            std::sync::Arc::new(cross_signing_service.clone()),
            std::sync::Arc::new(device_keys_service.clone()),
        );

        Self {
            device_keys_service,
            key_request_service,
            megolm_service,
            cross_signing_service,
            ssss_service,
            backup_service,
            dehydrated_device_service,
            secure_backup_service,
            to_device_service,
            verification_service,
            device_trust_service,
            to_device_storage,
        }
    }
}
