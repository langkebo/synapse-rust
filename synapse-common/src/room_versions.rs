//! Matrix room-version metadata (`RoomVersion`, capability map, `DEFAULT_ROOM_VERSION`).

use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Represents RoomVersionDisposition; see per-variant docs.
pub enum RoomVersionDisposition {
    /// `Stable` variant.
    Stable,
    /// `Unstable` variant.
    Unstable,
}

impl RoomVersionDisposition {
    /// Returns a view as str.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Unstable => "unstable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Represents RoomVersionCapability.
pub struct RoomVersionCapability {
    /// `version` field.
    pub version: &'static str,
    /// `disposition` field.
    pub disposition: RoomVersionDisposition,
    /// `can_create` field.
    pub can_create: bool,
    /// `can_join` field.
    pub can_join: bool,
    /// `can_parse` field.
    pub can_parse: bool,
    /// `can_federate` field.
    pub can_federate: bool,
}

impl RoomVersionCapability {
    /// Performs stable.
    pub const fn stable(version: &'static str) -> Self {
        Self {
            version,
            disposition: RoomVersionDisposition::Stable,
            can_create: true,
            can_join: true,
            can_parse: true,
            can_federate: true,
        }
    }

    /// A stable room version that can be parsed and joined but cannot be
    /// created on this server.  Used for room versions whose redaction format
    /// or auth rules are not yet fully implemented, so that we do not advertise
    /// creation support that would produce non-compliant PDUs.
    pub const fn stable_parse_only(version: &'static str) -> Self {
        Self {
            version,
            disposition: RoomVersionDisposition::Stable,
            can_create: false,
            can_join: true,
            can_parse: true,
            can_federate: true,
        }
    }

    /// Dispositions the str.
    pub const fn disposition_str(self) -> &'static str {
        self.disposition.as_str()
    }
}

/// Constant `DEFAULT_ROOM_VERSION`.
///
/// Element/Synapse also made `"11"` its default in v1.158.0 (MSC4239), so this
/// project's default is no longer a divergence — it merely landed earlier
/// (2026-09-12) than upstream.  See
/// <https://github.com/element-hq/synapse/blob/develop/CHANGES.md> (1.158.0rc1,
/// "Change default room version to 11, implementing MSC4239").
///
/// Consequences to keep in mind when reviewing federation behaviour:
/// version 11 uses the MSC2174/MSC3820 redaction format (`content.redacts`)
/// and permits self-redaction by the original author, so events created here
/// are not byte-identical to those a stock pre-1.158 Synapse would create.
/// A remote server that does not support v11 cannot join a room created with
/// this default.
pub const DEFAULT_ROOM_VERSION: &str = "11";

/// Constant `SUPPORTED_ROOM_VERSIONS`.
pub const SUPPORTED_ROOM_VERSIONS: &[RoomVersionCapability] = &[
    RoomVersionCapability::stable("1"),
    RoomVersionCapability::stable("2"),
    RoomVersionCapability::stable("3"),
    RoomVersionCapability::stable("4"),
    RoomVersionCapability::stable("5"),
    RoomVersionCapability::stable("6"),
    RoomVersionCapability::stable("7"),
    RoomVersionCapability::stable("8"),
    RoomVersionCapability::stable("9"),
    RoomVersionCapability::stable("10"),
    // v11+ use the MSC2174/MSC3820 redaction format (content.redacts) and
    // allow self-redaction by the original author.  Both behaviours are now
    // implemented in synapse-common::redaction (extract_redacts handles both
    // top-level and content.redacts) and in auth::power_levels::can_redact_event
    // (which grants self-redact for room versions >= 11), so v11 can be
    // advertised as creatable.
    //
    // v12/v13: 额外的事件认证规则（ED25519-only auth rules / 协议扩展）
    // 尚未在本服务端完整实现。降级为 parse+join+federate-only 可用，
    // 避免创建无法产生合规 PDU 的房间（fail-safe）。
    RoomVersionCapability::stable("11"),
    RoomVersionCapability::stable_parse_only("12"),
    RoomVersionCapability::stable_parse_only("13"),
];

/// Returns true if supported room version.
pub fn is_supported_room_version(version: &str) -> bool {
    SUPPORTED_ROOM_VERSIONS.iter().any(|capability| capability.version == version)
}

/// Cans the create.
pub fn can_create_room_version(version: &str) -> bool {
    SUPPORTED_ROOM_VERSIONS.iter().any(|capability| capability.version == version && capability.can_create)
}

/// Cans the join.
pub fn can_join_room_version(version: &str) -> bool {
    SUPPORTED_ROOM_VERSIONS.iter().any(|capability| capability.version == version && capability.can_join)
}

/// Cans the parse.
pub fn can_parse_room_version(version: &str) -> bool {
    SUPPORTED_ROOM_VERSIONS.iter().any(|capability| capability.version == version && capability.can_parse)
}

/// Cans the federate.
pub fn can_federate_room_version(version: &str) -> bool {
    SUPPORTED_ROOM_VERSIONS.iter().any(|capability| capability.version == version && capability.can_federate)
}

/// Resolves the room.
pub fn resolve_room_version(requested: Option<&str>) -> Option<&'static str> {
    let requested = requested.unwrap_or(DEFAULT_ROOM_VERSION);

    SUPPORTED_ROOM_VERSIONS
        .iter()
        .find(|capability| capability.version == requested && capability.can_create)
        .map(|capability| capability.version)
}

/// Clients the room.
pub fn client_room_versions_capability() -> Value {
    let mut available = serde_json::Map::new();

    for capability in SUPPORTED_ROOM_VERSIONS {
        if capability.can_create {
            available.insert(capability.version.to_string(), json!(capability.disposition_str()));
        }
    }

    json!({
        "default": DEFAULT_ROOM_VERSION,
        "available": available
    })
}

/// Federations the room.
pub fn federation_room_versions_capability() -> Value {
    let mut available = serde_json::Map::new();

    for capability in SUPPORTED_ROOM_VERSIONS {
        if capability.can_federate {
            available.insert(capability.version.to_string(), json!({ "status": capability.disposition_str() }));
        }
    }

    Value::Object(available)
}

#[cfg(test)]
mod tests {
    use super::{
        can_create_room_version, can_federate_room_version, can_join_room_version, can_parse_room_version,
        client_room_versions_capability, federation_room_versions_capability, is_supported_room_version,
        resolve_room_version, DEFAULT_ROOM_VERSION, SUPPORTED_ROOM_VERSIONS,
    };

    #[test]
    fn default_room_version_is_advertised_as_supported() {
        assert!(is_supported_room_version(DEFAULT_ROOM_VERSION));
    }

    #[test]
    fn default_room_version_is_11() {
        // Pinned to a literal ON PURPOSE (not `DEFAULT_ROOM_VERSION`) so that
        // changing the constant is a deliberate, reviewed act. Upstream Synapse
        // reached the same default in v1.158.0 (MSC4239); the literal keeps this
        // project's earlier switch auditable.
        assert_eq!(DEFAULT_ROOM_VERSION, "11");
        assert_eq!(resolve_room_version(None), Some("11"));
        // Every room-version surface must agree on the same literal.
        let capability = client_room_versions_capability();
        assert_eq!(capability["default"], "11");
    }

    #[test]
    fn resolve_room_version_defaults_and_rejects_unknown_versions() {
        assert_eq!(resolve_room_version(None), Some(DEFAULT_ROOM_VERSION));
        assert_eq!(resolve_room_version(Some("10")), Some("10"));
        // v11 is fully creatable after the redaction chain (P0-05/06/09)
        // and state resolution v2 (P0-10/11) landed.
        assert_eq!(resolve_room_version(Some("11")), Some("11"));
        // v12/v13 are parse/join/federate-only (not creatable) – see room_versions.rs
        // comment. resolve_room_version only returns creatable versions.
        assert_eq!(resolve_room_version(Some("12")), None);
        assert_eq!(resolve_room_version(Some("13")), None);
        // v14 is not a supported room version.
        assert_eq!(resolve_room_version(Some("14")), None);
    }

    #[test]
    fn room_version_support_matrix_keeps_current_versions_fully_enabled() {
        for supported in SUPPORTED_ROOM_VERSIONS {
            // All versions can be joined, parsed, and federated.
            assert!(can_join_room_version(supported.version));
            assert!(can_parse_room_version(supported.version));
            assert!(can_federate_room_version(supported.version));
        }
        // v1-v11 are fully creatable after the redaction chain and state
        // resolution v2 landed.
        for v in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"] {
            assert!(can_create_room_version(v), "v{v} must remain creatable");
        }
        // v12/v13 are deliberately parse/join/federate-only: their extra auth
        // rules are not fully implemented, so we must not advertise creation
        // support (fail-safe over over-declaration).
        assert!(!can_create_room_version("12"), "v12 must NOT be creatable");
        assert!(!can_create_room_version("13"), "v13 must NOT be creatable");
        assert!(can_join_room_version("12") && can_join_room_version("13"));
        assert!(!can_create_room_version("14"));
        assert!(!can_join_room_version("14"));
        assert!(!can_parse_room_version("14"));
        assert!(!can_federate_room_version("14"));
    }

    #[test]
    fn client_room_versions_capability_matches_supported_matrix() {
        let capability = client_room_versions_capability();
        let available = capability["available"].as_object().expect("available room versions should be an object");

        assert_eq!(capability["default"], DEFAULT_ROOM_VERSION);
        // Only creatable versions appear in the client capability list.
        // v12/v13 are parse-only → must NOT be advertised.
        let expected_creatable = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"];
        assert_eq!(available.len(), expected_creatable.len());

        for supported in SUPPORTED_ROOM_VERSIONS {
            if supported.can_create {
                assert_eq!(
                    available.get(supported.version).and_then(|value| value.as_str()),
                    Some(supported.disposition_str())
                );
            } else {
                assert!(
                    available.get(supported.version).is_none(),
                    "v{} should NOT appear in client room_versions.available",
                    supported.version
                );
            }
        }
    }

    #[test]
    fn federation_room_versions_capability_matches_supported_matrix() {
        let capability = federation_room_versions_capability();
        let available = capability.as_object().expect("federation room versions should be an object");

        assert_eq!(available.len(), SUPPORTED_ROOM_VERSIONS.len());

        for supported in SUPPORTED_ROOM_VERSIONS {
            assert_eq!(
                available.get(supported.version).and_then(|value| value.get("status")).and_then(|value| value.as_str()),
                Some(supported.disposition_str())
            );
        }
    }
}
