use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_rust::cache::CacheManager;
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_fresh_test_app_with_config(|_| {}).await.map(|(app, _)| app)
}

async fn setup_test_app_with_pool() -> Option<(axum::Router, Arc<sqlx::PgPool>, Arc<CacheManager>)> {
    super::setup_fresh_test_app_with_config(|_| {}).await.map(|(app, state)| {
        let pool = state.services.account.user_storage.pool().clone();
        let cache = state.cache;
        (app, pool, cache)
    })
}

async fn setup_test_app_with_state() -> Option<(axum::Router, synapse_rust::web::routes::state::AppState)> {
    super::setup_fresh_test_app_with_config(|_| {}).await
}

async fn promote_to_admin(pool: &sqlx::PgPool, cache: &CacheManager, user_id: &str) {
    sqlx::query("UPDATE users SET is_admin = TRUE WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("failed to promote user to admin");
    cache.delete(&format!("user:admin:{}", user_id)).await;
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    if status != StatusCode::OK {
        eprintln!("registration failed with status {}: {}", status, String::from_utf8_lossy(&body));
    }
    assert_eq!(
        status,
        StatusCode::OK,
        "registration failed with status {}: {}",
        status,
        String::from_utf8_lossy(&body)
    );

    let json: Value = serde_json::from_slice(&body).unwrap();

    (json["access_token"].as_str().unwrap().to_string(), json["user_id"].as_str().unwrap().to_string())
}

fn unique_username(prefix: &str) -> String {
    format!("{prefix}_{}", rand::random::<u32>())
}

#[tokio::test]
async fn test_account_data_round_trip_across_v3_and_r0() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = unique_username("account_data_routes");
    let (token, user_id) = register_user(&app, &username).await;
    let content = json!({ "theme": "dark", "layout": "compact" });

    let put_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/user/{}/account_data/im.vector.settings", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/user/{}/account_data/im.vector.settings", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, content);
}

#[tokio::test]
async fn test_account_data_list_returns_saved_entries() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = unique_username("account_data_list_routes");
    let (token, user_id) = register_user(&app, &username).await;

    for (data_type, content) in [
        ("im.vector.settings", json!({ "theme": "dark", "layout": "compact" })),
        ("m.fav_color", json!({ "value": "blue" })),
    ] {
        let put_request = Request::builder()
            .method("PUT")
            .uri(format!("/_matrix/client/v3/user/{}/account_data/{}", user_id, data_type))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(Body::from(content.to_string()))
            .unwrap();

        let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request).await.unwrap();
        assert_eq!(put_response.status(), StatusCode::OK);
    }

    let list_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/user/{}/account_data/", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let list_response = ServiceExt::<Request<Body>>::oneshot(app, list_request).await.unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(list_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["account_data"]["im.vector.settings"], json!({ "theme": "dark", "layout": "compact" }));
    assert_eq!(json["account_data"]["m.fav_color"], json!({ "value": "blue" }));
}

#[tokio::test]
async fn test_room_account_data_round_trip_across_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = unique_username("room_account_data_routes");
    let (token, user_id) = register_user(&app, &username).await;
    let room_id = "!room:localhost";
    let content = json!({ "tags": { "m.favourite": { "order": 0.1 } } });

    let put_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/r0/user/{}/rooms/{}/account_data/m.tag", user_id, room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/user/{}/rooms/{}/account_data/m.tag", user_id, room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, content);
}

#[tokio::test]
async fn test_filter_round_trip_across_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = unique_username("filter_routes");
    let (token, user_id) = register_user(&app, &username).await;
    let filter = json!({
        "room": {
            "timeline": {
                "limit": 25
            }
        }
    });

    let create_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/r0/user/{}/filter", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(filter.to_string()))
        .unwrap();

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request).await.unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let filter_id = json["filter_id"].as_str().unwrap();

    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/user/{}/filter/{}", user_id, filter_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, filter);
}

#[tokio::test]
async fn test_filter_post_route_round_trip() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = unique_username("filter_post_routes");
    let (token, user_id) = register_user(&app, &username).await;
    let filter = json!({
        "event_fields": ["type", "content"],
        "room": {
            "timeline": {
                "limit": 10
            }
        }
    });

    let create_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/user/{}/filter", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(filter.to_string()))
        .unwrap();

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request).await.unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let filter_id = json["filter_id"].as_str().unwrap();

    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/user/{}/filter/{}", user_id, filter_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app, get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, filter);
}

#[tokio::test]
async fn test_openid_request_token_route_is_shared() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = unique_username("openid_routes");
    let (token, user_id) = register_user(&app, &username).await;

    for path in [
        format!("/_matrix/client/r0/user/{}/openid/request_token", user_id),
        format!("/_matrix/client/v3/user/{}/openid/request_token", user_id),
    ] {
        let request = Request::builder()
            .method("GET")
            .uri(path)
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["token_type"], "Bearer");
        assert!(json["access_token"].as_str().is_some());
    }
}

#[tokio::test]
async fn test_tags_routes_work_across_v3_and_r0() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let username = unique_username("tags_routes");
    let (token, user_id) = register_user(&app, &username).await;
    let room_id = "!tags-room:localhost";

    let put_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/user/{}/rooms/{}/tags/m.favourite", user_id, room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "order": 0.25 }).to_string()))
        .unwrap();

    let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/user/{}/rooms/{}/tags", user_id, room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["tags"]["m.favourite"]["order"], json!(0.25));

    let global_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/user/{}/tags", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let global_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), global_request).await.unwrap();
    assert_eq!(global_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(global_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["tags"][room_id]["m.favourite"]["order"], json!(0.25));

    let delete_request = Request::builder()
        .method("DELETE")
        .uri(format!("/_matrix/client/r0/user/{}/rooms/{}/tags/m.favourite", user_id, room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let delete_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request).await.unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);

    let verify_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/user/{}/rooms/{}/tags", user_id, room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let verify_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), verify_request).await.unwrap();
    assert_eq!(verify_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(verify_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["tags"], json!({}));

    let v1_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v1/user/{}/rooms/{}/tags", user_id, room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let v1_response = ServiceExt::<Request<Body>>::oneshot(app, v1_request).await.unwrap();
    assert_eq!(v1_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_tags_routes_reject_admin_access_to_other_users_data() {
    let Some((app, pool, cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let (owner_token, owner_user_id) = register_user(&app, &format!("tags_owner_{}", rand::random::<u32>())).await;
    let (admin_token, admin_user_id) = register_user(&app, &format!("tags_admin_{}", rand::random::<u32>())).await;
    promote_to_admin(&pool, &cache, &admin_user_id).await;

    let room_id = "!tags-admin-room:localhost";
    let owner_put = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/user/{}/rooms/{}/tags/m.favourite", owner_user_id, room_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "order": 0.5 }).to_string()))
        .unwrap();
    let owner_put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), owner_put).await.unwrap();
    assert_eq!(owner_put_response.status(), StatusCode::OK);

    let admin_get = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/user/{}/rooms/{}/tags", owner_user_id, room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let admin_get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_get).await.unwrap();
    assert_eq!(admin_get_response.status(), StatusCode::FORBIDDEN);

    let admin_put = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/user/{}/rooms/{}/tags/m.lowpriority", owner_user_id, room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "order": 0.9 }).to_string()))
        .unwrap();
    let admin_put_response = ServiceExt::<Request<Body>>::oneshot(app, admin_put).await.unwrap();
    assert_eq!(admin_put_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_secret_storage_key_account_data_write_syncs_internal_ssss() {
    let Some((app, state)) = setup_test_app_with_state().await else {
        return;
    };
    let username = unique_username("sss_key_write_sync");
    let (token, user_id) = register_user(&app, &username).await;
    let key_id = format!("sync-key-{}", rand::random::<u32>());
    let key_content = json!({
        "algorithm": "m.secret_storage.v1.aes-hmac-sha2",
        "auth_data": {
            "key": "opaque-session-key",
            "iv": "opaque-iv",
            "mac": "opaque-mac",
            "signatures": {
                user_id.clone(): {
                    "ed25519:DEVICE": "signed"
                }
            }
        }
    });

    let put_request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/user/{}/account_data/m.secret_storage.key.{}", user_id, key_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(key_content.to_string()))
        .unwrap();

    let put_response = ServiceExt::<Request<Body>>::oneshot(app, put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let stored_key = state
        .services
        .e2ee
        .ssss_service
        .get_key(&user_id, &key_id)
        .await
        .expect("failed to load internal SSSS key")
        .expect("expected internal SSSS key to be mirrored from account_data");

    assert_eq!(stored_key.algorithm, "m.secret_storage.v1.aes-hmac-sha2");
    assert_eq!(stored_key.encrypted_key, "opaque-session-key");
    assert_eq!(
        stored_key.signatures,
        json!({
            user_id.clone(): {
                "ed25519:DEVICE": "signed"
            }
        })
    );
}

#[tokio::test]
async fn test_secret_storage_default_key_write_backfills_internal_ssss_from_standard_account_data() {
    let Some((app, state)) = setup_test_app_with_state().await else {
        return;
    };
    let username = unique_username("sss_default_backfill");
    let (token, user_id) = register_user(&app, &username).await;
    let key_id = format!("late-key-{}", rand::random::<u32>());
    let key_content = json!({
        "algorithm": "m.secret_storage.v1.aes-hmac-sha2",
        "auth_data": {
            "key": "late-account-data-key",
            "iv": "late-iv",
            "mac": "late-mac",
            "signatures": {
                user_id.clone(): {
                    "ed25519:DEVICE": "late-sig"
                }
            }
        }
    });

    state
        .services
        .core
        .account_data_service
        .set_account_data(&user_id, &format!("m.secret_storage.key.{key_id}"), &key_content)
        .await
        .expect("failed to seed standard secret storage key account_data");

    let missing_internal_key = state
        .services
        .e2ee
        .ssss_service
        .get_key(&user_id, &key_id)
        .await
        .expect("failed to query internal SSSS store before backfill");
    assert!(missing_internal_key.is_none(), "internal SSSS key should not exist before default_key backfill");

    let put_default = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/user/{}/account_data/m.secret_storage.default_key", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "key_id": key_id }).to_string()))
        .unwrap();

    let put_default_response = ServiceExt::<Request<Body>>::oneshot(app, put_default).await.unwrap();
    assert_eq!(put_default_response.status(), StatusCode::OK);

    let stored_key = state
        .services
        .e2ee
        .ssss_service
        .get_key(&user_id, &key_id)
        .await
        .expect("failed to load internal SSSS key after default_key backfill")
        .expect("expected internal SSSS key to be backfilled from standard account_data");

    assert_eq!(stored_key.algorithm, "m.secret_storage.v1.aes-hmac-sha2");
    assert_eq!(stored_key.encrypted_key, "late-account-data-key");
    assert_eq!(
        stored_key.signatures,
        json!({
            user_id.clone(): {
                "ed25519:DEVICE": "late-sig"
            }
        })
    );
}

#[tokio::test]
async fn test_secret_storage_default_key_falls_back_to_internal_ssss() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };
    let username = unique_username("sss_default_fallback");
    let (token, user_id) = register_user(&app, &username).await;
    let key_id = format!("fallback-key-{}", rand::random::<u32>());

    // Seed the internal SSSS store directly with one active key, but do NOT
    // write a `m.secret_storage.default_key` account_data event. Stock
    // Element clients that consult the standard event name during bootstrap
    // should still get the first key back, courtesy of the read-side bridge
    // added to `account_data::get_account_data`.
    sqlx::query(
        r"
        INSERT INTO e2ee_secret_storage_keys
            (key_id, key_name, user_id, algorithm, key_data,
             encrypted_key, public_key, signatures, created_ts, updated_ts, is_active)
        VALUES ($1, $1, $2, $3, $4, $5, NULL, $6, $7, $7, TRUE)
        ",
    )
    .bind(&key_id)
    .bind(&user_id)
    .bind("m.secret_storage.v1.aes-hmac-sha2")
    .bind(Vec::<u8>::new())
    .bind("opaque-ciphertext")
    .bind(serde_json::json!({}))
    .bind(current_timestamp_millis())
    .execute(&*pool)
    .await
    .expect("failed to seed internal SSSS key");

    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/user/{}/account_data/m.secret_storage.default_key", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app, get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["key_id"], key_id);
}

#[tokio::test]
async fn test_dehydrated_device_ssss_precondition_accepts_account_data_default_key() {
    // Element sets `m.secret_storage.default_key` via the standard account_data
    // surface, not via the homeserver's internal SSSS table. The dehydration
    // `PUT` endpoint must accept either signal as proof that SSSS is set up;
    // otherwise Element bootstrap on a fresh account is stuck at "no
    // dehydrated device" forever.
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };
    let username = unique_username("dh_account_data_ssss");
    let (token, user_id) = register_user(&app, &username).await;

    // Seed a master cross-signing key so we satisfy the cross-signing
    // precondition; the SSSS precondition should be satisfied by the
    // account_data write below, with NO rows in `e2ee_secret_storage_keys`.
    sqlx::query(
        r"
        INSERT INTO cross_signing_keys (user_id, key_type, key_data, signatures, added_ts)
        VALUES ($1, 'master', $2, $3, $4)
        ON CONFLICT (user_id, key_type) DO UPDATE
            SET key_data = EXCLUDED.key_data,
                signatures = EXCLUDED.signatures,
                added_ts = EXCLUDED.added_ts
        ",
    )
    .bind(&user_id)
    .bind("{}")
    .bind(serde_json::json!({}))
    .bind(current_timestamp_millis())
    .execute(&*pool)
    .await
    .expect("failed to seed cross-signing master key");

    let put_default = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/user/{}/account_data/m.secret_storage.default_key", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "key_id": "remote-bootstrap-key" }).to_string()))
        .unwrap();
    let put_default_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_default).await.unwrap();
    assert_eq!(put_default_response.status(), StatusCode::OK);

    let put_dh = Request::builder()
        .method("PUT")
        .uri("/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_id": "ACCDATASSSS01",
                "device_keys": {
                    "user_id": user_id,
                    "device_id": "ACCDATASSSS01",
                    "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
                    "keys": {
                        "curve25519:ACCDATASSSS01": "AAAA",
                        "ed25519:ACCDATASSSS01": "BBBB"
                    },
                    "signatures": {}
                },
                "device_data": {
                    "algorithm": "org.matrix.msc3814.v1.olm"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let put_dh_response = ServiceExt::<Request<Body>>::oneshot(app, put_dh).await.unwrap();
    assert_eq!(
        put_dh_response.status(),
        StatusCode::OK,
        "dehydrated_device PUT should pass SSSS precondition via m.secret_storage.default_key account_data"
    );
}
