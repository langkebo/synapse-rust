//! Guard tests for the container configuration bind mounts.
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
//! Mounting `./config` as a directory puts `postgres.conf` (wrong name) at that
//! path and postgres crash-loops with
//! `could not access the server configuration file ... No such file or directory`.
//! That was hit and reverted during this fix; the test below locks it in.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The compose files that define the runtime mounts.
const COMPOSE_FILES: [&str; 2] = ["docker/deploy/docker-compose.yml", "docker/docker-compose.yml"];

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("expected {p:?} to be readable: {e}"))
}

/// Returns the volume lines (trimmed) of a compose file that mention `token`.
fn mount_lines(compose: &str, token: &str) -> Vec<String> {
    compose.lines().map(str::trim).filter(|l| l.starts_with("- ") && l.contains(token)).map(str::to_string).collect()
}

// =============================================================================
// The app config directory must be mounted as a directory
// =============================================================================

/// Both compose files must mount the app config **directory**, not single files.
#[test]
fn app_config_is_mounted_as_a_directory() {
    for rel in COMPOSE_FILES {
        let compose = read(rel);

        assert!(
            compose.contains("- ./config:/app/config:ro"),
            "{rel} 必须把 config 挂载为**目录**（- ./config:/app/config:ro）。\
             单文件 bind mount 绑定 inode，宿主机原子替换后容器内路径会消失，\
             进程继续用旧配置服务（实测 No such file or directory + 必须重启恢复）。"
        );

        // No single-file app-config mounts may creep back in.
        assert!(
            mount_lines(&compose, "/app/config/").is_empty(),
            "{rel} 不应再出现挂载到 /app/config/ 下单个文件的条目：{:?}\n\
             请改回 `- ./config:/app/config:ro`。",
            mount_lines(&compose, "/app/config/")
        );
    }
}

/// The directory mount must be read-only — the server never writes its config.
#[test]
fn app_config_mount_is_read_only() {
    for rel in COMPOSE_FILES {
        let compose = read(rel);
        assert!(
            compose.contains("- ./config:/app/config:ro"),
            "{rel} 的 config 目录挂载必须带 :ro（服务只读配置，不应有写权限）"
        );
    }
}

// =============================================================================
// postgres.conf must stay a single-file mount (naming mismatch)
// =============================================================================

/// `postgres.conf` must remain a **single-file** mount.
///
/// On-disk name (`postgres.conf`) differs from the path postgres reads
/// (`/etc/postgresql/postgresql.conf`), so a directory mount breaks startup.
#[test]
fn postgres_config_stays_a_single_file_mount() {
    for rel in COMPOSE_FILES {
        let compose = read(rel);

        let mounts = mount_lines(&compose, "postgres.conf");
        assert!(
            !mounts.is_empty(),
            "{rel} 必须保留 postgres.conf 的单文件挂载；\
             改成目录挂载会让容器只看到 postgres.conf（而非 postgresql.conf），\
             postgres 将以 `could not access the server configuration file` 崩溃重启"
        );
        assert!(
            mounts.iter().any(|m| m.contains("/etc/postgresql/postgresql.conf")),
            "{rel}: postgres.conf 必须挂到 /etc/postgresql/postgresql.conf，实际: {mounts:?}"
        );
        assert!(
            !compose.contains("- ./config:/etc/postgresql"),
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
    for rel in COMPOSE_FILES {
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
