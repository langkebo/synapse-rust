//! Beacon info content parsing tests (Task A5).
//!
//! Locks the `m.beacon_info` state-event content contract: element-web and the
//! upstream matrix-js-sdk v40 `ContentHelpers.makeBeaconInfoContent` emit
//! *top-level* fields (`description`, `timeout`, `live`,
//! `org.matrix.msc3488.ts`, `org.matrix.msc3488.asset`). The backend must index
//! those without requiring the earlier nested `content["m.beacon_info"]` shape,
//! while still accepting that legacy nested shape as a fallback.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use synapse_rust::web::routes::handlers::room::state::parse_beacon_info_content;

const ROOM_ID: &str = "!beaconroom:example.com";
const EVENT_ID: &str = "$beaconevent:example.com";
const SENDER: &str = "@alice:example.com";
const FALLBACK_NOW: i64 = 1_000_000;

#[test]
fn parses_top_level_beacon_info_content_without_missing_beacon_info_error() {
    let content = json!({
        "description": "共享实时位置",
        "timeout": 86_400_000i64,
        "live": true,
        "org.matrix.msc3488.ts": 1_436_829_458_432i64,
        "org.matrix.msc3488.asset": { "type": "m.self" }
    });

    let params = parse_beacon_info_content(
        &content,
        ROOM_ID.to_string(),
        EVENT_ID.to_string(),
        SENDER.to_string(),
        SENDER.to_string(),
        FALLBACK_NOW,
    )
    .expect("top-level beacon_info content must parse without a Missing m.beacon_info error");

    assert_eq!(params.room_id, ROOM_ID);
    assert_eq!(params.event_id, EVENT_ID);
    assert_eq!(params.state_key, SENDER);
    assert_eq!(params.sender, SENDER);
    assert_eq!(params.timeout, 86_400_000);
    assert!(params.is_live);
    assert_eq!(params.description.as_deref(), Some("共享实时位置"));
    assert_eq!(params.asset_type, "m.self");
    assert_eq!(params.created_ts, 1_436_829_458_432);
}

#[test]
fn parses_top_level_content_with_m_stable_ts_and_asset() {
    let content = json!({
        "description": "stable fields",
        "timeout": 60_000i64,
        "live": false,
        "m.ts": 2_000i64,
        "m.asset": { "type": "m.pin" }
    });

    let params = parse_beacon_info_content(
        &content,
        ROOM_ID.into(),
        EVENT_ID.into(),
        SENDER.into(),
        SENDER.into(),
        FALLBACK_NOW,
    )
    .expect("top-level m.* fields must parse");

    assert_eq!(params.timeout, 60_000);
    assert!(!params.is_live);
    assert_eq!(params.description.as_deref(), Some("stable fields"));
    assert_eq!(params.asset_type, "m.pin");
    assert_eq!(params.created_ts, 2_000);
}

#[test]
fn top_level_content_defaults_live_asset_and_created_ts() {
    let content = json!({ "timeout": 42i64 });

    let params = parse_beacon_info_content(
        &content,
        ROOM_ID.into(),
        EVENT_ID.into(),
        SENDER.into(),
        SENDER.into(),
        FALLBACK_NOW,
    )
    .expect("timeout alone must parse");

    assert!(params.is_live, "live must default to true");
    assert_eq!(params.asset_type, "m.self");
    assert_eq!(params.created_ts, FALLBACK_NOW);
    assert!(params.description.is_none());
}

#[test]
fn parses_legacy_nested_beacon_info_content_as_fallback() {
    let content = json!({
        "m.beacon_info": {
            "description": "legacy nested",
            "timeout": 12_000i64,
            "live": true
        },
        "org.matrix.msc3488.asset": { "type": "m.self" }
    });

    let params = parse_beacon_info_content(
        &content,
        ROOM_ID.into(),
        EVENT_ID.into(),
        SENDER.into(),
        SENDER.into(),
        FALLBACK_NOW,
    )
    .expect("legacy nested beacon_info content must still parse");

    assert_eq!(params.timeout, 12_000);
    assert!(params.is_live);
    assert_eq!(params.description.as_deref(), Some("legacy nested"));
}

#[test]
fn rejects_content_missing_timeout_in_both_shapes() {
    let err = parse_beacon_info_content(
        &json!({ "description": "no timeout" }),
        ROOM_ID.into(),
        EVENT_ID.into(),
        SENDER.into(),
        SENDER.into(),
        FALLBACK_NOW,
    )
    .expect_err("content without timeout must be rejected");

    assert!(!err.message.contains("m.beacon_info"), "must not report a Missing m.beacon_info error");
    assert!(err.message.contains("timeout"), "error must reference the missing timeout field");
}
