//! Guards for the *input set* of the integration template fingerprint
//! (`synapse-test-utils::{is_schema_input, template_schema_manifest}`).
//!
//! The fingerprint must answer exactly one question: "is this schema still the
//! one the migrations describe?" Only `migrations/**/*.sql` can change that
//! answer. The input set used to be "every regular file in the directory", so
//! editing `migrations/README.md` renamed the template
//! (`4dbe2a3f35bb97eb` → `854961cd2ea546e5`, observed 2026-09-18) and forced a
//! needless full rebuild (~35–60s), while drowning out the real signal "editing
//! `migrations/` re-mints the template".
//!
//! Both directions are pinned, because either one alone is a broken gate:
//!   * a `.md` edit must NOT move the manifest, and
//!   * a `.sql` edit MUST move it — otherwise "exclude every file" would satisfy
//!     the first guard while silently freezing every template forever.
//!
//! Uses `std::env::temp_dir()` rather than `tempfile`: the root `Cargo.toml` is
//! being edited by a concurrent session and this task must not touch it.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A uniquely named scratch directory that removes itself on drop.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(label: &str) -> Self {
        let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("synapse-t8-{label}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).expect("scratch directory must be creatable");
        Self(path)
    }

    fn write(&self, name: &str, contents: &str) {
        fs::write(self.0.join(name), contents).expect("scratch file must be writable");
    }

    fn manifest(&self) -> String {
        synapse_test_utils::template_schema_manifest(&self.0)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn schema_input_accepts_sql_and_rejects_documentation() {
    assert!(synapse_test_utils::is_schema_input("00000000_unified_schema_v12.sql"));
    assert!(synapse_test_utils::is_schema_input("20260101000000_event_relations_index.sql"));
    assert!(!synapse_test_utils::is_schema_input("README.md"));
    assert!(!synapse_test_utils::is_schema_input("INDEXES.md"));
    assert!(!synapse_test_utils::is_schema_input("README"));
    assert!(
        !synapse_test_utils::is_schema_input("00000000_unified_schema_v12.sql.bak"),
        "only an exact `.sql` suffix counts; a backup copy must not fingerprint the template"
    );
}

#[test]
fn schema_input_fingerprint_ignores_markdown_but_tracks_sql() {
    let scratch = ScratchDir::new("fingerprint");
    scratch.write("x.sql", "CREATE TABLE t (id INT);\n");
    scratch.write("README.md", "v1\n");

    let before = scratch.manifest();

    // A pure documentation edit must not re-mint the template.
    scratch.write("README.md", "v2 — a completely different document\n");
    assert_eq!(scratch.manifest(), before, "a `.md` edit must not change the template fingerprint");

    // Adding a new `.md` must not either.
    scratch.write("INDEXES.md", "notes\n");
    assert_eq!(scratch.manifest(), before, "adding a `.md` must not change the template fingerprint");

    // But a `.sql` edit must. This is the reverse guard: if someone "simplifies"
    // `is_schema_input` to reject everything, the two assertions above stay green
    // while the template silently stops tracking the schema — this one goes red.
    scratch.write("x.sql", "CREATE TABLE t (id INT, name TEXT);\n");
    assert_ne!(scratch.manifest(), before, "a `.sql` edit MUST change the template fingerprint");

    // A newly added `.sql` file must move it too.
    let after_sql_edit = scratch.manifest();
    scratch.write("y.sql", "CREATE TABLE u (id INT);\n");
    assert_ne!(scratch.manifest(), after_sql_edit, "a new `.sql` file MUST change the fingerprint");
}

/// A schema input that cannot be read must **panic**, not silently drop out of
/// the manifest.
///
/// A dropped file means the fingerprint no longer describes the schema, so the
/// template is treated as up to date and a stale schema keeps being served — the
/// exact failure the fingerprint exists to prevent. `template_schema_manifest`
/// used `fs::read(…).ok()?` (and `.filter_map(Result::ok)` for the directory
/// entries) while the line above it already used `panic!`; this test covers the
/// branch that the readable-inputs-only tests could not reach (sweep D1).
#[cfg(unix)]
#[test]
fn unreadable_schema_input_fails_loudly_instead_of_being_skipped() {
    use std::os::unix::fs::PermissionsExt;

    let scratch = ScratchDir::new("unreadable");
    scratch.write("00000000_unified_schema_v12.sql", "CREATE TABLE t (id INT);\n");
    let path = scratch.0.join("00000000_unified_schema_v12.sql");
    let readable_manifest = scratch.manifest();
    assert!(
        readable_manifest.contains("00000000_unified_schema_v12.sql"),
        "precondition: a readable input is part of the manifest"
    );

    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("chmod 000");
    if fs::read(&path).is_ok() {
        // Running as root, or on a filesystem that ignores mode bits: the premise
        // cannot be established, so say so rather than asserting nothing.
        eprintln!("SKIPPING unreadable_schema_input_fails_loudly…: mode 000 is still readable here");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod 644");
        return;
    }

    let outcome = std::panic::catch_unwind(|| scratch.manifest());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod 644");

    let payload = outcome.expect_err("an unreadable schema input must not be skipped silently");
    let message = payload.downcast_ref::<String>().map_or("", String::as_str);
    assert!(
        message.contains("cannot read schema input"),
        "the panic must name the unreadable file and the reason, got: {message:?}"
    );

    // And once readable again the manifest is exactly what it was.
    assert_eq!(scratch.manifest(), readable_manifest, "restoring readability must restore the manifest");
}
