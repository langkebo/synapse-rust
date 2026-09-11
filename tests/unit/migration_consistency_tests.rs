#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn test_v11_baseline_primary_exists() {
    let root = project_root();
    let primary = root.join("migrations");
    assert!(primary.join("00000000_unified_schema_v11.sql").exists(), "missing v11 primary schema");
    assert!(primary.join("00000001_extensions_v10.sql").exists(), "missing v10 extensions (still used)");
}

/// Single-source contract (`2b16dc3c`): the deploy migrator mounts the canonical
/// `migrations/` directory directly, so there must be NO separately-maintained
/// copy under `docker/deploy/migrations`.
///
/// The previous version of this test asserted that
/// `docker/deploy/migrations/00000000_unified_schema_v07.sql` exists — i.e. it
/// *locked in* the hand-synced duplicate that had drifted (82 stale v7-lineage
/// files, missing 13 recent migrations). See `migrations/README.md`.
#[test]
fn deploy_mounts_canonical_migrations_and_has_no_copy() {
    let root = project_root();
    let canonical = root.join("migrations");
    let deploy_migrations = root.join("docker/deploy/migrations");

    assert!(canonical.join("00000000_unified_schema_v11.sql").exists(), "missing canonical v11 baseline");

    // A stale real directory (or a symlink — BSD/macOS `find` does not follow a
    // symlink search root, which breaks the migrator's baseline detection) must
    // not reappear.
    assert!(
        !deploy_migrations.exists() && !deploy_migrations.is_symlink(),
        "docker/deploy/migrations must not exist: the deploy path mounts ../../migrations \
         directly, and a copy here would silently drift again"
    );

    // The compose file is the thing that actually wires the canonical directory in.
    let compose = read(&root.join("docker/deploy/docker-compose.yml"));
    assert!(
        compose.contains("../../migrations:/migrations"),
        "docker/deploy/docker-compose.yml must bind-mount ../../migrations:/migrations"
    );
}

#[test]
fn test_build_sqlx_migration_source_outputs_v10_chain() {
    let root = project_root();
    let output_dir = root.join("artifacts/sqlx-migrations-test");
    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)
            .unwrap_or_else(|error| panic!("failed to clean {}: {error}", output_dir.display()));
    }

    let output = Command::new("python3")
        .arg("scripts/build_sqlx_migration_source.py")
        .arg(&output_dir)
        .current_dir(&root)
        .output()
        .expect("failed to run build_sqlx_migration_source.py");

    assert!(output.status.success(), "script failed: {}", String::from_utf8_lossy(&output.stderr));

    let manifest = read(&output_dir.join("manifest.json"));
    assert!(manifest.contains("\"baseline\": \"00000000_unified_schema_v11.sql\""));
}
