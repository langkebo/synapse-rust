#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Guard for the template readiness-marker convention (`AGENTS.md` rule 2:
//! one implementation per responsibility).
//!
//! The convention is defined once in Rust:
//! `synapse-common/src/test_isolation.rs` owns
//! [`TEMPLATE_READY_MARKER_PREFIX`], [`template_marker_dir`] and
//! [`template_ready_marker_path`]. Two shell scripts independently hard-code the
//! same strings:
//!
//! * `scripts/cleanup_test_schemas.sh` globs
//!   `synapse_test_template_ready_*` inside `${...}/synapse_test_templates` to
//!   decide which template schemas are live (keep reason #1);
//! * `scripts/ci/prepare_test_db.sh` touches
//!   `$MARKER_DIR/synapse_test_template_ready_${TEMPLATE_SCHEMA}`.
//!
//! Nothing used to pin the three sites together. Renaming the Rust constant (or
//! the marker directory leaf) would leave cleanup unable to recognise the live
//! template: the template then looks stale, and an `--apply` would CASCADE it —
//! forcing a seconds-long rebuild and, worse, dropping a template that
//! concurrent clones' `LIKE ... INCLUDING ALL` serial defaults still reference.
//!
//! These tests are static and read-only: they read the two scripts from disk and
//! compare their literal text against the single Rust definition. No database and
//! no shell execution is involved.

use std::fs;
use std::path::PathBuf;
use synapse_common::test_isolation::{template_marker_dir, template_ready_marker_path, TEMPLATE_READY_MARKER_PREFIX};

/// Repository root (`CARGO_MANIFEST_DIR` is the root crate directory).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Read a repository file by path relative to the crate root, naming it in the
/// panic so a moved/renamed script is impossible to confuse with a bad assertion.
fn read_repo_file(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

/// The two shell consumers of the readiness-marker convention.
const MARKER_CONSUMERS: [&str; 2] = ["scripts/cleanup_test_schemas.sh", "scripts/ci/prepare_test_db.sh"];

/// Per-script literals that must be present for the convention to actually be
/// consumed there. Each script locates the marker directory and then builds the
/// marker file name from the prefix, so both halves must appear in each script.
const MARKER_CONSUMER_LITERALS: [(&str, &[&str]); 2] = [
    ("scripts/cleanup_test_schemas.sh", &["synapse_test_templates", "synapse_test_template_ready_*"]),
    ("scripts/ci/prepare_test_db.sh", &["synapse_test_templates", "synapse_test_template_ready_${TEMPLATE_SCHEMA}"]),
];

/// Both scripts must be found, and each must contain the given needle.
///
/// This is the non-vacuity half of every other assertion below: a guard that
/// merely compares two constants passes when a script is moved, renamed or
/// emptied. Failing here first makes a vacuous comparison impossible.
fn assert_consumers_contain(needle: &str, why: &str) {
    for relative in MARKER_CONSUMERS {
        let source = read_repo_file(relative);
        assert!(
            source.contains(needle),
            "{relative} does not contain {needle:?} ({why}); the marker-convention guard would \
             otherwise be vacuous"
        );
    }
}

/// (c) Non-vacuity: both scripts exist and each carries the marker convention's
/// exact strings. Asserted before the equality checks so a moved/renamed script
/// shows up as this failure rather than as a silently passing comparison.
#[test]
fn both_marker_consumers_exist_and_name_the_convention() {
    for (relative, needles) in MARKER_CONSUMER_LITERALS {
        let path = repo_root().join(relative);
        assert!(path.is_file(), "marker-convention consumer {relative} not found at {}", path.display());
        let source = read_repo_file(relative);
        for needle in needles {
            assert!(
                source.contains(needle),
                "{relative} no longer names {needle:?}; the guard below would compare against a \
                 file that does not consume the convention at all"
            );
        }
    }
    // The prefix, and the `-<schema>` join shape the cleanup script both globs
    // and strips. The exact needles are the convention, not incidental text.
    assert_consumers_contain("synapse_test_template_ready_", "the ready-marker prefix must appear in each consumer");
    // The marker directory leaf, used by both scripts to locate the files.
    assert_consumers_contain("synapse_test_templates", "the marker directory leaf must appear in each consumer");
}

/// (a) The single Rust prefix must equal the prefix both shell scripts use.
///
/// `TEMPLATE_READY_MARKER_PREFIX` is interpolated into the scripts' literal text
/// comparison, so changing the constant (a "rename" of the convention) turns this
/// red instead of silently breaking cleanup.
#[test]
fn template_ready_marker_prefix_matches_both_shell_scripts() {
    // `template_ready_marker_path` is the only Rust writer of the convention and
    // derives its filename from the constant, so pinning it here also pins the
    // writer/reader pair together.
    let marker_name = template_ready_marker_path("probe_schema");
    let file_name = marker_name.file_name().and_then(|name| name.to_str()).expect("marker path must have a file name");
    assert_eq!(
        file_name,
        format!("{TEMPLATE_READY_MARKER_PREFIX}_probe_schema"),
        "template_ready_marker_path must be `<prefix>_<schema>`; the cleanup script's \
         `${{m##*synapse_test_template_ready_}}` strip relies on exactly that shape"
    );

    for relative in MARKER_CONSUMERS {
        let source = read_repo_file(relative);
        assert!(
            source.contains(TEMPLATE_READY_MARKER_PREFIX),
            "{relative} uses a ready-marker prefix different from \
             synapse_common::test_isolation::TEMPLATE_READY_MARKER_PREFIX \
             ({TEMPLATE_READY_MARKER_PREFIX:?}); cleanup would no longer recognise the live \
             template and could delete it"
        );
    }
}

/// (b) The directory leaf produced by [`template_marker_dir`] must be the leaf
/// both scripts use.
#[test]
fn template_marker_dir_leaf_matches_both_shell_scripts() {
    let dir = template_marker_dir();
    let leaf =
        dir.file_name().and_then(|name| name.to_str()).expect("template_marker_dir must end in a named directory");
    assert_eq!(
        leaf, "synapse_test_templates",
        "template_marker_dir's leaf is the cleanup script's marker root; a different leaf makes \
         the cleanup script look in the wrong directory and see no live templates"
    );

    for relative in MARKER_CONSUMERS {
        let source = read_repo_file(relative);
        assert!(
            source.contains(leaf),
            "{relative} does not reference the marker directory leaf {leaf:?} produced by \
             template_marker_dir()"
        );
    }
}
