//! E2EE assembly — device keys, cross-signing, megolm, backup, verification.
//!
//! ARCH-07/08 (2026-08-10): The `to_device_storage` field is "backing
//! storage" — it is constructed here, injected into `to_device_service` and
//! `SyncService`/`SlidingSyncService`, and also stored on the struct. The
//! stored copy is not accessed via the container after construction.

use std::sync::Arc;

use synapse_cache::CacheManager;
use synapse_e2ee::backup::KeyBackupService;
use synapse_e2ee::cross_signing::CrossSigningService;
use synapse_e2ee::crypto::key_at_rest::KeyAtRest;
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
    /// Build the E2EE wiring.
    ///
    /// Fallible because of the server-side megolm at-rest key: with neither
    /// `server.megolm_encryption_key_path` nor `server.macaroon_secret_key`
    /// configured there is nothing to protect stored sessions with, and that must
    /// stop startup with the operator-facing message from [`resolve_at_rest_key`].
    /// It used to be `-> Self` with a scoped `#[allow(clippy::panic)]` pending
    /// exactly this change (gate-integrity sweep, §1.6 of the follow-up doc).
    #[allow(clippy::expect_used)]
    pub async fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: &Arc<CacheManager>,
        user_storage: &Arc<dyn UserStore>,
        megolm_encryption_key_path: Option<&str>,
        macaroon_secret_key: Option<&str>,
    ) -> Result<Self, String> {
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
        // Propagated, not panicked: the message names the two config keys and is
        // surfaced by the composition root as a startup error.
        let at_rest_key = resolve_at_rest_key(megolm_encryption_key_path, macaroon_secret_key)?;
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

        Ok(Self {
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
        })
    }
}

/// Decide which server-side megolm at-rest key to use.
///
/// Matrix does **not** require a homeserver to hold megolm keys: Megolm is a
/// client-side ratchet, the server stores `m.room.encrypted` as opaque
/// ciphertext, and key-backup blobs are encrypted client-side. Upstream Synapse
/// likewise starts with no megolm key configured. So an **unset** path must not be
/// fatal — only a path the operator *did* configure and that is unusable is a real
/// misconfiguration (and stays fail-closed).
///
/// - `Some(non-empty)` -> load it; any read/decode/length problem is an error.
/// - `None` / empty / whitespace -> derive a stable key from
///   `server.macaroon_secret_key` (domain-separated SHA-256) and warn. Rotating
///   that secret therefore makes already-stored server-side megolm rows
///   undecryptable, which is why an explicit key is the documented way to
///   decouple the two.
/// - `None` and no macaroon secret -> error: *something* must protect the store.
fn resolve_at_rest_key(
    megolm_encryption_key_path: Option<&str>,
    macaroon_secret_key: Option<&str>,
) -> Result<[u8; 32], String> {
    match megolm_encryption_key_path.map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => KeyAtRest::load_plaintext(path).map_err(|error| {
            format!(
                "server.megolm_encryption_key_path is configured ({path}) but unusable: {error}. \
                 Refusing to start rather than protecting server-side megolm sessions with a key \
                 the operator did not intend."
            )
        }),
        None => {
            let secret = macaroon_secret_key.map(str::trim).filter(|secret| !secret.is_empty()).ok_or_else(|| {
                "neither server.megolm_encryption_key_path nor server.macaroon_secret_key is \
                     configured; one of them is required to protect server-side megolm sessions at rest"
                    .to_string()
            })?;
            tracing::warn!(
                "server.megolm_encryption_key_path is not configured: deriving the server-side megolm \
                 at-rest key from server.macaroon_secret_key. Server-side megolm storage is optional \
                 (Matrix keeps Megolm on the client); set an explicit path to decouple it from \
                 macaroon-secret rotation."
            );
            Ok(derive_at_rest_key(secret))
        }
    }
}

/// Derive a stable at-rest key from an existing server secret.
///
/// Domain-separated SHA-256 is deliberately enough: the input is already a
/// high-entropy server secret, and the output protects only the optional
/// server-side megolm store (never user-facing cryptography).
fn derive_at_rest_key(secret: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"synapse-rust/megolm-at-rest/v1\0");
    hasher.update(secret.as_bytes());
    hasher.finalize().into()
}

#[cfg(test)]
mod at_rest_key_tests {
    use super::*;

    #[test]
    fn unset_path_derives_from_the_macaroon_secret() {
        let derived = resolve_at_rest_key(None, Some("macaroon-secret")).expect("should derive");
        assert_eq!(derived, derive_at_rest_key("macaroon-secret"));
        assert_ne!(derived, [0u8; 32]);
    }

    #[test]
    fn empty_or_blank_path_counts_as_unset() {
        for path in ["", "   "] {
            let derived = resolve_at_rest_key(Some(path), Some("macaroon-secret")).expect("should derive");
            assert_eq!(derived, derive_at_rest_key("macaroon-secret"), "path {path:?} must count as unset");
        }
    }

    #[test]
    fn explicitly_configured_but_unreadable_path_is_an_error() {
        let error = resolve_at_rest_key(Some("/nonexistent/megolm.key"), Some("macaroon-secret"))
            .expect_err("an explicitly configured, unreadable path must stay fail-closed");
        assert!(error.contains("megolm_encryption_key_path is configured"), "{error}");
    }

    #[test]
    fn no_path_and_no_macaroon_secret_is_an_error() {
        let error = resolve_at_rest_key(None, None).expect_err("something must protect the store");
        assert!(error.contains("neither"), "{error}");
    }

    #[test]
    fn derivation_is_domain_separated() {
        assert_ne!(derive_at_rest_key("a"), derive_at_rest_key("b"));
    }
}
