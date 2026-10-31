use crate::routes::context::AdminContext;
use crate::routes::extractors::MatrixPath;
use crate::routes::extractors::RoomId;
use crate::routes::AdminUser;
use axum::{
    extract::State,
    routing::{delete, get},
    Json, Router,
};
use serde_json::{json, Value};
use synapse_common::ApiError;
use synapse_common::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use synapse_services::event_report_service::EventReport;

/// See [`create_report_router`].
pub fn create_report_router() -> Router<crate::routes::AppState> {
    Router::new()
        .route("/_synapse/admin/v1/reports", get(get_all_reports))
        .route("/_synapse/admin/v1/reports/{report_id}", get(get_report))
        .route("/_synapse/admin/v1/reports/{report_id}", delete(delete_report))
        .route("/_synapse/admin/v1/rooms/{room_id}/reports", get(get_room_reports))
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/reports/{report_id}",
            get(get_room_report).delete(delete_room_report),
        )
}

fn report_to_json(report: &EventReport) -> Value {
    json!({
        "id": report.id,
        "room_id": report.room_id,
        "event_id": report.event_id,
        "user_id": report.reporter_user_id,
        "reported_user_id": report.reported_user_id,
        "reason": report.reason,
        "content": report.description,
        "status": report.status,
        "score": report.score,
        "received_ts": report.received_ts
    })
}

/// Query parameters accepted by [`get_all_reports`].
///
/// Deliberately **not** `deny_unknown_fields`: the handler used to take a bare
/// `HashMap<String, String>` and ignored unknown parameters.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ReportsQuery {
    limit: Option<String>,
    /// Opaque cursor returned as `next_token` by the previous page.
    from: Option<String>,
    since_score: Option<String>,
    since_ts: Option<String>,
    since_id: Option<String>,
    offset: Option<String>,
}

/// Query parameters accepted by [`get_room_reports`].
/// 有意开放未知字段：旧 handler 是 `Query<HashMap>`，忽略未知参数（P2-C 裁决）。
#[derive(Debug, Default, serde::Deserialize)]
pub struct RoomReportsQuery {
    limit: Option<String>,
    /// Opaque cursor returned as `next_token` by the previous page.
    from: Option<String>,
    since_ts: Option<String>,
    since_id: Option<String>,
    offset: Option<String>,
}

/// Decode the global reports cursor `"{score}|{received_ts}|{id}"`.
///
/// The cursor mirrors the storage ordering (`score DESC, received_ts DESC,
/// id DESC`), so a page can be resumed without skipping or repeating rows.
fn parse_reports_cursor(raw: &str) -> Result<(i32, i64, i64), ApiError> {
    let mut parts = raw.split('|');
    let score = parts.next().and_then(|v| v.parse::<i32>().ok());
    let received_ts = parts.next().and_then(|v| v.parse::<i64>().ok());
    let id = parts.next().and_then(|v| v.parse::<i64>().ok());

    match (score, received_ts, id, parts.next()) {
        (Some(score), Some(received_ts), Some(id), None) => Ok((score, received_ts, id)),
        _ => Err(ApiError::invalid_param("Invalid `from` cursor; expected `{score}|{received_ts}|{id}`".to_string())),
    }
}

/// Decode the per-room reports cursor `"{received_ts}|{id}"`.
fn parse_room_reports_cursor(raw: &str) -> Result<(i64, i64), ApiError> {
    let mut parts = raw.split('|');
    let received_ts = parts.next().and_then(|v| v.parse::<i64>().ok());
    let id = parts.next().and_then(|v| v.parse::<i64>().ok());

    match (received_ts, id, parts.next()) {
        (Some(received_ts), Some(id), None) => Ok((received_ts, id)),
        _ => Err(ApiError::invalid_param("Invalid `from` cursor; expected `{received_ts}|{id}`".to_string())),
    }
}

/// Emit the next global cursor only when the current page was full.
fn next_reports_token(reports: &[EventReport], limit: i64) -> Option<String> {
    if reports.len() as i64 != limit {
        return None;
    }

    reports.last().map(|report| format!("{}|{}|{}", report.score, report.received_ts, report.id))
}

/// Emit the next per-room cursor only when the current page was full.
fn next_room_reports_token(reports: &[EventReport], limit: i64) -> Option<String> {
    if reports.len() as i64 != limit {
        return None;
    }

    reports.last().map(|report| format!("{}|{}", report.received_ts, report.id))
}

/// See [`get_all_reports`].
#[axum::debug_handler]
pub async fn get_all_reports(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    crate::routes::extractors::MatrixQuery(params): crate::routes::extractors::MatrixQuery<ReportsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .limit
        .as_deref()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);

    if params.offset.as_deref().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0) > 0 {
        return Err(ApiError::bad_request(
            "Legacy offset pagination is no longer supported; use `from` or since_ts/since_id cursors".to_string(),
        ));
    }

    // B-26: `from` is the upstream-compatible cursor and wins over the legacy
    // `since_*` trio, which is kept for one release as a transition path.
    let (since_score, since_ts, since_id) = match params.from.as_deref() {
        Some(cursor) => {
            let (score, ts, id) = parse_reports_cursor(cursor)?;
            (Some(score), Some(ts), Some(id))
        }
        None => (
            params.since_score.as_deref().and_then(|v| v.parse::<i32>().ok()),
            params.since_ts.as_deref().and_then(|v| v.parse::<i64>().ok()),
            params.since_id.as_deref().and_then(|v| v.parse::<i64>().ok()),
        ),
    };

    let reports = ctx.event_report_service.get_all_reports(limit, since_score, since_ts, since_id).await?;
    let next_token = next_reports_token(&reports, limit);
    let total = ctx.event_report_service.count_all_reports().await?;
    let report_list: Vec<Value> = reports.iter().map(report_to_json).collect();

    Ok(Json(json!({ "reports": report_list, "next_token": next_token, "total": total })))
}

/// See [`get_report`].
#[axum::debug_handler]
pub async fn get_report(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    MatrixPath(report_id): MatrixPath<i64>,
) -> Result<Json<Value>, ApiError> {
    let report = ctx.event_report_service.get_report(report_id).await?;

    match report {
        Some(report) => Ok(Json(report_to_json(&report))),
        None => Err(ApiError::not_found("Report not found".to_string())),
    }
}

/// See [`delete_report`].
#[axum::debug_handler]
pub async fn delete_report(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    MatrixPath(report_id): MatrixPath<i64>,
) -> Result<Json<Value>, ApiError> {
    ctx.event_report_service.delete_report(report_id).await?;

    Ok(Json(json!({})))
}

/// See [`get_room_reports`].
#[axum::debug_handler]
pub async fn get_room_reports(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    MatrixPath(room_id): MatrixPath<RoomId>,
    crate::routes::extractors::MatrixQuery(params): crate::routes::extractors::MatrixQuery<RoomReportsQuery>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = ctx.room_service.state().room_exists(&room_id).await?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = params
        .limit
        .as_deref()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);

    if params.offset.as_deref().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0) > 0 {
        return Err(ApiError::bad_request(
            "Legacy offset pagination is no longer supported; use `from` or since_ts/since_id cursors".to_string(),
        ));
    }

    // B-26: per-room cursor, `from` wins over the legacy `since_*` pair.
    let (since_ts, since_id) = match params.from.as_deref() {
        Some(cursor) => {
            let (ts, id) = parse_room_reports_cursor(cursor)?;
            (Some(ts), Some(id))
        }
        None => (
            params.since_ts.as_deref().and_then(|v| v.parse::<i64>().ok()),
            params.since_id.as_deref().and_then(|v| v.parse::<i64>().ok()),
        ),
    };

    let reports = ctx.event_report_service.get_reports_by_room(&room_id, limit, since_ts, since_id).await?;
    let next_token = next_room_reports_token(&reports, limit);
    let total = ctx.event_report_service.count_reports_by_room(&room_id).await?;
    let report_list: Vec<Value> = reports.iter().map(report_to_json).collect();

    Ok(Json(json!({ "reports": report_list, "next_token": next_token, "total": total })))
}

/// See [`get_room_report`].
#[axum::debug_handler]
pub async fn get_room_report(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    MatrixPath((room_id, report_id)): MatrixPath<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = ctx.room_service.state().room_exists(&room_id).await?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let report = ctx.event_report_service.get_report(report_id).await?;

    match report {
        Some(report) if report.room_id == room_id => Ok(Json(report_to_json(&report))),
        _ => Err(ApiError::not_found("Report not found".to_string())),
    }
}

/// See [`delete_room_report`].
#[axum::debug_handler]
pub async fn delete_room_report(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    MatrixPath((room_id, report_id)): MatrixPath<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let room_exists = ctx.room_service.state().room_exists(&room_id).await?;

    if !room_exists {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let report = ctx.event_report_service.get_report(report_id).await?;

    match report {
        Some(report) if report.room_id == room_id => {
            ctx.event_report_service.delete_report(report_id).await?;
            Ok(Json(json!({})))
        }
        _ => Err(ApiError::not_found("Report not found".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(score: i32, received_ts: i64, id: i64) -> EventReport {
        EventReport {
            id,
            event_id: format!("$event{id}:example.com"),
            room_id: "!room:example.com".to_string(),
            reporter_user_id: "@reporter:example.com".to_string(),
            reported_user_id: None,
            event_json: None,
            reason: None,
            description: None,
            status: "open".to_string(),
            score,
            received_ts,
            resolved_ts: None,
            resolved_by: None,
            resolution_reason: None,
        }
    }

    #[test]
    fn reports_cursor_round_trips() {
        assert_eq!(parse_reports_cursor("10|1700000000000|42").unwrap(), (10, 1_700_000_000_000, 42));
    }

    #[test]
    fn reports_cursor_rejects_malformed_input() {
        for raw in ["", "10", "10|1700000000000", "10|abc|42", "10|1700000000000|42|extra", "abc|1|2"] {
            assert!(parse_reports_cursor(raw).is_err(), "expected {raw:?} to be rejected");
        }
    }

    #[test]
    fn room_reports_cursor_round_trips() {
        assert_eq!(parse_room_reports_cursor("1700000000000|42").unwrap(), (1_700_000_000_000, 42));
    }

    #[test]
    fn room_reports_cursor_rejects_malformed_input() {
        for raw in ["", "1700000000000", "abc|42", "1700000000000|42|extra"] {
            assert!(parse_room_reports_cursor(raw).is_err(), "expected {raw:?} to be rejected");
        }
    }

    #[test]
    fn next_token_only_emitted_for_full_pages() {
        let page = vec![report(10, 200, 2), report(5, 100, 1)];
        assert_eq!(next_reports_token(&page, 2).as_deref(), Some("5|100|1"));
        assert_eq!(next_reports_token(&page, 3), None);
        assert_eq!(next_reports_token(&[], 2), None);
    }

    #[test]
    fn room_next_token_only_emitted_for_full_pages() {
        let page = vec![report(10, 200, 2), report(5, 100, 1)];
        assert_eq!(next_room_reports_token(&page, 2).as_deref(), Some("100|1"));
        assert_eq!(next_room_reports_token(&page, 5), None);
        assert_eq!(next_room_reports_token(&[], 2), None);
    }
}
