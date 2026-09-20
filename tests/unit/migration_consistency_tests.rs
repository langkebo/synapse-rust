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

/// Run one of the migration gate scripts from `root`, capturing its output.
fn run_gate_script(root: &Path, script: &str) -> std::process::Output {
    Command::new("python3")
        .arg(script)
        .current_dir(root)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {script}: {error}"))
}

/// Copy `src` to `dst`, creating `dst`'s parent directory first.
fn copy_file(src: &Path, dst: &Path) {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|error| panic!("failed to create {}: {error}", parent.display()));
    }
    fs::copy(src, dst).unwrap_or_else(|error| panic!("failed to copy {} -> {}: {error}", src.display(), dst.display()));
}

/// Fresh temp scene directory, unique per test and process so parallel test
/// threads cannot collide, and deliberately **outside** `migrations/` (the
/// repo's migration directory is never mutated by these probes).
fn temp_scene(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("synapse-migration-gate-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap_or_else(|error| panic!("failed to clean {}: {error}", dir.display()));
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// `migrations/` 必须只有**一个** `00000000_unified_schema_v*.sql`。
///
/// 历史基线若与最新基线并存，迁移器会把它当作"增量迁移"再执行一遍
/// （`find ... | sort | tail -1` 只挑最新基线为首选，其余仍进待应用列表），
/// 于是同一个库被追加应用两个版本的 baseline，并被写进 `schema_migrations`。
/// 本地机器磁盘上可能有残留、CI 全新检出没有 —— 两边 schema 就此分叉。
/// `bddd6109` 的"从 git tracking 移除但磁盘保留"正是这样落地的 v11。
#[test]
fn migrations_directory_has_exactly_one_baseline() {
    let root = project_root();
    let migrations = root.join("migrations");

    let mut baselines: Vec<String> = fs::read_dir(&migrations)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", migrations.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("00000000_unified_schema_v") && name.ends_with(".sql"))
        .collect();
    baselines.sort();

    assert_eq!(
        baselines,
        vec!["00000000_unified_schema_v12.sql".to_string()],
        "migrations/ 必须只保留一个基线；多出来的历史基线会被迁移器当增量执行并造成 \
         本地/CI schema 分叉（历史基线可从 `git show bddd6109^:migrations/...` 取回）"
    );
}

/// 迁移执行入口必须按"历史基线一律跳过"的判据处理，否则上面那条不变式
/// 只在文件层面成立、执行层面仍会分叉。
///
/// 执行入口只有一个实现：`docker/db_migrate.sh`。部署侧的
/// `docker/deploy/scripts/container-migrate.sh` 是薄包装（只桥接容器环境后 exec
/// 唯一实现），因此它**不得**再自带这份判据 —— 那正是"同一职责两份实现"的回归，
/// 由 `cleanup_schema_script_tests::deploy_migrator_delegates_to_the_single_implementation`
/// 守卫。
#[test]
fn migration_runners_skip_every_historical_baseline() {
    let root = project_root();
    let script = read(&root.join("docker/db_migrate.sh"));
    assert!(
        script.contains("00000000_unified_schema_v*.sql) return 0"),
        "docker/db_migrate.sh 必须以 `00000000_unified_schema_v*.sql` 模式跳过所有历史基线，\
         而不是硬编码某几个版本号（v11 残留即因漏列而成为增量迁移）"
    );

    let deploy = read(&root.join("docker/deploy/scripts/container-migrate.sh"));
    assert!(
        !deploy.contains("00000000_unified_schema_v*.sql) return 0"),
        "部署入口不得自带历史基线跳过判据：它是薄包装，该判据只在 docker/db_migrate.sh 里有一份"
    );
    assert!(
        deploy.contains("docker/db_migrate.sh"),
        "部署入口必须委托给 docker/db_migrate.sh，否则上面那条执行层不变式对它不成立"
    );
}

/// v12 基线内的每个对象只能**声明一次**，且折入块里的 10 个索引 / 9 个约束必须存在。
///
/// 该文件尾部曾是 `scripts/generate_next_baseline.py` 的 append 产物：脚本把
/// `current` 读成**自己的输出**再拼上 `extensions_v10 + p0_constraints_indexes.sql`，
/// 因此每跑一次就多一整段 —— 跑三次的净效果是 14 张扩展表 + 折入块各重复 3 遍
/// （1350 行纯冗余，占全文 24%）。没人发现，因为重复副本全是 `IF NOT EXISTS`，
/// 在"首次生效者决定 schema"的语义下整体空转。
///
/// 危害不是浪费行数：重复让"文件内容"与"实际 schema"脱钩 —— 去重时若按"这段看起来
/// 是复制体"直接删，就会连唯一一份定义一起删掉。本次去重时尾部正是**藏了 10 个
/// 前段不存在的索引**（device_signatures / event_edges / e2ee_audit_log /
/// push_notification_queue / federation_queue / rooms）外加 9 个约束 DO 块。
#[test]
fn baseline_declares_each_object_exactly_once() {
    let baseline = read(&project_root().join("migrations/00000000_unified_schema_v12.sql"));

    let mut tables: Vec<String> = Vec::new();
    let mut indexes: Vec<String> = Vec::new();

    for line in baseline.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("--") {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("CREATE TABLE ") {
            let rest = rest.strip_prefix("IF NOT EXISTS ").unwrap_or(rest);
            let end = rest.find(|c: char| c.is_whitespace() || c == '(' || c == ';').unwrap_or(rest.len());
            if end > 0 {
                tables.push(rest[..end].to_string());
            }
            continue;
        }

        for prefix in [
            "CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS ",
            "CREATE INDEX CONCURRENTLY IF NOT EXISTS ",
            "CREATE UNIQUE INDEX IF NOT EXISTS ",
            "CREATE INDEX IF NOT EXISTS ",
        ] {
            let Some(rest) = trimmed.strip_prefix(prefix) else { continue };
            let end = rest.find(|c: char| c.is_whitespace() || c == '(' || c == ';').unwrap_or(rest.len());
            if end > 0 {
                indexes.push(rest[..end].to_string());
            }
            break;
        }
    }

    fn duplicates(names: &[String]) -> Vec<String> {
        let mut sorted = names.to_vec();
        sorted.sort();
        let mut dups: Vec<String> = sorted.windows(2).filter(|w| w[0] == w[1]).map(|w| w[0].clone()).collect();
        dups.dedup();
        dups
    }

    assert!(tables.len() >= 200, "baseline 只解析出 {} 张表，解析口径或文件都被破坏了", tables.len());
    assert!(indexes.len() >= 300, "baseline 只解析出 {} 个索引，解析口径或文件都被破坏了", indexes.len());
    assert!(
        duplicates(&tables).is_empty(),
        "baseline 重复声明了这些表（重复段在 IF NOT EXISTS 下整体空转，且会让去重时误删唯一定义）: {:?}",
        duplicates(&tables)
    );
    assert!(duplicates(&indexes).is_empty(), "baseline 重复声明了这些索引名（同上）: {:?}", duplicates(&indexes));

    // 折入块（baseline 尾部"完整性约束与性能索引折入块"）里的对象在主体中**没有**
    // 等价定义，是本文件唯一来源。删掉这一段，全新库就会缺这些索引/约束。
    for index in [
        "idx_device_signatures_user_device",
        "idx_device_signatures_target",
        "idx_event_edges_prev_room",
        "idx_e2ee_audit_log_device",
        "idx_e2ee_audit_log_room_event",
        "idx_push_queue_user_pending",
        "idx_push_queue_retry",
        // `idx_federation_queue_dest_created` 与 `idx_federation_queue_pending` 定义完全相同，
        // 2026-09-17 已删除前者（DB review §1）；这里改为断言保留的那个。
        "idx_federation_queue_pending",
        "idx_federation_queue_retry",
        "idx_rooms_federated",
    ] {
        assert!(indexes.iter().any(|name| name == index), "baseline 缺少折入索引 {index}（P1/P3 折入块被删了？）");
    }

    for constraint in [
        "ck_room_memberships_valid",
        "fk_event_edges_prev",
        "fk_events_redacted_by",
        "uq_device_keys_user_device_algorithm_keyid",
        "ck_events_depth_nonneg",
        "ck_events_not_before_nonneg",
        "uq_backup_keys_room_session",
        "fk_backup_keys_room",
    ] {
        assert!(baseline.contains(constraint), "baseline 缺少折入约束 {constraint}（P0/P1/P2/P3 折入块被删了？）");
    }
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

    assert!(canonical.join("00000000_unified_schema_v12.sql").exists(), "missing canonical v12 baseline");

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

/// `schema_migrations.executed_at` is declared by several writers, and whichever one
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
///
/// `docker/deploy/scripts/container-migrate.sh` used to be one of these writers;
/// it is now a thin wrapper with no DDL at all, so the remaining writers below are
/// the complete set. The wrapper's "no migration logic" invariant is guarded by
/// `cleanup_schema_script_tests::deploy_migrator_delegates_to_the_single_implementation`.
#[test]
fn schema_migrations_executed_at_is_bigint_in_every_writer() {
    let root = project_root();
    let writers = [
        "migrations/00000000_unified_schema_v12.sql",
        "docker/db_migrate.sh",
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

/// C3/C10 (GATE_INTEGRITY_SWEEP_2026-09-19 §6): both migration gates must say
/// the `consolidated-baseline-only` case out loud instead of passing on an
/// empty subject set.
///
/// `check_baseline_consolidation.py`'s subject is timestamped incremental
/// migrations; `check_migration_consistency.py`'s undo-pairing sub-check
/// iterates the same set. With the consolidated baseline as the only forward
/// file those loops ran over 0 subjects, so "long green" was an artifact of an
/// empty scan surface. The marker assertion is conditional on the surface
/// actually being empty, so adding a real incremental migration does not make
/// this test lie — it just has to be a *deliberate* new state.
#[test]
fn migration_gates_are_explicit_about_the_consolidated_baseline_only_case() {
    let root = project_root();

    let consolidation = run_gate_script(&root, "scripts/check_baseline_consolidation.py");
    let consolidation_out = String::from_utf8_lossy(&consolidation.stdout);
    assert!(
        consolidation.status.success(),
        "check_baseline_consolidation.py must pass on the current tree: {consolidation_out}{}",
        String::from_utf8_lossy(&consolidation.stderr)
    );
    if consolidation_out.contains("已吸收全部 0 个增量迁移") {
        assert!(
            consolidation_out.contains("consolidated-baseline-only"),
            "an empty incremental-migration subject set must be reported with the documented \
             marker, not silently accepted: {consolidation_out}"
        );
    }

    let consistency = run_gate_script(&root, "scripts/check_migration_consistency.py");
    let consistency_out = String::from_utf8_lossy(&consistency.stdout);
    assert!(
        consistency.status.success(),
        "check_migration_consistency.py must pass on the current tree: {consistency_out}"
    );
    if consistency_out.contains("\"incremental_files\": []") {
        assert!(
            consistency_out.contains("\"marker\": \"consolidated-baseline-only\""),
            "an empty incremental subject set must carry the documented marker: {consistency_out}"
        );
    }
}

/// Red proof for C3: emptying the subject set by making the discovery regex
/// miss the real files must fail loudly, not report success on nothing.
///
/// The probe builds a temp copy (the repo's `migrations/` is never mutated)
/// holding the consolidated baseline plus one genuine incremental migration.
/// First the shipped regex detects the violation; then the regex is broken in
/// the temp copy and the gate must refuse to pass vacuously.
#[test]
fn baseline_consolidation_gate_fails_closed_on_an_emptied_scan_surface() {
    let root = project_root();
    let scene = temp_scene("baseline-consolidation-emptied-scan");
    let baseline = "00000000_unified_schema_v12.sql";

    copy_file(
        &root.join("scripts/check_baseline_consolidation.py"),
        &scene.join("scripts/check_baseline_consolidation.py"),
    );
    copy_file(&root.join("migrations").join(baseline), &scene.join("migrations").join(baseline));
    fs::write(
        scene.join("migrations/20260101000000_probe.sql"),
        "CREATE TABLE probe_gate_table (id BIGSERIAL PRIMARY KEY);\n",
    )
    .unwrap();

    let detected = run_gate_script(&scene, "scripts/check_baseline_consolidation.py");
    assert!(
        !detected.status.success(),
        "an incremental migration creating an object the baseline does not absorb must fail: {}",
        String::from_utf8_lossy(&detected.stdout)
    );

    // Simulate a naming-convention change: the regex no longer matches the real
    // incremental file. Pre-fix this printed "0 个增量迁移" and exited 0.
    let script = scene.join("scripts/check_baseline_consolidation.py");
    let broken = fs::read_to_string(&script).unwrap().replace(r#"r"^\d{14}_.*\.sql$""#, r#"r"^\d{20}_.*\.sql$""#);
    fs::write(&script, broken).unwrap();

    let emptied = run_gate_script(&scene, "scripts/check_baseline_consolidation.py");
    let emptied_out = String::from_utf8_lossy(&emptied.stdout);
    assert!(!emptied.status.success(), "an emptied scan surface must fail loudly, not pass vacuously: {emptied_out}");
    assert!(
        emptied_out.contains("扫描面自检失败"),
        "the scan-surface self-check must explain the failure: {emptied_out}"
    );

    fs::remove_dir_all(&scene).ok();
}

/// Red proof for C10: the undo-pairing sub-check must run whenever incremental
/// migrations exist, and a changed naming convention or deleted file must not
/// silently empty the scan.
#[test]
fn migration_consistency_gate_pairs_undo_files_and_fails_on_an_emptied_scan_surface() {
    let root = project_root();
    let scene = temp_scene("migration-consistency-emptied-scan");
    let baseline = "00000000_unified_schema_v12.sql";

    copy_file(
        &root.join("scripts/check_migration_consistency.py"),
        &scene.join("scripts/check_migration_consistency.py"),
    );
    copy_file(&root.join("migrations").join(baseline), &scene.join("migrations").join(baseline));
    copy_file(&root.join("docker/deploy/docker-compose.yml"), &scene.join("docker/deploy/docker-compose.yml"));

    // Undo pairing still runs when a genuine incremental migration exists.
    fs::write(scene.join("migrations/20260101000000_probe.sql"), "-- probe\n").unwrap();
    let missing_undo = run_gate_script(&scene, "scripts/check_migration_consistency.py");
    let missing_undo_out = String::from_utf8_lossy(&missing_undo.stdout);
    assert!(
        !missing_undo.status.success(),
        "an incremental migration without a `.undo.sql` companion must fail: {missing_undo_out}"
    );
    assert!(missing_undo_out.contains("missing_primary_undo"), "expected missing_primary_undo: {missing_undo_out}");

    fs::write(scene.join("migrations/20260101000000_probe.undo.sql"), "-- undo\n").unwrap();
    let paired = run_gate_script(&scene, "scripts/check_migration_consistency.py");
    let paired_out = String::from_utf8_lossy(&paired.stdout);
    assert!(paired.status.success(), "an incremental migration with a matching undo must pass: {paired_out}");
    assert!(
        paired_out.contains("\"incremental_files\"") && paired_out.contains("20260101000000_probe.sql"),
        "the paired incremental must be reported in the scan surface: {paired_out}"
    );
    assert!(
        paired_out.contains("\"marker\": null"),
        "the consolidated-baseline-only marker must not be emitted while incrementals exist: {paired_out}"
    );

    // A changed naming convention must not silently empty the scan surface.
    fs::remove_file(scene.join("migrations/20260101000000_probe.sql")).unwrap();
    fs::remove_file(scene.join("migrations/20260101000000_probe.undo.sql")).unwrap();
    fs::write(scene.join("migrations/20260101_probe.sql"), "-- probe\n").unwrap();
    let drifted = run_gate_script(&scene, "scripts/check_migration_consistency.py");
    let drifted_out = String::from_utf8_lossy(&drifted.stdout);
    assert!(
        !drifted.status.success(),
        "a changed naming convention must fail loudly, not pass vacuously: {drifted_out}"
    );
    assert!(drifted_out.contains("unaccounted_forward_migration"), "got: {drifted_out}");
    assert!(drifted_out.contains("empty_incremental_scan_surface"), "got: {drifted_out}");

    // A deleted file must not silently empty the scan surface either.
    fs::remove_file(scene.join("migrations").join(baseline)).unwrap();
    let deleted = run_gate_script(&scene, "scripts/check_migration_consistency.py");
    let deleted_out = String::from_utf8_lossy(&deleted.stdout);
    assert!(!deleted.status.success(), "deleting the only forward file must fail loudly: {deleted_out}");
    assert!(deleted_out.contains("missing_unified_baseline"), "got: {deleted_out}");

    fs::remove_dir_all(&scene).ok();
}
