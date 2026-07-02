//! Capability governance — domain logic for Matrix client capabilities,
//! version negotiation, and SSO provider discovery.
//!
//! This module is a pure domain service. It depends on `synapse_common` for
//! config types and room version metadata, but has zero HTTP awareness.
//! Route-surface checks are driven by a pre-computed set of registered
//! `(method, path)` tuples passed in by the HTTP layer.

use std::collections::HashSet;

use serde_json::{json, Map, Value};

use crate::config::Config;
use crate::room_versions::client_room_versions_capability;

// ---------------------------------------------------------------------------
// Route surface — lightweight representation of registered HTTP routes
// ---------------------------------------------------------------------------

/// A single `(method, path)` route entry used for capability gating.
///
/// The capability module does not import the main crate's `RouteEntry` type;
/// instead the HTTP layer converts its route manifests into these lightweight
/// entries before constructing a [`CapabilityGovernance`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RouteCheck {
    pub method: String,
    pub path: &'static str,
}

impl RouteCheck {
    pub fn new(method: String, path: &'static str) -> Self {
        Self { method, path }
    }
}

// ---------------------------------------------------------------------------
// Client API version support
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClientApiVersionFamily {
    LegacyR0,
    StableV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClientApiVersionSupport {
    version: &'static str,
    family: ClientApiVersionFamily,
}

impl ClientApiVersionSupport {
    pub const fn legacy(version: &'static str) -> Self {
        Self { version, family: ClientApiVersionFamily::LegacyR0 }
    }

    pub const fn stable(version: &'static str) -> Self {
        Self { version, family: ClientApiVersionFamily::StableV1 }
    }

    pub const fn version(self) -> &'static str {
        self.version
    }

    pub(crate) const fn family(self) -> ClientApiVersionFamily {
        self.family
    }
}

pub(crate) const CLIENT_API_VERSION_SUPPORT: &[ClientApiVersionSupport] = &[
    ClientApiVersionSupport::legacy("r0.5.0"),
    ClientApiVersionSupport::legacy("r0.6.0"),
    ClientApiVersionSupport::legacy("r0.6.1"),
    ClientApiVersionSupport::stable("v1.1"),
    ClientApiVersionSupport::stable("v1.2"),
    ClientApiVersionSupport::stable("v1.3"),
    ClientApiVersionSupport::stable("v1.4"),
    ClientApiVersionSupport::stable("v1.5"),
    ClientApiVersionSupport::stable("v1.6"),
    ClientApiVersionSupport::stable("v1.7"),
    ClientApiVersionSupport::stable("v1.8"),
    ClientApiVersionSupport::stable("v1.9"),
    ClientApiVersionSupport::stable("v1.10"),
    ClientApiVersionSupport::stable("v1.11"),
    ClientApiVersionSupport::stable("v1.12"),
    ClientApiVersionSupport::stable("v1.13"),
    // v1.14 added per VERSION_GAP_ANALYSIS.md audit (2026-06-28).
    // Only 1 niche gap: POST /v3/users/{userId}/report (MSC4260).
    // All other v1.14 changes (server_name removal from join/knock,
    // editorial clarifications) are already supported.
    ClientApiVersionSupport::stable("v1.14"),
];

const BASE_UNSTABLE_FEATURES: &[(&str, bool)] = &[
    ("m.lazy_load_members", true),
    ("m.require_identity_server", false),
    // Phone (MSISDN) verification is not implemented — no SMS service,
    // no msisdn requestToken/submitToken routes. Declaring this as false
    // prevents clients from offering a login-via-phone path that cannot
    // complete. See WEB_FRONTEND_BACKEND_EXECUTION_CHECKLIST feature gap.
    ("m.supports_login_via_phone_number", false),
    ("org.matrix.msc3882", true),
    ("uk.tcpip.msc4133", true),
];

// ---------------------------------------------------------------------------
// Capability governance types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GovernanceClass {
    ConfigControlled,
    RouteSurface,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CapabilityFlag {
    enabled: bool,
    governance: GovernanceClass,
}

impl CapabilityFlag {
    pub const fn config_controlled(enabled: bool) -> Self {
        Self { enabled, governance: GovernanceClass::ConfigControlled }
    }

    pub const fn route_surface(enabled: bool) -> Self {
        Self { enabled, governance: GovernanceClass::RouteSurface }
    }

    pub const fn enabled(self) -> bool {
        self.enabled
    }

    #[allow(dead_code)]
    pub const fn governance(self) -> GovernanceClass {
        self.governance
    }
}

// ---------------------------------------------------------------------------
// CapabilityGovernance service
// ---------------------------------------------------------------------------

/// Domain service that answers capability, version, and SSO-provider queries.
///
/// # Route surface
///
/// Many Matrix capabilities are gated on whether a specific HTTP route is
/// registered (the "route surface"). The HTTP layer collects all registered
/// `(method, path)` tuples from route manifests and passes them in via
/// [`RouteCheck`] entries. The capability functions then check whether the
/// required route is present in that set, independent of any particular HTTP
/// framework or crate-internal route-registration mechanism.
pub struct CapabilityGovernance {
    config: Config,
    route_surface: HashSet<RouteCheck>,
}

impl CapabilityGovernance {
    pub fn new(config: &Config, route_surface: Vec<RouteCheck>) -> Self {
        Self { config: config.clone(), route_surface: route_surface.into_iter().collect() }
    }

    // -----------------------------------------------------------------------
    // Route surface helpers
    // -----------------------------------------------------------------------

    fn manifest_has_route(&self, method: &str, path: &str) -> bool {
        self.route_surface.iter().any(|r| r.method == method && r.path == path)
    }

    fn openclaw_routes_enabled(&self) -> bool {
        #[cfg(feature = "openclaw-routes")]
        {
            self.config.experimental.openclaw_routes_enabled
        }

        #[cfg(not(feature = "openclaw-routes"))]
        {
            false
        }
    }

    // -----------------------------------------------------------------------
    // Client API version helpers
    // -----------------------------------------------------------------------

    /// Returns the ordered list of client API versions declared by this server.
    pub fn declared_client_api_versions() -> Vec<&'static str> {
        let mut seen_stable = false;

        CLIENT_API_VERSION_SUPPORT
            .iter()
            .map(|support| {
                match support.family() {
                    ClientApiVersionFamily::LegacyR0 => {
                        debug_assert!(!seen_stable, "legacy r0 versions must stay before stable v1 versions");
                    }
                    ClientApiVersionFamily::StableV1 => {
                        seen_stable = true;
                    }
                }
                support.version()
            })
            .collect()
    }

    /// Build the `GET /_matrix/client/versions` response body (public surface).
    pub fn build_client_versions(&self) -> Value {
        let mut unstable_features = serde_json::Map::new();

        for (feature, enabled) in BASE_UNSTABLE_FEATURES {
            unstable_features.insert((*feature).to_string(), json!(enabled));
        }

        // Route-surface driven unstable feature declarations. These are kept
        // consistent with the `/capabilities.unstable_features` surface so that
        // clients observe the same availability on both endpoints.
        unstable_features
            .insert("org.matrix.msc3886.sliding_sync".to_string(), json!(self.sliding_sync_capability().enabled()));
        unstable_features.insert("org.matrix.msc3266".to_string(), json!(self.msc3266_capability().enabled()));
        unstable_features.insert("org.matrix.msc3245".to_string(), json!(self.msc3245_capability().enabled()));
        unstable_features.insert("org.matrix.msc3983".to_string(), json!(self.msc3983_capability().enabled()));
        unstable_features.insert("org.matrix.msc3814".to_string(), json!(self.msc3814_capability().enabled()));
        unstable_features.insert("org.matrix.msc4143".to_string(), json!(self.msc4143_capability().enabled()));
        // Private `io.hula.*` extensions are intentionally NOT declared in
        // `/versions.unstable_features` — that surface is unauthenticated and
        // consumed by stock Matrix clients which do not understand the
        // `io.hula.*` namespace. These capabilities remain visible on the
        // authenticated `/capabilities` surface for clients that opt in.

        json!({
            "versions": Self::declared_client_api_versions(),
            "unstable_features": unstable_features
        })
    }

    // -----------------------------------------------------------------------
    // Capability flag functions (route-surface driven)
    // -----------------------------------------------------------------------

    fn room_summary_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(self.manifest_has_route("GET", "/_matrix/client/v3/rooms/{room_id}/summary"))
    }

    /// MSC3266 (Room summary batch) capability is driven by the route surface:
    /// the `org.matrix.msc3266` unstable feature is declared only when the
    /// `POST /_synapse/room_summary/v1/summaries/batch` endpoint is registered.
    fn msc3266_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(self.manifest_has_route("POST", "/_synapse/room_summary/v1/summaries/batch"))
    }

    /// MSC3814 (Dehydrated device) capability is driven by the route surface:
    /// declared only when the `GET /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device`
    /// endpoint is registered.
    fn msc3814_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(
            self.manifest_has_route("GET", "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device"),
        )
    }

    /// MSC4143 (MatrixRTC transports) capability is driven by the route surface:
    /// declared only when the `GET /_matrix/client/unstable/org.matrix.msc4143/rtc/transports`
    /// endpoint is registered.
    fn msc4143_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(
            self.manifest_has_route("GET", "/_matrix/client/unstable/org.matrix.msc4143/rtc/transports"),
        )
    }

    /// MSC3245 (Room summary) unstable feature is driven by the route surface:
    /// declared only when the room summary endpoint is registered.
    fn msc3245_capability(&self) -> CapabilityFlag {
        self.room_summary_capability()
    }

    /// MSC3983 (Thread) unstable feature is driven by the route surface:
    /// declared only when the threads endpoint is registered.
    fn msc3983_capability(&self) -> CapabilityFlag {
        self.thread_capability()
    }

    fn room_suggested_capability(&self) -> CapabilityFlag {
        // `m.room.suggested` reflects the space-suggested-rooms surface, which is
        // served by the room hierarchy endpoint (MSC2946). Derive from the
        // `/_matrix/client/v1/rooms/{room_id}/hierarchy` route registration instead
        // of aliasing `room_summary_capability()`, which tracks a different
        // endpoint (`/_matrix/client/v3/rooms/{room_id}/summary`).
        CapabilityFlag::route_surface(self.manifest_has_route("GET", "/_matrix/client/v1/rooms/{room_id}/hierarchy"))
    }

    fn voice_capability(&self) -> CapabilityFlag {
        // `m.voice` reflects whether the homeserver can issue TURN credentials.
        // Derive from the `/voip/turnServer` route registration instead of aliasing
        // `room_summary_capability()`, which tracks the room summary endpoint and
        // has nothing to do with VoIP availability.
        CapabilityFlag::route_surface(self.manifest_has_route("GET", "/_matrix/client/v3/voip/turnServer"))
    }

    fn thread_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(self.manifest_has_route("GET", "/_matrix/client/v1/threads"))
    }

    fn sliding_sync_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(self.manifest_has_route("POST", "/_matrix/client/v1/sync"))
    }

    fn change_password_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(self.manifest_has_route("POST", "/_matrix/client/v3/account/password"))
    }

    /// Public entry-point for external callers that need the boolean value
    /// without depending on the private `CapabilityFlag` type.
    pub fn change_password_capability_enabled(&self) -> bool {
        self.change_password_capability().enabled()
    }

    fn set_displayname_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(
            self.manifest_has_route("PUT", "/_matrix/client/v3/profile/{user_id}/displayname"),
        )
    }

    fn set_avatar_url_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(self.manifest_has_route("PUT", "/_matrix/client/v3/profile/{user_id}/avatar_url"))
    }

    fn threepid_changes_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(self.manifest_has_route("POST", "/_matrix/client/v3/account/3pid"))
    }

    // -----------------------------------------------------------------------
    // Capability flag functions (config-driven)
    // -----------------------------------------------------------------------

    fn sso_capability(&self) -> CapabilityFlag {
        CapabilityFlag::config_controlled(!self.sso_providers().is_empty())
    }

    fn openclaw_capability(&self) -> CapabilityFlag {
        CapabilityFlag::config_controlled(self.openclaw_routes_enabled())
    }

    fn ai_connection_capability(&self) -> CapabilityFlag {
        CapabilityFlag::config_controlled(self.openclaw_routes_enabled())
    }

    // -----------------------------------------------------------------------
    // Capability flag functions (config + route-surface)
    // -----------------------------------------------------------------------

    fn friends_capability(&self) -> CapabilityFlag {
        if !self.config.experimental.declare_private_extensions {
            return CapabilityFlag::config_controlled(false);
        }
        CapabilityFlag::route_surface(self.manifest_has_route("GET", "/_matrix/client/v3/friends"))
    }

    fn external_services_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(self.manifest_has_route("GET", "/_matrix/client/v1/external_services/health"))
    }

    fn voice_extended_capability(&self) -> CapabilityFlag {
        if !self.config.experimental.declare_private_extensions {
            return CapabilityFlag::config_controlled(false);
        }
        CapabilityFlag::route_surface(self.manifest_has_route("GET", "/_matrix/client/v1/voice/config"))
    }

    #[cfg(test)]
    fn widget_capability(&self) -> CapabilityFlag {
        CapabilityFlag::route_surface(self.manifest_has_route("POST", "/_matrix/client/v1/widgets"))
    }

    fn burn_after_read_capability(&self) -> CapabilityFlag {
        if !self.config.experimental.declare_private_extensions {
            return CapabilityFlag::config_controlled(false);
        }
        CapabilityFlag::route_surface(self.manifest_has_route("PUT", "/_matrix/client/v1/rooms/{room_id}/burn"))
    }

    // -----------------------------------------------------------------------
    // Response builders
    // -----------------------------------------------------------------------

    fn insert_enabled_capability(&self, capabilities: &mut Map<String, Value>, name: &str, enabled: bool) {
        capabilities.insert(name.to_string(), json!({ "enabled": enabled }));
    }

    /// List the SSO provider brands that should be declared in `m.sso.providers`.
    ///
    /// Returns a list of brand strings (e.g. `"saml"`, `"oidc"`) that clients
    /// use to render login buttons. The list is purely config-driven.
    pub fn sso_providers(&self) -> Vec<&'static str> {
        let mut providers = Vec::new();
        if self.config.saml.enabled {
            providers.push("saml");
        }
        // OIDC (external IdP or builtin provider) is treated as an SSO provider
        // for the `m.sso.providers` capability surface. Synapse exposes the same
        // `oidc` brand when an external OIDC IdP is configured; we mirror that.
        if self.config.oidc.is_enabled() || self.config.builtin_oidc.is_enabled() {
            providers.push("oidc");
        }
        #[cfg(feature = "cas-sso")]
        {
            providers.push("cas");
        }
        providers
    }

    fn build_capabilities_unstable_features(&self) -> Value {
        json!({
            "io.hula.friends": self.friends_capability().enabled(),
            "org.matrix.msc3245.voice": self.voice_capability().enabled(),
            "org.matrix.msc3983.thread": self.thread_capability().enabled(),
            "org.matrix.msc3886.sliding_sync": self.sliding_sync_capability().enabled(),
            "io.hula.burn_after_read": self.burn_after_read_capability().enabled()
        })
    }

    /// Build the `GET /_matrix/client/v3/capabilities` response body.
    ///
    /// When `authenticated` is `false`, only the public capability surface is
    /// returned (no `m.sso`, no `io.hula.*` private extensions).
    pub fn build_capabilities_response(&self, authenticated: bool) -> Value {
        let mut capabilities = Map::new();
        let sso_providers = self.sso_providers();

        self.insert_enabled_capability(
            &mut capabilities,
            "m.change_password",
            self.change_password_capability().enabled(),
        );
        capabilities.insert("m.room_versions".to_string(), client_room_versions_capability());
        self.insert_enabled_capability(
            &mut capabilities,
            "m.set_displayname",
            self.set_displayname_capability().enabled(),
        );
        self.insert_enabled_capability(
            &mut capabilities,
            "m.set_avatar_url",
            self.set_avatar_url_capability().enabled(),
        );
        self.insert_enabled_capability(
            &mut capabilities,
            "m.3pid_changes",
            self.threepid_changes_capability().enabled(),
        );
        self.insert_enabled_capability(&mut capabilities, "m.room.summary", self.room_summary_capability().enabled());
        self.insert_enabled_capability(
            &mut capabilities,
            "m.room.suggested",
            self.room_suggested_capability().enabled(),
        );
        self.insert_enabled_capability(&mut capabilities, "m.voice", self.voice_capability().enabled());
        self.insert_enabled_capability(&mut capabilities, "m.thread", self.thread_capability().enabled());
        // Sliding sync is declared via the standard `org.matrix.msc3886.sliding_sync`
        // unstable feature in `/versions` and `/capabilities.unstable_features`.
        // The private `io.hula.sliding_sync` capability is intentionally omitted
        // from the public surface — stock Element discovers sliding sync via the
        // standard MSC3886 identifier, not the `io.hula.*` namespace.

        // MSC4452: Preview URL capabilities API (Synapse v1.154 #19715).
        self.insert_enabled_capability(
            &mut capabilities,
            "io.element.msc4452.preview_url",
            self.config.experimental.msc4452_enabled,
        );

        if authenticated {
            let openclaw_enabled = self.openclaw_capability().enabled();

            capabilities.insert("io.hula.friends".to_string(), json!(self.friends_capability().enabled()));
            capabilities.insert(
                "m.sso".to_string(),
                json!({
                    "enabled": self.sso_capability().enabled(),
                    "providers": sso_providers
                }),
            );
            self.insert_enabled_capability(
                &mut capabilities,
                "ai_connection",
                self.ai_connection_capability().enabled(),
            );
            self.insert_enabled_capability(&mut capabilities, "openclaw", openclaw_enabled);
            self.insert_enabled_capability(
                &mut capabilities,
                "external_services",
                self.external_services_capability().enabled(),
            );
            self.insert_enabled_capability(
                &mut capabilities,
                "io.hula.voice_extended",
                self.voice_extended_capability().enabled(),
            );
            self.insert_enabled_capability(
                &mut capabilities,
                "io.hula.burn_after_read",
                self.burn_after_read_capability().enabled(),
            );
        }

        json!({
            "capabilities": capabilities,
            "unstable_features": self.build_capabilities_unstable_features()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Build a `CapabilityGovernance` with an empty route surface. This is the
    /// common case for tests that exercise config-driven behaviour or verify
    /// key presence rather than specific boolean values.
    fn governance_with_default_config() -> CapabilityGovernance {
        CapabilityGovernance::new(&Config::default(), vec![])
    }

    /// Build a `CapabilityGovernance` with a specific config and the given
    /// route entries.
    fn governance_with_routes(config: &Config, routes: Vec<(&str, &'static str)>) -> CapabilityGovernance {
        CapabilityGovernance::new(
            config,
            routes.into_iter().map(|(method, path)| RouteCheck::new(method.to_string(), path)).collect(),
        )
    }

    /// Build a `CapabilityGovernance` that has all the standard route-surface
    /// routes enabled. This is used for tests that need route-surface
    /// capabilities to return `true`.
    fn governance_with_full_routes(config: &Config) -> CapabilityGovernance {
        let routes = vec![
            ("GET", "/_matrix/client/v3/rooms/{room_id}/summary"),
            ("POST", "/_synapse/room_summary/v1/summaries/batch"),
            ("GET", "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device"),
            ("GET", "/_matrix/client/unstable/org.matrix.msc4143/rtc/transports"),
            ("GET", "/_matrix/client/v1/rooms/{room_id}/hierarchy"),
            ("GET", "/_matrix/client/v3/voip/turnServer"),
            ("GET", "/_matrix/client/v1/threads"),
            ("POST", "/_matrix/client/v1/sync"),
            ("POST", "/_matrix/client/v3/account/password"),
            ("PUT", "/_matrix/client/v3/profile/{user_id}/displayname"),
            ("PUT", "/_matrix/client/v3/profile/{user_id}/avatar_url"),
            ("POST", "/_matrix/client/v3/account/3pid"),
            ("GET", "/_matrix/client/v3/friends"),
            ("GET", "/_matrix/client/v1/external_services/health"),
            ("GET", "/_matrix/client/v1/voice/config"),
            ("POST", "/_matrix/client/v1/widgets"),
            ("PUT", "/_matrix/client/v1/rooms/{room_id}/burn"),
        ];
        CapabilityGovernance::new(
            config,
            routes.into_iter().map(|(method, path)| RouteCheck::new(method.to_string(), path)).collect(),
        )
    }

    // -----------------------------------------------------------------------
    // Client version support tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_client_versions_keeps_supported_versions_ordered_and_unique() {
        let g = governance_with_default_config();
        let body = g.build_client_versions();
        let versions = body["versions"].as_array().expect("versions should be an array");

        assert_eq!(versions.len(), CLIENT_API_VERSION_SUPPORT.len());
        for expected in CLIENT_API_VERSION_SUPPORT {
            assert_eq!(
                versions.iter().filter(|version| version.as_str() == Some(expected.version())).count(),
                1,
                "version {} should appear exactly once",
                expected.version()
            );
        }
    }

    #[test]
    fn test_client_version_support_keeps_legacy_before_stable_versions() {
        let first_stable_index = CLIENT_API_VERSION_SUPPORT
            .iter()
            .position(|support| support.family() == ClientApiVersionFamily::StableV1)
            .expect("stable v1 versions should be present");

        assert!(CLIENT_API_VERSION_SUPPORT[..first_stable_index]
            .iter()
            .all(|support| support.family() == ClientApiVersionFamily::LegacyR0));
        assert!(CLIENT_API_VERSION_SUPPORT[first_stable_index..]
            .iter()
            .all(|support| support.family() == ClientApiVersionFamily::StableV1));
    }

    #[test]
    fn test_versions_response_snapshot_keys() {
        let g = governance_with_default_config();
        let body = g.build_client_versions();

        let versions = body["versions"].as_array().expect("versions should be an array");
        assert!(!versions.is_empty(), "versions array must not be empty");

        let unstable = body["unstable_features"].as_object().expect("unstable_features should be an object");
        let expected_unstable: &[&str] = &[
            "m.lazy_load_members",
            "m.require_identity_server",
            "m.supports_login_via_phone_number",
            "org.matrix.msc3882",
            "uk.tcpip.msc4133",
            "org.matrix.msc3886.sliding_sync",
            "org.matrix.msc3266",
            "org.matrix.msc3245",
            "org.matrix.msc3983",
            "org.matrix.msc3814",
            "org.matrix.msc4143",
        ];
        for key in expected_unstable {
            assert!(unstable.contains_key(*key), "missing unstable feature: {key}");
        }

        // Private `io.hula.*` extensions must NOT appear in `/versions` —
        // that surface is unauthenticated and consumed by stock Matrix
        // clients which do not understand the `io.hula.*` namespace.
        assert!(
            !unstable.contains_key("io.hula.burn_after_read"),
            "private io.hula.burn_after_read must not leak to /versions"
        );
        assert!(!unstable.contains_key("io.hula.friends"), "private io.hula.friends must not leak to /versions");
    }

    // -----------------------------------------------------------------------
    // SSO provider tests
    // -----------------------------------------------------------------------

    #[test]
    #[cfg(not(feature = "cas-sso"))]
    fn test_sso_providers_none_by_default() {
        let g = governance_with_default_config();
        let providers = g.sso_providers();
        assert!(providers.is_empty(), "default config should have no SSO providers");
    }

    #[test]
    fn test_sso_providers_includes_saml_when_enabled() {
        let mut config = Config::default();
        config.saml.enabled = true;
        let g = CapabilityGovernance::new(&config, vec![]);
        let providers = g.sso_providers();
        assert!(providers.contains(&"saml"), "enabled SAML should appear as an SSO provider");
    }

    #[test]
    fn test_sso_providers_includes_oidc_when_enabled() {
        // Regression: `sso_providers()` previously omitted OIDC even when an
        // external IdP or builtin provider was configured, causing the
        // `m.sso.providers` capability to miss the `oidc` brand.
        let mut config = Config::default();
        assert!(
            !governance_with_routes(&config, vec![]).sso_providers().contains(&"oidc"),
            "default config should not list oidc provider"
        );

        // Simulate external OIDC enabled.
        config.oidc.enabled = true;
        config.oidc.issuer = "https://idp.example.com".to_string();
        config.oidc.client_id = "synapse-rust".to_string();
        assert!(
            governance_with_routes(&config, vec![]).sso_providers().contains(&"oidc"),
            "enabled external OIDC should appear as an sso provider"
        );

        // Simulate builtin OIDC enabled (without external OIDC).
        let mut config2 = Config::default();
        config2.builtin_oidc.enabled = true;
        config2.builtin_oidc.issuer = "https://hs.example.com".to_string();
        config2.builtin_oidc.users.push(crate::config::BuiltinOidcUser {
            id: "@alice:example.com".to_string(),
            username: "alice".to_string(),
            password: Some("password".to_string()),
            password_hash: None,
            email: "alice@example.com".to_string(),
            displayname: Some("Alice".to_string()),
        });
        assert!(
            governance_with_routes(&config2, vec![]).sso_providers().contains(&"oidc"),
            "enabled builtin OIDC should appear as an sso provider"
        );
    }

    // -----------------------------------------------------------------------
    // Capabilities response tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_capabilities_public_surface_hides_private_extensions() {
        let g = governance_with_full_routes(&Config::default());
        let body = g.build_capabilities_response(false);
        let capabilities = body["capabilities"].as_object().expect("capabilities should be an object");

        assert_eq!(capabilities["m.change_password"]["enabled"], g.change_password_capability().enabled());
        assert_eq!(capabilities["m.set_displayname"]["enabled"], g.set_displayname_capability().enabled());
        assert_eq!(capabilities["m.set_avatar_url"]["enabled"], g.set_avatar_url_capability().enabled());
        assert_eq!(capabilities["m.3pid_changes"]["enabled"], g.threepid_changes_capability().enabled());
        assert_eq!(capabilities["m.room.summary"]["enabled"], g.room_summary_capability().enabled());
        assert_eq!(capabilities["m.room.suggested"]["enabled"], g.room_suggested_capability().enabled());
        assert_eq!(capabilities["m.voice"]["enabled"], g.voice_capability().enabled());
        assert_eq!(capabilities["m.thread"]["enabled"], g.thread_capability().enabled());
        assert!(capabilities.contains_key("m.room_versions"));
        assert!(!capabilities.contains_key("m.sso"));
        assert!(!capabilities.contains_key("io.hula.friends"));
        assert!(!capabilities.contains_key("io.hula.widget"));
        assert!(!capabilities.contains_key("io.hula.burn_after_read"));
        // Private sliding sync capability must not leak to the public surface;
        // clients discover sliding sync via the standard MSC3886 identifier.
        assert!(!capabilities.contains_key("io.hula.sliding_sync"));
    }

    #[test]
    fn test_capabilities_authenticated_surface_tracks_config_and_feature_flags() {
        let mut config = Config::default();
        config.saml.enabled = true;
        #[cfg(feature = "openclaw-routes")]
        {
            config.experimental.openclaw_routes_enabled = false;
        }

        let g = governance_with_full_routes(&config);
        let body = g.build_capabilities_response(true);
        let capabilities = body["capabilities"].as_object().expect("capabilities should be an object");

        assert_eq!(capabilities["m.sso"]["enabled"], g.sso_capability().enabled());
        assert_eq!(capabilities["m.sso"]["providers"][0], "saml");
        assert_eq!(capabilities["openclaw"]["enabled"], g.openclaw_capability().enabled());
        assert_eq!(capabilities["io.hula.friends"], g.friends_capability().enabled());
        assert_eq!(capabilities["external_services"]["enabled"], g.external_services_capability().enabled());
        assert_eq!(capabilities["io.hula.voice_extended"]["enabled"], g.voice_extended_capability().enabled());
        assert_eq!(capabilities["io.hula.burn_after_read"]["enabled"], g.burn_after_read_capability().enabled());
        assert_eq!(body["unstable_features"]["org.matrix.msc3245.voice"], g.voice_capability().enabled());
        assert_eq!(body["unstable_features"]["org.matrix.msc3983.thread"], g.thread_capability().enabled());
        assert_eq!(body["unstable_features"]["org.matrix.msc3886.sliding_sync"], g.sliding_sync_capability().enabled());
        // Private io.hula.* extensions remain on the authenticated
        // `/capabilities.unstable_features` surface for opt-in clients.
        assert_eq!(body["unstable_features"]["io.hula.friends"], g.friends_capability().enabled());
        assert_eq!(body["unstable_features"]["io.hula.burn_after_read"], g.burn_after_read_capability().enabled());
    }

    #[test]
    fn test_declare_private_extensions_suppresses_hula_capabilities() {
        // When `declare_private_extensions = false`, all `io.hula.*`
        // capabilities should be disabled regardless of feature gates.
        let mut config = Config::default();
        config.experimental.declare_private_extensions = false;
        let g = governance_with_full_routes(&config);

        assert!(!g.friends_capability().enabled(), "friends should be suppressed");
        assert!(!g.voice_extended_capability().enabled(), "voice_extended should be suppressed");
        assert!(!g.burn_after_read_capability().enabled(), "burn_after_read should be suppressed");

        // Governance switches to ConfigControlled when suppressed.
        assert_eq!(g.friends_capability().governance(), GovernanceClass::ConfigControlled);
        assert_eq!(g.voice_extended_capability().governance(), GovernanceClass::ConfigControlled);
        assert_eq!(g.burn_after_read_capability().governance(), GovernanceClass::ConfigControlled);

        // The /capabilities response should not declare them as enabled.
        let body = g.build_capabilities_response(true);
        assert_eq!(body["capabilities"]["io.hula.friends"], false);
        assert_eq!(body["capabilities"]["io.hula.voice_extended"]["enabled"], false);
        assert_eq!(body["capabilities"]["io.hula.burn_after_read"]["enabled"], false);
    }

    #[test]
    fn test_capability_governance_classifies_route_and_config_sources() {
        let g = governance_with_full_routes(&Config::default());

        assert_eq!(g.room_summary_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.room_suggested_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.voice_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.thread_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.sliding_sync_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.change_password_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.set_displayname_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.set_avatar_url_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.threepid_changes_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.friends_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.external_services_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.voice_extended_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.widget_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.burn_after_read_capability().governance(), GovernanceClass::RouteSurface);
        assert_eq!(g.sso_capability().governance(), GovernanceClass::ConfigControlled);
        assert_eq!(g.openclaw_capability().governance(), GovernanceClass::ConfigControlled);
    }

    #[test]
    fn test_capabilities_response_snapshot_public_surface() {
        let g = governance_with_full_routes(&Config::default());
        let body = g.build_capabilities_response(false);
        let capabilities = body["capabilities"].as_object().expect("capabilities should be an object");

        let expected_public: &[&str] = &[
            "m.change_password",
            "m.room_versions",
            "m.set_displayname",
            "m.set_avatar_url",
            "m.3pid_changes",
            "m.room.summary",
            "m.room.suggested",
            "m.voice",
            "m.thread",
        ];
        for key in expected_public {
            assert!(capabilities.contains_key(*key), "missing public capability: {key}");
        }

        let private_only: &[&str] =
            &["m.sso", "io.hula.friends", "io.hula.widget", "io.hula.burn_after_read", "io.hula.sliding_sync"];
        for key in private_only {
            assert!(!capabilities.contains_key(*key), "private capability leaked to unauthenticated user: {key}");
        }
    }

    #[test]
    fn test_capabilities_response_snapshot_authenticated_surface() {
        let g = governance_with_full_routes(&Config::default());
        let body = g.build_capabilities_response(true);
        let capabilities = body["capabilities"].as_object().expect("capabilities should be an object");

        let authenticated_only: &[&str] = &[
            "io.hula.friends",
            "m.sso",
            "ai_connection",
            "openclaw",
            "external_services",
            "io.hula.voice_extended",
            "io.hula.burn_after_read",
        ];
        for key in authenticated_only {
            assert!(capabilities.contains_key(*key), "missing authenticated capability: {key}");
        }
    }

    #[test]
    fn test_all_capabilities_have_governance_classification() {
        let g = governance_with_full_routes(&Config::default());
        let body = g.build_capabilities_response(true);
        let capabilities = body["capabilities"].as_object().expect("capabilities should be an object");

        let known_keys: &[&str] = &[
            "m.change_password",
            "m.room_versions",
            "m.set_displayname",
            "m.set_avatar_url",
            "m.3pid_changes",
            "m.room.summary",
            "m.room.suggested",
            "m.voice",
            "m.thread",
            "io.hula.friends",
            "m.sso",
            "ai_connection",
            "openclaw",
            "external_services",
            "io.hula.voice_extended",
            "io.hula.burn_after_read",
            "io.element.msc4452.preview_url",
        ];

        for key in capabilities.keys() {
            assert!(known_keys.contains(&key.as_str()), "unexpected capability key in response: {key}");
        }

        for key in known_keys {
            if *key != "m.room_versions" {
                // m.room_versions is a special case with its own structure
                assert!(capabilities.contains_key(*key), "known capability missing from response: {key}");
            }
        }
    }

    #[test]
    fn test_no_residual_static_stable_governance() {
        // After P1-03.2 cleanup, no capability should use the legacy
        // StaticStable governance.  All capabilities must be either
        // RouteSurface or ConfigControlled.
        let g = governance_with_full_routes(&Config::default());
        let all_capabilities: &[CapabilityFlag] = &[
            g.change_password_capability(),
            g.set_displayname_capability(),
            g.set_avatar_url_capability(),
            g.threepid_changes_capability(),
            g.room_summary_capability(),
            g.room_suggested_capability(),
            g.voice_capability(),
            g.thread_capability(),
            g.sliding_sync_capability(),
            g.sso_capability(),
            g.openclaw_capability(),
            g.ai_connection_capability(),
            g.friends_capability(),
            g.external_services_capability(),
            g.voice_extended_capability(),
            g.widget_capability(),
            g.burn_after_read_capability(),
        ];

        for flag in all_capabilities {
            match flag.governance() {
                GovernanceClass::RouteSurface | GovernanceClass::ConfigControlled => {}
            }
        }
    }
}
