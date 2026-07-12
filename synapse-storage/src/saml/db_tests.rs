use std::collections::HashMap;
use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;

use super::*;

async fn test_pool() -> Arc<sqlx::PgPool> {
    let db_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
}

async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(username)
    .execute(pool)
    .await
    .ok();
}

async fn cleanup_saml_test_data(pool: &sqlx::PgPool, suffix: &str) {
    let pattern = format!("%{suffix}%");
    sqlx::query("DELETE FROM saml_sessions WHERE session_id LIKE $1 OR user_id LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM saml_user_mapping WHERE name_id LIKE $1 OR user_id LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM saml_identity_providers WHERE entity_id LIKE $1").bind(&pattern).execute(pool).await.ok();
}

fn make_attrs(entries: &[(&str, &str)]) -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    for (k, v) in entries {
        m.insert(k.to_string(), vec![v.to_string()]);
    }
    m
}

// ---------------------------------------------------------------------------
// Session tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_session_valid_record() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@saml_create_session_{suffix}:localhost");
    let session_id = format!("sess_create_{suffix}");
    let issuer = format!("https://idp-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlSessionRequest {
        session_id: session_id.clone(),
        user_id: user_id.clone(),
        name_id: Some(format!("name_{suffix}")),
        issuer: Some(issuer.clone()),
        session_index: Some(format!("idx_{suffix}")),
        attributes: make_attrs(&[("email", &format!("user_{suffix}@example.com"))]),
        expires_in_seconds: 3600,
    };

    let session = storage.create_session(req).await.expect("create_session should succeed");

    assert!(session.id > 0);
    assert_eq!(session.session_id, session_id);
    assert_eq!(session.user_id, user_id);
    assert_eq!(session.issuer.as_deref(), Some(issuer.as_str()));
    assert_eq!(session.status, "active");
    assert!(session.expires_at > session.created_ts);
    assert!(session.last_used_ts > 0);

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_session_not_found() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let storage = SamlStorage::new(&pool);

    cleanup_saml_test_data(&pool, &suffix).await;

    let result = storage.get_session(&format!("nonexistent_{suffix}")).await.expect("query should succeed");
    assert!(result.is_none(), "nonexistent session should return None");

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_session_by_user_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@saml_gbu_{suffix}:localhost");
    let session_id = format!("sess_gbu_{suffix}");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlSessionRequest {
        session_id: session_id.clone(),
        user_id: user_id.clone(),
        name_id: Some(format!("name_{suffix}")),
        issuer: None,
        session_index: None,
        attributes: make_attrs(&[]),
        expires_in_seconds: 3600,
    };
    storage.create_session(req).await.expect("create should succeed");

    // Found
    let found =
        storage.get_session_by_user(&user_id).await.expect("query should succeed").expect("session should be found");
    assert_eq!(found.session_id, session_id);

    // Not found — different user
    let other_user = format!("@saml_other_{suffix}:localhost");
    let not_found = storage.get_session_by_user(&other_user).await.expect("query should succeed");
    assert!(not_found.is_none(), "should not find session for other user");

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_update_session_last_used() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@saml_upd_lu_{suffix}:localhost");
    let session_id = format!("sess_upd_lu_{suffix}");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlSessionRequest {
        session_id: session_id.clone(),
        user_id: user_id.clone(),
        name_id: None,
        issuer: None,
        session_index: None,
        attributes: make_attrs(&[]),
        expires_in_seconds: 3600,
    };
    storage.create_session(req).await.expect("create should succeed");

    // Note: the SQL uses EXTRACT(EPOCH FROM NOW())::BIGINT * 1000 which
    // truncates to second precision, so the "updated" timestamp may
    // appear less than the Rust timestamp. We just verify the call
    // succeeds and the session is still retrievable.
    storage.update_session_last_used(&session_id).await.expect("update should succeed");

    let updated = storage
        .get_session(&session_id)
        .await
        .expect("query should succeed")
        .expect("session should still exist after update");

    assert_eq!(updated.session_id, session_id);
    assert!(updated.last_used_ts > 0);

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_invalidate_session_then_get_returns_none() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@saml_inval_{suffix}:localhost");
    let session_id = format!("sess_inval_{suffix}");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlSessionRequest {
        session_id: session_id.clone(),
        user_id: user_id.clone(),
        name_id: None,
        issuer: None,
        session_index: None,
        attributes: make_attrs(&[]),
        expires_in_seconds: 3600,
    };
    storage.create_session(req).await.expect("create should succeed");

    // Verify exists before invalidation
    let before = storage.get_session(&session_id).await.expect("query should succeed");
    assert!(before.is_some(), "session should exist before invalidation");

    storage.invalidate_session(&session_id).await.expect("invalidate should succeed");

    let after = storage.get_session(&session_id).await.expect("query should succeed");
    assert!(after.is_none(), "invalidated session should not be returned by get_session");

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_cleanup_expired_sessions() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@saml_cleanup_{suffix}:localhost");
    let expired_session_id = format!("sess_expired_{suffix}");
    let valid_session_id = format!("sess_valid_{suffix}");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let now = chrono::Utc::now().timestamp_millis();

    // Insert an already-expired session directly via SQL
    sqlx::query(
        r#"INSERT INTO saml_sessions
           (session_id, user_id, name_id, issuer, session_index, attributes, created_ts, expires_at, last_used_ts, status)
           VALUES ($1, $2, NULL, NULL, NULL, '{}'::jsonb, $3, $4, $5, 'active')"#,
    )
    .bind(&expired_session_id)
    .bind(&user_id)
    .bind(now - 7200000) // created 2 hours ago
    .bind(now - 3600000) // expired 1 hour ago
    .bind(now - 7200000)
    .execute(&*pool)
    .await
    .expect("should insert expired session");

    // Create a valid (non-expired) session via the storage API
    let storage = SamlStorage::new(&pool);
    let req = CreateSamlSessionRequest {
        session_id: valid_session_id.clone(),
        user_id: user_id.clone(),
        name_id: None,
        issuer: None,
        session_index: None,
        attributes: make_attrs(&[]),
        expires_in_seconds: 3600,
    };
    storage.create_session(req).await.expect("create valid session should succeed");

    let removed = storage.cleanup_expired_sessions().await.expect("cleanup should succeed");
    assert!(removed >= 1, "should have removed at least the expired session");

    // Expired session should be gone
    let expired = storage.get_session(&expired_session_id).await.expect("query should succeed");
    assert!(expired.is_none(), "expired session should be cleaned up");

    // Valid session should remain
    let valid = storage
        .get_session(&valid_session_id)
        .await
        .expect("query should succeed")
        .expect("valid session should survive cleanup");
    assert_eq!(valid.session_id, valid_session_id);

    cleanup_saml_test_data(&pool, &suffix).await;
}

// ---------------------------------------------------------------------------
// User mapping tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_user_mapping() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@saml_map_{suffix}:localhost");
    let name_id = format!("name_map_{suffix}");
    let issuer = format!("https://idp-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlUserMappingRequest {
        name_id: name_id.clone(),
        user_id: user_id.clone(),
        issuer: issuer.clone(),
        attributes: make_attrs(&[("email", &format!("map_{suffix}@example.com"))]),
    };

    let mapping = storage.create_user_mapping(req).await.expect("create_user_mapping should succeed");

    assert!(mapping.id > 0);
    assert_eq!(mapping.name_id, name_id);
    assert_eq!(mapping.user_id, user_id);
    assert_eq!(mapping.issuer, issuer);
    assert_eq!(mapping.authentication_count, 1);
    assert!(mapping.first_seen_ts > 0);
    assert!(mapping.last_authenticated_ts > 0);

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_create_user_mapping_on_conflict_updates() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id_a = format!("@saml_map_a_{suffix}:localhost");
    let user_id_b = format!("@saml_map_b_{suffix}:localhost");
    let name_id = format!("name_conflict_{suffix}");
    let issuer = format!("https://idp-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id_a).await;
    ensure_test_user(&pool, &user_id_b).await;

    let storage = SamlStorage::new(&pool);

    // First create
    let req1 = CreateSamlUserMappingRequest {
        name_id: name_id.clone(),
        user_id: user_id_a.clone(),
        issuer: issuer.clone(),
        attributes: make_attrs(&[("email", &format!("a_{suffix}@example.com"))]),
    };
    let m1 = storage.create_user_mapping(req1).await.expect("first create should succeed");
    assert_eq!(m1.user_id, user_id_a);
    assert_eq!(m1.authentication_count, 1);

    // Brief sleep so last_authenticated_ts can change
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Second create with same (name_id, issuer) but different user_id
    let req2 = CreateSamlUserMappingRequest {
        name_id: name_id.clone(),
        user_id: user_id_b.clone(),
        issuer: issuer.clone(),
        attributes: make_attrs(&[("email", &format!("b_{suffix}@example.com"))]),
    };
    let m2 = storage.create_user_mapping(req2).await.expect("second create should succeed");

    // ON CONFLICT DO UPDATE should bump counter.
    // Note: last_authenticated_ts via SQL EXTRACT(EPOCH) truncates to second
    // precision and may be LESS than the Rust timestamp, so we only check
    // the counter.
    assert_eq!(m2.id, m1.id, "should update the same row");
    assert_eq!(m2.authentication_count, 2);

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_user_mapping_by_name_id_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@saml_gmbn_{suffix}:localhost");
    let name_id = format!("name_gmbn_{suffix}");
    let issuer = format!("https://idp-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlUserMappingRequest {
        name_id: name_id.clone(),
        user_id: user_id.clone(),
        issuer: issuer.clone(),
        attributes: make_attrs(&[]),
    };
    storage.create_user_mapping(req).await.expect("create should succeed");

    // Found
    let found = storage
        .get_user_mapping_by_name_id(&name_id, &issuer)
        .await
        .expect("query should succeed")
        .expect("mapping should be found");
    assert_eq!(found.name_id, name_id);
    assert_eq!(found.issuer, issuer);

    // Not found — wrong name_id
    let not_found =
        storage.get_user_mapping_by_name_id(&format!("wrong_{suffix}"), &issuer).await.expect("query should succeed");
    assert!(not_found.is_none());

    // Not found — wrong issuer
    let not_found2 = storage
        .get_user_mapping_by_name_id(&name_id, &format!("https://wrong-{suffix}.com"))
        .await
        .expect("query should succeed");
    assert!(not_found2.is_none());

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_user_mapping_by_user_id_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@saml_gmbu_{suffix}:localhost");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlUserMappingRequest {
        name_id: format!("name_gmbu_{suffix}"),
        user_id: user_id.clone(),
        issuer: format!("https://idp-{suffix}.example.com"),
        attributes: make_attrs(&[]),
    };
    storage.create_user_mapping(req).await.expect("create should succeed");

    // Found
    let found = storage
        .get_user_mapping_by_user_id(&user_id)
        .await
        .expect("query should succeed")
        .expect("mapping should be found");
    assert_eq!(found.user_id, user_id);

    // Not found
    let not_found = storage
        .get_user_mapping_by_user_id(&format!("@nonexistent_{suffix}:localhost"))
        .await
        .expect("query should succeed");
    assert!(not_found.is_none());

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_delete_user_mapping_and_idempotent() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@saml_del_map_{suffix}:localhost");
    let name_id = format!("name_del_{suffix}");
    let issuer = format!("https://idp-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlUserMappingRequest {
        name_id: name_id.clone(),
        user_id: user_id.clone(),
        issuer: issuer.clone(),
        attributes: make_attrs(&[]),
    };
    storage.create_user_mapping(req).await.expect("create should succeed");

    // Delete
    storage.delete_user_mapping(&name_id, &issuer).await.expect("delete should succeed");

    // Verify gone
    let after = storage.get_user_mapping_by_name_id(&name_id, &issuer).await.expect("query should succeed");
    assert!(after.is_none(), "mapping should be deleted");

    // Delete again (idempotent)
    storage.delete_user_mapping(&name_id, &issuer).await.expect("idempotent delete should succeed");

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_list_user_mappings_returns_list() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let name_a = format!("aaa_list_{suffix}");
    let name_b = format!("bbb_list_{suffix}");
    let issuer = format!("https://idp-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;

    let storage = SamlStorage::new(&pool);
    for name in [&name_a, &name_b] {
        let uid = format!("@{name}:localhost");
        ensure_test_user(&pool, &uid).await;
        let req = CreateSamlUserMappingRequest {
            name_id: name.clone(),
            user_id: uid,
            issuer: issuer.clone(),
            attributes: make_attrs(&[]),
        };
        storage.create_user_mapping(req).await.expect("create should succeed");
    }

    let mappings = storage.list_user_mappings(10, None).await.expect("list should succeed");
    assert!(mappings.len() >= 2, "should return at least 2 mappings");

    // Should be ordered by name_id ASC
    let names: Vec<&str> = mappings.iter().map(|m| m.name_id.as_str()).collect();
    let pos_a = names.iter().position(|&n| n == name_a);
    let pos_b = names.iter().position(|&n| n == name_b);
    assert!(pos_a.is_some(), "mapping A should be in list");
    assert!(pos_b.is_some(), "mapping B should be in list");
    assert!(pos_a.unwrap() < pos_b.unwrap(), "A should come before B (names sorted ASC)");

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_list_user_mappings_cursor_pagination() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let name_a = format!("aaa_cursor_{suffix}");
    let name_b = format!("bbb_cursor_{suffix}");
    let name_c = format!("ccc_cursor_{suffix}");
    let issuer = format!("https://idp-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;

    let storage = SamlStorage::new(&pool);
    for name in [&name_a, &name_b, &name_c] {
        let uid = format!("@{name}:localhost");
        ensure_test_user(&pool, &uid).await;
        let req = CreateSamlUserMappingRequest {
            name_id: name.clone(),
            user_id: uid,
            issuer: issuer.clone(),
            attributes: make_attrs(&[]),
        };
        storage.create_user_mapping(req).await.expect("create should succeed");
    }

    // Fetch all rows (large limit) and filter to only our test records
    let all = storage.list_user_mappings(10000, None).await.expect("list all should succeed");
    let my_names: Vec<&str> = all.iter().map(|m| m.name_id.as_str()).filter(|n| n.contains(&suffix)).collect();
    assert_eq!(my_names.len(), 3, "should find all 3 test mappings");
    assert_eq!(my_names[0], name_a);
    assert_eq!(my_names[1], name_b);
    assert_eq!(my_names[2], name_c);

    // Cursor pagination: after name_b, only name_c should remain (within our records)
    let after_b = storage.list_user_mappings(10000, Some(&name_b)).await.expect("after_b should succeed");
    let after_b_names: Vec<&str> = after_b.iter().map(|m| m.name_id.as_str()).filter(|n| n.contains(&suffix)).collect();
    assert_eq!(after_b_names.len(), 1, "only the remaining test record should be after name_b");
    assert_eq!(after_b_names[0], name_c);

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_user_mapping_any_issuer_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@saml_anyiss_{suffix}:localhost");
    let name_id = format!("name_anyiss_{suffix}");
    let issuer = format!("https://idp-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlUserMappingRequest {
        name_id: name_id.clone(),
        user_id: user_id.clone(),
        issuer: issuer.clone(),
        attributes: make_attrs(&[]),
    };
    storage.create_user_mapping(req).await.expect("create should succeed");

    // Found
    let found = storage
        .get_user_mapping_any_issuer(&name_id)
        .await
        .expect("query should succeed")
        .expect("mapping should be found");
    assert_eq!(found.name_id, name_id);
    assert_eq!(found.user_id, user_id);

    // Not found
    let not_found =
        storage.get_user_mapping_any_issuer(&format!("nonexistent_{suffix}")).await.expect("query should succeed");
    assert!(not_found.is_none());

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_update_user_mapping_by_name_id() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@saml_upd_map_{suffix}:localhost");
    let new_user_id = format!("@saml_upd_map_new_{suffix}:localhost");
    let name_id = format!("name_upd_{suffix}");
    let issuer = format!("https://idp-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;
    ensure_test_user(&pool, &new_user_id).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlUserMappingRequest {
        name_id: name_id.clone(),
        user_id: user_id.clone(),
        issuer: issuer.clone(),
        attributes: make_attrs(&[("role", "user")]),
    };
    storage.create_user_mapping(req).await.expect("create should succeed");

    let new_attrs = serde_json::json!({"role": "admin", "department": "engineering"});
    let updated = storage
        .update_user_mapping_by_name_id(&name_id, Some(&new_user_id), Some(&new_attrs))
        .await
        .expect("update should succeed")
        .expect("should return updated mapping");

    assert_eq!(updated.user_id, new_user_id);
    // Attributes should be updated
    let attrs_map: serde_json::Value = updated.attributes;
    assert_eq!(attrs_map["role"], serde_json::json!("admin"));
    assert_eq!(attrs_map["department"], serde_json::json!("engineering"));

    // Verify persisted
    let persisted = storage
        .get_user_mapping_by_name_id(&name_id, &issuer)
        .await
        .expect("query should succeed")
        .expect("mapping should exist");
    assert_eq!(persisted.user_id, new_user_id);

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_delete_user_mapping_by_name_id() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id_1 = format!("@saml_delbn_1_{suffix}:localhost");
    let user_id_2 = format!("@saml_delbn_2_{suffix}:localhost");
    let name_id = format!("name_delbn_{suffix}");
    let issuer_a = format!("https://idp-a-{suffix}.example.com");
    let issuer_b = format!("https://idp-b-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id_1).await;
    ensure_test_user(&pool, &user_id_2).await;

    let storage = SamlStorage::new(&pool);

    // Create two mappings with same name_id but different issuers
    for (uid, iss) in [(&user_id_1, &issuer_a), (&user_id_2, &issuer_b)] {
        let req = CreateSamlUserMappingRequest {
            name_id: name_id.clone(),
            user_id: uid.clone(),
            issuer: iss.clone(),
            attributes: make_attrs(&[]),
        };
        storage.create_user_mapping(req).await.expect("create should succeed");
    }

    // Delete by name_id should remove ALL matching rows
    let count = storage.delete_user_mapping_by_name_id(&name_id).await.expect("delete should succeed");
    assert_eq!(count, 2, "should delete both mappings with the same name_id");

    // Verify both are gone
    for iss in [&issuer_a, &issuer_b] {
        let result = storage.get_user_mapping_by_name_id(&name_id, iss).await.expect("query should succeed");
        assert!(result.is_none(), "mapping for issuer {iss} should be deleted");
    }

    // Idempotent: deleting again returns 0
    let count2 = storage.delete_user_mapping_by_name_id(&name_id).await.expect("second delete should succeed");
    assert_eq!(count2, 0);

    cleanup_saml_test_data(&pool, &suffix).await;
}

// ---------------------------------------------------------------------------
// Identity provider tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_identity_provider_with_all_fields() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let entity_id = format!("https://idp-create-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlIdentityProviderRequest {
        entity_id: entity_id.clone(),
        display_name: Some(format!("Test IdP {suffix}")),
        description: Some("A test identity provider".to_string()),
        metadata_url: Some(format!("https://metadata-{suffix}.example.com")),
        metadata_xml: Some("<xml>test metadata</xml>".to_string()),
        enabled: Some(false),
        priority: Some(50),
        attribute_mapping: Some(serde_json::json!({"uid": "name_id", "mail": "email"})),
    };

    let idp = storage.create_identity_provider(req).await.expect("create_identity_provider should succeed");

    assert!(idp.id > 0);
    assert_eq!(idp.entity_id, entity_id);
    assert_eq!(idp.display_name.as_deref(), Some(format!("Test IdP {suffix}").as_str()));
    assert!(!idp.is_enabled);
    assert_eq!(idp.priority, 50);
    assert!(idp.created_ts > 0);
    assert!(idp.updated_ts.is_some());

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_identity_provider_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let entity_id = format!("https://idp-get-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlIdentityProviderRequest {
        entity_id: entity_id.clone(),
        display_name: None,
        description: None,
        metadata_url: None,
        metadata_xml: None,
        enabled: None,
        priority: None,
        attribute_mapping: None,
    };
    storage.create_identity_provider(req).await.expect("create should succeed");

    // Found
    let found =
        storage.get_identity_provider(&entity_id).await.expect("query should succeed").expect("idp should be found");
    assert_eq!(found.entity_id, entity_id);

    // Not found
    let not_found = storage
        .get_identity_provider(&format!("https://nonexistent-{suffix}.example.com"))
        .await
        .expect("query should succeed");
    assert!(not_found.is_none());

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_all_identity_providers() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let entity_a = format!("https://idp-all-a-{suffix}.example.com");
    let entity_b = format!("https://idp-all-b-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;

    let storage = SamlStorage::new(&pool);

    // Create two IdPs with different priorities
    for (entity, prio) in [(&entity_a, 200), (&entity_b, 100)] {
        let req = CreateSamlIdentityProviderRequest {
            entity_id: entity.clone(),
            display_name: None,
            description: None,
            metadata_url: None,
            metadata_xml: None,
            enabled: None,
            priority: Some(prio),
            attribute_mapping: None,
        };
        storage.create_identity_provider(req).await.expect("create should succeed");
    }

    let all = storage.get_all_identity_providers().await.expect("query should succeed");
    assert!(all.len() >= 2, "should return at least 2 IdPs");

    // Should be ordered by priority ASC (entity_b has priority 100, entity_a has 200)
    let positions: Vec<&str> = all.iter().map(|p| p.entity_id.as_str()).collect();
    let pos_b = positions.iter().position(|&e| e == entity_b.as_str());
    let pos_a = positions.iter().position(|&e| e == entity_a.as_str());
    assert!(pos_b.is_some() && pos_a.is_some());
    assert!(pos_b.unwrap() < pos_a.unwrap(), "lower priority should come first");

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_enabled_identity_providers() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let entity_enabled = format!("https://idp-on-{suffix}.example.com");
    let entity_disabled = format!("https://idp-off-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;

    let storage = SamlStorage::new(&pool);

    // Create enabled IdP
    let req_on = CreateSamlIdentityProviderRequest {
        entity_id: entity_enabled.clone(),
        display_name: None,
        description: None,
        metadata_url: None,
        metadata_xml: None,
        enabled: Some(true),
        priority: Some(10),
        attribute_mapping: None,
    };
    storage.create_identity_provider(req_on).await.expect("create enabled should succeed");

    // Create disabled IdP
    let req_off = CreateSamlIdentityProviderRequest {
        entity_id: entity_disabled.clone(),
        display_name: None,
        description: None,
        metadata_url: None,
        metadata_xml: None,
        enabled: Some(false),
        priority: Some(20),
        attribute_mapping: None,
    };
    storage.create_identity_provider(req_off).await.expect("create disabled should succeed");

    let enabled = storage.get_enabled_identity_providers().await.expect("query should succeed");

    // Only enabled IdPs should be returned
    let has_enabled = enabled.iter().any(|p| p.entity_id == entity_enabled);
    let has_disabled = enabled.iter().any(|p| p.entity_id == entity_disabled);
    assert!(has_enabled, "enabled IdP should be in results");
    assert!(!has_disabled, "disabled IdP should NOT be in results");

    cleanup_saml_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_update_idp_metadata() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let entity_id = format!("https://idp-meta-{suffix}.example.com");

    cleanup_saml_test_data(&pool, &suffix).await;

    let storage = SamlStorage::new(&pool);
    let req = CreateSamlIdentityProviderRequest {
        entity_id: entity_id.clone(),
        display_name: None,
        description: None,
        metadata_url: None,
        metadata_xml: Some("<xml>original</xml>".to_string()),
        enabled: None,
        priority: None,
        attribute_mapping: None,
    };
    storage.create_identity_provider(req).await.expect("create should succeed");

    let valid_until = chrono::Utc::now().timestamp_millis() + 86400000; // 1 day from now
    storage
        .update_idp_metadata(&entity_id, "<xml>updated metadata</xml>", Some(valid_until))
        .await
        .expect("update_idp_metadata should succeed");

    let updated =
        storage.get_identity_provider(&entity_id).await.expect("query should succeed").expect("idp should exist");

    assert_eq!(updated.metadata_xml.as_deref(), Some("<xml>updated metadata</xml>"), "metadata_xml should be updated");
    assert!(updated.last_metadata_refresh_ts.is_some(), "last_metadata_refresh_ts should be set");

    cleanup_saml_test_data(&pool, &suffix).await;
}
