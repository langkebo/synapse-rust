#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Guards the **content-fingerprint sidecar** of the SDK-lane ledger fixtures.
//!
//! ## Why this file exists
//!
//! `tests/unit/fixtures/ledger_export_sdk/*.json` are byte-stable goldens, so
//! their `synapse_rust_commit` can only be a zero placeholder and their
//! `generated_at` a fixed sentinel. That left the cross-repo contract binding
//! anchored to *"the sibling checkout's HEAD when the mirror was regenerated"* —
//! a **working-tree position**, not the ledger's **content**
//! (2026-10-13 audit §2 B1 / §4 X2).
//!
//! `manifest.json` (written by `src/bin/synapse_ledger_fingerprint.rs`) carries
//! the content half of that binding: a sha256 over the canonical projection of
//! every entry, re-derivable by any consumer. These tests are the **机械判据**
//! for the claim that it is still accurate:
//!
//! 1. [`sidecar_fingerprints_match_the_committed_fixtures`] — the committed
//!    sidecar must equal what the code derives *now* (so editing a fixture
//!    without refreshing the sidecar fails here, not in some downstream repo);
//! 2. the three `fingerprint_discriminates_*` tests — the digest must actually
//!    depend on `method`, on `entries` order, and on `query_params`; without
//!    them a constant-returning implementation would pass check 1 vacuously;
//! 3. [`sidecar_carries_no_volatile_fields`] — commit/timestamp must stay out,
//!    otherwise the sidecar reintroduces the position-bound coupling it replaced.

use std::path::PathBuf;

use serde_json::Value;
use synapse_web::routes::ledger_export::{route_surface_sha256, LedgerArtifact, LedgerEntryJson};

/// The SDK lane always ships exactly these three profiles.
const PROFILES: [&str; 3] = ["default", "worker", "all"];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/unit/fixtures/ledger_export_sdk")
}

fn read_artifact(profile: &str) -> LedgerArtifact {
    let path = fixtures_dir().join(format!("{profile}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not a ledger artefact: {e}", path.display()))
}

fn read_manifest() -> Value {
    let path = fixtures_dir().join("manifest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\n\
             Regenerate fixtures + sidecar with:\n    \
             bash scripts/generate_sdk_ledger_fixtures.sh",
            path.display()
        )
    });
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()))
}

#[test]
fn sidecar_fingerprints_match_the_committed_fixtures() {
    let manifest = read_manifest();
    for profile in PROFILES {
        let artifact = read_artifact(profile);

        let recorded = manifest["profiles"][profile]["route_surface_sha256"]
            .as_str()
            .unwrap_or_else(|| panic!("manifest lacks profiles.{profile}.route_surface_sha256"));
        assert_eq!(
            recorded,
            route_surface_sha256(&artifact.entries),
            "sidecar fingerprint for '{profile}' is stale — the fixture changed without \
             regenerating the sidecar. Re-run:\n    bash scripts/generate_sdk_ledger_fixtures.sh"
        );

        let recorded_count = manifest["profiles"][profile]["entry_count"]
            .as_u64()
            .unwrap_or_else(|| panic!("manifest lacks profiles.{profile}.entry_count"));
        assert_eq!(
            recorded_count as usize, artifact.entry_count,
            "sidecar entry_count for '{profile}' disagrees with the fixture"
        );

        assert_eq!(
            manifest["ledger_schema_version"].as_str().unwrap_or_default(),
            artifact.schema_version,
            "sidecar ledger_schema_version disagrees with fixture '{profile}' — consumers use \
             it to refuse comparisons across incompatible ledger schemas"
        );
    }
}

#[test]
fn fingerprint_discriminates_method_changes() {
    let artifact = read_artifact("all");
    let before = route_surface_sha256(&artifact.entries);

    let mut mutated: Vec<LedgerEntryJson> = artifact.entries;
    let first = mutated.first_mut().expect("fixture must carry entries");
    first.method = if first.method == "GET" { "POST".to_string() } else { "GET".to_string() };

    assert_ne!(
        before,
        route_surface_sha256(&mutated),
        "fingerprint ignored a `method` change — it would not detect wire-level drift"
    );
}

#[test]
fn fingerprint_discriminates_entry_order() {
    let artifact = read_artifact("all");
    assert!(artifact.entries.len() >= 2, "fixture must carry at least two entries");
    let before = route_surface_sha256(&artifact.entries);

    let mut reordered = artifact.entries;
    reordered.swap(0, 1);

    assert_ne!(
        before,
        route_surface_sha256(&reordered),
        "fingerprint ignored entry order — a reordering would silently keep the same digest"
    );
}

#[test]
fn fingerprint_discriminates_query_params() {
    let artifact = read_artifact("all");
    let idx = artifact
        .entries
        .iter()
        .position(|e| !e.query_params.is_empty())
        .expect("the 'all' profile must contain at least one entry with query params");
    let before = route_surface_sha256(&artifact.entries);

    let mut mutated = artifact.entries;
    mutated[idx].query_params.push("__probe__".to_string());

    assert_ne!(
        before,
        route_surface_sha256(&mutated),
        "fingerprint ignored query_params — the SDK's wire-level keys are part of the contract"
    );
}

#[test]
fn sidecar_carries_no_volatile_fields() {
    let text = read_manifest().to_string();
    for forbidden in ["synapse_rust_commit", "generated_at", "pinned_at"] {
        assert!(
            !text.contains(forbidden),
            "the sidecar must not carry the volatile field '{forbidden}' — that is exactly the \
             position-bound coupling it replaces (2026-10-13 audit §2 B1)"
        );
    }
}
