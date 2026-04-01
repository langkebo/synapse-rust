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
    if !super::init_test_database().await {
        return None;
    }
    let container = ServiceContainer::new_test();
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(create_router(state))
}

async fn register_user(app: &axum::Router, username: &str) -> String {
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
    json["access_token"].as_str().unwrap().to_string()
}

fn parse_mxc_uri(content_uri: &str) -> (String, String) {
    let without_scheme = content_uri.strip_prefix("mxc://").unwrap();
    let (server_name, media_id) = without_scheme.split_once('/').unwrap();
    (server_name.to_string(), media_id.to_string())
}

#[tokio::test]
async fn test_media_routes_share_content_across_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(
        &app,
        &format!("media_routes_shared_{}", rand::random::<u32>()),
    )
    .await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/v1/upload?filename=shared.png")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "image/png")
        .body(Body::from(vec![0x89, 0x50, 0x4E, 0x47, 0x0D]))
        .unwrap();

    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request)
        .await
        .unwrap();
    assert_eq!(upload_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(upload_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let (server_name, media_id) = parse_mxc_uri(json["content_uri"].as_str().unwrap());

    let v3_download_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/media/v3/download/{}/{}",
            server_name, media_id
        ))
        .body(Body::empty())
        .unwrap();

    let v3_download_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_download_request)
            .await
            .unwrap();
    assert_eq!(v3_download_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v3_download_response.into_body(), 2048)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), &[0x89, 0x50, 0x4E, 0x47, 0x0D]);

    let r1_download_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/media/r1/download/{}/{}/shared.png",
            server_name, media_id
        ))
        .body(Body::empty())
        .unwrap();

    let r1_download_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), r1_download_request)
            .await
            .unwrap();
    assert_eq!(r1_download_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r1_download_response.into_body(), 2048)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), &[0x89, 0x50, 0x4E, 0x47, 0x0D]);

    let config_request = Request::builder()
        .method("GET")
        .uri("/_matrix/media/r0/config")
        .body(Body::empty())
        .unwrap();

    let config_response = ServiceExt::<Request<Body>>::oneshot(app, config_request)
        .await
        .unwrap();
    assert_eq!(config_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(config_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["m.upload.size"], 50 * 1024 * 1024);
}

#[tokio::test]
async fn test_media_preview_and_delete_boundaries() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(&app, "media_routes_delete").await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/r0/upload?filename=delete.txt")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("delete-me"))
        .unwrap();

    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request)
        .await
        .unwrap();
    assert_eq!(upload_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(upload_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let (server_name, media_id) = parse_mxc_uri(json["content_uri"].as_str().unwrap());

    let v1_preview_request = Request::builder()
        .method("GET")
        .uri("/_matrix/media/v1/preview_url?url=https://example.com")
        .body(Body::empty())
        .unwrap();
    let v1_preview_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v1_preview_request)
        .await
        .unwrap();
    assert_eq!(v1_preview_response.status(), StatusCode::OK);

    let v3_preview_request = Request::builder()
        .method("GET")
        .uri("/_matrix/media/v3/preview_url?url=https://example.com")
        .body(Body::empty())
        .unwrap();
    let v3_preview_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_preview_request)
        .await
        .unwrap();
    assert_eq!(v3_preview_response.status(), StatusCode::OK);

    let delete_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/media/v3/delete/{}/{}",
            server_name, media_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let delete_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request)
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);

    let missing_thumbnail_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/media/r0/thumbnail/{}/{}?width=64&height=64",
            server_name, media_id
        ))
        .body(Body::empty())
        .unwrap();
    let missing_thumbnail_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), missing_thumbnail_request)
            .await
            .unwrap();
    assert_eq!(missing_thumbnail_response.status(), StatusCode::NOT_FOUND);

    let download_after_delete_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/media/v3/download/{}/{}",
            server_name, media_id
        ))
        .body(Body::empty())
        .unwrap();
    let download_after_delete_response =
        ServiceExt::<Request<Body>>::oneshot(app, download_after_delete_request)
            .await
            .unwrap();
    assert_eq!(
        download_after_delete_response.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn test_media_upload_and_route_boundaries_are_preserved() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(&app, "media_routes_boundaries").await;

    let r0_upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/r0/upload?filename=legacy.txt")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("legacy-upload"))
        .unwrap();
    let r0_upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_upload_request)
        .await
        .unwrap();
    assert_eq!(r0_upload_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_upload_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let (server_name, media_id) = parse_mxc_uri(json["content_uri"].as_str().unwrap());

    let v3_put_upload_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/media/v3/upload/{}/{}?filename=modern.txt",
            server_name, media_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("modern-upload"))
        .unwrap();
    let v3_put_upload_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_put_upload_request)
            .await
            .unwrap();
    assert_eq!(v3_put_upload_response.status(), StatusCode::OK);

    let v1_delete_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/media/v1/delete/{}/{}",
            server_name, media_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let v1_delete_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v1_delete_request)
        .await
        .unwrap();
    assert_eq!(v1_delete_response.status(), StatusCode::OK);

    let r1_preview_request = Request::builder()
        .method("GET")
        .uri("/_matrix/media/r1/preview_url?url=https://example.com")
        .body(Body::empty())
        .unwrap();
    let r1_preview_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r1_preview_request)
        .await
        .unwrap();
    assert_eq!(r1_preview_response.status(), StatusCode::NOT_FOUND);

    let r0_delete_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/media/r0/delete/{}/{}",
            server_name, media_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let r0_delete_response = ServiceExt::<Request<Body>>::oneshot(app, r0_delete_request)
        .await
        .unwrap();
    assert_eq!(r0_delete_response.status(), StatusCode::NOT_FOUND);
}
