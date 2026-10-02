//! MSC3720 account-status endpoints.
//!
//! Two thin adapters over `synapse_services::account::AccountStatusService`:
//!
//! * [`client_account_status`] — `POST /_matrix/client/unstable/org.matrix.msc3720/account_status`
//!   (authenticated; remote users are looked up over federation);
//! * [`federation_account_status`] — `POST /_matrix/federation/unstable/org.matrix.msc3720/account_status`
//!   (federated-auth protected by the federation router; every requested user
//!   must be local, else `400 M_INVALID_PARAM`).
//!
//! Wire contract (MSC3720):
//!
//! * request `{ "user_ids": ["@a:hs", ...] }`;
//! * response `{ "account_statuses": { "@a:hs": { "exists": true, "deactivated": false } }, "failures": [] }`;
//! * an empty `user_ids` list yields `200 {}` (the MSC specifies an empty body);
//! * missing `user_ids` → `400 M_MISSING_PARAM`; a malformed user ID → `400 M_INVALID_PARAM`.
//!
//! Feature gate: like the MSC4452 preview-url endpoint, both routes are always
//! registered but **fail closed with 403 `M_FORBIDDEN`** unless
//! `experimental.msc3720_enabled` is set. This keeps the route ledger static
//! while remaining capability-driven (the `org.matrix.msc3720.account_status`
//! capability reports the same boolean).

use crate::routes::context::{AuthContext, FederationContext};
use crate::routes::AuthenticatedUser;
use axum::{extract::State, Json};
use serde_json::{json, Map, Value};
use synapse_common::ApiError;
use synapse_services::account::{AccountStatusError, AccountStatusService, AccountStatuses};

/// See [`parse_body`].
fn ensure_enabled(config: &synapse_common::config::Config) -> Result<(), ApiError> {
    if config.experimental.msc3720_enabled {
        return Ok(());
    }
    Err(ApiError::forbidden("Account status is disabled (MSC3720 not enabled)".to_string()))
}

/// Parse and validate the request body.
///
/// `user_ids` must be present, an array, and contain only strings; anything
/// else maps onto the MSC's `M_MISSING_PARAM` / `M_INVALID_PARAM` codes instead
/// of axum's default rejection codes.
fn parse_body(body: &Value) -> Result<Vec<String>, ApiError> {
    let object = body.as_object().ok_or_else(|| ApiError::invalid_param("Request body must be a JSON object"))?;
    let user_ids = object
        .get("user_ids")
        .ok_or_else(|| ApiError::missing_param("Missing user_ids"))?
        .as_array()
        .ok_or_else(|| ApiError::invalid_param("user_ids must be an array of strings"))?;

    user_ids
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| ApiError::invalid_param("user_ids must be an array of strings"))
        })
        .collect()
}

/// Render the MSC3720 response body. An empty request yields `{}`.
fn render(statuses: AccountStatuses) -> Value {
    if statuses.statuses.is_empty() && statuses.failures.is_empty() {
        return json!({});
    }

    let mut account_statuses = Map::new();
    for (user_id, status) in statuses.statuses {
        account_statuses.insert(user_id, serde_json::to_value(status).unwrap_or(Value::Null));
    }

    json!({
        "account_statuses": Value::Object(account_statuses),
        "failures": statuses.failures,
    })
}

/// Map a domain error onto the MSC's error codes.
fn map_error(error: AccountStatusError) -> ApiError {
    match error {
        AccountStatusError::InvalidUserId(_) | AccountStatusError::NotLocalUser(_) => {
            ApiError::invalid_param(error.to_string())
        }
        AccountStatusError::Storage(cause) => ApiError::internal_with_cause("Failed to read account status", cause),
    }
}

/// `POST /_matrix/client/unstable/org.matrix.msc3720/account_status`.
pub(crate) async fn client_account_status(
    State(ctx): State<AuthContext>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_enabled(&ctx.config)?;
    let user_ids = parse_body(&body)?;

    let service =
        AccountStatusService::new(ctx.user_service.store().clone(), ctx.federation_client.clone(), &ctx.config);
    let statuses = service.get_account_statuses(&user_ids, true).await.map_err(map_error)?;

    Ok(Json(render(statuses)))
}

/// `POST /_matrix/federation/unstable/org.matrix.msc3720/account_status`.
pub(crate) async fn federation_account_status(
    State(ctx): State<FederationContext>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_enabled(&ctx.config)?;
    let user_ids = parse_body(&body)?;

    // `allow_remote = false`: the MSC requires a 400 M_INVALID_PARAM for any
    // user that is not local to this homeserver.
    let service = AccountStatusService::local_only(ctx.user_service.store().clone(), &ctx.config);
    let statuses = service.get_account_statuses(&user_ids, false).await.map_err(map_error)?;

    Ok(Json(render(statuses)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_body_rejects_missing_user_ids_with_missing_param() {
        let error = parse_body(&json!({})).expect_err("missing user_ids must be rejected");
        assert_eq!(error.code, synapse_common::MatrixErrorCode::MissingParam);
    }

    #[test]
    fn parse_body_rejects_non_array_and_non_string_entries() {
        let not_array = parse_body(&json!({ "user_ids": "@a:hs" })).expect_err("non-array must be rejected");
        assert_eq!(not_array.code, synapse_common::MatrixErrorCode::InvalidParam);

        let not_string = parse_body(&json!({ "user_ids": [1] })).expect_err("non-string entry must be rejected");
        assert_eq!(not_string.code, synapse_common::MatrixErrorCode::InvalidParam);
    }

    #[test]
    fn parse_body_accepts_strings() {
        let user_ids = parse_body(&json!({ "user_ids": ["@a:hs", "@b:hs"] })).expect("valid body");
        assert_eq!(user_ids, vec!["@a:hs".to_string(), "@b:hs".to_string()]);
    }

    #[test]
    fn render_empty_request_is_an_empty_object() {
        assert_eq!(render(AccountStatuses::default()), json!({}));
    }

    #[test]
    fn render_reports_statuses_and_failures() {
        use synapse_services::account::AccountStatus;

        let mut statuses = AccountStatuses::default();
        statuses.statuses.insert("@a:hs".to_string(), AccountStatus { exists: true, deactivated: Some(false) });
        statuses.failures.push("@b:remote".to_string());

        assert_eq!(
            render(statuses),
            json!({
                "account_statuses": { "@a:hs": { "exists": true, "deactivated": false } },
                "failures": ["@b:remote"],
            })
        );
    }

    #[test]
    fn render_omits_deactivated_when_the_account_does_not_exist() {
        use synapse_services::account::AccountStatus;

        let mut statuses = AccountStatuses::default();
        statuses.statuses.insert("@ghost:hs".to_string(), AccountStatus { exists: false, deactivated: None });

        assert_eq!(
            render(statuses),
            json!({ "account_statuses": { "@ghost:hs": { "exists": false } }, "failures": [] })
        );
    }
}
