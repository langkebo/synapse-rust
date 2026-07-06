//! E2EE assembly — device keys, cross-signing, megolm, backup, verification.

use std::sync::Arc;

use synapse_cache::CacheManager;
use synapse_e2ee::backup::KeyBackupService;
use synapse_e2ee::cross_signing::CrossSigningService;
use synapse_e2ee::device_keys::DeviceKeyService;
use synapse_e2ee::key_request::KeyRequestService;
use synapse_e2ee::megolm::MegolmProvider;
use synapse_e2ee::ssss::SecretStorageService;
use synapse_e2ee::to_device::ToDeviceService;
use synapse_e2ee::verification::VerificationService;
use synapse_storage::UserStore;

#[derive(Clone)]
pub struct E2eeServices {
    pub device_keys_service: DeviceKeyService,
    pub key_request_service: KeyRequestService,
    pub megolm_service: MegolmProvider,
    pub cross_signing_service: CrossSigningService,
    pub ssss_service: SecretStorageService,
    pub backup_service: KeyBackupService,
    pub dehydrated_device_service: crate::dehydrated_device_service::DehydratedDeviceService,
    pub secure_backup_service: synapse_e2ee::secure_backup::SecureBackupService,
    pub to_device_service: ToDeviceService,
    pub verification_service: VerificationService,
    pub device_trust_service: synapse_e2ee::device_trust::DeviceTrustService,
    pub to_device_storage: synapse_e2ee::to_device::ToDeviceStorage,
}

impl E2eeServices {
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        user_storage: &Arc<dyn UserStore>,
        megolm_encryption_key_path: Option<&str>,
    ) -> Self {
        let device_key_storage = synapse_e2ee::device_keys::DeviceKeyStorage::new(pool);
        let device_key_storage_for_cs = Arc::new(device_key_storage.clone());
        let backup_device_key_storage = device_key_storage.clone();
        let cross_signing_storage = synapse_e2ee::cross_signing::CrossSigningStorage::new(pool);
        let cross_signing_storage_arc = Arc::new(cross_signing_storage.clone());
        let dehydrated_device_storage = synapse_storage::DehydratedDeviceStorage::new(pool);

        let device_keys_service = DeviceKeyService::new(device_key_storage, cache.clone())
            .with_cross_signing_storage(cross_signing_storage_arc)
            .with_dehydrated_device_storage(dehydrated_device_storage.clone());

        let megolm_storage = synapse_e2ee::megolm::MegolmSessionStorage::new(pool);
        let encryption_key = generate_encryption_key(megolm_encryption_key_path);
        let megolm_service = MegolmProvider::from_env(megolm_storage, cache.clone(), encryption_key);

        let key_request_storage = synapse_e2ee::key_request::KeyRequestStorage::new(pool.as_ref());
        let key_request_service = KeyRequestService::new(key_request_storage, megolm_service.clone());

        let dehydrated_device_service =
            crate::dehydrated_device_service::DehydratedDeviceService::new(dehydrated_device_storage);

        let dehydrated_device_provider: Arc<dyn synapse_common::traits::DehydratedDeviceProvider> =
            Arc::new(dehydrated_device_service.clone());

        let cross_signing_service = CrossSigningService::new(cross_signing_storage)
            .with_device_keys_storage(device_key_storage_for_cs)
            .with_dehydrated_device_service(dehydrated_device_provider.clone());

        let ssss_storage = synapse_e2ee::ssss::SecretStorage::new(pool);
        let ssss_service = synapse_e2ee::ssss::SecretStorageService::new(ssss_storage)
            .with_dehydrated_device_service(dehydrated_device_provider);

        let key_backup_storage = synapse_e2ee::backup::KeyBackupStorage::new(pool);
        let backup_service =
            KeyBackupService::new(&key_backup_storage).with_device_key_storage(backup_device_key_storage);

        let secure_backup_service = synapse_e2ee::secure_backup::SecureBackupService::new(pool);

        let to_device_storage = synapse_e2ee::to_device::ToDeviceStorage::new(pool);
        let to_device_service = ToDeviceService::new(to_device_storage.clone()).with_user_storage(user_storage.clone());

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

pub(crate) fn generate_encryption_key(config_path: Option<&str>) -> [u8; 32] {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

    let path = config_path.map(|p| p.to_string());

    if let Some(ref p) = path {
        let path_buf = std::path::PathBuf::from(p);
        if path_buf.exists() {
            match std::fs::read_to_string(&path_buf) {
                Ok(content) => {
                    let trimmed = content.trim();
                    match B64.decode(trimmed) {
                        Ok(bytes) if bytes.len() == 32 => {
                            let mut key = [0u8; 32];
                            key.copy_from_slice(&bytes);
                            ::tracing::info!(path = %path_buf.display(), "Loaded megolm encryption key");
                            return key;
                        }
                        Ok(bytes) => {
                            ::tracing::error!(
                                "Megolm key at {} has wrong length ({} != 32); refusing to \
                                 overwrite — fix or remove the file",
                                path_buf.display(),
                                bytes.len()
                            );
                        }
                        Err(e) => {
                            ::tracing::error!(
                                "Megolm key at {} is not valid base64: {} — refusing to \
                                 overwrite",
                                path_buf.display(),
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    ::tracing::error!(
                        "Failed to read megolm key {}: {} — generating ephemeral key",
                        path_buf.display(),
                        e
                    );
                }
            }
        }
    }

    let mut key = [0u8; 32];
    use rand::RngCore;
    rand::rng().fill_bytes(&mut key);

    if let Some(ref p) = path {
        let path_buf = std::path::PathBuf::from(p);
        if !path_buf.exists() {
            if let Some(parent) = path_buf.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let encoded = B64.encode(key);
            match std::fs::write(&path_buf, encoded.as_bytes()) {
                Ok(_) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Err(e) = std::fs::set_permissions(&path_buf, std::fs::Permissions::from_mode(0o600)) {
                            ::tracing::warn!(path = %path_buf.display(), error = %e, "Failed to set 0600 permissions on megolm key file");
                        }
                    }
                    ::tracing::info!(path = %path_buf.display(), "Persisted new megolm encryption key");
                }
                Err(e) => {
                    ::tracing::error!(
                        "Failed to persist megolm key to {}: {} — key is ephemeral, \
                         existing encrypted sessions will be lost on restart",
                        path_buf.display(),
                        e
                    );
                }
            }
        }
    } else {
        ::tracing::warn!(
            "server.megolm_encryption_key_path is not configured; megolm encryption key is \
             ephemeral — all encrypted megolm sessions will be unreadable after server \
             restart. Set `server.megolm_encryption_key_path` or \
             `SYNAPSE__SERVER__MEGOLM_ENCRYPTION_KEY_PATH` to a writable file path for \
             production."
        );
    }

    key
}
