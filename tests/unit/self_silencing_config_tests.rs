//! Guards against "silence my own warning" configuration (B1-3).
//!
//! The service used to emit a `tracing::warn!` from the route-manifest
//! validation path on **every boot**, telling operators that ISSUE-13's legacy
//! `/v3` aliases are deprecated. That state is not something an operator can
//! change, so the warning was permanent noise — and it dragged in four
//! artifacts to keep it quiet:
//!
//! * `ServerConfig::suppress_vendor_endpoint_warning` — **never read by any
//!   code path**; the warning site consulted the env var directly instead,
//! * a `SYNAPSE__SERVER__SUPPRESS_VENDOR_ENDPOINT_WARNING` env read,
//! * `docker/config/homeserver.yaml` key,
//! * `docker/deploy/docker-compose.yml` env passthrough.
//!
//! The information was not lost: ISSUE-13 is annotated at every alias
//! declaration site, which is where a maintainer actually reads. These are
//! properties of the source text, so they are guarded statically.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

/// Roots scanned for a reintroduced suppression knob.
const SCAN_ROOTS: [&str; 8] = [
    "src",
    "docker",
    "synapse-common",
    "synapse-services",
    "synapse-storage",
    "synapse-cache",
    "synapse-e2ee",
    "synapse-federation",
];

/// Files that must keep carrying the ISSUE-13 deprecation notice now that the
/// per-boot WARN is gone.
const DEPRECATION_NOTICE_SITES: [(&str, &str); 2] =
    [("src/web/routes/sync.rs", "/my_rooms"), ("src/web/routes/handlers/search/mod.rs", "/search_rooms")];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

/// True for lines that carry executable text (i.e. not a whole-line comment).
///
/// Comments are allowed — and required — to narrate the removal; code is not
/// allowed to reintroduce the knob. Without this split the guard would be
/// satisfied by deleting the explanation instead of keeping the knob out.
fn is_code_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    !(trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with('*')
        || trimmed.starts_with("/*"))
}

/// Collect every `.rs` / `.yaml` / `.yml` file under `dir`, recursively.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name == "target" || name == "node_modules" {
                continue;
            }
            collect(&path, out);
        } else if matches!(path.extension().and_then(|e| e.to_str()), Some("rs" | "yaml" | "yml")) {
            out.push(path);
        }
    }
}

/// A suppression knob for the *service's own* endpoint-deprecation notices.
///
/// Deliberately narrow: the shape is `suppress_…(endpoint|r0|alias)…`, which
/// cannot match an upstream-Synapse option such as
/// `suppress_key_server_warning`.
fn is_endpoint_suppression_knob(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    let Some(rest) = lowered.split("suppress_").nth(1) else { return false };
    let identifier: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    identifier.contains("endpoint") || identifier.contains("r0") || identifier.contains("alias")
}

/// The suppression knobs still present in `source` (code lines only).
fn suppression_knobs_in(label: &str, source: &str) -> Vec<String> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| is_code_line(line))
        .filter(|(_, line)| is_endpoint_suppression_knob(line))
        .map(|(index, line)| format!("{label}:{}: {}", index + 1, line.trim()))
        .collect()
}

#[test]
fn no_endpoint_suppression_knob_remains() {
    let mut files = Vec::new();
    for root in SCAN_ROOTS {
        collect(&repo_root().join(root), &mut files);
    }
    assert!(!files.is_empty(), "the scan found no source files; the roots are wrong and this test is vacuous");

    let mut found = Vec::new();
    for path in files {
        let Ok(source) = fs::read_to_string(&path) else { continue };
        let label = path.strip_prefix(repo_root()).unwrap_or(&path).display().to_string();
        found.extend(suppression_knobs_in(&label, &source));
    }
    assert!(
        found.is_empty(),
        "an endpoint-deprecation suppression knob is back. A notice the service emits on its own \
         schedule must not be silenceable by bespoke config — put the deprecation at the route's \
         declaration site, or delete the notice:\n{}",
        found.join("\n")
    );
}

#[test]
fn startup_validation_does_not_warn_about_unchangeable_state() {
    let source = read("src/web/routes/assembly.rs");
    let start = source
        .find("ledger.validate()")
        .expect("assembly.rs lost the `ledger.validate()` call; this guard needs updating");
    let end = source[start..]
        .find("Err(err) =>")
        .map(|offset| start + offset)
        .expect("assembly.rs lost the `Err(err) =>` arm of the validate match");
    let validation_block = &source[start..end];
    assert!(
        !validation_block.contains("warn!"),
        "the manifest-validation path warns on every boot. Operators cannot change whether the \
         manifest is valid, so a WARN there is noise; log it once at info, or not at all:\n{validation_block}"
    );
    assert!(
        !validation_block.contains("SUPPRESS_"),
        "the manifest-validation path reads a suppression env var again:\n{validation_block}"
    );
}

#[test]
fn the_removed_warning_left_its_deprecation_notice_behind() {
    for (file, endpoint) in DEPRECATION_NOTICE_SITES {
        let source = read(file);
        assert!(
            source.contains(endpoint),
            "{file} no longer declares {endpoint}; this guard is pinned to a route that moved"
        );
        assert!(
            source.contains("ISSUE-13"),
            "{file} serves the deprecated {endpoint} alias without an ISSUE-13 notice. The per-boot \
             WARN was removed on the condition that the deprecation stays documented at the \
             declaration site — otherwise removing it lost information instead of noise"
        );
    }
}

#[test]
fn the_predicate_rejects_a_reintroduced_knob() {
    // Non-vacuity: a predicate that never matches would make the first test
    // pass no matter what was reintroduced.
    let reintroduced = concat!(
        "  pub suppress_vendor_endpoint_warning: bool,\n",
        "      SYNAPSE__SERVER__SUPPRESS_VENDOR_ENDPOINT_WARNING: ${X:-true}\n",
        "        " // a comment-only line must NOT match
    );
    let hits = suppression_knobs_in("fake", reintroduced);
    assert_eq!(hits.len(), 2, "both the field and the env passthrough must be flagged, got {hits:?}");
    assert!(
        suppression_knobs_in("fake", "  # suppress_vendor_endpoint_warning: true\n").is_empty(),
        "a commented-out knob is not a live knob; flagging it would force people to delete the explanation"
    );
    assert!(
        suppression_knobs_in("fake", "  pub suppress_key_server_warning: bool,\n").is_empty(),
        "`suppress_key_server_warning` mirrors upstream Synapse's config surface and is out of scope"
    );
}
