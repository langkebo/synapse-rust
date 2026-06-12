use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use synapse_rust::common::room_versions::{DEFAULT_ROOM_VERSION, SUPPORTED_ROOM_VERSIONS};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_test_app().await
}

#[tokio::test]
async fn test_register_and_login_routes_work_across_r0_and_v3() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let r0_register_request =
        Request::builder().method("GET").uri("/_matrix/client/r0/register").body(Body::empty()).unwrap();
    let r0_register_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_register_request).await.unwrap();
    assert_eq!(r0_register_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_register_response.into_body(), 2048).await.unwrap();
    let r0_register_json: Value = serde_json::from_slice(&body).unwrap();

    let v3_register_request =
        Request::builder().method("GET").uri("/_matrix/client/v3/register").body(Body::empty()).unwrap();
    let v3_register_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_register_request).await.unwrap();
    assert_eq!(v3_register_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v3_register_response.into_body(), 2048).await.unwrap();
    let v3_register_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(r0_register_json, v3_register_json);

    let r0_login_request =
        Request::builder().method("GET").uri("/_matrix/client/r0/login").body(Body::empty()).unwrap();
    let r0_login_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_login_request).await.unwrap();
    assert_eq!(r0_login_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_login_response.into_body(), 2048).await.unwrap();
    let r0_login_json: Value = serde_json::from_slice(&body).unwrap();

    let v3_login_request =
        Request::builder().method("GET").uri("/_matrix/client/v3/login").body(Body::empty()).unwrap();
    let v3_login_response = ServiceExt::<Request<Body>>::oneshot(app, v3_login_request).await.unwrap();
    assert_eq!(v3_login_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v3_login_response.into_body(), 2048).await.unwrap();
    let v3_login_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(r0_login_json, v3_login_json);
}

#[tokio::test]
async fn test_auth_router_preserves_qr_and_refresh_boundaries() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let v1_qr_request =
        Request::builder().method("GET").uri("/_matrix/client/v1/login/get_qr_code").body(Body::empty()).unwrap();
    let v1_qr_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v1_qr_request).await.unwrap();
    assert_eq!(v1_qr_response.status(), StatusCode::UNAUTHORIZED);

    let r0_qr_request =
        Request::builder().method("GET").uri("/_matrix/client/r0/login/get_qr_code").body(Body::empty()).unwrap();
    let r0_qr_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_qr_request).await.unwrap();
    assert_eq!(r0_qr_response.status(), StatusCode::NOT_FOUND);

    for path in ["/_matrix/client/r0/refresh", "/_matrix/client/v3/refresh"] {
        let refresh_request = Request::builder()
            .method("POST")
            .uri(path)
            .header("Content-Type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let refresh_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), refresh_request).await.unwrap();

        assert_ne!(refresh_response.status(), StatusCode::NOT_FOUND);
        assert_ne!(refresh_response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}

#[tokio::test]
async fn test_client_capabilities_and_media_config_routes_work_across_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let r0_capabilities_request =
        Request::builder().method("GET").uri("/_matrix/client/r0/capabilities").body(Body::empty()).unwrap();
    let r0_capabilities_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_capabilities_request).await.unwrap();
    assert_eq!(r0_capabilities_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_capabilities_response.into_body(), 2048).await.unwrap();
    let r0_capabilities_json: Value = serde_json::from_slice(&body).unwrap();

    let v3_capabilities_request =
        Request::builder().method("GET").uri("/_matrix/client/v3/capabilities").body(Body::empty()).unwrap();
    let v3_capabilities_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_capabilities_request).await.unwrap();
    assert_eq!(v3_capabilities_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v3_capabilities_response.into_body(), 2048).await.unwrap();
    let v3_capabilities_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(r0_capabilities_json, v3_capabilities_json);

    let mut media_config_jsons = Vec::new();
    for path in
        ["/_matrix/client/v1/media/config", "/_matrix/client/r0/media/config", "/_matrix/client/v3/media/config"]
    {
        let media_config_request = Request::builder().method("GET").uri(path).body(Body::empty()).unwrap();
        let media_config_response =
            ServiceExt::<Request<Body>>::oneshot(app.clone(), media_config_request).await.unwrap();
        assert_eq!(media_config_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(media_config_response.into_body(), 1024).await.unwrap();
        media_config_jsons.push(serde_json::from_slice::<Value>(&body).unwrap());
    }

    assert_eq!(media_config_jsons[0], media_config_jsons[1]);
    assert_eq!(media_config_jsons[1], media_config_jsons[2]);
}

#[tokio::test]
async fn test_versions_and_public_capabilities_match_declared_room_version_surface() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let versions_request =
        Request::builder().method("GET").uri("/_matrix/client/versions").body(Body::empty()).unwrap();
    let versions_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), versions_request).await.unwrap();
    assert_eq!(versions_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(versions_response.into_body(), 4096).await.unwrap();
    let versions_json: Value = serde_json::from_slice(&body).unwrap();
    let declared_versions = versions_json["versions"].as_array().expect("versions should be an array");
    assert!(declared_versions.iter().any(|version| version.as_str() == Some("v1.13")));
    assert!(!declared_versions.iter().any(|version| version.as_str() == Some("v1.14")));

    let capabilities_request =
        Request::builder().method("GET").uri("/_matrix/client/v3/capabilities").body(Body::empty()).unwrap();
    let capabilities_response = ServiceExt::<Request<Body>>::oneshot(app, capabilities_request).await.unwrap();
    assert_eq!(capabilities_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(capabilities_response.into_body(), 4096).await.unwrap();
    let capabilities_json: Value = serde_json::from_slice(&body).unwrap();
    let capabilities = capabilities_json["capabilities"].as_object().expect("capabilities should be an object");
    let room_versions = capabilities["m.room_versions"].as_object().expect("m.room_versions should be an object");
    let available = room_versions["available"].as_object().expect("available should be an object");

    assert_eq!(capabilities["m.change_password"]["enabled"], true);
    assert_eq!(capabilities["m.set_displayname"]["enabled"], true);
    assert_eq!(capabilities["m.set_avatar_url"]["enabled"], true);
    assert_eq!(capabilities["m.3pid_changes"]["enabled"], true);
    assert_eq!(room_versions["default"], DEFAULT_ROOM_VERSION);
    assert_eq!(available.len(), SUPPORTED_ROOM_VERSIONS.len());
    for supported in SUPPORTED_ROOM_VERSIONS {
        assert_eq!(
            available.get(supported.version).and_then(|value| value.as_str()),
            Some(supported.disposition_str()),
            "declared room version surface should include {}",
            supported.version
        );
    }

    assert!(!capabilities.contains_key("m.sso"));
    assert!(!capabilities.contains_key("io.hula.friends"));
    assert!(!capabilities.contains_key("io.hula.widget"));
    assert!(!capabilities.contains_key("io.hula.burn_after_read"));
}
