use crate::routes::context::MediaContext;
use crate::routes::AuthenticatedUser;
use axum::{
    extract::{Json, Query, State},
    http::{header, HeaderValue},
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::ApiError;

/// See [`media_config`].
pub(crate) async fn media_config(State(ctx): State<MediaContext>, _auth_user: AuthenticatedUser) -> impl IntoResponse {
    let route_owner = synapse_services::worker::topology_validator::current_instance_worker_type(&ctx.config.worker);
    (
        [(header::HeaderName::from_static("x-synapse-route-owner"), HeaderValue::from_static(route_owner.as_str()))],
        Json(json!({
            "m.upload.size": ctx.config.server.max_upload_size
        })),
    )
}

/// Query parameters accepted by [`preview_url`].
///
/// Deliberately **not** `deny_unknown_fields`: the handler used to take a bare
/// `Query<Value>` and silently ignored every parameter it did not read. A
/// client adding `?foo=1` must keep getting a 200, not a 400.
///
/// `ts` is declared but never read on purpose, and that is load-bearing twice
/// over:
/// - `scripts/contract/ledger_annotations.txt` declares `query_params=ts,url`
///   for the four preview endpoints, and the contract gate's reverse guard
///   compares that list against the fields of this struct. Dropping the field
///   would silently turn the annotation into a lie until the whole contract
///   chain (derived tables, sidecar, snapshots) is regenerated;
/// - it keeps `?ts=…` a *known* key, so adding `deny_unknown_fields` later
///   cannot start rejecting clients that still send the legacy parameter.
///
/// `#[expect]` rather than `#[allow]`: if anything ever starts reading `ts`,
/// the expectation becomes unfulfilled and the build fails, which is exactly
/// the reminder we want. See CODE-12 in
/// `docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md`.
#[derive(Debug, Deserialize)]
pub(crate) struct PreviewUrlParams {
    /// Optional at the serde layer so the handler keeps answering a missing
    /// `url` with the Matrix `M_BAD_REQUEST` body rather than axum's plain-text
    /// query rejection.
    url: Option<String>,
    #[expect(
        dead_code,
        reason = "legacy Matrix preview_url parameter: parsed for API compatibility and contract \
                  annotation, deliberately not read (the response reflects the page, not a \
                  client-supplied timestamp)"
    )]
    ts: Option<i64>,
}

/// See [`preview_url`].
pub(crate) async fn preview_url(
    State(ctx): State<MediaContext>,
    _auth_user: AuthenticatedUser,
    Query(params): Query<PreviewUrlParams>,
) -> Result<Json<Value>, ApiError> {
    // P2-11: MSC4452 endpoint-level 403 enforcement (Synapse v1.154 #19715).
    // When the `io.element.msc4452.preview_url` capability is disabled
    // (controlled by `config.experimental.msc4452_enabled`, default false),
    // the preview_url endpoint MUST return 403 Forbidden, matching the
    // capability declaration in capability_governance.rs. This is a
    // capability-driven feature gate, not a permission check — fail-closed
    // when the feature is off.
    if !ctx.config.experimental.msc4452_enabled {
        return Err(ApiError::forbidden("URL preview is disabled (MSC4452 not enabled)".to_string()));
    }

    // Code-12: there used to be a dead expression here —
    // `let _ = ts.unwrap_or_else(current_timestamp_millis);` — which discarded the
    // value on both branches, bought one wasted `current_timestamp_millis()` call on
    // the `None` path, and read as if a timestamp were used for caching. The field
    // stays declared (see `PreviewUrlParams`), the statement does not.
    let PreviewUrlParams { url, ts: _ } = params;
    let url = url.as_deref().ok_or_else(|| ApiError::bad_request("URL required".to_string()))?;

    let blacklist = &ctx.config.url_preview.ip_range_blacklist;
    // The first hop is validated here so a blacklisted target is a **403** (a policy
    // decision) rather than an empty preview (which would look like "the site had no
    // metadata"). The resolution is then handed to the service so the fetch connects
    // to the address that was checked instead of resolving again (PERF-13). Every hop
    // after a redirect is re-validated inside the service — that is where the
    // DNS-rebinding pinning lives (`synapse_services::url_preview::fetch_html`).
    let first_hop = match synapse_common::check_url_and_resolve(url, blacklist) {
        Ok((host, ips)) => synapse_services::url_preview::VerifiedFirstHop::new(url, host, ips),
        Err(e) => return Err(ApiError::forbidden(format!("URL not allowed: {e}"))),
    };

    Ok(Json(ctx.media_domain_service.preview_url(url, Some(&first_hop), blacklist).await?))
}
