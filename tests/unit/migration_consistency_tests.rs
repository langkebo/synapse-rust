use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn test_v7_batches_have_primary_and_deploy_rollbacks() {
    let root = project_root();
    let primary = root.join("migrations");
    let deploy = root.join("docker/deploy/migrations");
    let required = [
        "20260515000001_consolidated_schema_contract_and_features_v7",
        "20260515000002_consolidated_stream_ordering_online_fix_v7",
        "20260515000003_consolidated_drop_redundant_tables_v7",
    ];

    for name in required {
        assert!(
            primary.join(format!("{name}.sql")).exists(),
            "missing primary migration for {name}"
        );
        assert!(
            primary.join(format!("{name}.undo.sql")).exists(),
            "missing primary rollback for {name}"
        );
        assert!(
            deploy.join(format!("{name}.sql")).exists(),
            "missing deploy migration for {name}"
        );
        assert!(
            deploy.join(format!("{name}.undo.sql")).exists(),
            "missing deploy rollback for {name}"
        );
    }
}

#[test]
fn test_v7_primary_and_deploy_migrations_match() {
    let root = project_root();
    let primary = root.join("migrations");
    let deploy = root.join("docker/deploy/migrations");
    let mirrored = [
        "00000000_unified_schema_v7.sql",
        "00000001_extensions.sql",
        "00000001_extensions.undo.sql",
        "20260515000001_consolidated_schema_contract_and_features_v7.sql",
        "20260515000001_consolidated_schema_contract_and_features_v7.undo.sql",
        "20260515000002_consolidated_stream_ordering_online_fix_v7.sql",
        "20260515000002_consolidated_stream_ordering_online_fix_v7.undo.sql",
        "20260515000003_consolidated_drop_redundant_tables_v7.sql",
        "20260515000003_consolidated_drop_redundant_tables_v7.undo.sql",
    ];

    for file_name in mirrored {
        let primary_contents = read(&primary.join(file_name));
        let deploy_contents = read(&deploy.join(file_name));
        assert_eq!(
            primary_contents, deploy_contents,
            "deploy mirror drifted for {file_name}"
        );
    }
}

#[test]
fn test_build_sqlx_migration_source_outputs_v7_chain() {
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

    assert!(
        output.status.success(),
        "script failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest = read(&output_dir.join("manifest.json"));
    assert!(manifest.contains("\"baseline\": \"00000000_unified_schema_v7.sql\""));
    assert!(manifest.contains("20260515000001_consolidated_schema_contract_and_features_v7.sql"));
    assert!(manifest.contains("20260515000002_consolidated_stream_ordering_online_fix_v7.sql"));
    assert!(manifest.contains("20260515000003_consolidated_drop_redundant_tables_v7.sql"));
}
