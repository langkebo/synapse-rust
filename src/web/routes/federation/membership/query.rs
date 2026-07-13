use crate::common::*;
use crate::web::middleware::FederationRequestAuth;
use crate::web::routes::context::FederationContext;
use axum::extract::{Extension, Json, Path, State};
use serde_json::{json, Value};

use super::federatable_room_version;

pub(crate) async fn get_room_members(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
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

pub(crate) async fn get_joined_room_members(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
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

pub(crate) async fn get_user_devices(
    State(ctx): State<FederationContext>,
    Extension(_auth): Extension<FederationRequestAuth>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !super::user_matches_origin(&user_id, &ctx.server_name) {
        return Err(ApiError::not_found("User is not hosted on this server".to_string()));
    }

    super::validate_federation_origin_shares_user_room(&ctx, &user_id, &_auth.origin).await?;

    let devices = ctx.account_device_list_service.get_user_devices(&user_id).await?;

    let stream_id = ctx
        .device_storage
        .get_max_device_list_stream_id_for_user(&user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get device stream id", &e))?;

    let (master_key, self_signing_key) = ctx
        .cross_signing_service
        .get_public_cross_signing_keys(&user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get cross-signing keys", &e))?;

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
        "master_key": master_key,
        "self_signing_key": self_signing_key
    })))
}

pub(crate) async fn get_joining_rules(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
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
