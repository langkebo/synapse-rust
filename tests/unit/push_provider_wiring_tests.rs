//! Static guard: the push notification service must have its providers initialized
//! exactly where it is constructed.
//!
//! `PushNotificationService::new` starts with every provider unset, and
//! `initialize_providers` is the only thing that reads `push_config` and builds
//! them. Before this guard existed the call site was missing entirely: every
//! delivery took the "provider unavailable" path, so no push was ever sent.
//! Deleting the call again would be completely silent in the type system, which is
//! why it is pinned here rather than left to review.

use std::fs;
use std::path::Path;

const ADMIN_WIRING: &str = "synapse-services/src/wiring/admin.rs";

fn admin_wiring_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(ADMIN_WIRING);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()))
}

/// The container wiring must call `initialize_providers()` after constructing the
/// push service and before handing it out as a shared `Arc`.
#[test]
fn admin_wiring_calls_initialize_providers() {
    let source = admin_wiring_source();

    let call_line = source
        .lines()
        .position(|line| !line.trim_start().starts_with("//") && line.contains("initialize_providers()"))
        .unwrap_or_else(|| {
            panic!(
                "{ADMIN_WIRING} must call `PushNotificationService::initialize_providers()`; without it every \
                 push delivery reports a failure (or is silently skipped) because no provider is ever built"
            )
        });

    let arc_line = source
        .lines()
        .position(|line| line.contains("Arc::new(push_notification_service)"))
        .unwrap_or_else(|| panic!("{ADMIN_WIRING} must hand the push service to the container as an `Arc`"));

    assert!(
        call_line < arc_line,
        "{ADMIN_WIRING}: `initialize_providers()` must run before the service is wrapped in `Arc` \
         (providers are only built while the service is still owned mutably)"
    );
}

/// The initialization result must be handled explicitly — neither silently dropped
/// nor able to abort startup, which would make push configuration a boot dependency.
#[test]
fn admin_wiring_handles_initialization_failure_explicitly() {
    let source = admin_wiring_source();
    let lines: Vec<&str> = source.lines().collect();

    let index = lines
        .iter()
        .position(|line| !line.trim_start().starts_with("//") && line.contains("initialize_providers()"))
        .expect("the call site must exist");

    // The result is consumed by an `if let Err(..)` one line up, so look at the
    // statement as a whole rather than only the text after the call.
    let start = index.saturating_sub(3);
    let statement = lines[start..=index].join("\n");

    assert!(
        statement.contains("Err("),
        "{ADMIN_WIRING} must match on the `initialize_providers()` result instead of discarding it; got:\n{statement}"
    );
    assert!(
        !statement.contains(".await?") && !statement.contains(".unwrap()"),
        "{ADMIN_WIRING} must not abort startup when the push config is unreadable; log the error and continue; got:\n{statement}"
    );
    assert!(
        source.contains("push_config") || source.contains("push providers"),
        "{ADMIN_WIRING} must explain why the initialization result is only logged"
    );
}

/// `push_notification_log.created_ts` is `NOT NULL` without a default, and omitting
/// it from the insert made every delivery log write fail with 23502 — which in turn
/// pushed already-delivered notifications back into the retry path.
#[test]
fn notification_log_insert_supplies_created_ts() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("synapse-storage/src/push_notification.rs");
    let source =
        fs::read_to_string(&path).unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));

    let insert = source.find("INSERT INTO push_notification_log (").expect("the notification log insert must exist");
    let statement = &source[insert..source.len().min(insert + 600)];

    assert!(
        statement.contains("created_ts"),
        "the `push_notification_log` insert must list `created_ts`: the column is NOT NULL with no default"
    );
}
