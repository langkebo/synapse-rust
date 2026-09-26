//! 守卫：联邦状态 PDU 的投影（`synapse_web::routes::federation::pdu`）。
//!
//! **这份守卫为什么存在。** `/send_join` v1+v2 的 `state` 数组曾经是 5 个键的
//! `messaging::get_state_events` 投影（`event_id`/`sender`/`type`/`content`/`state_key`），
//! `auth_chain` 是另一份 6 键手工拼装 —— 两者都**不是 PDU**：没有 `origin_server_ts`，
//! 也没有 `room_id`/`origin`/`depth`/`prev_events`/`auth_events`/`hashes`/`signatures`。
//! 对一个加入方来说这些字段是必须的，缺了既无法校验也无法据此建图。
//!
//! **判据（两类，都可被变异打红）：**
//!   * 形状：完整记录必须产出规范要求的**全部**顶层键；缺图元数据时必须**省略**
//!     （而不是填 `[]`/`0`）并报 `MissingGraphMetadata`；
//!   * 决策：`signature_action` 的表必须在「不完整」时**优先拒绝签名**，
//!     在残缺的 hashes/signatures 对上必须**回落到本机签名**而不是混搭。
//!
//! 变异自证见 `docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md` §21.5；
//! 简言之：给不完整分支补上 `prev_events: []` 派发、或把 `MissingGraphMetadata`
//! 的分支改成 `SignLocally`，下面 `incomplete_*` / `signature_decision_*` 必须转红。

use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;
use serde_json::{json, Value};
use synapse_storage::event::StateEvent;
use synapse_web::federation::signing::{sign_and_hash_event, verify_event_content_hash};
use synapse_web::routes::federation::pdu::{
    apply_stored_signature_material, is_auth_chain_member, signature_action, state_pdu, PduCompleteness,
    SignatureAction,
};

/// 规范要求的顶层键（`state_key` 仅状态事件有）。
const REQUIRED_KEYS: [&str; 8] =
    ["event_id", "room_id", "sender", "type", "content", "origin_server_ts", "origin", "state_key"];

/// 图元数据键：只有真实持久化时才允许出现。
const GRAPH_KEYS: [&str; 3] = ["depth", "prev_events", "auth_events"];

fn record() -> StateEvent {
    StateEvent {
        event_id: "$pdu1:server.example".to_string(),
        room_id: "!room:server.example".to_string(),
        sender: "@alice:server.example".to_string(),
        event_type: Some("m.room.member".to_string()),
        content: json!({"membership": "join"}),
        state_key: Some("@alice:server.example".to_string()),
        unsigned: None,
        is_redacted: Some(false),
        origin_server_ts: 1_700_000_000_000,
        depth: Some(7),
        processed_ts: None,
        not_before: None,
        status: None,
        origin: Some("server.example".to_string()),
        user_id: Some("@alice:server.example".to_string()),
        stream_ordering: Some(11),
        prev_events: Some(json!(["$parent:server.example"])),
        auth_events: Some(json!(["$create:server.example"])),
        signatures: None,
        hashes: None,
    }
}

fn stored_pair() -> (Value, Value) {
    (json!({"sha256": "c29tZS1oYXNo"}), json!({"server.example": {"ed25519:1": "c2ln"}}))
}

#[test]
fn complete_record_projects_every_required_key() {
    let (pdu, completeness) = state_pdu("server.example", &record(), None);

    assert_eq!(completeness, PduCompleteness::Complete);
    let object = pdu.as_object().expect("PDU must be a JSON object");
    for key in REQUIRED_KEYS {
        assert!(object.contains_key(key), "PDU is missing required key `{key}`: {pdu}");
    }
    for key in GRAPH_KEYS {
        assert!(object.contains_key(key), "complete PDU is missing graph key `{key}`: {pdu}");
    }

    // 值必须来自记录本身，而不是常量。
    assert_eq!(pdu["event_id"], json!("$pdu1:server.example"));
    assert_eq!(pdu["room_id"], json!("!room:server.example"));
    assert_eq!(pdu["type"], json!("m.room.member"));
    assert_eq!(pdu["depth"], json!(7));
    assert_eq!(pdu["prev_events"], json!(["$parent:server.example"]));
    assert_eq!(pdu["auth_events"], json!(["$create:server.example"]));
    assert_eq!(pdu["origin_server_ts"], json!(1_700_000_000_000_i64));
}

#[test]
fn incomplete_record_omits_graph_keys_instead_of_fabricating_them() {
    let mut incomplete = record();
    incomplete.depth = None;
    incomplete.prev_events = None;
    incomplete.auth_events = None;

    let (pdu, completeness) = state_pdu("server.example", &incomplete, None);

    assert_eq!(completeness, PduCompleteness::MissingGraphMetadata);
    let object = pdu.as_object().expect("PDU must be a JSON object");
    for key in GRAPH_KEYS {
        assert!(
            !object.contains_key(key),
            "incomplete PDU must omit `{key}` — an invented `[]`/`0` is a fabricated DAG \
             position that would make peers file the event as a root: {pdu}"
        );
    }
    // 其余必填键仍然必须在位：省略图字段不等于退化成旧的 5 键投影。
    for key in REQUIRED_KEYS {
        assert!(object.contains_key(key), "PDU is missing required key `{key}`: {pdu}");
    }
}

#[test]
fn non_array_graph_metadata_counts_as_missing() {
    for bad in [json!({}), json!("$parent:server.example"), json!([1, 2]), json!([])] {
        if bad == json!([]) {
            // 真正的空数组是合法图数据（如 m.room.create），必须判为 Complete。
            let mut create = record();
            create.prev_events = Some(json!([]));
            create.auth_events = Some(json!([]));
            let (_, completeness) = state_pdu("server.example", &create, None);
            assert_eq!(completeness, PduCompleteness::Complete, "empty array is valid graph data");
            continue;
        }
        let mut broken = record();
        broken.prev_events = Some(bad.clone());
        let (pdu, completeness) = state_pdu("server.example", &broken, None);
        assert_eq!(
            completeness,
            PduCompleteness::MissingGraphMetadata,
            "non-array prev_events {bad} must be treated as missing"
        );
        assert!(!pdu.as_object().expect("object").contains_key("prev_events"));
    }
}

#[test]
fn missing_origin_falls_back_to_this_server_and_self_is_normalised() {
    for origin in [None, Some(""), Some("self"), Some("undefined")] {
        let mut event = record();
        event.origin = origin.map(str::to_string);
        let (pdu, _) = state_pdu("server.example", &event, None);
        assert_eq!(pdu["origin"], json!("server.example"), "origin {origin:?} must normalise");
    }

    let mut remote = record();
    remote.origin = Some("remote.example".to_string());
    let (pdu, _) = state_pdu("server.example", &remote, None);
    assert_eq!(pdu["origin"], json!("remote.example"));
}

#[test]
fn signature_decision_refuses_incomplete_before_reusing_stored_material() {
    let mut incomplete = record();
    incomplete.depth = None;
    incomplete.prev_events = None;
    incomplete.auth_events = None;
    let (hashes, signatures) = stored_pair();
    incomplete.hashes = Some(hashes);
    incomplete.signatures = Some(signatures);

    assert_eq!(
        signature_action(&incomplete, PduCompleteness::MissingGraphMetadata),
        SignatureAction::RefuseIncomplete,
        "a malformed PDU must not become verifiable just because the row carries hashes"
    );
}

#[test]
fn signature_decision_keeps_only_a_complete_stored_pair() {
    let mut complete = record();

    let (hashes, signatures) = stored_pair();
    complete.hashes = Some(hashes);
    complete.signatures = Some(signatures.clone());
    assert_eq!(signature_action(&complete, PduCompleteness::Complete), SignatureAction::KeepStored);

    // 只有 hashes、没有 signatures ⇒ 必须回落到本机签名（混搭两份不同字节序列的
    // hashes/signatures 会产出永远校验不过的 PDU）。
    complete.signatures = None;
    assert_eq!(signature_action(&complete, PduCompleteness::Complete), SignatureAction::SignLocally);

    // 空 signatures 对象同样不算“已签名”。
    complete.signatures = Some(json!({}));
    assert_eq!(signature_action(&complete, PduCompleteness::Complete), SignatureAction::SignLocally);

    // 空 sha256 不算“已有内容哈希”。
    complete.hashes = Some(json!({"sha256": ""}));
    complete.signatures = Some(signatures);
    assert_eq!(signature_action(&complete, PduCompleteness::Complete), SignatureAction::SignLocally);

    // 两者都缺 ⇒ 本机签名。
    complete.hashes = None;
    complete.signatures = None;
    assert_eq!(signature_action(&complete, PduCompleteness::Complete), SignatureAction::SignLocally);
}

#[test]
fn stored_pair_is_attached_verbatim() {
    let mut complete = record();
    let (hashes, signatures) = stored_pair();
    complete.hashes = Some(hashes.clone());
    complete.signatures = Some(signatures.clone());

    let (mut pdu, _) = state_pdu("server.example", &complete, None);
    assert!(apply_stored_signature_material(&complete, &mut pdu));
    assert_eq!(pdu["hashes"], hashes, "stored hashes must be emitted byte-identical");
    assert_eq!(pdu["signatures"], signatures, "stored signatures must be emitted byte-identical");

    // 残缺材料不得附着。
    complete.hashes = None;
    let (mut pdu2, _) = state_pdu("server.example", &complete, None);
    assert!(!apply_stored_signature_material(&complete, &mut pdu2));
    assert!(pdu2.get("hashes").is_none());
}

/// R6: `event_id` is a PDU field for v1/v2 only.  Emitting it for a v3+ room
/// hands the receiver a PDU whose reference hash cannot match the ID the peer
/// derives (and which our own signer no longer covers).
#[test]
fn state_pdu_event_id_is_room_version_dependent() {
    let record = record();

    for version in ["3", "4", "10", "11", "12"] {
        let (pdu, _) = state_pdu("server.example", &record, Some(version));
        assert!(pdu.get("event_id").is_none(), "v{version} PDU must not carry event_id: {pdu}");
    }

    let (v1, _) = state_pdu("server.example", &record, Some("1"));
    assert_eq!(v1["event_id"], serde_json::json!(record.event_id), "v1 keeps the server-assigned id");
    let (v2, _) = state_pdu("server.example", &record, Some("2"));
    assert_eq!(v2["event_id"], serde_json::json!(record.event_id));

    // Unresolvable version keeps the historical shape; the caller logs it
    // (`build_pdus` warns) rather than guessing a version's redaction rules.
    let (unknown, _) = state_pdu("server.example", &record, None);
    assert!(unknown.get("event_id").is_some());
}

#[test]
fn projected_pdu_is_canonicalizable_and_round_trips_through_signing() {
    // 投影结果必须能被 `sign_and_hash_event` 规范化 + 签名，并随即通过内容哈希校验。
    // 这条锁住的是“投影出来的键/值不会让规范化失败或让哈希对不上”。
    let (mut pdu, completeness) = state_pdu("server.example", &record(), None);
    assert_eq!(completeness, PduCompleteness::Complete);

    let key_id = "ed25519:pdu_guard";
    let secret = STANDARD_NO_PAD.encode([7u8; 32]);
    sign_and_hash_event("10", "server.example", key_id, &secret, &mut pdu)
        .expect("signing a projected PDU must succeed");

    assert!(pdu["hashes"]["sha256"].as_str().is_some_and(|hash| !hash.is_empty()));
    assert!(pdu["signatures"]["server.example"][key_id].is_string());
    verify_event_content_hash(&pdu).expect("the projected PDU's content hash must verify");
}

#[test]
fn auth_chain_membership_rule_is_the_five_type_rule() {
    for event_type in
        ["m.room.create", "m.room.member", "m.room.power_levels", "m.room.join_rules", "m.room.history_visibility"]
    {
        let mut event = record();
        event.event_type = Some(event_type.to_string());
        assert!(is_auth_chain_member(&event), "{event_type} must be an auth-chain member");
    }

    for event_type in ["m.room.message", "m.room.name", "m.room.encryption", "m.reaction"] {
        let mut event = record();
        event.event_type = Some(event_type.to_string());
        assert!(!is_auth_chain_member(&event), "{event_type} must not be an auth-chain member");
    }

    let mut untyped = record();
    untyped.event_type = None;
    assert!(!is_auth_chain_member(&untyped), "NULL event_type must not panic nor count as auth chain");
}
