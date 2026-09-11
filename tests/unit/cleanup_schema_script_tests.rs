#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Static guards for `scripts/cleanup_test_schemas.sh`.
//!
//! The script is the only byte-level remedy for test-schema accumulation, and it
//! shipped with four defects that made it simultaneously ineffective and unsafe
//! (see `docs/audit/P5_test_schema_accumulation_2026-09-12.md`):
//!
//! 1. matched only `test_%`, so `media_test_*` (1,033) and `synapse_test_*` (48)
//!    could never be cleaned;
//! 2. kept *every* `test_template%` unconditionally, so the 38 superseded
//!    fingerprint templates were immortal;
//! 3. defaulted to `localhost:15432/synapse_test` and never reported which
//!    database it actually reached, so in a differently-configured environment
//!    it connected somewhere else, found nothing, and printed success;
//! 4. discarded all `psql` stderr, so failures printed only `WARN`, and ran
//!    destructively with no dry-run.
//!
//! These are all properties of the script text, so they are guarded statically —
//! no database required.

use std::fs;
use std::path::PathBuf;

fn script() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/cleanup_test_schemas.sh");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn cleanup_script_covers_every_leaked_schema_family() {
    let source = script();
    for pattern in ["test\\_%", "media\\_test\\_%", "synapse\\_test\\_%"] {
        assert!(
            source.contains(pattern),
            "cleanup script must match the `{pattern}` family; matching only `test_%` leaves \
             media_test_* and synapse_test_* to accumulate forever"
        );
    }
}

#[test]
fn cleanup_script_prunes_superseded_templates_but_spares_arbitrary_names() {
    let source = script();
    // The template family must be selected by an ANCHORED fingerprint regex, so
    // that a template configured under an arbitrary name (e.g.
    // `TEST_DB_TEMPLATE_SCHEMA=public`, or a production database) can never be a
    // candidate.
    // NOTE: the script writes this inside a double-quoted bash string, so the
    // end-anchor appears as `\$`. Match the anchor-free prefix and assert the
    // escape separately rather than embedding bash quoting rules here.
    assert!(
        source.contains("^test_template_v[0-9]+_[0-9a-f]{16}"),
        "superseded templates must be matched by the anchored fingerprint pattern, not by a \
         `test_template%` LIKE — an unconditional keep makes 38 stale templates immortal, while \
         an unanchored match risks dropping a hand-configured template"
    );
    assert!(
        source.contains(r"\$'") || source.contains(r"\\$"),
        "the fingerprint regex must be end-anchored so `test_template_v2_<hex>_extra` cannot match"
    );
    // The live template must come from the ready-marker directory.
    assert!(
        source.contains("synapse_test_templates"),
        "the live template set must be derived from the harness's ready-marker directory"
    );
    // Fail-safe: refusing to proceed with an empty keep-set is required.
    assert!(
        source.contains("找不到任何 live 模板标记"),
        "the script must abort when no live template marker can be found, rather than falling \
         back to dropping every template"
    );
}

#[test]
fn cleanup_script_is_dry_run_by_default_and_reports_its_target() {
    let source = script();
    // `APPLY` must default to 0 and the default branch must not drop anything.
    assert!(
        source.contains("APPLY=0\n") || source.contains("APPLY=0\r\n"),
        "the script must default to dry-run (APPLY=0); destructive-by-default is how a \
         wrongly-targeted run silently drops someone else's schemas"
    );
    assert!(source.contains("--apply"), "the destructive path must require an explicit --apply flag");
    // It must tell the operator which database it actually reached.
    assert!(
        source.contains("current_database()") && source.contains("inet_server_addr()"),
        "the script must print the database and server it actually connected to; the previous \
         version silently targeted localhost:15432/synapse_test"
    );
    // Connection must be overridable through the URLs the test harness uses.
    assert!(
        source.contains("TEST_DATABASE_URL") && source.contains("DATABASE_URL"),
        "connection resolution must honour DATABASE_URL / TEST_DATABASE_URL"
    );
}

#[test]
fn cleanup_script_preserves_drop_failure_reasons() {
    let source = script();
    // The DROP invocation must not send stderr to /dev/null.
    let drop_lines: Vec<&str> = source.lines().filter(|line| line.contains("DROP SCHEMA")).collect();
    assert!(!drop_lines.is_empty(), "script must contain a DROP SCHEMA statement");
    for line in drop_lines {
        assert!(
            !line.contains("2>/dev/null"),
            "DROP failures must keep their stderr; discarding it reduces every failure to a \
             bare WARN with no cause: {line}"
        );
    }
}
