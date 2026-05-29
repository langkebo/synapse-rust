use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomVersionCapability {
    pub version: &'static str,
    pub disposition: &'static str,
}

pub const DEFAULT_ROOM_VERSION: &str = "10";

pub const SUPPORTED_ROOM_VERSIONS: &[RoomVersionCapability] = &[
    RoomVersionCapability { version: "1", disposition: "stable" },
    RoomVersionCapability { version: "2", disposition: "stable" },
    RoomVersionCapability { version: "3", disposition: "stable" },
    RoomVersionCapability { version: "4", disposition: "stable" },
    RoomVersionCapability { version: "5", disposition: "stable" },
    RoomVersionCapability { version: "6", disposition: "stable" },
    RoomVersionCapability { version: "7", disposition: "stable" },
    RoomVersionCapability { version: "8", disposition: "stable" },
    RoomVersionCapability { version: "9", disposition: "stable" },
    RoomVersionCapability { version: "10", disposition: "stable" },
    RoomVersionCapability { version: "11", disposition: "stable" },
];

pub fn is_supported_room_version(version: &str) -> bool {
    SUPPORTED_ROOM_VERSIONS
        .iter()
        .any(|capability| capability.version == version)
}

pub fn client_room_versions_capability() -> Value {
    let mut available = serde_json::Map::new();

    for capability in SUPPORTED_ROOM_VERSIONS {
        available.insert(capability.version.to_string(), json!(capability.disposition));
    }

    json!({
        "default": DEFAULT_ROOM_VERSION,
        "available": available
    })
}

pub fn federation_room_versions_capability() -> Value {
    let mut available = serde_json::Map::new();

    for capability in SUPPORTED_ROOM_VERSIONS {
        available.insert(
            capability.version.to_string(),
            json!({ "status": capability.disposition }),
        );
    }

    Value::Object(available)
}

#[cfg(test)]
mod tests {
    use super::{
        client_room_versions_capability, federation_room_versions_capability,
        is_supported_room_version, DEFAULT_ROOM_VERSION, SUPPORTED_ROOM_VERSIONS,
    };

    #[test]
    fn default_room_version_is_advertised_as_supported() {
        assert!(is_supported_room_version(DEFAULT_ROOM_VERSION));
    }

    #[test]
    fn client_room_versions_capability_matches_supported_matrix() {
        let capability = client_room_versions_capability();
        let available = capability["available"]
            .as_object()
            .expect("available room versions should be an object");

        assert_eq!(capability["default"], DEFAULT_ROOM_VERSION);
        assert_eq!(available.len(), SUPPORTED_ROOM_VERSIONS.len());

        for supported in SUPPORTED_ROOM_VERSIONS {
            assert_eq!(
                available
                    .get(supported.version)
                    .and_then(|value| value.as_str()),
                Some(supported.disposition)
            );
        }
    }

    #[test]
    fn federation_room_versions_capability_matches_supported_matrix() {
        let capability = federation_room_versions_capability();
        let available = capability
            .as_object()
            .expect("federation room versions should be an object");

        assert_eq!(available.len(), SUPPORTED_ROOM_VERSIONS.len());

        for supported in SUPPORTED_ROOM_VERSIONS {
            assert_eq!(
                available
                    .get(supported.version)
                    .and_then(|value| value.get("status"))
                    .and_then(|value| value.as_str()),
                Some(supported.disposition)
            );
        }
    }
}
