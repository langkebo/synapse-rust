//! Stable JSON artefact representation of
//! [`route_ledger::RouteLedger`] for offline consumers
//! (the `synapse_ledger_export` binary, the SDK contract-sync pipeline).
//!
//! This module owns the types and render function; the binary is a thin
//! CLI around [`build_artifact`] and [`render`]. Keeping the data layer
//! in the library means integration tests can round-trip artefacts
//! without spawning a subprocess.
//!
//! Schema is documented in
//! `docs/synapse-rust/LEDGER_EXPORT_SCHEMA.md` and frozen at
//! [`SCHEMA_VERSION`] = `"1"`. See
//! `matrix-js-sdk/docs/api-contract/LEDGER_DRIVEN_SDK_PLAN_2026-05-02.md`
//! for the downstream consumer.

use serde::{Deserialize, Serialize};

use super::{declared_route_manifest_for_profile, ProfileFlags};

/// Frozen JSON schema version. Breaking existing keys bumps MAJOR;
/// additive optional fields bump MINOR.
pub const SCHEMA_VERSION: &str = "2";

/// Top-level artefact shape. Serialised key order matches declaration
/// order here; `serde_json`'s `PrettyFormatter` respects that.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerArtifact {
    /// The `schema_version` field.
    pub schema_version: String,
    /// The `generated_at` field.
    pub generated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `synapse_rust_commit` field.
    pub synapse_rust_commit: Option<String>,
    /// The `state_profile` field.
    pub state_profile: String,
    /// The `profile_flags` field.
    pub profile_flags: ProfileFlagsJson,
    /// The `entry_count` field.
    pub entry_count: usize,
    /// The `entries` field.
    pub entries: Vec<LedgerEntryJson>,
}

/// The `ProfileFlagsJson` struct.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileFlagsJson {
    /// The `oidc_enabled` field.
    pub oidc_enabled: bool,
    /// The `worker_enabled` field.
    pub worker_enabled: bool,
    /// The `saml_enabled` field.
    pub saml_enabled: bool,
}

impl From<&ProfileFlags> for ProfileFlagsJson {
    fn from(f: &ProfileFlags) -> Self {
        Self { oidc_enabled: f.oidc_enabled, worker_enabled: f.worker_enabled, saml_enabled: f.saml_enabled }
    }
}

/// The `LedgerEntryJson` struct (schema v2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerEntryJson {
    /// HTTP method (e.g. "GET", "POST").
    pub method: String,
    /// Absolute HTTP path (e.g. "/_matrix/client/v3/rooms/{room_id}/keys").
    pub path: String,
    /// Name of the router module that registers this route.
    pub registered_by: String,
    /// Extracted `{name}` captures from the path, in declaration order.
    pub path_params: Vec<String>,
    /// Optional query parameters recognized by this endpoint.
    #[serde(default)]
    pub query_params: Vec<String>,
    /// Optional auth requirement: "user", "admin", "optional", "federation", or "none".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    // ---------- 新增字段（B-1 / B-5）----------
    /// Functional module name (rooms, search, friends, push, rendezvous...).
    /// Defaults to `registered_by`; SDK codegen groups routes by this field.
    /// Omitted when equal to `registered_by` to keep JSON compact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Lifecycle status. `Stable` → omitted; `Deprecated` / `Removed` → serialized.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<LedgerEntryStatusJson>,
}

/// Serialized form of [`super::route_ledger::RouteStatus`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerEntryStatusJson {
    /// `"deprecated"` or `"removed"`.
    pub state: String,
    /// Recommended replacement absolute path (only for `deprecated`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    /// Planned sunset time, ISO 8601 or quarter string (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sunset_at: Option<String>,
}

/// Named profile presets recognised by the exporter. Centralised here
/// so both the binary CLI and consumer tests agree on the set.
pub fn profile_for_name(name: &str) -> Option<ProfileFlags> {
    let saml_enabled = cfg!(feature = "saml-sso");

    match name {
        "default" => Some(ProfileFlags::DEFAULT),
        "oidc" => Some(ProfileFlags { oidc_enabled: true, ..ProfileFlags::DEFAULT }),
        "worker" => Some(ProfileFlags { worker_enabled: true, ..ProfileFlags::DEFAULT }),
        "saml" => Some(ProfileFlags { saml_enabled, oidc_enabled: true, ..ProfileFlags::DEFAULT }),
        "all" => Some(ProfileFlags { oidc_enabled: true, worker_enabled: true, saml_enabled }),
        _ => None,
    }
}

/// Extract `{name}` captures from an Axum path, in declaration order.
/// Defensive against unbalanced braces (never panics).
pub fn extract_path_params(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end) = path[i + 1..].find('}') {
                let name = &path[i + 1..i + 1 + end];
                out.push(name.to_string());
                i = i + 1 + end + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Build a deterministic artefact from a profile-driven manifest.
/// Entries sorted by `(path, method, registered_by)` so byte-diffs
/// track semantic changes.
pub fn build_artifact(
    profile_name: &str,
    flags: &ProfileFlags,
    synapse_rust_commit: Option<String>,
    generated_at: String,
) -> LedgerArtifact {
    use super::route_ledger::RouteStatus;

    let ledger = declared_route_manifest_for_profile(flags);
    let mut entries: Vec<LedgerEntryJson> = ledger
        .iter()
        .map(|e| {
            // module: 默认等于 registered_by；只在与 registered_by 不同时才填充
            let module_opt = if e.module == e.registered_by {
                None
            } else {
                Some(e.module.to_string())
            };

            // status: Stable → None; Deprecated → LedgerEntryStatusJson
            let status_opt = match e.status {
                RouteStatus::Stable => None,
                RouteStatus::Deprecated { replacement, sunset_at } => {
                    Some(LedgerEntryStatusJson {
                        state: "deprecated".to_string(),
                        replacement: Some(replacement.to_string()),
                        sunset_at: sunset_at.map(String::from),
                    })
                }
                RouteStatus::Removed => Some(LedgerEntryStatusJson {
                    state: "removed".to_string(),
                    replacement: None,
                    sunset_at: None,
                }),
            };

            LedgerEntryJson {
                method: e.method.as_str().to_string(),
                path: e.path.to_string(),
                registered_by: e.registered_by.to_string(),
                path_params: extract_path_params(e.path),
                query_params: e.query_params.iter().map(|s| s.to_string()).collect(),
                auth: e.auth.map(|s| s.to_string()),
                module: module_opt,
                status: status_opt,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        a.path.cmp(&b.path).then_with(|| a.method.cmp(&b.method)).then_with(|| a.registered_by.cmp(&b.registered_by))
    });
    LedgerArtifact {
        schema_version: SCHEMA_VERSION.to_string(),
        generated_at,
        synapse_rust_commit,
        state_profile: profile_name.to_string(),
        profile_flags: ProfileFlagsJson::from(flags),
        entry_count: entries.len(),
        entries,
    }
}

/// Pretty-print with two-space indent and a trailing newline. Output is
/// byte-stable across runs given identical input.
pub fn render(artifact: &LedgerArtifact) -> String {
    let buf = Vec::with_capacity(4096);
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(buf, formatter);
    match artifact.serialize(&mut ser) {
        Ok(()) => {
            let mut s = String::from_utf8(ser.into_inner()).unwrap_or_else(|e| {
                tracing::error!("LedgerArtifact produced non-UTF-8 JSON: {}", e);
                String::from("{}\n")
            });
            s.push('\n');
            s
        }
        Err(e) => {
            tracing::error!("Failed to serialize LedgerArtifact: {}", e);
            "{}\n".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_param_extraction_happy_path() {
        assert_eq!(extract_path_params("/"), Vec::<String>::new());
        assert_eq!(
            extract_path_params("/rooms/{room_id}/keys/{session_id}"),
            vec!["room_id".to_string(), "session_id".to_string()]
        );
    }

    #[test]
    fn path_param_extraction_handles_unbalanced_brace() {
        assert_eq!(extract_path_params("/foo/{unterminated"), Vec::<String>::new());
    }

    #[test]
    fn profile_default_builds_artefact() {
        let artifact =
            build_artifact("default", &ProfileFlags::DEFAULT, Some("deadbeef".into()), "2026-05-02T00:00:00Z".into());
        assert_eq!(artifact.schema_version, SCHEMA_VERSION);
        assert_eq!(artifact.state_profile, "default");
        assert_eq!(artifact.entry_count, artifact.entries.len());
        assert!(artifact.entry_count > 100, "expected substantial ledger; got {}", artifact.entry_count);
    }

    #[test]
    fn worker_profile_strictly_supersets_default() {
        let default = build_artifact("default", &ProfileFlags::DEFAULT, None, "2026-05-02T00:00:00Z".into());
        let worker = build_artifact(
            "worker",
            &ProfileFlags { worker_enabled: true, ..ProfileFlags::DEFAULT },
            None,
            "2026-05-02T00:00:00Z".into(),
        );
        assert!(worker.entry_count > default.entry_count);
        let heartbeat = "/_synapse/worker/v1/workers/{worker_id}/heartbeat";
        assert!(worker.entries.iter().any(|e| e.path == heartbeat));
        assert!(!default.entries.iter().any(|e| e.path == heartbeat));
    }

    #[test]
    fn render_roundtrips_byte_stable() {
        let artifact =
            build_artifact("default", &ProfileFlags::DEFAULT, Some("aaaaaaaa".into()), "2026-05-02T00:00:00Z".into());
        let once = render(&artifact);
        let reparsed: LedgerArtifact = serde_json::from_str(&once).unwrap();
        assert_eq!(reparsed, artifact);
        let twice = render(&reparsed);
        assert_eq!(once, twice);
    }

    #[test]
    fn entries_sorted_path_method_registered() {
        let artifact = build_artifact("all", &profile_for_name("all").unwrap(), None, "2026-05-02T00:00:00Z".into());
        let mut prev: Option<(&str, &str, &str)> = None;
        for e in &artifact.entries {
            let cur = (e.path.as_str(), e.method.as_str(), e.registered_by.as_str());
            if let Some(p) = prev {
                assert!(p <= cur, "sort violated: {p:?} before {cur:?}");
            }
            prev = Some(cur);
        }
    }

    #[test]
    fn unknown_profile_returns_none() {
        assert!(profile_for_name("this-does-not-exist").is_none());
    }

    #[test]
    fn profile_flags_respect_compiled_features() {
        let saml = profile_for_name("saml").expect("saml profile should exist");
        let all = profile_for_name("all").expect("all profile should exist");

        assert_eq!(saml.saml_enabled, cfg!(feature = "saml-sso"));
        assert_eq!(all.saml_enabled, cfg!(feature = "saml-sso"));
        assert!(all.oidc_enabled);
        assert!(all.worker_enabled);
    }
}
