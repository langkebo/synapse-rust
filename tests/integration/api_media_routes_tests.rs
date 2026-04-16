use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
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
async fn test_media_delete_forbids_admin_override() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let owner_token = register_user(&app, &format!("media_owner_{}", rand::random::<u32>())).await;
    let (admin_token, _) = super::get_admin_token(&app).await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/r0/upload?filename=admin-delete.txt")
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "text/plain")
        .body(Body::from("owner-media"))
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

    let admin_delete_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/media/v3/delete/{}/{}",
            server_name, media_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let admin_delete_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_delete_request)
            .await
            .unwrap();
    assert_eq!(admin_delete_response.status(), StatusCode::FORBIDDEN);

    let owner_download_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/media/v3/download/{}/{}",
            server_name, media_id
        ))
        .body(Body::empty())
        .unwrap();

    let owner_download_response =
        ServiceExt::<Request<Body>>::oneshot(app, owner_download_request)
            .await
            .unwrap();
    assert_eq!(owner_download_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_media_download_is_public_but_delete_requires_authentication() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(
        &app,
        &format!("media_routes_public_read_{}", rand::random::<u32>()),
    )
    .await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/v3/upload?filename=public-read.txt")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("public-media"))
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

    let anonymous_download_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/media/v3/download/{}/{}",
            server_name, media_id
        ))
        .body(Body::empty())
        .unwrap();
    let anonymous_download_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), anonymous_download_request)
            .await
            .unwrap();
    assert_eq!(anonymous_download_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(anonymous_download_response.into_body(), 2048)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), b"public-media");

    let anonymous_delete_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/media/v3/delete/{}/{}",
            server_name, media_id
        ))
        .body(Body::empty())
        .unwrap();
    let anonymous_delete_response =
        ServiceExt::<Request<Body>>::oneshot(app, anonymous_delete_request)
            .await
            .unwrap();
    assert_eq!(anonymous_delete_response.status(), StatusCode::UNAUTHORIZED);
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
    let named_media_id = format!("boundary_media_{}", rand::random::<u32>());

    let v3_put_upload_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/media/v3/upload/{}/{}?filename=modern.txt",
            server_name, named_media_id
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

#[tokio::test]
async fn test_named_media_upload_uses_provided_media_id() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(
        &app,
        &format!("media_routes_named_{}", rand::random::<u32>()),
    )
    .await;

    let bootstrap_upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/v3/upload?filename=bootstrap.txt")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("bootstrap"))
        .unwrap();
    let bootstrap_upload_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), bootstrap_upload_request)
            .await
            .unwrap();
    assert_eq!(bootstrap_upload_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(bootstrap_upload_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let (server_name, _) = parse_mxc_uri(json["content_uri"].as_str().unwrap());
    let named_media_id = format!("named_media_{}", rand::random::<u32>());

    let named_upload_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/media/v3/upload/{}/{}?filename=named.txt",
            server_name, named_media_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("named-upload"))
        .unwrap();
    let named_upload_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), named_upload_request)
            .await
            .unwrap();
    assert_eq!(named_upload_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(named_upload_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["content_uri"].as_str().unwrap(),
        format!("mxc://{}/{}", server_name, named_media_id)
    );
    assert_eq!(json["media_id"].as_str().unwrap(), named_media_id);

    let download_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/media/v3/download/{}/{}",
            server_name, named_media_id
        ))
        .body(Body::empty())
        .unwrap();
    let download_response = ServiceExt::<Request<Body>>::oneshot(app, download_request)
        .await
        .unwrap();
    assert_eq!(download_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(download_response.into_body(), 2048)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), b"named-upload");
}

#[tokio::test]
async fn test_named_media_upload_rejects_duplicate_media_id() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(
        &app,
        &format!("media_routes_duplicate_{}", rand::random::<u32>()),
    )
    .await;

    let media_id = format!("fixed_media_{}", rand::random::<u32>());
    let bootstrap_upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/v3/upload?filename=bootstrap.txt")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("bootstrap"))
        .unwrap();
    let bootstrap_upload_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), bootstrap_upload_request)
            .await
            .unwrap();
    assert_eq!(bootstrap_upload_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(bootstrap_upload_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let (server_name, _) = parse_mxc_uri(json["content_uri"].as_str().unwrap());

    let first_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/media/v3/upload/{}/{}?filename=first.txt",
            server_name, media_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("first"))
        .unwrap();
    let first_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), first_request)
        .await
        .unwrap();
    assert_eq!(first_response.status(), StatusCode::OK);

    let duplicate_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/media/v3/upload/{}/{}?filename=second.txt",
            server_name, media_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("second"))
        .unwrap();
    let duplicate_response = ServiceExt::<Request<Body>>::oneshot(app, duplicate_request)
        .await
        .unwrap();
    assert_eq!(duplicate_response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_named_media_upload_rejects_non_local_server_name() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(
        &app,
        &format!("media_routes_server_name_{}", rand::random::<u32>()),
    )
    .await;
    let media_id = format!("fixed_media_{}", rand::random::<u32>());

    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/media/v3/upload/{}/{}?filename=bad.txt",
            "remote.example.com",
            media_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("bad-server-name"))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_BAD_JSON");
}

#[tokio::test]
async fn test_media_download_rejects_foreign_server_namespace() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(
        &app,
        &format!("media_routes_foreign_download_{}", rand::random::<u32>()),
    )
    .await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/v3/upload?filename=foreign-download.txt")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("local-only-media"))
        .unwrap();
    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request)
        .await
        .unwrap();
    assert_eq!(upload_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(upload_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let (_, media_id) = parse_mxc_uri(json["content_uri"].as_str().unwrap());

    let download_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/media/v3/download/{}/{}",
            "remote.example.com", media_id
        ))
        .body(Body::empty())
        .unwrap();
    let download_response = ServiceExt::<Request<Body>>::oneshot(app, download_request)
        .await
        .unwrap();
    assert_eq!(download_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_media_delete_rejects_foreign_server_namespace() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(
        &app,
        &format!("media_routes_foreign_delete_{}", rand::random::<u32>()),
    )
    .await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/v3/upload?filename=foreign-delete.txt")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("local-media"))
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

    let delete_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/media/v3/delete/{}/{}",
            "remote.example.com", media_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let delete_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request)
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::NOT_FOUND);

    let local_download_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/media/v3/download/{}/{}", server_name, media_id))
        .body(Body::empty())
        .unwrap();
    let local_download_response =
        ServiceExt::<Request<Body>>::oneshot(app, local_download_request)
            .await
            .unwrap();
    assert_eq!(local_download_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_media_thumbnail_rejects_foreign_server_namespace() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let token = register_user(
        &app,
        &format!("media_routes_foreign_thumbnail_{}", rand::random::<u32>()),
    )
    .await;

    let upload_request = Request::builder()
        .method("POST")
        .uri("/_matrix/media/v3/upload?filename=foreign-thumbnail.txt")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "text/plain")
        .body(Body::from("local-thumbnail-source"))
        .unwrap();
    let upload_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upload_request)
        .await
        .unwrap();
    assert_eq!(upload_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(upload_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let (_, media_id) = parse_mxc_uri(json["content_uri"].as_str().unwrap());

    let thumbnail_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/media/v3/thumbnail/{}/{}?width=64&height=64",
            "remote.example.com", media_id
        ))
        .body(Body::empty())
        .unwrap();
    let thumbnail_response = ServiceExt::<Request<Body>>::oneshot(app, thumbnail_request)
        .await
        .unwrap();
    assert_eq!(thumbnail_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_legacy_media_download_missing_returns_not_found_status() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let missing_media_id = format!("missing_media_{}", rand::random::<u32>());

    let v1_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/media/v1/download/{}/{}",
            "example.com", missing_media_id
        ))
        .body(Body::empty())
        .unwrap();
    let v1_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v1_request)
        .await
        .unwrap();
    assert_eq!(v1_response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(v1_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");

    let r1_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/media/r1/download/{}/{}/missing.txt",
            "example.com", missing_media_id
        ))
        .body(Body::empty())
        .unwrap();
    let r1_response = ServiceExt::<Request<Body>>::oneshot(app, r1_request)
        .await
        .unwrap();
    assert_eq!(r1_response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(r1_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}
