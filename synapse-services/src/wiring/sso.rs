//! SSO — SAML, CAS, OIDC, builtin OIDC.
//!
//! ARCH-07/08 (2026-08-10): The `saml_storage` and `cas_storage` fields are
//! "backing storage" — they are constructed here, injected into the
//! corresponding `*_service`, and also stored on the struct. The stored
//! copies are not accessed via the container after construction.

use std::sync::Arc;

use synapse_common::config::Config;

/// The `SsoServices` struct.
#[derive(Clone)]
pub struct SsoServices {
    #[cfg(feature = "saml-sso")]
    /// The `saml_storage` field.
    pub saml_storage: Arc<dyn synapse_storage::saml::SamlStoreApi>,
    #[cfg(feature = "saml-sso")]
    /// The `saml_service` field.
    pub saml_service: Arc<crate::saml_service::SamlService>,
    #[cfg(feature = "cas-sso")]
    /// The `cas_storage` field.
    pub cas_storage: Arc<dyn synapse_storage::cas::CasStoreApi>,
    #[cfg(feature = "cas-sso")]
    /// The `cas_service` field.
    pub cas_service: Arc<crate::cas_service::CasService>,
    /// The `oidc_service` field.
    pub oidc_service: Option<Arc<crate::oidc_service::OidcService>>,
    /// The `oidc_mapping_storage` field.
    pub oidc_mapping_storage: Arc<dyn synapse_storage::oidc_user_mapping::OidcUserMappingStoreApi>,
    /// The `oidc_session_storage` field.
    pub oidc_session_storage: Arc<dyn synapse_storage::oidc_session_storage::OidcSessionStoreApi>,
    #[cfg(feature = "builtin-oidc")]
    /// The `builtin_oidc_provider` field.
    pub builtin_oidc_provider: Option<Arc<crate::builtin_oidc_provider::BuiltinOidcProvider>>,
    #[cfg(not(feature = "builtin-oidc"))]
    /// The `builtin_oidc_provider` field.
    pub builtin_oidc_provider: Option<()>,
}

impl SsoServices {
    /// See [`new`].
    /// See [`new`].
    pub async fn new(pool: &Arc<sqlx::PgPool>, config: &Config) -> Self {
        #[cfg(feature = "saml-sso")]
        let saml_storage: Arc<dyn synapse_storage::saml::SamlStoreApi> =
            Arc::new(synapse_storage::saml::SamlStorage::new(pool));
        #[cfg(feature = "saml-sso")]
        let saml_service = Arc::new(crate::saml_service::SamlService::new(
            Arc::new(config.saml.clone()),
            saml_storage.clone(),
            config.server.name.clone(),
        ));

        #[cfg(feature = "cas-sso")]
        let cas_storage: Arc<dyn synapse_storage::cas::CasStoreApi> =
            Arc::new(synapse_storage::cas::CasStorage::new(pool));
        #[cfg(feature = "cas-sso")]
        let cas_service =
            Arc::new(crate::cas_service::CasService::new(cas_storage.clone(), config.server.name.clone()));

        let oidc_service = if config.oidc.is_enabled() {
            Some(Arc::new(crate::oidc_service::OidcService::new(Arc::new(config.oidc.clone()))))
        } else {
            None
        };

        #[cfg(feature = "builtin-oidc")]
        let builtin_oidc_provider = if config.builtin_oidc.is_enabled() {
            match crate::builtin_oidc_provider::BuiltinOidcProvider::new(Arc::new(config.builtin_oidc.clone())) {
                Ok(p) => Some(Arc::new(p)),
                Err(e) => {
                    ::tracing::error!(
                        error = %e,
                        builtin_oidc_enabled = true,
                        issuer = %config.builtin_oidc.issuer,
                        "Failed to initialize BuiltinOidcProvider, disabling"
                    );
                    None
                }
            }
        } else {
            None
        };
        #[cfg(not(feature = "builtin-oidc"))]
        let builtin_oidc_provider: Option<()> = None;

        #[cfg(feature = "builtin-oidc")]
        {
            let external_enabled = oidc_service.is_some();
            let builtin_enabled = builtin_oidc_provider.is_some();
            if external_enabled && builtin_enabled {
                ::tracing::warn!(
                    "Both external OIDC (oidc.issuer) and builtin OIDC provider are enabled. \
                     Builtin OIDC is intended for development/testing only. \
                     In production, use an external IdP and disable builtin OIDC."
                );
            }
        }
        #[cfg(not(feature = "builtin-oidc"))]
        {
            if oidc_service.is_some() {
                ::tracing::info!(
                    external_oidc_enabled = true,
                    builtin_oidc_compiled = false,
                    "External OIDC provider enabled"
                );
            }
        }

        let oidc_mapping_storage: Arc<dyn synapse_storage::oidc_user_mapping::OidcUserMappingStoreApi> =
            Arc::new(synapse_storage::oidc_user_mapping::OidcUserMappingStorage::new(pool.clone()));
        let oidc_session_storage: Arc<dyn synapse_storage::oidc_session_storage::OidcSessionStoreApi> =
            Arc::new(synapse_storage::oidc_session_storage::OidcSessionStorage::new(pool));

        Self {
            #[cfg(feature = "saml-sso")]
            saml_storage,
            #[cfg(feature = "saml-sso")]
            saml_service,
            #[cfg(feature = "cas-sso")]
            cas_storage,
            #[cfg(feature = "cas-sso")]
            cas_service,
            oidc_service,
            builtin_oidc_provider,
            oidc_mapping_storage,
            oidc_session_storage,
        }
    }
}
