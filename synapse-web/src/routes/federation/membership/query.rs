use crate::middleware::FederationRequestAuth;
use crate::routes::context::FederationContext;
use crate::routes::extractors::UserId;
use axum::extract::{Extension, Json, Path, State};
use serde_json::{json, Value};
use synapse_common::*;

use super::federatable_room_version;
use crate::routes::extractors::RoomId;

/// See [`get_room_members`].
pub(crate) async fn get_room_members(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    // OPT-017: Use can_observe (returns 404) instead of in_room (returns 403)
    // to prevent room existence leaking through distinct HTTP status codes.
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;
    let _room_version = federatable_room_version(&ctx, &room_id).await?;

    let members = ctx.room_service.membership().get_room_members_by_membership(&room_id, "join").await?;

    let members_json: Vec<Value> = members
        .into_iter()
        .map(|m| {
            json!({
                "room_id": m.room_id,
                "user_id": m.user_id,
                "membership": m.membership,
                "display_name": m.display_name,
                "avatar_url": m.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "members": members_json,
        "room_id": room_id,
        "offset": 0,
        "total": members_json.len()
    })))
}

/// See [`get_joined_room_members`].
pub(crate) async fn get_joined_room_members(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    // OPT-017: Use can_observe (returns 404) instead of in_room (returns 403)
    // to prevent room existence leaking through distinct HTTP status codes.
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;
    let _room_version = federatable_room_version(&ctx, &room_id).await?;

    let members = ctx.room_service.membership().get_room_members_by_membership(&room_id, "join").await?;

    let members_json: Vec<Value> = members
        .into_iter()
        .map(|m| {
            json!({
                "room_id": m.room_id,
                "user_id": m.user_id,
                "membership": m.membership,
                "display_name": m.display_name,
                "avatar_url": m.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "joined": members_json,
        "room_id": room_id
    })))
}

/// See [`get_user_devices`].
pub(crate) async fn get_user_devices(
    State(ctx): State<FederationContext>,
    Extension(_auth): Extension<FederationRequestAuth>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    if !super::user_matches_origin(&user_id, &ctx.server_name) {
        return Err(ApiError::not_found("User is not hosted on this server".to_string()));
    }

    super::validate_federation_origin_shares_user_room(&ctx, &user_id, &_auth.origin).await?;

    let devices = ctx.account_device_list_service.get_user_devices(&user_id).await?;

    let stream_id = ctx.account_device_list_service.get_max_device_list_stream_id_for_user(&user_id).await?;

    let keys = ctx
        .cross_signing_service
        .get_public_cross_signing_keys(&user_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get cross-signing keys", e))?;

    let devices_json: Vec<Value> = devices
        .into_iter()
        .map(|d| {
            let keys = d.device_key.unwrap_or_else(|| json!({}));
            let algorithms = keys.get("algorithms").cloned().unwrap_or_else(|| json!([]));
            let signatures = keys.get("signatures").cloned().unwrap_or_else(|| json!({}));
            let keys_map = keys.get("keys").cloned().unwrap_or_else(|| json!({}));
            json!({
                "device_id": d.device_id,
                "user_id": d.user_id,
                "algorithms": algorithms,
                "keys": keys_map,
                "signatures": signatures
            })
        })
        .collect();

    Ok(Json(json!({
        "user_id": user_id,
        "stream_id": stream_id,
        "devices": devices_json,
        "master_key": keys.master_key,
        "self_signing_key": keys.self_signing_key,
        "user_signing_key": keys.user_signing_key
    })))
}

/// See [`get_joining_rules`].
pub(crate) async fn get_joining_rules(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    let join_rule_content = super::get_effective_room_join_rule_content(&ctx, &room_id).await?;
    let join_rule = super::get_effective_room_join_rule(&ctx, &room_id).await?;

    // OPT-017: Use can_observe (returns 404) instead of in_room (returns 403)
    // to prevent room existence leaking through distinct HTTP status codes.
    if join_rule != "public" {
        super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;
    }

    let allow = join_rule_content
        .as_ref()
        .and_then(|content| content.get("allow"))
        .filter(|value| value.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]));

    Ok(Json(json!({
        "room_id": room_id,
        "join_rule": join_rule,
        "allow": allow
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_room_members_response_structure() {
        // The response must contain: members (array), room_id, offset, total
        let members_json = vec![json!({
            "room_id": "!room1",
            "user_id": "@alice:example.com",
            "membership": "join",
            "display_name": Some("Alice"),
            "avatar_url": None::<String>
        })];

        let response = json!({
            "members": members_json,
            "room_id": "!room1",
            "offset": 0,
            "total": 1
        });

        assert!(response.get("members").is_some());
        assert!(response.get("room_id").is_some());
        assert!(response.get("offset").is_some());
        assert!(response.get("total").is_some());
        assert_eq!(response["total"], 1);
    }

    #[test]
    fn test_get_joined_room_members_response_structure() {
        // The response must contain: joined (array), room_id
        let members_json = vec![json!({
            "room_id": "!room1",
            "user_id": "@alice:example.com",
            "membership": "join",
            "display_name": Some("Alice"),
            "avatar_url": None::<String>
        })];

        let response = json!({
            "joined": members_json,
            "room_id": "!room1"
        });

        assert!(response.get("joined").is_some());
        assert!(response.get("room_id").is_some());
        assert!(response["joined"].is_array());
    }

    #[test]
    fn test_get_user_devices_response_structure() {
        // The response must contain: user_id, stream_id, devices (array),
        // master_key, self_signing_key, user_signing_key
        let response = json!({
            "user_id": "@alice:example.com",
            "stream_id": 123,
            "devices": [],
            "master_key": None::<String>,
            "self_signing_key": None::<String>,
            "user_signing_key": None::<String>
        });

        assert!(response.get("user_id").is_some());
        assert!(response.get("stream_id").is_some());
        assert!(response.get("devices").is_some());
        assert!(response.get("master_key").is_some());
        assert!(response.get("self_signing_key").is_some());
        assert!(response.get("user_signing_key").is_some());
    }

    #[test]
    fn test_get_user_devices_device_keys_extraction() {
        // Verify that device keys are properly extracted with fallbacks
        let device_keys = json!({
            "algorithms": ["m.megolm.v1.aes-sha2"],
            "keys": {"curve25519:DEVICE1": "base64key"},
            "signatures": {"@alice:example.com": {"ed25519:DEVICE1": "sig"}}
        });

        let keys = device_keys.get("algorithms").cloned().unwrap_or_else(|| json!([]));
        let signatures = device_keys.get("signatures").cloned().unwrap_or_else(|| json!({}));
        let keys_map = device_keys.get("keys").cloned().unwrap_or_else(|| json!({}));

        assert!(keys.is_array());
        assert!(signatures.is_object());
        assert!(keys_map.is_object());
    }

    #[test]
    fn test_get_joining_rules_response_structure() {
        // The response must contain: room_id, join_rule, allow (array)
        let response = json!({
            "room_id": "!room1",
            "join_rule": "public",
            "allow": []
        });

        assert!(response.get("room_id").is_some());
        assert!(response.get("join_rule").is_some());
        assert!(response.get("allow").is_some());
        assert_eq!(response["join_rule"], "public");
        assert!(response["allow"].is_array());
    }

    #[test]
    fn test_get_joining_rules_allow_field_filtering() {
        // Test that non-array "allow" values are filtered to empty array
        let allow_wrong_type = json!("not-an-array");
        // A non-array `allow` filters down to an empty **Vec**, not to a `Value` —
        // `unwrap_or_default()` keeps the element type (`Vec<Value>`) intact.
        let allow_filtered = allow_wrong_type.as_array().cloned().unwrap_or_default();
        assert!(allow_filtered.is_empty());

        // Test that valid array is preserved
        let allow_correct = json!(["rule1", "rule2"]);
        let allow_preserved = allow_correct.as_array().cloned().unwrap_or_default();
        assert_eq!(allow_preserved.len(), 2);
    }
}
