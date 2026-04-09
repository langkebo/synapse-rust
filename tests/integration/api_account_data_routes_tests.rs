use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    let pool = super::get_test_pool().await?;
    let container = ServiceContainer::new_test_with_pool(pool);
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(create_router(state))
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn test_account_data_round_trip_across_v3_and_r0() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "account_data_routes").await;
    let content = json!({ "theme": "dark", "layout": "compact" });

    let put_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/user/{}/account_data/im.vector.settings",
            user_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request)
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/user/{}/account_data/im.vector.settings",
            user_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, content);
}

#[tokio::test]
async fn test_account_data_list_returns_saved_entries() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "account_data_list_routes").await;

    for (data_type, content) in [
        (
            "im.vector.settings",
            json!({ "theme": "dark", "layout": "compact" }),
        ),
        ("m.fav_color", json!({ "value": "blue" })),
    ] {
        let put_request = Request::builder()
            .method("PUT")
            .uri(format!(
                "/_matrix/client/v3/user/{}/account_data/{}",
                user_id, data_type
            ))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(Body::from(content.to_string()))
            .unwrap();

        let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request)
            .await
            .unwrap();
        assert_eq!(put_response.status(), StatusCode::OK);
    }

    let list_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/user/{}/account_data/", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let list_response = ServiceExt::<Request<Body>>::oneshot(app, list_request)
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(list_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["account_data"]["im.vector.settings"],
        json!({ "theme": "dark", "layout": "compact" })
    );
    assert_eq!(
        json["account_data"]["m.fav_color"],
        json!({ "value": "blue" })
    );
}

#[tokio::test]
async fn test_room_account_data_round_trip_across_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "room_account_data_routes").await;
    let room_id = "!room:localhost";
    let content = json!({ "tags": { "m.favourite": { "order": 0.1 } } });

    let put_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/user/{}/rooms/{}/account_data/m.tag",
            user_id, room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request)
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/user/{}/rooms/{}/account_data/m.tag",
            user_id, room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, content);
}

#[tokio::test]
async fn test_filter_round_trip_across_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "filter_routes").await;
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

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let filter_id = json["filter_id"].as_str().unwrap();

    let get_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/user/{}/filter/{}",
            user_id, filter_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, filter);
}

#[tokio::test]
async fn test_filter_post_route_round_trip() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "filter_post_routes").await;
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

    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let filter_id = json["filter_id"].as_str().unwrap();

    let get_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/user/{}/filter/{}",
            user_id, filter_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app, get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, filter);
}

#[tokio::test]
async fn test_openid_request_token_route_is_shared() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (token, user_id) = register_user(&app, "openid_routes").await;

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

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
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
    let (token, user_id) = register_user(&app, "tags_routes").await;
    let room_id = "!tags-room:localhost";

    let put_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/user/{}/rooms/{}/tags/m.favourite",
            user_id, room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "order": 0.25 }).to_string()))
        .unwrap();

    let put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), put_request)
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/user/{}/rooms/{}/tags",
            user_id, room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request)
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["tags"]["m.favourite"]["order"], json!(0.25));

    let global_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/user/{}/tags", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let global_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), global_request)
        .await
        .unwrap();
    assert_eq!(global_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(global_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["tags"][room_id]["m.favourite"]["order"], json!(0.25));

    let delete_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_matrix/client/r0/user/{}/rooms/{}/tags/m.favourite",
            user_id, room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let delete_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request)
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);

    let verify_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/user/{}/rooms/{}/tags",
            user_id, room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let verify_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), verify_request)
        .await
        .unwrap();
    assert_eq!(verify_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(verify_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["tags"], json!({}));

    let v1_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v1/user/{}/rooms/{}/tags",
            user_id, room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let v1_response = ServiceExt::<Request<Body>>::oneshot(app, v1_request)
        .await
        .unwrap();
    assert_eq!(v1_response.status(), StatusCode::NOT_FOUND);
}
