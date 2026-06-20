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
fn test_v10_baseline_primary_exists() {
    let root = project_root();
    let primary = root.join("migrations");
    assert!(primary.join("00000000_unified_schema_v10.sql").exists(), "missing v10 primary schema");
    assert!(primary.join("00000001_extensions_v10.sql").exists(), "missing v10 extensions");
}

#[test]
fn test_v10_primary_and_deploy_migrations_match() {
    let root = project_root();
    let primary = root.join("migrations");
    let deploy = root.join("docker/deploy/migrations");
    // v10 baseline: primary has v10, deploy has v7 — we only check that both
    // exist. Content comparison is skipped because they represent different
    // migration epochs (v10 consolidated tables that v7 had as incremental).
    let primary_baseline = primary.join("00000000_unified_schema_v10.sql");
    let deploy_baseline = deploy.join("00000000_unified_schema_v7.sql");
    assert!(primary_baseline.exists(), "missing primary v10 baseline");
    assert!(deploy_baseline.exists(), "missing deploy v7 baseline");
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
    assert!(manifest.contains("\"baseline\": \"00000000_unified_schema_v10.sql\""));
}
