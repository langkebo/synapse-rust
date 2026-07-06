use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use synapse_common::room_versions::{DEFAULT_ROOM_VERSION, SUPPORTED_ROOM_VERSIONS};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_fresh_test_app().await
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
    assert!(declared_versions.iter().any(|version| version.as_str() == Some("v1.14")));

    let capabilities_request =
        Request::builder().method("GET").uri("/_matrix/client/v3/capabilities").body(Body::empty()).unwrap();
    let capabilities_response = ServiceExt::<Request<Body>>::oneshot(app, capabilities_request).await.unwrap();
    assert_eq!(capabilities_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(capabilities_response.into_body(), 4096).await.unwrap();
    let capabilities_json: Value = serde_json::from_slice(&body).unwrap();
    let capabilities = capabilities_json["capabilities"].as_object().expect("capabilities should be an object");
    let unstable = capabilities_json["unstable_features"].as_object().expect("unstable_features should be an object");
    let room_versions = capabilities["m.room_versions"].as_object().expect("m.room_versions should be an object");
    let available = room_versions["available"].as_object().expect("available should be an object");

    assert_eq!(capabilities["m.change_password"]["enabled"], true);
    assert_eq!(capabilities["m.set_displayname"]["enabled"], true);
    assert_eq!(capabilities["m.set_avatar_url"]["enabled"], true);
    assert_eq!(capabilities["m.3pid_changes"]["enabled"], true);
    assert_eq!(capabilities["m.room.summary"]["enabled"], true);
    assert_eq!(capabilities["m.room.suggested"]["enabled"], true);
    assert_eq!(capabilities["m.voice"]["enabled"], true);
    assert_eq!(capabilities["m.thread"]["enabled"], true);
    assert_eq!(room_versions["default"], DEFAULT_ROOM_VERSION);
    // Only creatable versions appear in the client capability list.
    let creatable_count = SUPPORTED_ROOM_VERSIONS.iter().filter(|c| c.can_create).count();
    assert_eq!(available.len(), creatable_count);
    for supported in SUPPORTED_ROOM_VERSIONS {
        if supported.can_create {
            assert_eq!(
                available.get(supported.version).and_then(|value| value.as_str()),
                Some(supported.disposition_str()),
                "declared room version surface should include {}",
                supported.version
            );
        } else {
            assert!(
                available.get(supported.version).is_none(),
                "non-creatable room version {} should not appear in client capabilities",
                supported.version
            );
        }
    }
    assert_eq!(unstable["org.matrix.msc3245.voice"], true);
    assert_eq!(unstable["org.matrix.msc3983.thread"], true);
    assert_eq!(unstable["org.matrix.msc3886.sliding_sync"], true);
    assert_eq!(unstable["io.hula.friends"], cfg!(feature = "friends"));
    assert_eq!(unstable["io.hula.burn_after_read"], cfg!(feature = "burn-after-read"));

    assert!(!capabilities.contains_key("m.sso"));
    assert!(!capabilities.contains_key("io.hula.friends"));
    assert!(!capabilities.contains_key("io.hula.widget"));
    assert!(!capabilities.contains_key("io.hula.burn_after_read"));
}

#[tokio::test]
async fn test_auth_metadata_returns_unrecognized_when_oidc_is_disabled() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/unstable/org.matrix.msc2965/auth_metadata")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_UNRECOGNIZED");
}

#[tokio::test]
async fn test_auth_issuer_returns_unrecognized_when_oidc_is_disabled() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/unstable/org.matrix.msc2965/auth_issuer")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_UNRECOGNIZED");
}
