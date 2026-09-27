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

    /// Dispositions the str.
    pub const fn disposition_str(self) -> &'static str {
        self.disposition.as_str()
    }
}

/// Constant `DEFAULT_ROOM_VERSION`.
///
/// Changed to "12" in O-1 Phase 2 after enabling v12 room creation in Phase 1.
/// This matches upstream Synapse v1.162.0rc1 which raised the default to "12".
/// Room version 12 is defined by **MSC4304** (base v11 + MSC4289 creator
/// privilege + MSC4291 room IDs as hashes of the create event + MSC4297 state
/// resolution v2.1 + MSC4307 `auth_events` room check).  Do NOT cite MSC4239
/// here: that MSC is the *room version 11* release, which made **v11** the
/// default; the two were conflated in this comment until 2026-09-26.
///
/// Consequences to keep in mind when reviewing federation behaviour:
/// version 12 requires ED25519-only signatures and complete PDU fields
/// (depth, prev_events, auth_events). Remote servers without v12 support
/// cannot join rooms created here.
pub const DEFAULT_ROOM_VERSION: &str = "12";

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
    // v12（MSC4304）= room v11 + MSC4289（创建者特权）+ MSC4291（room ID = create 事件 id）
    // + MSC4297（State Resolution v2.1）+ MSC4307（`auth_events` 同房校验）。
    //
    // 现状（2026-09-27，逐项见 `docs/audit/ROOM_V12_PLAN_STATUS_2026-09-27.md`）：
    // MSC4291 创建侧（C-1/C-2）与无域名 room id 语法/DB CHECK（C-3）、MSC4307 规则 3.5（B-2）、
    // v12 的 `auth_events` 不含 create（D-4）、入站 create 形态（D-1）均已落地；
    // **MSC4289（E 组）与 MSC4297（F 组）仍未完成**，所以 v12 目前仍属"声明领先实现"。
    //
    // `"13"` **不再列入**：上游规范稳定列表止于 v12（`content/rooms/_index.md`），
    // 上游 Synapse 1.161.0 的 `KNOWN_ROOM_VERSIONS` 只识别 `1..12` + 三个 unstable
    // （`org.matrix.hydra.11`、`org.matrix.msc3757.10/11`）。曾以 `stable_parse_only("13")`
    // 占位，但 `redaction_rules("13")` / `uses_reference_hash_event_id("13")` 都返回
    // fail-closed 的否定答案 ⇒ "可 parse/join/federate" 是假声明（G-50）。按裁定 Q5(b) 移除。
    RoomVersionCapability::stable("11"),
    RoomVersionCapability::stable("12"),
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

/// Whether `version` is numerically at least `minimum`.
///
/// Room versions are numeric identifiers (`"1"` … `"13"`), so they must **never**
/// be compared as strings: lexicographically `"2" >= "12"` and `"9" >= "12"` are
/// both true, which silently routes every version below 10 into a "12+" branch.
/// That exact bug shipped in `MessagingService::create_event`, where it made
/// rooms v2–v9 take the v12 PDU-graph path.
///
/// A version that does not parse as a number (an unknown or experimental
/// identifier) is treated as **not** reaching the minimum — failing closed
/// rather than granting a newer behaviour we cannot order.
///
/// This is the single comparison helper: do not re-derive it at call sites.
pub fn room_version_at_least(version: &str, minimum: u32) -> bool {
    version.parse::<u32>().map(|parsed| parsed >= minimum).unwrap_or(false)
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
    fn default_room_version_is_12() {
        // Pinned to a literal ON PURPOSE (not `DEFAULT_ROOM_VERSION`) so that
        // changing the constant is a deliberate, auditable act.
        // Changed from "11" to "12" in O-1 Phase 2 to match upstream Synapse v1.162.0rc1.
        assert_eq!(DEFAULT_ROOM_VERSION, "12");
        assert_eq!(resolve_room_version(None), Some("12"));
        // Every room-version surface must agree on the same literal.
        let capability = client_room_versions_capability();
        assert_eq!(capability["default"], "12");
    }

    #[test]
    fn resolve_room_version_defaults_and_rejects_unknown_versions() {
        assert_eq!(resolve_room_version(None), Some(DEFAULT_ROOM_VERSION));
        assert_eq!(resolve_room_version(Some("10")), Some("10"));
        // v11 is fully creatable after the redaction chain (P0-05/06/09)
        // and state resolution v2 (P0-10/11) landed.
        assert_eq!(resolve_room_version(Some("11")), Some("11"));
        // v12 is creatable after O-1 Phase 1 (PDU fields + ED25519-only).
        assert_eq!(resolve_room_version(Some("12")), Some("12"));
        // v13 is not a room version at all (Q5): unsupported, so not creatable.
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
        // v1-v12 are fully creatable after the redaction chain and state
        // resolution v2 landed. v12 support was added in O-1 Phase 1.
        for v in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"] {
            assert!(can_create_room_version(v), "v{v} must remain creatable");
        }
        // v12 is creatable; `"13"` is not a room version at all (Q5), so every
        // capability for it is false — including join/federate, which used to be
        // claimed by the removed `stable_parse_only("13")` placeholder.
        assert!(can_create_room_version("12"), "v12 must be creatable");
        assert!(!can_create_room_version("13"), "v13 must NOT be creatable");
        assert!(!can_join_room_version("13"), "v13 does not exist, so it cannot be joined");
        assert!(!can_parse_room_version("13") && !can_federate_room_version("13"));
        assert!(!is_supported_room_version("13"));
        assert!(can_join_room_version("12"));
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
        // Only creatable versions appear in the client capability list. v1-v12 are
        // creatable today (G-1 will narrow this to v12 alone, decision Q1(a));
        // v13 is not a room version and appears nowhere.
        let expected_creatable = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"];
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
