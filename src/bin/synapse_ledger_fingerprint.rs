//! Writes the **content-fingerprint sidecar** for the SDK-lane ledger fixtures.
//!
//! ## Why the sidecar exists
//!
//! See `route_surface_sha256` in `synapse_web::routes::ledger_export` for the
//! long version. Short version: the fixtures under
//! `tests/unit/fixtures/ledger_export_sdk/` are **byte-stable goldens**, so they
//! can only carry a zero placeholder for `synapse_rust_commit` and a fixed
//! sentinel for `generated_at`. That left the cross-repo contract binding
//! anchored to *"the sibling checkout's HEAD at the moment the mirror was
//! regenerated"* — a working-tree position, not the ledger's content
//! (2026-10-13 audit §2 B1 / §4 X2).
//!
//! The sidecar carries the **content** half of that binding instead: a sha256
//! over the canonical projection of every entry. Any consumer re-derives the
//! same value from the fixture it ingested — no access to this repository, no
//! dependence on commit history.
//!
//! It **deliberately carries no commit and no timestamp**: both are volatile by
//! nature, and mixing them in would recreate exactly the position-bound problem
//! this file exists to remove.
//!
//! ## Usage
//!
//! ```text
//! cargo run --quiet --bin synapse_ledger_fingerprint -- \
//!     --dir=tests/unit/fixtures/ledger_export_sdk
//! ```
//!
//! Writes `<dir>/manifest.json`. Needs no feature flags (it only reads and
//! re-hashes committed fixtures).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use synapse_web::routes::ledger_export::{route_surface_sha256, LedgerArtifact};

/// Sidecar schema version — independent of the ledger's own `SCHEMA_VERSION`
/// (this file's shape can change without touching the ledger contract).
const SIDECAR_SCHEMA_VERSION: &str = "1";

/// The SDK lane always ships exactly these three profiles.
const PROFILES: [&str; 3] = ["default", "worker", "all"];

/// Human-readable statement of the fingerprint preimage. Kept in the artefact
/// itself so a consumer can re-implement it from the file alone; the normative
/// implementation is `entry_fingerprint_line` in `ledger_export.rs`.
const ALGORITHM: &str = "sha256 over one line per entry, in `entries` order: \
method \\t path \\t registered_by \\t path_params(comma-joined) \\t \
query_params(comma-joined) \\t auth (empty string when absent); each line is \
newline-terminated, concatenated with no separator";

#[derive(Serialize)]
struct ProfileFingerprint {
    entry_count: usize,
    route_surface_sha256: String,
}

#[derive(Serialize)]
struct FingerprintManifest {
    schema_version: &'static str,
    fingerprint_algorithm: &'static str,
    /// Mirrors `LedgerArtifact::schema_version` so a consumer can refuse to
    /// compare fingerprints across incompatible ledger schemas.
    ledger_schema_version: String,
    profiles: BTreeMap<String, ProfileFingerprint>,
}

fn parse_args(argv: &[String]) -> Result<PathBuf, String> {
    let mut dir: Option<PathBuf> = None;
    for arg in argv {
        match arg.strip_prefix("--dir=") {
            Some(value) if !value.is_empty() => dir = Some(PathBuf::from(value)),
            _ => return Err(format!("unrecognised argument: {arg}")),
        }
    }
    dir.ok_or_else(|| "missing required --dir=<fixture directory>".to_string())
}

fn read_artifact(dir: &Path, profile: &str) -> Result<LedgerArtifact, String> {
    let path = dir.join(format!("{profile}.json"));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let artifact: LedgerArtifact =
        serde_json::from_str(&text).map_err(|e| format!("{} is not a ledger artefact: {e}", path.display()))?;
    if artifact.entry_count != artifact.entries.len() {
        return Err(format!(
            "{} declares entry_count={} but carries {} entries",
            path.display(),
            artifact.entry_count,
            artifact.entries.len()
        ));
    }
    Ok(artifact)
}

fn main() -> ExitCode {
    synapse_rust::common::crypto::ensure_token_hash_secret_or_exit("synapse_ledger_fingerprint");
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let dir = match parse_args(&argv) {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("synapse_ledger_fingerprint: {err}");
            eprintln!("usage: synapse_ledger_fingerprint --dir=<fixture directory>");
            return ExitCode::from(2);
        }
    };

    let mut profiles: BTreeMap<String, ProfileFingerprint> = BTreeMap::new();
    let mut ledger_schema_version: Option<String> = None;

    for profile in PROFILES {
        let artifact = match read_artifact(&dir, profile) {
            Ok(artifact) => artifact,
            Err(err) => {
                eprintln!("synapse_ledger_fingerprint: {err}");
                return ExitCode::FAILURE;
            }
        };
        match &ledger_schema_version {
            None => ledger_schema_version = Some(artifact.schema_version.clone()),
            Some(seen) if seen != &artifact.schema_version => {
                eprintln!(
                    "synapse_ledger_fingerprint: profiles disagree on schema_version \
                     ('{seen}' vs '{}' in {profile}.json); refusing to write a sidecar \
                     that would let consumers compare across incompatible schemas",
                    artifact.schema_version
                );
                return ExitCode::FAILURE;
            }
            Some(_) => {}
        }
        profiles.insert(
            profile.to_string(),
            ProfileFingerprint {
                entry_count: artifact.entry_count,
                route_surface_sha256: route_surface_sha256(&artifact.entries),
            },
        );
    }

    let manifest = FingerprintManifest {
        schema_version: SIDECAR_SCHEMA_VERSION,
        fingerprint_algorithm: ALGORITHM,
        ledger_schema_version: ledger_schema_version.unwrap_or_default(),
        profiles,
    };
    let mut json = match serde_json::to_string_pretty(&manifest) {
        Ok(json) => json,
        Err(err) => {
            eprintln!("synapse_ledger_fingerprint: cannot serialise manifest: {err}");
            return ExitCode::FAILURE;
        }
    };
    json.push('\n');

    let out = dir.join("manifest.json");
    if let Err(err) = std::fs::write(&out, json) {
        eprintln!("synapse_ledger_fingerprint: cannot write {}: {err}", out.display());
        return ExitCode::FAILURE;
    }
    println!("wrote {}", out.display());
    ExitCode::SUCCESS
}
