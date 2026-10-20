use super::ensure_room_view_access;
use crate::routes::context::RoomContext;
use crate::routes::MatrixJson;
use crate::routes::{
    extractors::{RoomId, UserId},
    is_member_ctx, is_member_or_creator_ctx, validate_membership, validate_moderation_reason, validate_room_id,
    validate_user_id, AuthenticatedUser,
};
use crate::utils::auth::resolve_request_id;
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;

/// Percent-decodes a single application/x-www-form-urlencoded component.
///
/// `+` is decoded as a space (form encoding), and malformed `%` sequences are
/// passed through verbatim rather than rejected — a bad `via` hint must not
/// turn a join into a 400.
fn percent_decode_component(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(hi), Some(lo)) => {
                        out.push((hi * 16 + lo) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Extracts the spec-defined `via` server list for join/knock.
///
/// Matrix `POST /_matrix/client/v3/join/{roomIdOrAlias}` and
/// `POST /_matrix/client/v3/knock/{roomIdOrAlias}` take `via` as a **repeated
/// query parameter** (`?via=srv1&via=srv2`), not as a JSON body field — see
/// ruma's `join_room_by_id_or_alias` and MSC4156 (`server_name` → `via`).
///
/// Before this fix the join handler read a non-standard body key
/// `via_servers`, so the standard client payload `{"via":[...]}` was silently
/// ignored and federated joins fell back to the room-id domain. `knock`
/// ignored `via` entirely.
///
/// `legacy_body_via_servers` is retained as a back-compat fallback so
/// deployments already sending the old shape keep working; `via` always wins.
pub(crate) fn extract_via_servers(query: &[(String, String)], legacy_body_via_servers: Option<&Value>) -> Vec<String> {
    let from_query: Vec<String> = query
        .iter()
        .filter(|(k, _)| k == "via")
        .map(|(_, v)| percent_decode_component(v))
        .filter(|v| !v.is_empty())
        .collect();

    if !from_query.is_empty() {
        return from_query;
    }

    legacy_body_via_servers
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(str::to_string).collect())
        .unwrap_or_default()
}

/// Request body for [`join_room`].
///
/// The spec makes the body optional and only defines `reason`, which is copied
/// into the `m.room.member` event content.
#[derive(Debug, Deserialize)]
pub struct JoinRoomBody {
    #[serde(default)]
    pub reason: Option<String>,
}

/// See [`join_room`].
pub(crate) async fn join_room(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    body: Option<MatrixJson<JoinRoomBody>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let reason = body.as_ref().and_then(|b| b.reason.as_deref());
    validate_moderation_reason(reason)?;

    ctx.room_service.membership().join_room_with_reason(&room_id, &auth_user.user_id, reason).await?;
    Ok(Json(json!({
        "room_id": room_id,
        "joined_ts": current_timestamp_millis()
    })))
}

/// Request body for [`join_room_by_id_or_alias`].
///
/// The body is optional; `via_servers` is only a back-compat fallback for the
/// spec's `?via=` query parameter, and `reason` is copied into the
/// `m.room.member` event content.
#[derive(Debug, Deserialize)]
pub struct JoinRoomByIdOrAliasBody {
    #[serde(default)]
    pub via_servers: Option<Value>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// See [`join_room_by_id_or_alias`].
pub(crate) async fn join_room_by_id_or_alias(
    State(ctx): State<RoomContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(room_id_or_alias): Path<String>,
    Query(query): Query<Vec<(String, String)>>,
    body: Option<MatrixJson<JoinRoomByIdOrAliasBody>>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);

    // Spec-defined `via` hint (`?via=srv`), with the pre-fix body key
    // `via_servers` kept as a back-compat fallback. See
    // [`extract_via_servers`].
    let legacy_via = body.as_ref().and_then(|b| b.via_servers.as_ref());
    let via_servers = extract_via_servers(&query, legacy_via);
    let reason = body.as_ref().and_then(|b| b.reason.as_deref());
    validate_moderation_reason(reason)?;

    let room_id = if room_id_or_alias.starts_with('!') {
        validate_room_id(&room_id_or_alias)?;
        room_id_or_alias.clone()
    } else if room_id_or_alias.starts_with('#') {
        // Try local alias lookup first.
        match ctx.room_service.state().get_room_by_alias(&room_id_or_alias).await {
            Ok(Some(rid)) => rid,
            Ok(None) => {
                // Local lookup failed — try federation directory query for
                // remote aliases.
                // Reference: element-hq/synapse `synapse/handlers/directory.py::DirectoryHandler.get_association`
                let local_server = &ctx.server_name;
                let remote_server =
                    room_id_or_alias.rsplit_once(':').map(|(_, srv)| srv).filter(|srv| *srv != local_server.as_str());

                if let Some(remote_server) = remote_server {
                    let federation_client = ctx.federation_client.clone();
                    let dir_response =
                        federation_client.query_directory(remote_server, &room_id_or_alias).await.map_err(|e| {
                            ::tracing::warn!(
                                room_alias = %room_id_or_alias,
                                server = %remote_server,
                                error = %e,
                                "Federation query_directory failed"
                            );
                            ApiError::not_found(format!("Room alias not found locally or via federation: {e}"))
                        })?;
                    dir_response.room_id
                } else {
                    return Err(ApiError::not_found("Room ID not found for alias".to_string()));
                }
            }
            Err(e) => return Err(ApiError::not_found(format!("Room alias not found: {e}"))),
        }
    } else {
        let alias = format!("#{}:{}", room_id_or_alias, ctx.server_name);
        ctx.room_service
            .state()
            .get_room_by_alias(&alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    };

    ::tracing::info!(
        request_id = %request_id,
        user_id = %auth_user.user_id,
        room_id = %room_id,
        via_servers = ?via_servers,
        "User joining room by id or alias"
    );

    ctx.room_service
        .membership()
        .join_room_with_via_servers_and_reason(&room_id, &auth_user.user_id, &via_servers, reason)
        .await?;

    Ok(Json(json!({
        "room_id": room_id
    })))
}

/// Request body for [`leave_room`].
#[derive(Debug, Deserialize)]
pub struct LeaveRoomBody {
    /// MSC4267: leave the room and forget it in one transaction.
    #[serde(default)]
    pub forget: Option<bool>,
}

/// See [`leave_room`].
// `request_id`（以及为它而读的 `headers`）只在 `friends` 打开时的 DM 同步失败日志里使用；
// feature-off（`core-matrix-min` 车道）下必然未使用，故按配置精确放行该 lint。
#[cfg_attr(not(feature = "friends"), allow(unused_variables))]
pub(crate) async fn leave_room(
    State(ctx): State<RoomContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    body: Option<MatrixJson<LeaveRoomBody>>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_id(&room_id)?;

    // MSC4267: body may carry `forget: true`. Per spec the field is optional
    // and defaults to `false`, so Element Web's plain leave requests are
    // unaffected. When `true`, the server runs leave + forget in a single
    // transaction so a subsequent explicit /forget cannot race in between.
    let forget = body.as_ref().and_then(|b| b.forget).unwrap_or(false);

    if forget {
        ctx.room_service.membership().leave_and_forget(&room_id, &auth_user.user_id).await?;
    } else {
        ctx.room_service.membership().leave_room(&room_id, &auth_user.user_id).await?;
    }

    #[cfg(feature = "friends")]
    if let Err(error) = ctx
        .friend_room_service
        .sync_dm_room_membership_change(&room_id, &auth_user.user_id, "left", Some(&auth_user.user_id), None)
        .await
    {
        ::tracing::warn!(
            request_id = %request_id,
            room_id = %room_id,
            user_id = %auth_user.user_id,
            error = %error,
            "Failed to sync friend DM leave state"
        );
    }
    Ok(Json(json!({})))
}

/// Request body for [`knock_room`].
#[derive(Debug, Deserialize)]
pub struct KnockRoomBody {
    /// Legacy back-compat fallback for the spec's `?via=` query parameter.
    #[serde(default)]
    pub via_servers: Option<Value>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// See [`knock_room`].
pub(crate) async fn knock_room(
    State(ctx): State<RoomContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(room_id_or_alias): Path<String>,
    Query(query): Query<Vec<(String, String)>>,
    MatrixJson(body): MatrixJson<KnockRoomBody>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);

    // MSC4156 / spec: `knock` takes `via` as a repeated query parameter, same
    // as `join`. This server's knock path is currently local-only (it resolves
    // aliases against local state and never federates the knock), so `via`
    // cannot yet influence destination selection — but the parameter is
    // *accepted* rather than silently dropped, and logged so an operator can
    // see that a client asked for federated knock. When federated knocking is
    // implemented, thread this into the service call.
    let via_servers = extract_via_servers(&query, body.via_servers.as_ref());

    let room_id = if room_id_or_alias.starts_with('!') {
        validate_room_id(&room_id_or_alias)?;
        room_id_or_alias.clone()
    } else if room_id_or_alias.starts_with('#') {
        ctx.room_service
            .state()
            .get_room_by_alias(&room_id_or_alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    } else {
        let alias = format!("#{}:{}", room_id_or_alias, ctx.server_name);
        ctx.room_service
            .state()
            .get_room_by_alias(&alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {e}")))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    };

    let reason = body.reason;

    ::tracing::info!(
        request_id = %request_id,
        user_id = %auth_user.user_id,
        room_id = %room_id,
        via_servers = ?via_servers,
        "User knocking on room"
    );

    ctx.room_service.membership().knock_room(&room_id, &auth_user.user_id, reason.as_deref()).await?;

    Ok(Json(json!({
        "room_id": room_id
    })))
}

/// Request body for [`invite_user`].
///
/// The spec allows two mutually exclusive shapes:
///
/// * `{"user_id": "@bob:example.org"}` — invite a known Matrix ID;
/// * `{"id_server", "id_access_token", "medium", "address"}` — invite a
///   third-party identifier (email / phone number), resolved through the
///   identity server.
#[derive(Debug, Deserialize)]
pub struct InviteUserBody {
    pub user_id: Option<String>,
    pub id_server: Option<String>,
    pub id_access_token: Option<String>,
    pub medium: Option<String>,
    pub address: Option<String>,
}

/// See [`invite_user`].
pub(crate) async fn invite_user(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    MatrixJson(body): MatrixJson<InviteUserBody>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let Some(invitee) = body.user_id.as_deref() else {
        return invite_by_3pid(&ctx, &auth_user.user_id, &room_id, &body).await;
    };

    validate_user_id(invitee)?;

    // Matrix spec: inviting a non-existent user must return M_BAD_REQUEST
    // (400) rather than M_NOT_FOUND (404). Only local users can be checked
    // against our database; remote users go through the federation invite
    // path which has its own error handling.
    //
    // U-2: the *active* predicate, so a deactivated local account is rejected
    // here with the same 400 that `MembershipService::invite_user` would raise
    // as its own not-found — otherwise the route would forward a request the
    // service is guaranteed to reject.
    if !ctx.room_service.membership().is_remote_user(invitee)
        && !ctx.account_identity_service.active_user_exists(invitee).await?
    {
        return Err(ApiError::bad_request("User not found".to_string()));
    }

    ctx.room_auth.can_invite_user(&room_id, &auth_user.user_id).await?;

    ctx.room_service.membership().invite_user(&room_id, &auth_user.user_id, invitee).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "invited_user_id": invitee,
        "invited_ts": current_timestamp_millis()
    })))
}

/// Third-party-identifier branch of [`invite_user`].
///
/// Mirrors upstream Synapse's `RoomMemberHandler.do_3pid_invite`: resolve the
/// 3PID to an MXID first (local bindings, then the identity server's `invite`
/// endpoint) and issue a normal `m.room.member` invite for the resulting user.
///
/// The `m.room.third_party_invite` fallback (identity-server `store-invite`)
/// is not implemented; when no Matrix ID can be resolved the request fails
/// with `M_NOT_FOUND` instead of silently succeeding.
async fn invite_by_3pid(
    ctx: &RoomContext,
    inviter: &str,
    room_id: &RoomId,
    body: &InviteUserBody,
) -> Result<Json<Value>, ApiError> {
    let (Some(id_server), Some(id_access_token), Some(medium), Some(address)) =
        (body.id_server.as_deref(), body.id_access_token.as_deref(), body.medium.as_deref(), body.address.as_deref())
    else {
        return Err(ApiError::bad_request(
            "Request must contain either user_id or id_server, id_access_token, medium and address".to_string(),
        ));
    };

    if id_server.is_empty() || id_access_token.is_empty() || medium.is_empty() || address.is_empty() {
        return Err(ApiError::bad_request(
            "id_server, id_access_token, medium and address must not be empty".to_string(),
        ));
    }

    if medium != "email" && medium != "msisdn" {
        return Err(ApiError::bad_request("medium must be 'email' or 'msisdn'".to_string()));
    }

    ctx.room_auth.can_invite_user(room_id, inviter).await?;

    let invitee = match ctx.identity_service.lookup_3pid(medium, address).await? {
        Some(user_id) => user_id,
        None => {
            let invitation =
                ctx.identity_service.invite_3pid(room_id, inviter, medium, address, id_server, id_access_token).await?;
            invitation.user_id.ok_or_else(|| {
                ApiError::not_found("No Matrix user is bound to the given third-party identifier".to_string())
            })?
        }
    };

    ctx.room_service.membership().invite_user(room_id, inviter, &invitee).await?;

    Ok(Json(json!({ "room_id": room_id })))
}

/// The `GET /rooms/{roomId}/members` query parameters (MSC4502 pagination).
///
/// Deliberately **not** `deny_unknown_fields`: the handler used to take a bare
/// query map and ignored unknown parameters. `limit` stays a string so a
/// non-numeric value keeps falling back to the default instead of becoming a
/// deserialisation error.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct RoomMembersQuery {
    pub(crate) at: Option<String>,
    pub(crate) dir: Option<String>,
    pub(crate) limit: Option<String>,
    pub(crate) membership: Option<String>,
    pub(crate) not_membership: Option<String>,
}

/// The `GET /rooms/{roomId}/members/recent` query parameters.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct RoomMembersRecentQuery {
    pub(crate) from: Option<String>,
    pub(crate) limit: Option<String>,
}

/// See [`get_room_members`].
///
/// MSC4502: Supports `at` (cursor), `dir` (f|b), `limit` (max 1000),
/// `membership`, `not_membership` filters for efficient pagination.
pub(crate) async fn get_room_members(
    State(ctx): State<RoomContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(query): Query<RoomMembersQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    let request_id = resolve_request_id(&headers);

    let room = ctx.room_service.get_room(&room_id).await?;

    let is_member =
        is_member_or_creator_ctx(&ctx, &auth_user.user_id, &room_id, room.get("creator").and_then(|v| v.as_str()))
            .await?;

    if !room.get("is_public").and_then(|v| v.as_bool()).unwrap_or(false) && !is_member {
        ::tracing::warn!(
            target: "security_audit",
            request_id = %request_id,
            event = "unauthorized_room_members_access",
            user_id = %auth_user.user_id,
            room_id = %room_id,
            "User attempted to access members of private room without being a member"
        );
        return Err(ApiError::forbidden(
            "You must be a member to view the member list of this private room".to_string(),
        ));
    }

    // MSC4502 pagination params
    let at = query.at.as_deref();
    let dir = query.dir.as_deref();
    // P4-fix: clamp BOTH ends (was `.min(1000)` only). A negative `limit` reached
    // PostgreSQL as a negative LIMIT ⇒ "LIMIT must not be negative" ⇒ mapped to
    // HTTP 500 for malformed client input; `limit=0` produced an empty page that
    // still carried a `next_batch_token`. `metadata.rs` already uses
    // `.clamp(1, 100)` for the same reason.
    let limit: i64 = query.limit.as_deref().and_then(|v| v.parse::<i64>().ok()).unwrap_or(100).clamp(1, 1000);
    let membership_filter = query.membership.as_deref();
    let not_membership_filter = query.not_membership.as_deref();

    if let Some(mf) = membership_filter {
        validate_membership(mf)?;
    }
    if let Some(nmf) = not_membership_filter {
        validate_membership(nmf)?;
    }
    if let Some(d) = dir {
        if d != "f" && d != "b" {
            return Err(ApiError::bad_request("dir must be 'f' or 'b'".to_string()));
        }
    }

    // MSC4502: Use paginated method for O(log n) performance
    let members = ctx
        .room_service
        .membership()
        .get_room_members_paginated(
            &room_id,
            &auth_user.user_id,
            membership_filter,
            not_membership_filter,
            limit,
            at,
            dir,
        )
        .await?;

    Ok(Json(members))
}

/// See [`get_room_members_recent`].
pub(crate) async fn get_room_members_recent(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(query): Query<RoomMembersRecentQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    let members = ctx.room_service.membership().get_room_members(&room_id, &auth_user.user_id).await?;

    let from = query.from.as_deref().and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);
    // P4-fix: this path paginates an in-memory slice rather than issuing a SQL
    // LIMIT, so a negative value merely fails to parse and falls back to the
    // default (no 500). `limit=0` is still degenerate — it returns an empty chunk
    // while advancing `end`/producing a continuation — so clamp the lower bound
    // for consistency with the SQL-backed handlers.
    let limit = query.limit.as_deref().and_then(|value| value.parse::<usize>().ok()).unwrap_or(100).clamp(1, 1000);

    let chunk = members.get("chunk").and_then(|value| value.as_array()).cloned().unwrap_or_default();

    let end_index = std::cmp::min(from.saturating_add(limit), chunk.len());
    let sliced_chunk = if from < chunk.len() { chunk[from..end_index].to_vec() } else { Vec::new() };

    Ok(Json(json!({
        "chunk": sliced_chunk,
        "start": from.to_string(),
        "end": end_index.to_string()
    })))
}

/// See [`get_joined_members`].
pub(crate) async fn get_joined_members(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room = ctx.room_service.get_room(&room_id).await?;

    let is_member = is_member_ctx(&ctx, &auth_user.user_id, &room_id).await?;

    if !room.get("is_public").and_then(|v| v.as_bool()).unwrap_or(false) && !is_member {
        return Err(ApiError::forbidden(
            "You must be a member to view the joined members of this private room".to_string(),
        ));
    }

    let members = ctx.room_service.membership().get_joined_members_with_profiles(&room_id).await?;

    let joined: std::collections::HashMap<String, Value> = members
        .into_iter()
        .map(|m| {
            let user_id = m.user_id.clone();
            let display_name = m.display_name.clone().or_else(|| {
                let uid = &user_id;
                uid.strip_prefix('@').and_then(|s| s.split(':').next()).map(|s| s.to_string())
            });
            (
                user_id,
                json!({
                    "display_name": display_name,
                    "avatar_url": m.avatar_url
                }),
            )
        })
        .collect();

    Ok(Json(json!({
        "joined": joined
    })))
}

/// See [`get_room_membership`].
pub(crate) async fn get_room_membership(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, target_user_id)): Path<(RoomId, UserId)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_user_id(target_user_id.as_str())?;

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let membership = ctx
        .room_service
        .membership()
        .get_room_member_record(&room_id, target_user_id.as_str())
        .await?
        .map_or_else(|| "leave".to_string(), |m| m.membership);

    Ok(Json(json!({
        "membership": membership
    })))
}

/// Request body for [`get_membership_events`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MembershipEventsRequest {
    /// Maximum number of membership events to return (defaults to 100, capped at 1000).
    pub limit: Option<u64>,
}

/// See [`get_membership_events`].
pub(crate) async fn get_membership_events(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    MatrixJson(body): MatrixJson<MembershipEventsRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let limit = body.limit.unwrap_or(100).min(1000) as i64;

    let memberships = ctx.room_service.membership().get_membership_history(&room_id, limit).await?;

    let events: Vec<Value> = memberships
        .into_iter()
        .map(|m| {
            json!({
                "event_id": m.event_id,
                "type": m.event_type,
                "sender": m.sender,
                "state_key": m.user_id,
                "content": {
                    "membership": m.membership
                },
                "origin_server_ts": m.joined_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "events": events
    })))
}

/// See [`get_room_invites`].
pub(crate) async fn get_room_invites(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let _invites = ctx.room_service.membership().get_joined_members_with_profiles(&room_id).await?;

    let invited_members: Vec<serde_json::Value> = ctx
        .room_service
        .membership()
        .get_room_members_by_membership(&room_id, "invite")
        .await?
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "user_id": m.user_id,
                "sender": m.sender,
                "display_name": m.display_name,
                "avatar_url": m.avatar_url,
                "event_id": m.event_id,
                "reason": m.reason,
                "updated_ts": m.updated_ts
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "invites": invited_members,
        "total": invited_members.len()
    })))
}

/// Request body for [`kick_user`].
#[derive(Debug, Deserialize)]
pub struct KickUserBody {
    pub user_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

/// See [`kick_user`].
pub(crate) async fn kick_user(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    MatrixJson(body): MatrixJson<KickUserBody>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body.user_id.as_str();

    validate_user_id(target)?;

    let reason = body.reason.as_deref();
    validate_moderation_reason(reason)?;

    ctx.room_service.membership().kick_user(&room_id, target, &auth_user.user_id, reason).await?;

    Ok(Json(json!({})))
}

/// Request body for [`ban_user`].
#[derive(Debug, Deserialize)]
pub struct BanUserBody {
    pub user_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

/// See [`ban_user`].
pub(crate) async fn ban_user(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    MatrixJson(body): MatrixJson<BanUserBody>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body.user_id.as_str();

    validate_user_id(target)?;

    let reason = body.reason.as_deref();
    validate_moderation_reason(reason)?;

    ctx.room_service.membership().ban_user(&room_id, target, &auth_user.user_id, reason).await?;

    Ok(Json(json!({})))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Request body for [`unban_user`].
#[derive(Debug, Deserialize)]
pub struct UnbanUserBody {
    pub user_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

/// See [`unban_user`].
pub(crate) async fn unban_user(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    MatrixJson(body): MatrixJson<UnbanUserBody>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body.user_id.as_str();

    validate_user_id(target)?;

    let reason = body.reason.as_deref();
    validate_moderation_reason(reason)?;

    ctx.room_service.membership().unban_user(&room_id, target, &auth_user.user_id, reason).await?;

    Ok(Json(json!({})))
}

#[cfg(test)]
mod via_servers_tests {
    //! MSC4156 / spec `via` extraction for `join` and `knock`.
    //!
    //! Regression context: the join handler previously read a **non-standard
    //! body key** `via_servers`, so the standard client payload
    //! `{"via":[...]}` (and the spec's `?via=` query parameter) was silently
    //! ignored and federated joins fell back to the room-id domain. `knock`
    //! ignored `via` entirely.
    //!
    //! Reference: ruma `join_room_by_id_or_alias` (via is a repeated query
    //! parameter) and MSC4156 ("Migrate server_name to via").

    use super::{extract_via_servers, percent_decode_component};
    use serde_json::json;

    fn q(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
    }

    // ------------------------------------------------------------------
    // percent decoding
    // ------------------------------------------------------------------

    #[test]
    fn percent_decode_leaves_plain_text_untouched() {
        assert_eq!(percent_decode_component("matrix.org"), "matrix.org");
    }

    #[test]
    fn percent_decode_handles_encoded_characters() {
        assert_eq!(percent_decode_component("a%2Eb"), "a.b");
        // `%3A` is ':' — a port-bearing server name would arrive encoded.
        assert_eq!(percent_decode_component("srv%3A8448"), "srv:8448");
    }

    #[test]
    fn percent_decode_treats_plus_as_space() {
        assert_eq!(percent_decode_component("a+b"), "a b");
    }

    #[test]
    fn percent_decode_passes_malformed_sequences_through() {
        // A bad `via` hint must not turn a join into a 400.
        assert_eq!(percent_decode_component("100%"), "100%");
        assert_eq!(percent_decode_component("%zz"), "%zz");
    }

    // ------------------------------------------------------------------
    // spec-defined query parameter
    // ------------------------------------------------------------------

    #[test]
    fn reads_repeated_via_query_parameter() {
        let query = q(&[("via", "srv1.example"), ("via", "srv2.example")]);
        assert_eq!(extract_via_servers(&query, None), vec!["srv1.example", "srv2.example"]);
    }

    #[test]
    fn reads_percent_encoded_via_query_parameter() {
        let query = q(&[("via", "srv%3A8448")]);
        assert_eq!(extract_via_servers(&query, None), vec!["srv:8448"]);
    }

    #[test]
    fn empty_via_values_are_dropped() {
        let query = q(&[("via", ""), ("via", "srv1.example")]);
        assert_eq!(extract_via_servers(&query, None), vec!["srv1.example"]);
    }

    // ------------------------------------------------------------------
    // back-compat: pre-fix body key
    // ------------------------------------------------------------------

    #[test]
    fn falls_back_to_legacy_body_key_when_no_query_via() {
        let legacy = json!(["legacy.example"]);
        assert_eq!(extract_via_servers(&[], Some(&legacy)), vec!["legacy.example"]);
    }

    #[test]
    fn query_via_wins_over_legacy_body_key() {
        let query = q(&[("via", "query.example")]);
        let legacy = json!(["legacy.example"]);
        assert_eq!(
            extract_via_servers(&query, Some(&legacy)),
            vec!["query.example"],
            "规范参数 `via` 必须优先于遗留 body 键 `via_servers`"
        );
    }

    #[test]
    fn legacy_body_key_ignores_non_string_entries() {
        let legacy = json!(["ok.example", 42, null, {"nested": true}]);
        assert_eq!(extract_via_servers(&[], Some(&legacy)), vec!["ok.example"]);
    }

    #[test]
    fn legacy_body_key_that_is_not_an_array_yields_empty() {
        let legacy = json!("srv1.example");
        assert!(extract_via_servers(&[], Some(&legacy)).is_empty());
    }

    // ------------------------------------------------------------------
    // no hints at all
    // ------------------------------------------------------------------

    #[test]
    fn returns_empty_when_no_via_anywhere() {
        assert!(extract_via_servers(&[], None).is_empty());
        assert!(extract_via_servers(&[], Some(&json!([]))).is_empty());
    }

    #[test]
    fn unrelated_query_parameters_are_ignored() {
        let query = q(&[("limit", "10"), ("dir", "f"), ("via", "srv1.example")]);
        assert_eq!(extract_via_servers(&query, None), vec!["srv1.example"]);
    }
}
