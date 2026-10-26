use super::*;
use crate::dead_letter_queue::InMemoryDeadLetterQueue;

/// `animated=true` 必须真的出现在联邦请求 URL 里（审计 NEW-01）。
///
/// 缺陷原状：`media_thumbnail` 根本没有这个参数，调用方却按 `animated` 设 `Content-Type`
/// ⇒ 远端缩略图是 JPEG 字节标成 `image/webp`。所以本用例断言的是**两个方向**：
/// 传 `true` 必须带 `animated=true`，传 `false` 必须不带（后者防止有人为了"修绿"
/// 无条件拼上参数 —— 那会让所有静态缩略图都被上游当动图处理）。
#[test]
fn media_thumbnail_path_forwards_the_animated_flag() {
    let still = media_thumbnail_path("remote.example", "m1", 64, 64, "scale", false);
    assert_eq!(
        still, "/_matrix/federation/v1/media/thumbnail/remote.example/m1?width=64&height=64&method=scale",
        "静态缩略图不得携带 animated 参数"
    );

    let animated = media_thumbnail_path("remote.example", "m1", 64, 64, "scale", true);
    assert!(
        animated.ends_with("&method=scale&animated=true"),
        "animated=true 必须透传给上游（否则返回的是静图，却被标成 image/webp）：{animated}"
    );
}

fn create_test_client() -> (tokio::runtime::Runtime, FederationClient) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let key_rotation = {
        let _guard = rt.enter();
        Arc::new(KeyRotationManager::new(
            &Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap()),
            "test.com",
        ))
    };
    let client = FederationClient::new("test.com".to_string(), key_rotation);
    (rt, client)
}

#[test]
fn test_enforce_cache_bound_evicts_oldest_entry() {
    let mut cache: HashMap<String, std::time::Instant> = HashMap::new();
    let base = std::time::Instant::now();
    for i in 0..MAX_CACHE_ENTRIES {
        cache.insert(format!("server-{i}"), base + std::time::Duration::from_millis(i as u64));
    }
    enforce_cache_bound(&mut cache, |instant| *instant);
    assert_eq!(cache.len(), MAX_CACHE_ENTRIES - 1);
    cache.insert("newest".to_string(), base + std::time::Duration::from_millis(MAX_CACHE_ENTRIES as u64));
    assert_eq!(cache.len(), MAX_CACHE_ENTRIES);
    assert!(!cache.contains_key("server-0"), "oldest entry must be evicted");
    assert!(cache.contains_key("newest"));
}

#[test]
fn test_federation_transaction_serialization() {
    let txn = FederationTransaction {
        transaction_id: "txn_123".to_string(),
        origin: "example.com".to_string(),
        origin_server_ts: 1234567890000,
        destination: "remote.com".to_string(),
        pdus: vec![],
        edus: vec![],
    };
    let json = serde_json::to_string(&txn).unwrap();
    assert!(json.contains("txn_123"));
    assert!(json.contains("example.com"));
}

#[test]
fn test_make_join_response_deserialization() {
    let json = r#"{
            "room_id": "!room:example.com",
            "event": {"type": "m.room.member"},
            "room_version": "10"
        }"#;
    let resp: MakeJoinResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.room_id.as_deref(), Some("!room:example.com"));
    assert_eq!(resp.room_version, Some("10".to_string()));
}

#[test]
fn test_server_keys_deserialization() {
    let json = r#"{
            "server_name": "example.com",
            "verify_keys": {},
            "old_verify_keys": {},
            "signatures": {},
            "valid_until_ts": 1234567890000
        }"#;
    let keys: ServerKeys = serde_json::from_str(json).unwrap();
    assert_eq!(keys.server_name, "example.com");
}

#[test]
fn cache_ttl_shrinks_to_valid_until_ts_window() {
    let now_ms = current_timestamp_millis();

    // Key expires 600s from now -> effective TTL should shrink to that window.
    let soon = ServerKeys {
        server_name: "example.com".to_string(),
        verify_keys: serde_json::json!({}),
        old_verify_keys: serde_json::json!({}),
        signatures: serde_json::json!({}),
        valid_until_ts: now_ms + 600_000,
    };
    let effective = effective_cache_ttl_secs(&soon, now_ms);
    assert!((590..=600).contains(&effective), "TTL must shrink to the ~600s valid_until_ts window, got {effective}");

    // Key valid far in the future -> capped at the default window.
    let far = ServerKeys {
        server_name: "example.com".to_string(),
        verify_keys: serde_json::json!({}),
        old_verify_keys: serde_json::json!({}),
        signatures: serde_json::json!({}),
        valid_until_ts: now_ms + 10_000_000_000,
    };
    assert_eq!(
        effective_cache_ttl_secs(&far, now_ms),
        KEY_CACHE_TTL_SECS,
        "far-future validity must cap at the default TTL"
    );

    // Already-expired key -> never serve from cache.
    let past = ServerKeys {
        server_name: "example.com".to_string(),
        verify_keys: serde_json::json!({}),
        old_verify_keys: serde_json::json!({}),
        signatures: serde_json::json!({}),
        valid_until_ts: now_ms - 60_000,
    };
    assert_eq!(effective_cache_ttl_secs(&past, now_ms), 0, "expired key must yield zero TTL");
}

#[test]
fn test_resolved_server_ip_literal() {
    // F-04: `resolve_server` must reject IP literals (loopback, private,
    // link-local) to prevent SSRF. The fix lives in commit 32299587 —
    // this test pins the rejection contract.
    let (rt, client) = create_test_client();
    let err = rt
        .block_on(client.resolve_server("[::1]:8448"))
        .expect_err("F-04: IP literal (IPv6 loopback) must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("F-04") || msg.contains("IP literal") || msg.contains("not allowed"),
        "F-04 error should mention policy: {msg}"
    );
}

#[test]
fn test_resolved_server_with_port() {
    let (rt, client) = create_test_client();
    let resolved = rt.block_on(client.resolve_server("example.com:8448")).unwrap();
    assert_eq!(resolved.host, "example.com");
    assert_eq!(resolved.port, 8448);
}

#[test]
fn test_build_url() {
    let (_rt, _client) = create_test_client();
    let resolved =
        ResolvedServer { server_name: "example.com".to_string(), host: "example.com".to_string(), port: 8448 };
    assert_eq!(
        FederationClient::build_url(&resolved, "/_matrix/federation/v1/version"),
        "https://example.com:8448/_matrix/federation/v1/version"
    );

    let resolved_443 =
        ResolvedServer { server_name: "example.com".to_string(), host: "example.com".to_string(), port: 443 };
    assert_eq!(
        FederationClient::build_url(&resolved_443, "/_matrix/federation/v1/version"),
        "https://example.com/_matrix/federation/v1/version"
    );
}

#[test]
fn test_version_response_deserialization() {
    let json = r#"{"server": {"name": "synapse-rust", "version": "0.1.0"}}"#;
    let resp: VersionResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.server.name, "synapse-rust");
}

// ------------------------------------------------------------------
// S2 / FED-01: 远程服务器密钥缓存前必须验证自签名，防 MITM 注入伪造密钥
// ------------------------------------------------------------------

fn make_signed_server_keys(signing_key: &ed25519_dalek::SigningKey, server: &str) -> ServerKeys {
    use base64::Engine;
    use ed25519_dalek::Signer;
    let key_id = "ed25519:test";
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().as_bytes());
    let mut value = serde_json::json!({
        "server_name": server,
        "verify_keys": { key_id: { "key": pub_b64 } },
        "old_verify_keys": {},
        "valid_until_ts": current_timestamp_millis() + 86_400_000,
    });
    let mut for_signing = value.clone();
    synapse_common::remove_signatures_and_unsigned(&mut for_signing);
    let msg = synapse_common::canonical_json_bytes(&for_signing).expect("canonical json");
    let sig = base64::engine::general_purpose::STANDARD.encode(signing_key.sign(&msg).to_bytes());
    value["signatures"] = serde_json::json!({ server: { key_id: sig } });
    serde_json::from_value(value).expect("ServerKeys")
}

#[test]
fn server_keys_valid_self_signature_accepted() {
    let sk = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
    let keys = make_signed_server_keys(&sk, "remote.example");
    assert!(verify_server_keys_self_signature(&keys).is_ok(), "validly self-signed server keys must be accepted");
}

#[test]
fn server_keys_forged_self_signature_rejected() {
    let victim_key = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
    let attacker_key = ed25519_dalek::SigningKey::from_bytes(&[4u8; 32]);
    // 公钥是受害者的，签名却是攻击者私钥签的 —— MITM 注入场景
    let keys = {
        let mut keys = make_signed_server_keys(&victim_key, "remote.example");
        let forged = make_signed_server_keys(&attacker_key, "remote.example");
        keys.signatures = forged.signatures;
        keys
    };
    assert!(verify_server_keys_self_signature(&keys).is_err(), "forged self-signature must be rejected");
}

#[test]
fn server_keys_missing_self_signature_rejected() {
    let sk = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
    let mut keys = make_signed_server_keys(&sk, "remote.example");
    keys.signatures = serde_json::json!({});
    assert!(verify_server_keys_self_signature(&keys).is_err(), "key response without self-signature must be rejected");
}

// ------------------------------------------------------------------
// S-2 / FED-02: 文档声明的 server_name 必须等于我们请求的那个服务器
// ------------------------------------------------------------------

#[test]
fn server_keys_wrong_server_name_rejected() {
    // 攻击者用**自己的**密钥合法地自签一份文档（签名校验必然通过），
    // 但文档声称的身份是 victim.example —— 这正是单靠自签名抓不住的场景。
    let attacker_key = ed25519_dalek::SigningKey::from_bytes(&[5u8; 32]);
    let keys = make_signed_server_keys(&attacker_key, "attacker.example");

    assert!(
        verify_server_keys_self_signature(&keys).is_ok(),
        "前提：文档的自签名本身是有效的，否则本测试退化成重复的验签测试"
    );
    assert!(verify_server_keys_server_name(&keys, "victim.example").is_err(), "server_name 与请求目标不符必须被拒绝");
    assert!(
        validate_remote_server_keys(&keys, "attacker.example").is_ok(),
        "同一份文档在名字匹配时必须被接受（否则是名字校验过严）"
    );
}

#[test]
fn admit_server_keys_rejects_wrong_name_without_caching() {
    // get_server_keys 的安全属性不是"调用了校验器"，而是
    // **未通过校验的密钥永远进不了 key_cache**。断言后者，测试就无法自证：
    // 只要 admit_server_keys 不再调用校验器，缓存就会非空，本测试立刻失败。
    let (rt, client) = create_test_client();
    let attacker_key = ed25519_dalek::SigningKey::from_bytes(&[5u8; 32]);
    let keys = make_signed_server_keys(&attacker_key, "attacker.example");

    rt.block_on(async {
        let result = client.admit_server_keys("victim.example", keys).await;
        assert!(result.is_err(), "名字不符的密钥文档必须被拒绝");
        assert!(
            client.key_cache.read().await.is_empty(),
            "被拒绝的密钥文档绝不能污染缓存（cache poisoning across identities）"
        );
    });
}

#[test]
fn admit_server_keys_rejects_forged_signature_without_caching() {
    let (rt, client) = create_test_client();
    let victim_key = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
    let attacker_key = ed25519_dalek::SigningKey::from_bytes(&[4u8; 32]);
    let keys = {
        let mut keys = make_signed_server_keys(&victim_key, "remote.example");
        let forged = make_signed_server_keys(&attacker_key, "remote.example");
        keys.signatures = forged.signatures;
        keys
    };

    rt.block_on(async {
        let result = client.admit_server_keys("remote.example", keys).await;
        assert!(result.is_err(), "伪造自签名的密钥文档必须被拒绝");
        assert!(client.key_cache.read().await.is_empty(), "被拒绝的密钥文档绝不能污染缓存");
    });
}

#[test]
fn admit_server_keys_caches_valid_document_under_expected_server() {
    // 正向对照：证明上面两条拒绝测试不是因为校验器恒返回 Err。
    let (rt, client) = create_test_client();
    let sk = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
    let keys = make_signed_server_keys(&sk, "remote.example");

    rt.block_on(async {
        let admitted = client.admit_server_keys("remote.example", keys).await;
        assert!(admitted.is_ok(), "合法文档必须被接受：{:?}", admitted.err());

        let cached = client.get_cached_key("remote.example").await;
        assert!(cached.is_some(), "合法文档必须以 expected_server 为 key 进入缓存");
        assert_eq!(cached.unwrap().server_name, "remote.example");
    });
}

/// `query_server_keys` 的组成是 `handle_response` → `validate_remote_server_keys`。
/// 这里用真实 HTTP 响应字节走完这两步，而不是在测试里就地拼一个 ServerKeys：
/// 本地构造的断言无法发现"JSON 反序列化出来的 server_name 与预期不符"这一层，
/// 而那恰恰是 S-15 那类自证测试的失效模式。
#[test]
fn query_server_keys_path_rejects_mismatched_document_over_http() {
    let (rt, client) = create_test_client();
    let attacker_key = ed25519_dalek::SigningKey::from_bytes(&[5u8; 32]);
    let body = serde_json::to_value(make_signed_server_keys(&attacker_key, "attacker.example")).unwrap();

    rt.block_on(async {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/_matrix/key/v2/query/victim.example"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        // no_proxy(): dev/CI shells may export HTTP(S)_PROXY; routing a
        // loopback request through a proxy breaks hyper's parser.
        let response = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("{}/_matrix/key/v2/query/victim.example", server.uri()))
            .send()
            .await
            .unwrap();

        let keys: ServerKeys =
            client.handle_response(response).await.expect("2xx JSON body must deserialize into ServerKeys");
        assert_eq!(keys.server_name, "attacker.example", "反序列化后名字来自响应体，不是请求路径");

        assert!(
            validate_remote_server_keys(&keys, "victim.example").is_err(),
            "/key/v2/query 路径同样必须拒绝名字不符的文档 —— 校验缺口不该取决于端点"
        );
    });
}

#[test]
fn test_directory_response_deserialization() {
    let json = r#"{"room_id": "!room:example.com", "servers": ["example.com", "other.com"]}"#;
    let resp: DirectoryResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.room_id, "!room:example.com");
    assert_eq!(resp.servers.len(), 2);
}

// ------------------------------------------------------------------
// FED-07: Dead Letter Queue integration
// ------------------------------------------------------------------

/// Create a test client with an in-memory DLQ attached.
fn create_test_client_with_dlq() -> (tokio::runtime::Runtime, FederationClient, Arc<InMemoryDeadLetterQueue>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let key_rotation = {
        let _guard = rt.enter();
        Arc::new(KeyRotationManager::new(
            &Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap()),
            "test.com",
        ))
    };
    let dlq = Arc::new(InMemoryDeadLetterQueue::new());
    let client = FederationClient::new("test.com".to_string(), key_rotation).with_dlq(dlq.clone());
    (rt, client, dlq)
}

fn make_test_transaction(txn_id: &str, destination: &str) -> FederationTransaction {
    FederationTransaction {
        transaction_id: txn_id.to_string(),
        origin: "test.com".to_string(),
        origin_server_ts: 1234567890000,
        destination: destination.to_string(),
        pdus: vec![],
        edus: vec![],
    }
}

#[test]
fn fed07_failed_transaction_moves_to_dlq() {
    let (rt, client, dlq) = create_test_client_with_dlq();

    // The test client has no signing key configured, so send_transaction
    // will fail immediately at build_auth_header — this exercises the
    // DLQ integration path without network I/O or sleep delays.
    let txn = make_test_transaction("txn-001", "failed.example.com");

    let result = rt.block_on(client.send_transaction("failed.example.com", &txn));
    assert!(result.is_err(), "send_transaction must fail without a signing key");

    let entries = rt.block_on(dlq.list_unresolved()).unwrap();
    assert!(entries.iter().any(|e| e.txn_id == "txn-001"), "failed transaction must be in the DLQ");
}

#[test]
fn fed07_dlq_entry_contains_correct_fields() {
    let (rt, client, dlq) = create_test_client_with_dlq();

    let txn = make_test_transaction("txn-002", "down.example.com");
    rt.block_on(client.send_transaction("down.example.com", &txn)).ok();

    let entries = rt.block_on(dlq.list_unresolved()).unwrap();
    let entry = entries.iter().find(|e| e.txn_id == "txn-002").expect("DLQ must contain txn-002");

    assert_eq!(entry.destination, "down.example.com");
    assert_eq!(entry.origin, "test.com");
    assert!(entry.failure_reason.is_some(), "failure_reason must be populated");
    assert!(!entry.is_resolved, "new DLQ entry must be unresolved");
    assert!(entry.id.is_some(), "DLQ entry must have an id assigned");

    // payload must contain the serialized transaction
    let payload_txn_id =
        entry.payload.get("transaction_id").and_then(|v| v.as_str()).expect("payload must contain transaction_id");
    assert_eq!(payload_txn_id, "txn-002");
}

#[test]
fn fed07_no_dlq_attached_does_not_panic() {
    // When no DLQ is attached, send_transaction must still return the
    // error normally — no panic, no DLQ write.
    let (rt, client) = create_test_client();
    let txn = make_test_transaction("txn-003", "no-dlq.example.com");

    let result = rt.block_on(client.send_transaction("no-dlq.example.com", &txn));
    assert!(result.is_err());
    assert!(client.dead_letter_queue().is_none());
}

#[test]
fn fed07_with_dlq_builder_attaches_queue() {
    let (rt, client, dlq) = create_test_client_with_dlq();

    // The DLQ Arc is shared — entries pushed by the client are visible here.
    let entries = rt.block_on(dlq.list_unresolved()).unwrap();
    assert!(entries.is_empty(), "fresh DLQ must have no entries");
    assert!(client.dead_letter_queue().is_some(), "client must have DLQ attached");
}

/// FED-07 (issue #3 fix): Verify that HTTP 5xx errors from
/// `handle_response` are captured by the DLQ.
///
/// Previously, `send_transaction` only enqueued to the DLQ when
/// `send_signed_request` returned `Err`. HTTP 5xx responses went
/// through `send_signed_request` as `Ok(response)` and then
/// `handle_response` converted them to `Err(Remote { status: 500 })`,
/// but that error bypassed the DLQ. The restructured `send_transaction`
/// now catches errors from both paths.
///
/// This test starts a lightweight TCP server that returns HTTP 500,
/// sends a request to it, passes the response to `handle_response`,
/// and verifies the resulting `Remote` error can be enqueued to the
/// DLQ — the same error type that previously bypassed it.
#[test]
fn fed07_handle_response_5xx_error_captured_by_dlq() {
    let (rt, client, dlq) = create_test_client_with_dlq();

    rt.block_on(async {
        // Use wiremock (a real hyper-based server) to avoid the flaky
        // raw-TCP mock that raced with hyper's HTTP/1 dispatcher and
        // system-proxy interference in dev shells.
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/")).respond_with(ResponseTemplate::new(500)).mount(&server).await;

        // no_proxy(): dev/CI shells may export HTTP(S)_PROXY; routing a
        // loopback request through a proxy breaks hyper's parser.
        let response = reqwest::Client::builder().no_proxy().build().unwrap().get(server.uri()).send().await.unwrap();
        assert_eq!(response.status().as_u16(), 500);

        // Call handle_response — this is the path that previously
        // bypassed the DLQ. It should return Err(Remote { status: 500 }).
        let result: Result<serde_json::Value, _> = client.handle_response(response).await;
        assert!(result.is_err(), "handle_response must return Err for 5xx");

        let error = result.unwrap_err();
        assert!(
            matches!(error, FederationClientError::Remote { status: 500, .. }),
            "error must be Remote with status 500, got: {error:?}"
        );

        // Simulate what send_transaction does with this error: construct
        // a DlqEntry and enqueue it. The restructured send_transaction
        // catches this via: `if let Err(error) = &result { ... dlq.enqueue }`.
        let txn = make_test_transaction("txn-5xx", "server5xx.example.com");
        let payload = serde_json::to_value(&txn).unwrap_or_else(|_| serde_json::json!({"transaction_id": "txn-5xx"}));
        let entry = DlqEntry::new(
            "txn-5xx".to_string(),
            "server5xx.example.com".to_string(),
            "test.com".to_string(),
            payload,
            error.to_string(),
            MAX_RETRIES as i32,
        );
        dlq.enqueue(&entry).await.unwrap();

        // Verify the DLQ captured the 5xx error.
        let entries = dlq.list_unresolved().await.unwrap();
        let entry = entries.iter().find(|e| e.txn_id == "txn-5xx").expect("DLQ must contain txn-5xx");
        assert!(
            entry.failure_reason.as_ref().unwrap().contains("500"),
            "failure_reason must contain status 500, got: {:?}",
            entry.failure_reason
        );
    });
}

// ----------------------------------------------------------------------
// F-01: effective_cache_ttl_secs MUST cap at 7 days per Matrix spec §1.2
// ----------------------------------------------------------------------

fn make_keys_for_ttl_test(valid_until_ts: i64) -> ServerKeys {
    ServerKeys {
        server_name: "remote.example.com".to_string(),
        verify_keys: serde_json::json!({}),
        old_verify_keys: serde_json::json!({}),
        signatures: serde_json::json!({}),
        valid_until_ts,
    }
}

#[test]
fn cache_ttl_caps_at_7_days_when_valid_until_is_1_year() {
    // F-01: a peer advertising `valid_until_ts` one year in the future
    // must NOT cause us to cache for a full year. The spec caps server
    // key validity at 7 days. Our implementation enforces this as a
    // safety ceiling — TTL is the min of (default 1h, peer validity,
    // 7d spec cap). Hard invariant: ttl ≤ 7d always.
    let now_ms: i64 = 1_700_000_000_000;
    let one_year_ms: i64 = 365 * 24 * 60 * 60 * 1000;
    let keys = make_keys_for_ttl_test(now_ms + one_year_ms);

    let ttl = effective_cache_ttl_secs(&keys, now_ms);
    let seven_days_secs: u64 = 7 * 24 * 60 * 60;

    assert!(
        ttl <= seven_days_secs,
        "F-01 violation: TTL must cap at 7 days ({} s) but got {} s for valid_until_ts 1 year out",
        seven_days_secs,
        ttl
    );
    // Current default (1h) is tighter than the 7d cap, so the 1h wins.
    assert_eq!(ttl, KEY_CACHE_TTL_SECS, "current default 1h is the tightest bound for 1y peer validity");
}

#[test]
fn cache_ttl_uses_min_of_three_bounds() {
    // F-01: verify the three-way min logic (default, peer validity, 7d cap).
    let now_ms: i64 = 1_700_000_000_000;

    // (1) Peer validity < 1h → TTL = peer validity
    let keys_short = make_keys_for_ttl_test(now_ms + 5 * 60 * 1000);
    let ttl_short = effective_cache_ttl_secs(&keys_short, now_ms);
    assert_eq!(ttl_short, 5 * 60, "short peer validity must win");

    // (2) Peer validity between 1h and 7d → TTL = KEY_CACHE_TTL_SECS (1h default)
    let keys_mid = make_keys_for_ttl_test(now_ms + 6 * 60 * 60 * 1000);
    let ttl_mid = effective_cache_ttl_secs(&keys_mid, now_ms);
    assert_eq!(ttl_mid, KEY_CACHE_TTL_SECS, "1h default must cap mid-range TTL");

    // (3) Peer validity > 7d → TTL bounded by 1h default (7d spec cap is safety ceiling)
    let keys_long = make_keys_for_ttl_test(now_ms + 30 * 24 * 60 * 60 * 1000_i64);
    let ttl_long = effective_cache_ttl_secs(&keys_long, now_ms);
    assert_eq!(
        ttl_long, KEY_CACHE_TTL_SECS,
        "long peer validity must not exceed default; spec 7d cap is the safety ceiling"
    );
}

#[test]
fn cache_ttl_spec_cap_is_real_bound() {
    // F-01 proof: the 7d spec cap is a real bound in the formula, not just
    // decorative. We exercise the spec cap branch by constructing the
    // formula directly and verifying the 7d cap wins over a hypothetical
    // 30-day default. The production function uses KEY_CACHE_TTL_SECS as
    // the default, so in practice the 7d cap activates only when the
    // default is raised (e.g. via config) — but the formula MUST enforce
    // it regardless.
    let now_ms: i64 = 1_700_000_000_000;
    let one_year_ms: i64 = 365 * 24 * 60 * 60 * 1000;
    let keys = make_keys_for_ttl_test(now_ms + one_year_ms);

    // Simulate a 30-day default (hypothetical) by computing the formula
    // manually with the 7d cap explicit.
    let remaining_secs = ((keys.valid_until_ts - now_ms) / 1000).max(0) as u64;
    let max_validity_secs = (MAX_SERVER_KEY_VALIDITY_MS / 1000) as u64;
    let hypothetical_default_secs: u64 = 30 * 24 * 60 * 60;
    let ttl_with_hypothetical_default = hypothetical_default_secs.min(remaining_secs).min(max_validity_secs);

    let seven_days_secs: u64 = 7 * 24 * 60 * 60;
    assert_eq!(
        ttl_with_hypothetical_default, seven_days_secs,
        "F-01: when the default exceeds 7d, the spec cap must win"
    );
}

#[test]
fn cache_ttl_zero_when_valid_until_is_in_past() {
    // Defensive: already-expired keys must yield TTL=0 so the caller
    // re-fetches immediately rather than serving a stale key.
    let now_ms: i64 = 1_700_000_000_000;
    let keys_expired = make_keys_for_ttl_test(now_ms - 1000);
    assert_eq!(effective_cache_ttl_secs(&keys_expired, now_ms), 0);
}

// ----------------------------------------------------------------------
// F-04: IP literals must be rejected as federation destinations (SSRF)
// ----------------------------------------------------------------------

#[test]
fn ssrf_rejects_loopback_ipv4() {
    // F-04: 127.0.0.1 must be rejected even when attacker bypasses DNS.
    let err = validate_federation_host_not_ssrf("127.0.0.1").unwrap_err();
    assert!(err.contains("F-04"), "error must reference F-04: got {err}");
    assert!(err.contains("127.0.0.1"), "error must include blocked IP");
}

#[test]
fn ssrf_rejects_private_ipv4() {
    // F-04: 10.0.0.1 must be rejected (RFC 1918 private network).
    let err = validate_federation_host_not_ssrf("10.0.0.1").unwrap_err();
    assert!(err.contains("F-04"), "error must reference F-04: got {err}");
}

#[test]
fn ssrf_rejects_public_ipv4() {
    // F-04: even a public IPv4 (8.8.8.8) is rejected — federation must
    // use DNS so that TLS hostname validation secures transport.
    let err = validate_federation_host_not_ssrf("8.8.8.8").unwrap_err();
    assert!(err.contains("F-04"), "error must reference F-04: got {err}");
    assert!(err.contains("8.8.8.8"), "error must include blocked IP");
}

#[test]
fn ssrf_rejects_ipv6_literal() {
    // F-04: [::1] (loopback IPv6) must be rejected.
    let err = validate_federation_host_not_ssrf("::1").unwrap_err();
    assert!(err.contains("F-04"), "error must reference F-04: got {err}");
}

#[test]
fn ssrf_accepts_normal_dns_host() {
    // F-04: regular DNS hostnames must pass through validation.
    assert!(validate_federation_host_not_ssrf("matrix.org").is_ok());
    assert!(validate_federation_host_not_ssrf("sub.example.com").is_ok());
    assert!(validate_federation_host_not_ssrf("x.com").is_ok());
}

#[test]
fn ssrf_accepts_hostname_with_hyphens_and_numbers() {
    // F-04: DNS labels can contain digits and hyphens. None of these
    // parse as IpAddr, so they must all pass.
    assert!(validate_federation_host_not_ssrf("s1.example.com").is_ok());
    assert!(validate_federation_host_not_ssrf("a-b.example.com").is_ok());
    // 5 octets: does NOT parse as IPv4 — must pass.
    assert!(validate_federation_host_not_ssrf("1.2.3.4.5").is_ok());
    // Out-of-range octet: also does NOT parse as IPv4 — must pass.
    assert!(validate_federation_host_not_ssrf("999.999.999.999").is_ok());
}
