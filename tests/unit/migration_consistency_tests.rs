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
    assert!(primary.join("00000000_unified_schema_v12.sql").exists(), "missing v11 primary schema");
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

    assert!(canonical.join("00000000_unified_schema_v12.sql").exists(), "missing canonical v11 baseline");

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

/// `schema_migrations.executed_at` is declared in five places, and whichever one
/// runs first decides the real column type. They must all say BIGINT (millisecond
/// epoch), matching the canonical migration.
///
/// Two writers used to say TIMESTAMPTZ. On a database whose table had already
/// been created as BIGINT, their writes failed with
/// `column "executed_at" is of type bigint but expression is of type timestamp
/// with time zone`, so migration records silently disappeared. Observed
/// 2026-09-15: the deploy `migrator` exited 1 with 0 rows in
/// `schema_migrations`, which also made `deploy.sh`'s version-consistency gate
/// impossible to pass.
#[test]
fn schema_migrations_executed_at_is_bigint_in_every_writer() {
    let root = project_root();
    let writers = [
        "migrations/00000000_unified_schema_v12.sql",
        "docker/db_migrate.sh",
        "docker/deploy/scripts/container-migrate.sh",
        "docker/deploy/scripts/init-db.sql",
        "synapse-services/src/database_initializer/mod.rs",
    ];

    for rel in writers {
        let text = read(&root.join(rel));
        assert!(text.contains("executed_at BIGINT"), "{rel} 必须把 executed_at 声明为 BIGINT（canonical 迁移的类型）");
        assert!(
            !text.contains("executed_at TIMESTAMPTZ"),
            "{rel} 不得把 executed_at 声明为 TIMESTAMPTZ：先建表的一方决定列类型，另一方会写不进迁移记录"
        );
        assert!(
            !text.contains("executed_at = NOW()"),
            "{rel} 不得给 executed_at 赋 NOW()（timestamptz），应写毫秒 bigint 表达式"
        );
    }
}

/// The extension gate in `container-migrate.sh` matches `,$ENABLED_EXTENSIONS,`
/// against a `,$feature,` pattern. The variable must NOT be quoted in that
/// pattern: the migrator container runs `/bin/sh` (busybox ash), which keeps the
/// quotes as literal pattern characters, so the quoted form never matches.
///
/// Observed 2026-09-15 with `ENABLED_EXTENSIONS=friends,burn-after-read`:
/// `00000001_extensions_v10.sql` was reported as "未启用" and skipped, leaving the
/// friends tables uncreated and failing `deploy.sh`'s gate — even though the
/// `friends` feature was explicitly enabled.
#[test]
fn extension_gate_pattern_does_not_quote_the_feature_variable() {
    let script = read(&project_root().join("docker/deploy/scripts/container-migrate.sh"));
    assert!(
        script.contains("*,$feature,*)"),
        "扩展门控必须使用未加引号的 `*,$feature,*)` 模式：容器内 /bin/sh（busybox ash）\
         会把模式里变量的引号当作字面量，导致已启用的 feature 仍被判为未启用"
    );
}

/// The Makefile's `migrate-status` / `migrate-audit` targets query
/// `schema_migrations`. They used `success` (the canonical column is
/// `is_success`) and `EXTRACT(EPOCH FROM executed_at)` (only meaningful on a
/// timestamptz, while `executed_at` is BIGINT milliseconds), so both aborted with
/// `column "success" does not exist` — one error per run, hiding the other.
#[test]
fn makefile_migration_queries_match_the_schema_migrations_schema() {
    let makefile = read(&project_root().join("Makefile"));
    let mut checked = 0usize;

    for line in makefile.lines().filter(|line| line.contains("schema_migrations")) {
        checked += 1;
        let without_is_success = line.replace("is_success", "");
        assert!(
            !without_is_success.contains("success"),
            "Makefile 查询了不存在的 `success` 列（canonical 是 is_success）: {line}"
        );
        assert!(
            !line.contains("EXTRACT(EPOCH FROM executed_at)"),
            "executed_at 是毫秒 bigint，不能对它用 EXTRACT(EPOCH ...)（那是 timestamptz 的用法）: {line}"
        );
    }

    assert!(checked > 0, "Makefile 中应仍有查询 schema_migrations 的目标");
}

/// H-15：迁移执行只允许一条路径 —— `docker/db_migrate.sh`（记账表 `schema_migrations`）。
///
/// Makefile 里曾并存第二、三条路径：
///   * `migrate` / `migrate-check` / `migrate-undo` / `migrate-baseline` 走 `sqlx migrate`，
///     写的是 `_sqlx_migrations`，与项目实际的 `schema_migrations` 是两套账，而且绕过
///     `container-migrate.sh` 的扩展门控；
///   * `flyway-info` / `flyway-migrate` 挂载 `scripts/db/flyway.conf` 与 `scripts/db/undo`，
///     该目录在仓库里根本不存在，目标必然失败。
///
/// 只保留只读查询目标（`migrate-status` / `migrate-audit`），它们读的是同一个
/// `schema_migrations`，不构成第二条执行路径。
#[test]
fn makefile_exposes_a_single_migration_execution_path() {
    let makefile = read(&project_root().join("Makefile"));

    assert!(
        !makefile.to_lowercase().contains("flyway"),
        "Makefile 不得保留 flyway 迁移路径：它挂载的 scripts/db/ 不存在，目标已死（H-15）"
    );
    assert!(
        !makefile.contains("sqlx migrate"),
        "Makefile 不得保留 `sqlx migrate` 路径：它写 _sqlx_migrations，与 schema_migrations 两套账（H-15）"
    );
    for target in ["\nmigrate:", "\nmigrate-check:", "\nmigrate-undo:", "\nmigrate-baseline:"] {
        assert!(
            !makefile.contains(target),
            "Makefile 不得再定义第二个迁移执行目标 `{}`：执行入口只有 docker/db_migrate.sh（H-15）",
            target.trim_start_matches('\n').trim_end_matches(':')
        );
    }
    assert!(makefile.contains("\nmigrate-status:"), "只读迁移查询目标应保留");
}

/// H-14 的调用侧不变式：CI 里调用 `docker/db_migrate.sh` 的步骤必须**显式给出目标**
/// （`DATABASE_URL` 或 `DB_HOST`）。
///
/// 裸调用会落到脚本从 `.env` 兜底出来的 `localhost:5432` —— 两个 compose 栈都不把
/// 5432 发布到宿主，那个端口上的是宿主自装的 PostgreSQL。2026-09-15 实测：一条
/// `validate` 命令在那个实例上建了库。
///
/// 脚本侧的判据必须同时承认 `DATABASE_URL` 与 `DB_HOST`：`db-migration-gate.yml`
/// 与 `drift-detection.yml` 只给 `DB_HOST`/`DB_PORT`/`DB_NAME`/`DB_USER`/`DB_PASSWORD`，
/// 只认 `DATABASE_URL` 会把这两条 CI 直接判死。
#[test]
fn every_ci_db_migrate_call_supplies_an_explicit_target() {
    let root = project_root();
    let guard = read(&root.join("docker/db_migrate.sh"));
    assert!(
        guard.contains("CALLER_SUPPLIED_DB_HOST"),
        "db_migrate.sh 的护栏必须把调用方显式给出的 DB_HOST 也算作「显式目标」，\
         否则只给 DB_* 而不给 DATABASE_URL 的 CI 会被误拒（H-14）"
    );

    let workflows = root.join(".github/workflows");
    let mut checked = 0usize;
    let mut entries: Vec<_> = fs::read_dir(&workflows)
        .expect(".github/workflows must exist")
        .map(|entry| entry.expect("readable dir entry").path())
        .filter(|path| matches!(path.extension().and_then(|ext| ext.to_str()), Some("yml" | "yaml")))
        .collect();
    entries.sort();

    for path in entries {
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        let text = read(&path);
        let lines: Vec<&str> = text.lines().collect();

        for (index, line) in lines.iter().enumerate() {
            if !line.contains("bash docker/db_migrate.sh") {
                continue;
            }
            checked += 1;
            let window = lines[index.saturating_sub(15)..=index].join("\n");
            assert!(
                window.contains("DATABASE_URL:") || window.contains("DB_HOST:"),
                "{file}:{} 调用 db_migrate.sh 但没有显式给出目标（DATABASE_URL / DB_HOST）——\
                 裸调用会打到宿主自装的 PostgreSQL（H-14）",
                index + 1
            );
        }
    }

    assert!(checked > 0, "应至少有一个 CI 步骤调用 db_migrate.sh");
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
    assert!(manifest.contains("\"baseline\": \"00000000_unified_schema_v12.sql\""));
}
