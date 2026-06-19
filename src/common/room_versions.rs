use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoomVersionDisposition {
    Stable,
    Unstable,
}

impl RoomVersionDisposition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Unstable => "unstable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomVersionCapability {
    pub version: &'static str,
    pub disposition: RoomVersionDisposition,
    pub can_create: bool,
    pub can_join: bool,
    pub can_parse: bool,
    pub can_federate: bool,
}

impl RoomVersionCapability {
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

    pub const fn disposition_str(self) -> &'static str {
        self.disposition.as_str()
    }
}

pub const DEFAULT_ROOM_VERSION: &str = "10";

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
    // (which grants self-redact for room versions >= 11), so these versions
    // can be advertised as creatable.
    RoomVersionCapability::stable("11"),
    RoomVersionCapability::stable("12"),
    RoomVersionCapability::stable("13"),
];

pub fn is_supported_room_version(version: &str) -> bool {
    SUPPORTED_ROOM_VERSIONS.iter().any(|capability| capability.version == version)
}

pub fn can_create_room_version(version: &str) -> bool {
    SUPPORTED_ROOM_VERSIONS.iter().any(|capability| capability.version == version && capability.can_create)
}

pub fn can_join_room_version(version: &str) -> bool {
    SUPPORTED_ROOM_VERSIONS.iter().any(|capability| capability.version == version && capability.can_join)
}

pub fn can_parse_room_version(version: &str) -> bool {
    SUPPORTED_ROOM_VERSIONS.iter().any(|capability| capability.version == version && capability.can_parse)
}

pub fn can_federate_room_version(version: &str) -> bool {
    SUPPORTED_ROOM_VERSIONS.iter().any(|capability| capability.version == version && capability.can_federate)
}

pub fn resolve_room_version(requested: Option<&str>) -> Option<&'static str> {
    let requested = requested.unwrap_or(DEFAULT_ROOM_VERSION);

    SUPPORTED_ROOM_VERSIONS
        .iter()
        .find(|capability| capability.version == requested && capability.can_create)
        .map(|capability| capability.version)
}

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
        resolve_room_version, RoomVersionCapability, RoomVersionDisposition, DEFAULT_ROOM_VERSION,
        SUPPORTED_ROOM_VERSIONS,
    };

    #[test]
    fn default_room_version_is_advertised_as_supported() {
        assert!(is_supported_room_version(DEFAULT_ROOM_VERSION));
    }

    #[test]
    fn resolve_room_version_defaults_and_rejects_unknown_versions() {
        assert_eq!(resolve_room_version(None), Some(DEFAULT_ROOM_VERSION));
        assert_eq!(resolve_room_version(Some("10")), Some("10"));
        // v11+ are now fully creatable after the redaction chain (P0-05/06/09)
        // and state resolution v2 (P0-10/11) landed.
        assert_eq!(resolve_room_version(Some("11")), Some("11"));
        assert_eq!(resolve_room_version(Some("12")), Some("12"));
        assert_eq!(resolve_room_version(Some("13")), Some("13"));
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
            // All supported versions (v1-v13) are now fully creatable after
            // the redaction chain and state resolution v2 landed.
            assert!(can_create_room_version(supported.version));
        }
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
        let creatable_count = SUPPORTED_ROOM_VERSIONS.iter().filter(|c| c.can_create).count();
        assert_eq!(available.len(), creatable_count);

        for supported in SUPPORTED_ROOM_VERSIONS {
            if supported.can_create {
                assert_eq!(
                    available.get(supported.version).and_then(|value| value.as_str()),
                    Some(supported.disposition_str())
                );
            } else {
                assert!(available.get(supported.version).is_none());
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

    #[test]
    fn test_room_version_disposition_as_str() {
        assert_eq!(RoomVersionDisposition::Stable.as_str(), "stable");
        assert_eq!(RoomVersionDisposition::Unstable.as_str(), "unstable");
    }

    #[test]
    fn test_room_version_capability_stable_constructor() {
        let cap = RoomVersionCapability::stable("1");
        assert_eq!(cap.version, "1");
        assert_eq!(cap.disposition, RoomVersionDisposition::Stable);
        assert!(cap.can_create);
        assert!(cap.can_join);
        assert!(cap.can_parse);
        assert!(cap.can_federate);
    }

    #[test]
    fn test_room_version_capability_stable_parse_only_constructor() {
        let cap = RoomVersionCapability::stable_parse_only("11");
        assert_eq!(cap.version, "11");
        assert_eq!(cap.disposition, RoomVersionDisposition::Stable);
        assert!(!cap.can_create);
        assert!(cap.can_join);
        assert!(cap.can_parse);
        assert!(cap.can_federate);
    }

    #[test]
    fn test_room_version_capability_disposition_str() {
        let stable = RoomVersionCapability::stable("1");
        assert_eq!(stable.disposition_str(), "stable");
    }

    #[test]
    fn test_is_supported_room_version_rejects_unknown() {
        assert!(!is_supported_room_version("0"));
        assert!(!is_supported_room_version("14"));
        assert!(!is_supported_room_version("99"));
        assert!(!is_supported_room_version(""));
    }

    #[test]
    fn test_can_create_room_version_edge_cases() {
        assert!(can_create_room_version("1"));
        assert!(can_create_room_version("10"));
        // v11+ are now creatable after redaction chain and state resolution v2 landed.
        assert!(can_create_room_version("11"));
        assert!(can_create_room_version("13"));
        assert!(!can_create_room_version("14"));
        assert!(!can_create_room_version(""));
    }

    #[test]
    fn test_can_join_room_version_edge_cases() {
        assert!(can_join_room_version("1"));
        assert!(can_join_room_version("13"));
        assert!(!can_join_room_version("14"));
        assert!(!can_join_room_version(""));
    }

    #[test]
    fn test_can_parse_room_version_edge_cases() {
        assert!(can_parse_room_version("1"));
        assert!(can_parse_room_version("13"));
        assert!(!can_parse_room_version("14"));
    }

    #[test]
    fn test_can_federate_room_version_edge_cases() {
        assert!(can_federate_room_version("1"));
        assert!(can_federate_room_version("13"));
        assert!(!can_federate_room_version("14"));
    }

    #[test]
    fn test_resolve_room_version_explicit_default() {
        assert_eq!(resolve_room_version(Some(DEFAULT_ROOM_VERSION)), Some(DEFAULT_ROOM_VERSION));
    }

    #[test]
    fn test_client_room_versions_capability_structure() {
        let capability = client_room_versions_capability();
        assert!(capability.get("default").is_some());
        assert!(capability.get("available").is_some());
        assert!(capability["available"].is_object());
    }

    #[test]
    fn test_federation_room_versions_capability_structure() {
        let capability = federation_room_versions_capability();
        assert!(capability.is_object());
        // Each entry should have a "status" field
        for (_, value) in capability.as_object().unwrap() {
            assert!(value.get("status").is_some());
        }
    }
}
