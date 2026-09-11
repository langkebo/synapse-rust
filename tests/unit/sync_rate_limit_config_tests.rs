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
//! On 2026-09-11 both shipped `rate_limit.yaml` files had `sync.enabled: false`
//! while both `homeserver.yaml` files declared `rate_limit.sync.enabled: true`
//! — and because the file config replaces the whole `rate_limit:` section, the
//! file won. Measured consequence on the local stack:
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

/// Extracts the effective `sync.enabled` value from a `rate_limit.yaml`.
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

/// Extracts the effective `rate_limit.sync.enabled` from a `homeserver.yaml`
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

/// Every shipped rate-limit config file, with whether it is the production one.
const RATE_LIMIT_CONFIGS: [(&str, bool); 2] =
    [("docker/deploy/config/rate_limit.yaml", true), ("docker/config/rate_limit.yaml", false)];

// =============================================================================
// The core regression guard
// =============================================================================

/// The **production** `rate_limit.yaml` must keep the dedicated sync limiter
/// enabled. Disabling it leaves `/sync` completely unthrottled, because the
/// generic IP middleware exempts `/sync`.
#[test]
fn deploy_config_keeps_sync_rate_limiter_enabled() {
    let (path, _) = RATE_LIMIT_CONFIGS[0];
    let value = sync_enabled(&read(path));
    assert_eq!(
        value,
        Some(true),
        "{path} 必须保持 sync.enabled: true —— /sync 被路由 ledger 标记为 \
         rate_limit_exempt，不受通用 IP 限流约束，本段是它唯一的限流来源。\
         设为 false 时 `timeout=0` 紧循环可无限刷（实测 120/120 全 200）。"
    );
}

/// The bundled `rate_limit.yaml` must still move in lockstep with the
/// production constants — only `enabled` is allowed to differ (dev is laxer).
#[test]
fn both_rate_limit_configs_agree_on_sync_rule_values() {
    let deploy = read(RATE_LIMIT_CONFIGS[0].0);
    let dev = read(RATE_LIMIT_CONFIGS[1].0);

    let numbers = |yaml: &str| -> Vec<String> {
        yaml.lines()
            .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
            .filter(|l| l.starts_with("per_second:") || l.starts_with("burst_size:"))
            .collect()
    };

    assert_eq!(
        numbers(&deploy),
        numbers(&dev),
        "两份 rate_limit.yaml 的限流数值已经漂移；\
         它们靠手工 cp 同步（见 docker/deploy/README.md），没有 CI 检查"
    );
}

/// The dev config may relax the limiter, but if it does, it must say why —
/// otherwise it reads as a production template.
#[test]
fn dev_config_documents_why_it_is_laxer() {
    let (path, is_prod) = RATE_LIMIT_CONFIGS[1];
    assert!(!is_prod);
    let text = read(path);
    if sync_enabled(&text) == Some(false) {
        assert!(
            text.contains("生产") || text.contains("production"),
            "{path} 关闭了 sync 限流，必须注明这是开发用宽松值、生产见 deploy 配置；\
             否则容易被当作模板复制到生产"
        );
    }
}

// =============================================================================
// The contradiction that made this hard to spot
// =============================================================================

/// `homeserver.yaml` must not assert `rate_limit.sync.enabled: true` while the
/// file config says otherwise — that contradiction is what hid the bug.
///
/// The file config replaces the *entire* `rate_limit:` section (see
/// `src/web/middleware/rate_limit.rs`), so the nested declaration is inert.
/// If it is going to stay for documentation value it must be accompanied by a
/// note saying so; the deploy copy already has one.
#[test]
fn homeserver_yaml_does_not_contradict_the_file_config_silently() {
    for rel in ["docker/deploy/config/homeserver.yaml", "docker/config/homeserver.yaml"] {
        let text = read(rel);
        let declared = homeserver_rate_limit_sync_enabled(&text);
        if declared == Some(true) {
            let documented = text.contains("inert") || text.contains("rate_limit.yaml") || text.contains("replaces");
            assert!(
                documented,
                "{rel} 声明了 rate_limit.sync.enabled: true，但该段被 \
                 RATE_LIMIT_CONFIG_PATH 指向的文件整体替换、运行时**不生效**。\
                 必须像 docker/deploy/config/homeserver.yaml 那样加注释说明，\
                 否则会误导运维以为限流已开。"
            );
        }
    }
}

// =============================================================================
// Route-ledger exemption is the premise of the guard above
// =============================================================================

/// Re-assert the premise: `/sync` really is exempt from the generic limiter.
///
/// If sync stops being exempt, this file's reasoning changes and the
/// production limiter requirement should be revisited.
#[test]
fn sync_routes_are_still_exempt_from_the_generic_ip_limiter() {
    let source = read("src/web/routes/sync.rs");
    assert!(
        source.contains("with_rate_limit_exempt(true)"),
        "src/web/routes/sync.rs 不再把 /sync 标记为 exempt；\
         若它已回到通用 IP 限流覆盖范围，请重新评估本文件的守卫条件"
    );
}
