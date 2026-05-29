//! Golden-file round-trip for [`synapse_rust::web::routes::ledger_export`].
//!
//! Asserts that:
//! - `render(build_artifact(...))` for each committed profile fixture is
//!   byte-identical to the checked-in file.
//! - Re-parsing the rendered output yields an artefact `== artefact` (so the
//!   schema is faithfully round-trippable).
//! - Per-profile invariants (entry counts, worker-body inclusion, ordering)
//!   hold for every fixture.
//!
//! Fixture generation (invoke manually when the ledger genuinely changes):
//! ```
//! cargo run --bin synapse_ledger_export -- \
//!     --profile=default \
//!     --timestamp=2026-05-02T00:00:00Z \
//!     --commit=0000000000000000000000000000000000000000 \
//!     --output=tests/unit/fixtures/ledger_export/default.json
//! ```
//! (plus `worker`, `openclaw`, `all` — see `regenerate_fixtures.sh` if
//! that script ever lands).
//!
//! Note: these checked-in fixtures intentionally reflect the compile-aware
//! default build of the exporter binary. The SDK's full-extension ingest path
//! uses `tests/unit/fixtures/ledger_export_sdk/`, regenerated via
//! `scripts/generate_sdk_ledger_fixtures.sh`.

use synapse_rust::web::routes::ledger_export::{
    build_artifact, profile_for_name, render, LedgerArtifact, SCHEMA_VERSION,
};

/// Fixed timestamp baked into every fixture so the artefact stays
/// byte-stable across runs. Must match the `--timestamp` value used to
/// regenerate the fixtures.
#[allow(dead_code)]
const FIXTURE_TIMESTAMP: &str = "2026-05-02T00:00:00Z";
/// Fixed 40-hex placeholder SHA baked into every fixture. The value is
/// meaningless — the test only asserts round-trip equality.
#[allow(dead_code)]
const FIXTURE_COMMIT: &str = "0000000000000000000000000000000000000000";

#[allow(dead_code)]
fn regen_instructions(profile: &str) -> String {
    format!(
        "\n\nTo regenerate (only if the ledger genuinely changed):\n    \
         cargo run --bin synapse_ledger_export -- \\\n        \
         --profile={profile} \\\n        \
         --timestamp={FIXTURE_TIMESTAMP} \\\n        \
         --commit={FIXTURE_COMMIT} \\\n        \
         --output=tests/unit/fixtures/ledger_export/{profile}.json",
    )
}

#[allow(dead_code)]
fn assert_fixture_matches(profile: &str, fixture_bytes: &[u8]) {
    let flags = profile_for_name(profile).unwrap_or_else(|| panic!("unknown profile in test: {profile}"));
    let artifact = build_artifact(profile, &flags, Some(FIXTURE_COMMIT.to_string()), FIXTURE_TIMESTAMP.to_string());
    let rendered = render(&artifact);

    let fixture = std::str::from_utf8(fixture_bytes)
        .unwrap_or_else(|e| panic!("fixture for profile '{profile}' is not UTF-8: {e}"));

    if rendered != fixture {
        // Produce a compact diff summary instead of dumping two 200 KB blobs.
        let live_parsed: LedgerArtifact = serde_json::from_str(&rendered).unwrap();
        let fixture_parsed: LedgerArtifact = serde_json::from_str(fixture).unwrap();

        let mut summary = String::new();
        summary.push_str(&format!("ledger fixture drift on profile '{profile}'\n"));
        summary.push_str(&format!(
            "  entry_count: live={} fixture={}\n",
            live_parsed.entry_count, fixture_parsed.entry_count
        ));
        summary.push_str(&format!(
            "  schema_version: live={} fixture={}\n",
            live_parsed.schema_version, fixture_parsed.schema_version
        ));
        summary.push_str(&format!(
            "  profile_flags: live={:?} fixture={:?}\n",
            live_parsed.profile_flags, fixture_parsed.profile_flags
        ));

        let live_set: std::collections::BTreeSet<(String, String, String)> =
            live_parsed.entries.iter().map(|e| (e.method.clone(), e.path.clone(), e.registered_by.clone())).collect();
        let fix_set: std::collections::BTreeSet<(String, String, String)> = fixture_parsed
            .entries
            .iter()
            .map(|e| (e.method.clone(), e.path.clone(), e.registered_by.clone()))
            .collect();

        let added: Vec<_> = live_set.difference(&fix_set).collect();
        let removed: Vec<_> = fix_set.difference(&live_set).collect();
        if !added.is_empty() {
            summary.push_str(&format!("  added in live ({} entries):\n", added.len()));
            for (m, p, reg) in added.iter().take(20) {
                summary.push_str(&format!("    + {m} {p} [{reg}]\n"));
            }
            if added.len() > 20 {
                summary.push_str(&format!("    ... {} more\n", added.len() - 20));
            }
        }
        if !removed.is_empty() {
            summary.push_str(&format!("  removed in live ({} entries):\n", removed.len()));
            for (m, p, reg) in removed.iter().take(20) {
                summary.push_str(&format!("    - {m} {p} [{reg}]\n"));
            }
            if removed.len() > 20 {
                summary.push_str(&format!("    ... {} more\n", removed.len() - 20));
            }
        }
        summary.push_str(&regen_instructions(profile));
        panic!("{}", summary);
    }

    // Sanity: re-parsing the rendered output round-trips to an equal artefact.
    let reparsed: LedgerArtifact = serde_json::from_str(&rendered)
        .unwrap_or_else(|e| panic!("rendered output for '{profile}' failed to re-parse: {e}"));
    assert_eq!(reparsed, artifact, "round-trip inequality for profile '{profile}'");
    assert_eq!(reparsed.schema_version, SCHEMA_VERSION);
}

#[test]
#[cfg(not(feature = "all-extensions"))]
fn default_profile_matches_fixture() {
    let fixture = include_bytes!("fixtures/ledger_export/default.json");
    assert_fixture_matches("default", fixture);
}

#[test]
#[cfg(not(feature = "all-extensions"))]
fn worker_profile_matches_fixture() {
    let fixture = include_bytes!("fixtures/ledger_export/worker.json");
    assert_fixture_matches("worker", fixture);
}

#[test]
#[cfg(not(feature = "all-extensions"))]
fn openclaw_profile_matches_fixture() {
    let fixture = include_bytes!("fixtures/ledger_export/openclaw.json");
    assert_fixture_matches("openclaw", fixture);
}

#[test]
#[cfg(not(feature = "all-extensions"))]
fn all_profile_matches_fixture() {
    let fixture = include_bytes!("fixtures/ledger_export/all.json");
    assert_fixture_matches("all", fixture);
}

#[test]
#[cfg(not(feature = "all-extensions"))]
fn worker_fixture_strictly_supersets_default_fixture() {
    let d_bytes = include_bytes!("fixtures/ledger_export/default.json");
    let w_bytes = include_bytes!("fixtures/ledger_export/worker.json");
    let d: LedgerArtifact = serde_json::from_slice(d_bytes).unwrap();
    let w: LedgerArtifact = serde_json::from_slice(w_bytes).unwrap();
    assert!(
        w.entry_count > d.entry_count,
        "worker ({}) must have more entries than default ({})",
        w.entry_count,
        d.entry_count
    );
    let default_set: std::collections::BTreeSet<(String, String)> =
        d.entries.iter().map(|e| (e.method.clone(), e.path.clone())).collect();
    for e in &w.entries {
        if e.registered_by == "worker_body" {
            continue;
        }
        assert!(
            default_set.contains(&(e.method.clone(), e.path.clone())),
            "worker profile dropped a non-worker_body route: {} {} [{}]",
            e.method,
            e.path,
            e.registered_by
        );
    }
}

#[test]
#[cfg(not(feature = "all-extensions"))]
fn all_fixture_contains_every_other_profile_entry() {
    let combos = [
        "fixtures/ledger_export/default.json",
        "fixtures/ledger_export/worker.json",
        "fixtures/ledger_export/openclaw.json",
    ];
    let all: LedgerArtifact = serde_json::from_slice(include_bytes!("fixtures/ledger_export/all.json")).unwrap();
    let all_set: std::collections::BTreeSet<(String, String)> =
        all.entries.iter().map(|e| (e.method.clone(), e.path.clone())).collect();
    for label in combos {
        let bytes: &[u8] = match label {
            "fixtures/ledger_export/default.json" => {
                include_bytes!("fixtures/ledger_export/default.json")
            }
            "fixtures/ledger_export/worker.json" => {
                include_bytes!("fixtures/ledger_export/worker.json")
            }
            "fixtures/ledger_export/openclaw.json" => {
                include_bytes!("fixtures/ledger_export/openclaw.json")
            }
            _ => unreachable!(),
        };
        let art: LedgerArtifact = serde_json::from_slice(bytes).unwrap();
        for e in &art.entries {
            assert!(
                all_set.contains(&(e.method.clone(), e.path.clone())),
                "profile 'all' missing route from {}: {} {}",
                label,
                e.method,
                e.path
            );
        }
    }
}
