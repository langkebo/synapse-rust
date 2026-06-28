use crate::common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::common::ApiError;
use synapse_services::admin_federation_service::{
    decode_destination_cursor, decode_pending_federation_cursor, encode_destination_cursor,
    encode_pending_federation_cursor,
};
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_storage::federation_blacklist::decode_federation_blacklist_cursor;
use tracing::info;

pub fn create_federation_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/federation/destinations", get(get_destinations))
        .route("/_synapse/admin/v1/federation/destinations/{destination}", get(get_destination))
        .route("/_synapse/admin/v1/federation/destinations/{destination}/reset_connection", post(reset_connection))
        .route("/_synapse/admin/v1/federation/destinations/{destination}/reset", post(reset_connection))
        .route("/_synapse/admin/v1/federation/destinations/{destination}", delete(delete_destination))
        .route("/_synapse/admin/v1/federation/destinations/{destination}/rooms", get(get_destination_rooms))
        .route("/_synapse/admin/v1/federation/rewrite", post(rewrite_federation))
        .route("/_synapse/admin/v1/federation/resolve", post(resolve_federation))
        .route("/_synapse/admin/v1/federation/confirm", post(confirm_federation))
        .route("/_synapse/admin/v1/federation/pending", get(list_pending_federation))
        .route("/_synapse/admin/v1/federation/blacklist", get(get_blacklist))
        .route("/_synapse/admin/v1/federation/blacklist/{server_name}", post(add_to_blacklist))
        .route("/_synapse/admin/v1/federation/blacklist/{server_name}", delete(remove_from_blacklist))
        .route("/_synapse/admin/v1/federation/cache", get(get_federation_cache))
        .route("/_synapse/admin/v1/federation/cache/{key}", delete(delete_federation_cache_entry))
        .route("/_synapse/admin/v1/federation/cache/clear", post(clear_federation_cache))
}

pub fn admin_federation_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_synapse/admin/v1/federation/destinations"),
        (Method::GET, "/_synapse/admin/v1/federation/destinations/{destination}"),
        (Method::POST, "/_synapse/admin/v1/federation/destinations/{destination}/reset_connection"),
        (Method::POST, "/_synapse/admin/v1/federation/destinations/{destination}/reset"),
        (Method::DELETE, "/_synapse/admin/v1/federation/destinations/{destination}"),
        (Method::GET, "/_synapse/admin/v1/federation/destinations/{destination}/rooms"),
        (Method::POST, "/_synapse/admin/v1/federation/rewrite"),
        (Method::POST, "/_synapse/admin/v1/federation/resolve"),
        (Method::POST, "/_synapse/admin/v1/federation/confirm"),
        (Method::GET, "/_synapse/admin/v1/federation/pending"),
        (Method::GET, "/_synapse/admin/v1/federation/blacklist"),
        (Method::POST, "/_synapse/admin/v1/federation/blacklist/{server_name}"),
        (Method::DELETE, "/_synapse/admin/v1/federation/blacklist/{server_name}"),
        (Method::GET, "/_synapse/admin/v1/federation/cache"),
        (Method::DELETE, "/_synapse/admin/v1/federation/cache/{key}"),
        (Method::POST, "/_synapse/admin/v1/federation/cache/clear"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::federation"))
    .collect()
}

#[derive(Debug, Deserialize)]
pub struct RewriteRequest {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct ResolveRequest {
    pub server_name: String,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmRequest {
    pub server_name: String,
    pub accept: bool,
}

#[derive(Debug, Deserialize)]
pub struct ListPendingQuery {
    pub limit: Option<i32>,
    pub from: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlacklistQuery {
    pub limit: Option<i32>,
    pub from: Option<String>,
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_pending_federation_cursor, encode_pending_federation_cursor};

    #[test]
    fn test_pending_federation_cursor_round_trip() {
        let cursor = encode_pending_federation_cursor(&crate::services::PendingFederationCursor {
            updated_ts: 1_700_000_000_000,
            server_name: "matrix.example.com".to_string(),
        });
        assert_eq!(
            decode_pending_federation_cursor(Some(&cursor)),
            Some(crate::services::PendingFederationCursor {
                updated_ts: 1_700_000_000_000,
                server_name: "matrix.example.com".to_string(),
            })
        );
    }

    #[test]
    fn test_pending_federation_cursor_rejects_invalid_value() {
        assert_eq!(decode_pending_federation_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_pending_federation_cursor(Some("123|")), None);
    }
}

#[derive(Debug, Deserialize)]
pub struct DestinationsQuery {
    pub limit: Option<i32>,
    pub from: Option<String>,
    pub offset: Option<i64>,
}

fn validate_destinations_query(
    query: &DestinationsQuery,
) -> Result<(i32, Option<crate::services::DestinationCursor>), ApiError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let cursor = decode_destination_cursor(query.from.as_deref());

    if query.offset.unwrap_or(0) > 0 {
        return Err(ApiError::bad_request(
            "Legacy offset pagination is no longer supported; use from cursor".to_string(),
        ));
    }
    if query.from.is_some() && cursor.is_none() {
        return Err(ApiError::bad_request("Invalid destination pagination cursor".to_string()));
    }

    Ok((limit, cursor))
}

#[axum::debug_handler]
pub async fn get_destinations(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<DestinationsQuery>,
) -> Result<Json<Value>, ApiError> {
    let (limit, cursor) = validate_destinations_query(&query)?;

    let (destinations, total, next_batch) =
        state.services.admin.federation.admin_federation_service.list_destinations(limit, cursor).await?;

    Ok(Json(json!({
        "destinations": destinations,
        "total": total,
        "total_count": total,
        "next_batch": next_batch.as_ref().map(encode_destination_cursor),
    })))
}

#[axum::debug_handler]
pub async fn get_destination(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match state.services.admin.federation.admin_federation_service.get_destination(&destination).await? {
        Some(row) => Ok(Json(json!(row))),
        None => Err(ApiError::not_found("Destination not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn reset_connection(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state.services.admin.federation.admin_federation_service.reset_connection(&destination).await?;
    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn delete_destination(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state.services.admin.federation.admin_federation_service.delete_destination(&destination).await?;
    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_destination_rooms(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room_list =
        state.services.admin.federation.admin_federation_service.get_destination_rooms(&destination).await?;

    Ok(Json(json!({ "rooms": room_list, "total": room_list.len() })))
}

#[axum::debug_handler]
pub async fn rewrite_federation(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<RewriteRequest>,
) -> Result<Json<Value>, ApiError> {
    let from_server = &body.from;
    let to_server = &body.to;
    let rooms_count = state
        .services
        .admin
        .federation
        .admin_federation_service
        .rewrite_federation(from_server, to_server, &admin.user_id)
        .await?;

    info!(
        "Federation rewrite from {} to {}: {} rooms affected by {}",
        from_server, to_server, rooms_count, admin.user_id
    );

    Ok(Json(json!({
        "from": from_server,
        "to": to_server,
        "rewritten": true,
        "rooms_affected": rooms_count,
        "rewritten_by": admin.user_id
    })))
}

#[axum::debug_handler]
pub async fn resolve_federation(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<ResolveRequest>,
) -> Result<Json<Value>, ApiError> {
    let server_name = &body.server_name;
    let result = state.services.admin.federation.admin_federation_service.resolve_federation(server_name).await?;

    info!("Federation resolve for {}: resolved={}, blacklisted={}", server_name, result.resolved, result.blacklisted);

    Ok(Json(json!({
        "server_name": server_name,
        "resolved": result.resolved,
        "blacklisted": result.blacklisted,
        "in_destinations": result.in_destinations,
        "resolved_by": admin.user_id
    })))
}

#[axum::debug_handler]
pub async fn confirm_federation(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<ConfirmRequest>,
) -> Result<Json<Value>, ApiError> {
    let result = state
        .services
        .admin
        .federation
        .admin_federation_service
        .confirm_federation(&body.server_name, body.accept, &admin.user_id)
        .await?;

    info!(
        "Federation admission {} for server '{}' by admin '{}'",
        if body.accept { "accepted" } else { "rejected" },
        body.server_name,
        admin.user_id
    );

    Ok(Json(json!({
        "server_name": body.server_name,
        "status": result.status,
        "previous_status": result.previous_status,
        "updated_ts": result.updated_ts,
        "confirmed_by": admin.user_id
    })))
}

#[axum::debug_handler]
pub async fn list_pending_federation(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<ListPendingQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(500);
    let cursor = decode_pending_federation_cursor(query.from.as_deref());
    let (list, total, next_batch) =
        state.services.admin.federation.admin_federation_service.list_pending_federation(limit, cursor).await?;
    let next_batch = next_batch.as_ref().map(encode_pending_federation_cursor);

    Ok(Json(json!({
        "servers": list,
        "total": total,
        "limit": limit,
        "next_batch": next_batch
    })))
}

#[axum::debug_handler]
pub async fn get_blacklist(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<BlacklistQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).clamp(MIN_PAGINATION_LIMIT as i32, MAX_PAGINATION_LIMIT as i32);
    let from = decode_federation_blacklist_cursor(query.from.as_deref());

    if query.from.is_some() && from.is_none() {
        return Err(ApiError::bad_request("Invalid from cursor".to_string()));
    }

    let (blacklist, next_batch) =
        state.services.admin.federation.federation_blacklist_service.get_blacklist(limit, from).await?;

    let list: Vec<Value> = blacklist
        .iter()
        .map(|row| {
            json!({
                "server_name": row.server_name,
                "added_at": row.created_ts,
                "reason": row.reason
            })
        })
        .collect();

    Ok(Json(json!({
        "blacklist": list,
        "total": list.len(),
        "next_batch": next_batch
    })))
}

#[axum::debug_handler]
pub async fn add_to_blacklist(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(server_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state.services.admin.federation.admin_federation_service.add_to_blacklist(&server_name, &admin.user_id).await?;
    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn remove_from_blacklist(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(server_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .admin
        .federation
        .admin_federation_service
        .remove_from_blacklist(&server_name, &admin.user_id)
        .await?;
    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_federation_cache(_admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let entries = state.services.admin.federation.admin_federation_service.get_federation_cache().await?;

    Ok(Json(json!({ "cache": entries, "total": entries.len() })))
}

#[axum::debug_handler]
pub async fn delete_federation_cache_entry(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state.services.admin.federation.admin_federation_service.delete_federation_cache_entry(&key).await?;
    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn clear_federation_cache(_admin: AdminUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let deleted = state.services.admin.federation.admin_federation_service.clear_federation_cache().await?;
    Ok(Json(json!({ "deleted": deleted })))
}

#[cfg(test)]
mod destinations_query_tests {
    use super::{validate_destinations_query, DestinationsQuery};

    #[test]
    fn rejects_legacy_offset_pagination() {
        let query = DestinationsQuery { limit: Some(50), from: None, offset: Some(10) };

        let err = validate_destinations_query(&query).expect_err("legacy offset must be rejected");
        assert!(err.to_string().contains("Legacy offset pagination"));
    }

    #[test]
    fn rejects_invalid_from_cursor() {
        let query = DestinationsQuery { limit: Some(50), from: Some("bad-cursor".to_string()), offset: None };

        let err = validate_destinations_query(&query).expect_err("invalid cursor must be rejected");
        assert!(err.to_string().contains("Invalid destination pagination cursor"));
    }

    #[test]
    fn accepts_valid_cursor() {
        let query =
            DestinationsQuery { limit: Some(50), from: Some("v1|matrix.example.com".to_string()), offset: Some(0) };

        let (limit, cursor) = validate_destinations_query(&query).expect("cursor should be accepted");
        assert_eq!(limit, 50);
        assert_eq!(cursor.expect("cursor should exist").server_name, "matrix.example.com");
    }
}
