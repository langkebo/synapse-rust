//! Guard tests for the container configuration source and bind mounts.
//!
//! ## Why this file exists
//!
//! The compose files used to bind-mount **individual config files**:
//!
//! ```yaml
//! - ./config/homeserver.yaml:/app/config/homeserver.yaml:ro
//! - ./config/rate_limit.yaml:/app/config/rate_limit.yaml:ro
//! ```
//!
//! A single-file bind mount is bound to the host **inode**. An atomic replace
//! (write-temp-then-rename — what editors, `sed -i`, and config-management
//! tooling do) swaps the inode, and the container's mount keeps pointing at the
//! old one. The path then *disappears inside the container* while the process
//! keeps running and keeps serving the **stale** config.
//!
//! Measured on 2026-09-11 (docs/audit/P4_performance_baseline_2026-09-11.md §5.6):
//!
//! ```console
//! $ docker exec synapse-app head -2 /app/config/rate_limit.yaml
//! head: cannot open '/app/config/rate_limit.yaml' for reading: No such file or directory
//! $ docker logs synapse-app | grep 'Failed to reload'
//! WARN ... Failed to reload rate limit config: Failed to read config file:
//!      No such file or directory (os error 2)
//! ```
//!
//! The app was still enforcing the *old* limits and only a full
//! `docker compose restart` recovered. Mounting the **directory** makes the
//! container resolve files by name, so host-side replacement no longer matters.
//!
//! ## Why postgres is deliberately excluded
//!
//! `postgres.conf` must stay a single-file mount: the host file is named
//! `postgres.conf` but postgres reads `/etc/postgresql/postgresql.conf`.
//! Mounting the config directory at that path puts `postgres.conf` (wrong name)
//! there and postgres crash-loops with
//! `could not access the server configuration file ... No such file or directory`.
//! That was hit and reverted during this fix; the test below locks it in.
//!
//! ## Two stacks, one config source
//!
//! `docker/docker-compose.yml` (dev/CI) and `docker/deploy/docker-compose.yml`
//! (production) are **not** duplicates — different service sets and different
//! jobs — and are deliberately kept side by side. What must stay single is the
//! *config*: both mount `docker/config/`, and neither keeps a copy.
//! `docker/deploy/config/` used to be a hand-synced copy; it drifted (dev
//! `sync.enabled: false` vs deploy `true`,
//! see `docs/audit/S_series_verification_2026-09-11.md` §2) and `/sync` ended up
//! with no rate limiting at all. Same failure mode as the migrations duplicate
//! directory (`2b16dc3c`), same fix: one source, mounted by path.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The compose files that define runtime mounts, with the app-config mount line
/// each one must use to reach the single canonical `docker/config/` tree
/// (relative to the compose file's own directory).
const COMPOSE_FILES: [(&str, &str); 2] = [
    ("docker/deploy/docker-compose.yml", "- ../config:/app/config:ro"),
    ("docker/docker-compose.yml", "- ./config:/app/config:ro"),
];

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("expected {p:?} to be readable: {e}"))
}

/// Returns the volume lines (trimmed) of a compose file that mention `token`.
fn mount_lines(compose: &str, token: &str) -> Vec<String> {
    compose.lines().map(str::trim).filter(|l| l.starts_with("- ") && l.contains(token)).map(str::to_string).collect()
}

// =============================================================================
// Config lives in exactly one place
// =============================================================================

/// `docker/config/` is the only config tree; the deploy side must mount it
/// rather than keep a copy.
///
/// Mirrors `migration_consistency_tests::deploy_mounts_canonical_migrations_and_has_no_copy`.
#[test]
fn deploy_mounts_canonical_config_and_has_no_copy() {
    let root = repo_root();
    let canonical = root.join("docker/config");
    let deploy_config = root.join("docker/deploy/config");

    assert!(canonical.join("homeserver.yaml").exists(), "missing canonical homeserver.yaml");

    // A stale real directory (or a symlink) must not reappear: `deploy.sh`
    // resolves config paths from the repo root, so a copy here would silently
    // drift again — that is precisely what caused the /sync limiter outage.
    assert!(
        !deploy_config.exists() && !deploy_config.is_symlink(),
        "docker/deploy/config must not exist: docker/deploy/docker-compose.yml mounts \
         ../config directly, and a copy here would silently drift again"
    );

    // The compose file is the thing that actually wires the canonical directory in.
    let compose = read("docker/deploy/docker-compose.yml");
    assert!(
        compose.contains("- ../config:/app/config:ro"),
        "docker/deploy/docker-compose.yml 必须 bind-mount ../config:/app/config:ro（唯一配置源）"
    );
}

// =============================================================================
// Both stacks read that one source
// =============================================================================

/// Both stacks must mount the **same** config tree, each as a directory.
///
/// They are separate compose files for good reason (dev/CI vs production), but
/// a divergence in *which* config they read is exactly the bug that removed
/// `/sync` rate limiting on 2026-09-11.
#[test]
fn every_stack_mounts_the_single_config_source() {
    for (rel, expected) in COMPOSE_FILES {
        let compose = read(rel);
        assert!(
            compose.contains(expected),
            "{rel} 必须挂载唯一配置源（{expected}）—— \
             两栈的服务集合可以不同，但读的配置必须同一份，\
             否则会重现 2026-09-11 的 /sync 零限流（两侧 rate_limit.yaml 漂移）"
        );
    }
}

/// The config directory must be mounted as a **directory**, not as single files,
/// in every stack.
#[test]
fn app_config_is_mounted_as_a_directory() {
    for (rel, expected) in COMPOSE_FILES {
        let compose = read(rel);

        assert!(
            compose.contains(expected),
            "{rel} 必须把 config 挂载为**目录**（{expected}）。\
             单文件 bind mount 绑定 inode，宿主机原子替换后容器内路径会消失，\
             进程继续用旧配置服务（实测 No such file or directory + 必须重启恢复）。"
        );

        // No single-file app-config mounts may creep back in.
        assert!(
            mount_lines(&compose, "/app/config/").is_empty(),
            "{rel} 不应再出现挂载到 /app/config/ 下单个文件的条目：{:?}\n\
             请改回 `{expected}`。",
            mount_lines(&compose, "/app/config/")
        );
    }
}

// =============================================================================
// postgres.conf must stay a single-file mount (naming mismatch)
// =============================================================================

/// `postgres.conf` must remain a **single-file** mount in every stack.
///
/// On-disk name (`postgres.conf`) differs from the path postgres reads
/// (`/etc/postgresql/postgresql.conf`), so a directory mount breaks startup.
#[test]
fn postgres_config_stays_a_single_file_mount() {
    for (rel, _) in COMPOSE_FILES {
        let compose = read(rel);

        let mounts = mount_lines(&compose, "postgres.conf");
        assert!(
            !mounts.is_empty(),
            "{rel} 必须保留 postgres.conf 的单文件挂载；\
             改成目录挂载会让容器只看到 postgres.conf（而非 postgresql.conf），\
             postgres 将以 `could not access the server configuration file` 崩溃重启"
        );
        assert!(
            mounts.iter().any(|m| m.contains("config/postgres.conf") && m.contains("/etc/postgresql/postgresql.conf")),
            "{rel}: postgres.conf 必须挂到 /etc/postgresql/postgresql.conf，实际: {mounts:?}"
        );
        assert!(
            !compose.lines().map(str::trim).any(|l| l.ends_with(":/etc/postgresql:ro")),
            "{rel}: 不得把整个 config 目录挂到 /etc/postgresql（已实测会让 postgres 崩溃）"
        );
    }
}

// =============================================================================
// The explanation must survive, not just the directive
// =============================================================================

/// The directory mount must carry a comment explaining why, so a future
/// "tidy-up" does not revert it to the more obvious single-file form.
#[test]
fn directory_mount_is_documented_in_place() {
    for (rel, _) in COMPOSE_FILES {
        let compose = read(rel);
        let mentions_reason = compose.contains("inode") || compose.contains("原子替换") || compose.contains("§5.6");
        assert!(
            mentions_reason,
            "{rel} 的目录挂载必须就地说明原因（inode / 原子替换 / §5.6 引用），\
             否则后人很可能把它「简化」回单文件挂载并重新引入该缺陷"
        );
    }
}

/// The image must not already ship files at `/app/config` that the read-only
/// directory mount would shadow — the mount replaces the whole directory.
#[test]
fn image_does_not_preload_files_into_the_mounted_app_config_dir() {
    let dockerfile = read("docker/Dockerfile");
    // The image creates /app/config empty; defaults live in /app/config_defaults.
    assert!(
        dockerfile.contains("/app/config_defaults"),
        "Dockerfile 应把内置默认配置放在 /app/config_defaults（而非 /app/config），\
         这样整目录挂载 /app/config 才不会遮蔽它"
    );
}

// =============================================================================
// The deploy script must validate the canonical paths, not a local copy
// =============================================================================

/// `deploy.sh` must check (and must NOT create) the canonical config dir.
///
/// `mkdir -p ... config` used to run in the deploy directory, which would
/// recreate an empty `docker/deploy/config/` and mount it — the service would
/// then start with no configuration at all.
#[test]
fn deploy_sh_validates_canonical_config_and_creates_no_local_copy() {
    let script = read("docker/deploy/deploy.sh");
    assert!(
        script.contains("$PROJECT_ROOT/docker/config/"),
        "deploy.sh 必须校验 $PROJECT_ROOT/docker/config/ 下的配置文件存在"
    );
    assert!(
        !script.contains("mkdir -p ssl media logs backups config") && !script.contains("mkdir -p config"),
        "deploy.sh 不得在 deploy 目录下创建 config/ —— 那会重建一个空副本目录并被 compose 挂载，\
         服务将因缺少配置而启动失败"
    );
}

// =============================================================================
// H-8: 发布配置必须能反序列化进 Rust Config（serde 路径门禁）
// =============================================================================

/// `homeserver.yaml` 必须能完整反序列化为 `Config`。
///
/// 这是 H-8 的解决：
/// - `Config` 自带 `#[serde(deny_unknown_fields)]`（`synapse-common/src/config/mod.rs:126`）
/// - 反序列化失败说明配置含已删字段或未定义字段（H-1 的假旋钮即此例）
/// - 比旧方式（逐行 YAML 扫描，`sync_rate_limit_config_tests.rs:81`）可靠得多
/// - 零运行时成本，纯编译期 + 测试期断言
#[test]
fn homeserver_yaml_deserializes_into_config() {
    use synapse_common::config::Config;

    let root = repo_root();
    let yaml_path = root.join("docker/config/homeserver.yaml");
    let yaml_text = fs::read_to_string(&yaml_path)
        .unwrap_or_else(|e| panic!("docker/config/homeserver.yaml must be readable for serde round-trip test: {e}"));

    let cfg: Config = serde_yaml::from_str(&yaml_text)
        .unwrap_or_else(|e| panic!("homeserver.yaml must deserialize into Config: {e}"));

    // 基本 sanity 检查：确保反序列化产生了有意义的值
    assert!(!cfg.server.name.is_empty(), "server.name must be set");
    assert_eq!(cfg.database.max_size, 50, "database.max_size should default to 50 (pool_size removed in H-1)");
}

/// Verify that the homeserver.yaml does NOT contain the deprecated `database.pool_size`
/// field (H-1 fix). Note: `redis.pool_size` is still valid and present.
/// The serde deny_unknown_fields guard would have caught it in
/// `homeserver_yaml_deserializes_into_config` if it were in the wrong place.
#[test]
fn homeserver_yaml_has_no_deprecated_pool_size() {
    let root = repo_root();
    let yaml_text = fs::read_to_string(root.join("docker/config/homeserver.yaml")).expect("must read homeserver.yaml");

    // Check that database.pool_size specifically is absent.
    // We look for the pattern after "database:" section header
    let db_section_start = yaml_text.find("\ndatabase:\n").expect("database section must exist");
    let db_section_end = yaml_text.find("\nredis:\n").expect("redis section must exist");
    let db_section = &yaml_text[db_section_start..db_section_end];

    assert!(
        !db_section.contains("pool_size:"),
        "database.pool_size must not exist (H-1: removed as false knob; max_size is the real bound)"
    );

    // Verify redis.pool_size is still present (it's a valid field,
    // indented under the redis section header)
    let redis_section = yaml_text.find("\nredis:\n").expect("redis section must exist");
    assert!(
        &yaml_text[redis_section..].split('\n').take(20).collect::<String>().contains("pool_size:"),
        "redis.pool_size should still exist (valid Redis pool config)"
    );
}
