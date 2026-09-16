//! Guard tests for the `/sync` rate-limit configuration.
//!
//! ## Why this file exists
//!
//! `/_matrix/client/{r0,v3}/sync` (and the sliding-sync variants) are marked
//! `rate_limit_exempt` in the route ledger, so they are **not** covered by the
//! generic per-IP middleware limit (`default: per_second: 50`). The dedicated
//! `sync:` section of `rate_limit.yaml` is therefore the *only* limiter for
//! `/sync`.
//!
//! On 2026-09-11 the shipped `rate_limit.yaml` had `sync.enabled: false` while
//! `homeserver.yaml` declared `rate_limit.sync.enabled: true` — and because the
//! file config replaces the whole `rate_limit:` section, the file won. Measured
//! consequence on the local stack:
//!
//! ```console
//! # config: sync.enabled = false
//! $ for i in $(seq 1 120); do curl ... "/_matrix/client/v3/sync?timeout=0"; done | sort | uniq -c
//!     120 200          # zero throttling
//!
//! # config: sync.enabled = true (this fix)
//! $ ... | sort | uniq -c
//!      15 200
//!     105 429          # M_LIMIT_EXCEEDED + retry_after_ms
//! ```
//!
//! So an authenticated user could spin `timeout=0` and generate unbounded DB
//! read amplification. These tests make that regression impossible to
//! reintroduce silently.
//!
//! ## Single config source
//!
//! `docker/config/` is the only config tree: `docker/deploy/docker-compose.yml`
//! mounts `../config`, and `docker/Dockerfile` bakes the same files into the
//! image. There is no longer a second copy to drift against (the dev/prod
//! divergence that caused the 2026-09-11 outage came from exactly such a copy).

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Reads a config file relative to the repo root.
fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("expected {p:?} to be readable: {e}"))
}

/// Extracts the effective `sync.enabled` value from `rate_limit.yaml`.
///
/// Deliberately a tiny purpose-built parser: the file is flat and the key
/// appears exactly once. Comments are stripped so a commented-out line cannot
/// masquerade as the setting.
fn sync_enabled(yaml: &str) -> Option<bool> {
    let mut in_sync = false;
    for raw in yaml.lines() {
        let line = raw.trim_end();
        let code = line.split('#').next().unwrap_or("").trim_end();
        if code.is_empty() {
            continue;
        }
        // Top-level key (no leading whitespace).
        if !code.starts_with(' ') && !code.starts_with('\t') {
            in_sync = code.trim() == "sync:";
            continue;
        }
        if in_sync {
            let t = code.trim();
            if let Some(rest) = t.strip_prefix("enabled:") {
                return Some(rest.trim() == "true");
            }
        }
    }
    None
}

/// Extracts the effective `rate_limit.sync.enabled` from `homeserver.yaml`
/// (nested under the top-level `rate_limit:` key).
fn homeserver_rate_limit_sync_enabled(yaml: &str) -> Option<bool> {
    let mut in_rate_limit = false;
    let mut in_sync = false;
    for raw in yaml.lines() {
        let line = raw.trim_end();
        let code = line.split('#').next().unwrap_or("").trim_end();
        if code.is_empty() {
            continue;
        }
        let indent = code.len() - code.trim_start().len();
        let t = code.trim();

        if indent == 0 {
            in_rate_limit = t == "rate_limit:";
            in_sync = false;
            continue;
        }
        if in_rate_limit {
            if indent == 2 {
                in_sync = t == "sync:";
                continue;
            }
            if in_sync {
                if let Some(rest) = t.strip_prefix("enabled:") {
                    return Some(rest.trim() == "true");
                }
            }
        }
    }
    None
}

/// The single shipped rate-limit config (canonical, mounted by deploy compose
/// and baked into the image).
const RATE_LIMIT_CONFIG: &str = "docker/config/rate_limit.yaml";
const HOMESERVER_CONFIG: &str = "docker/config/homeserver.yaml";

// =============================================================================
// The core regression guard
// =============================================================================

/// The shipped `rate_limit.yaml` must keep the dedicated sync limiter enabled.
/// Disabling it leaves `/sync` completely unthrottled, because the generic IP
/// middleware exempts `/sync`.
#[test]
fn bundled_config_keeps_sync_rate_limiter_enabled() {
    let value = sync_enabled(&read(RATE_LIMIT_CONFIG));
    assert_eq!(
        value,
        Some(true),
        "{RATE_LIMIT_CONFIG} 必须保持 sync.enabled: true —— /sync 被路由 ledger 标记为 \
         rate_limit_exempt，不受通用 IP 限流约束，本段是它唯一的限流来源。\
         设为 false 时 `timeout=0` 紧循环可无限刷（实测 120/120 全 200）。"
    );
}

// =============================================================================
// The contradiction that made this hard to spot
// =============================================================================

/// `homeserver.yaml` must not assert `rate_limit.sync.enabled: true` while the
/// file config says otherwise — that contradiction is what hid the bug.
///
/// The file config replaces the *entire* `rate_limit:` section (see
/// `synapse-web/src/middleware/rate_limit.rs`), so the nested declaration is inert.
/// If it is going to stay for documentation value it must be accompanied by a
/// note saying so.
#[test]
fn homeserver_yaml_does_not_contradict_the_file_config_silently() {
    let text = read(HOMESERVER_CONFIG);
    if homeserver_rate_limit_sync_enabled(&text) == Some(true) {
        let documented = text.contains("inert") || text.contains("rate_limit.yaml") || text.contains("不生效");
        assert!(
            documented,
            "{HOMESERVER_CONFIG} 声明了 rate_limit.sync.enabled: true，但该段被 \
             RATE_LIMIT_CONFIG_PATH 指向的文件整体替换、运行时**不生效**。\
             必须加注释说明，否则会误导运维以为限流已开。"
        );
    }
}

// =============================================================================
// Route-ledger exemption is the premise of the guard above
// =============================================================================

/// Re-assert the premise: `/sync` really is exempt from the generic limiter.
///
/// If sync stops being exempt, this file's reasoning changes and the
/// production limiter requirement should be revisited.
///
/// B2-2 deleted `sync::sync_route_manifest()`, so the annotation no longer
/// lives in `sync.rs`: it lives in `scripts/contract/ledger_annotations.txt`
/// and is materialised into the derived route table. Asserting on the ledger
/// (rather than on source text) checks the thing the limiter actually reads.
#[test]
fn sync_routes_are_still_exempt_from_the_generic_ip_limiter() {
    use synapse_web::routes::declared_ledger_all;

    let exempt: Vec<&str> = declared_ledger_all().iter().filter(|e| e.rate_limit_exempt).map(|e| e.path).collect();
    assert!(
        exempt.contains(&"/_matrix/client/v3/sync"),
        "/sync 不再被标记为 rate-limit exempt（派生表 + ledger_annotations.txt）；\
         若它已回到通用 IP 限流覆盖范围，请重新评估本文件的守卫条件。\
         当前 exempt 列表：{exempt:?}"
    );
}

// =============================================================================
// Dead surface removal (P5 死代码)
// =============================================================================

/// `RateLimitConfigAdapter` must stay deleted.
///
/// It declared a full duplicate of `RateLimitConfigFile`'s field set plus a
/// `From<RateLimitConfigFile>` impl, but **nothing ever constructed it**. Its
/// stated purpose ("B-1: leaf types are unified, so this is a straight field
/// move") described a bridge between two types that had already been unified —
/// so the adapter was a leftover with no callers, and every field of
/// `RateLimitConfigFile` was read directly instead.
///
/// A public duplicate of a config struct is a real maintenance hazard: a field
/// added to `RateLimitConfigFile` would silently *not* propagate to the
/// adapter, and any new caller would read stale semantics.
#[test]
fn rate_limit_config_adapter_stays_deleted() {
    let source = fs::read_to_string(repo_root().join("synapse-common/src/rate_limit_config.rs"))
        .expect("rate_limit_config.rs must be readable");
    assert!(
        !source.contains("RateLimitConfigAdapter"),
        "RateLimitConfigAdapter 应保持删除状态：它从未被构造，\
         且其存在理由（桥接两个已统一的叶类型）已消失。\
         若确实需要，请连同真实调用方一起提交。"
    );

    let lib = fs::read_to_string(repo_root().join("synapse-common/src/lib.rs")).expect("lib.rs must be readable");
    assert!(!lib.contains("RateLimitConfigAdapter"), "不应再从 synapse-common 重导出 RateLimitConfigAdapter");
}
