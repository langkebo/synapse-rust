//! MSC4512 — request-path proxying to application services, and the
//! application-service-initiated federation proxy endpoint.
//!
//! Two halves, matching the MSC:
//!
//! * **Inbound**: an application service that declares a `proxy_prefix` (see
//!   [`synapse_services::application_service::proxy`]) serves
//!   `/_matrix/{client,federation}/<version>/<proxy_prefix>/...`. Those paths
//!   are dynamic — the prefix is chosen by the registration file, not the
//!   router — so they cannot be registered as axum routes. They are handled by
//!   [`msc4512_proxy_fallback`], which is installed as the global fallback in
//!   [`crate::routes::assembly`]. Every unauthenticated request that reaches
//!   the fallback still gets `M_UNRECOGNIZED`; only requests that resolve to a
//!   registered prefix are forwarded, and only after authentication.
//!
//! * **Outbound**: [`fed_proxy`] is a normal, registered client-server route
//!   (`POST /_matrix/client/v1/appservice/fed_proxy`). An application service
//!   uses it to have the homeserver sign and send a Server-Server request to a
//!   remote destination on its behalf.
//!
//! Security posture follows the MSC:
//!
//! * proxied requests MUST be authenticated — there is no unauthenticated
//!   passthrough;
//! * the caller's `Authorization` header is replaced with the application
//!   service's `hs_token`, and the verified identity is passed through a
//!   dedicated header instead (`X-Matrix-User-Identifier` for client-server,
//!   `X-Matrix-Origin` for server-server);
//! * hop-by-hop headers (RFC 9110 §7.6.1) are stripped in both directions;
//! * outbound `fed_proxy` requests are confined to the calling service's own
//!   `proxy_prefix`.

use axum::body::{Body, Bytes};
use axum::extract::{FromRef, State};
use axum::http::header;
use axum::http::request::Parts;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Map, Value};
use synapse_common::ApiError;
use synapse_services::application_service::proxy::{
    prefix_matches, proxy_config_from_value, split_scheme_and_remainder, AppServiceProxyTarget, ProxyScheme,
};
use tracing::warn;

use crate::middleware::federation_auth::{authenticate_federation_request, is_local_federation_destination};
use crate::routes::context::FederationContext;
use crate::routes::{AppState, AuthenticatedUser};

/// The response body shared with the plain fallback in
/// [`crate::routes::assembly`]: an unmatched path is `M_UNRECOGNIZED`.
pub(crate) const UNRECOGNIZED_BODY: &[u8] = b"{\"errcode\":\"M_UNRECOGNIZED\",\"error\":\"Unrecognized request\"}";

/// Header carrying the verified client-server user id to the application
/// service (MSC4512).
const USER_IDENTIFIER_HEADER: &str = "x-matrix-user-identifier";
/// Header carrying the verified server-server origin to the application
/// service (MSC4512).
const ORIGIN_HEADER: &str = "x-matrix-origin";

/// The `M_UNRECOGNIZED` 404 used for every path that is not a live proxied
/// request — including a disabled proxy feature, an unclaimed prefix, and a
/// federation request when federation ingress is off.
fn unrecognized() -> Response {
    // Pre-serialised to avoid a per-request allocation on the fallback path.
    (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "application/json")], Bytes::from_static(UNRECOGNIZED_BODY))
        .into_response()
}

/// The global router fallback: dispatch a request to its application service
/// when it falls under a registered `proxy_prefix`, otherwise 404.
pub async fn msc4512_proxy_fallback(State(state): State<AppState>, request: Request<Body>) -> Response {
    let config = state.services.core.config.clone();
    if !config.experimental.msc4512_enabled {
        return unrecognized();
    }

    let (parts, body) = request.into_parts();

    let Some((scheme, remainder)) = split_scheme_and_remainder(parts.uri.path()) else {
        return unrecognized();
    };

    let target = match state.services.admin.modules.app_service_manager.resolve_proxy_target(remainder).await {
        Ok(Some(target)) => target,
        Ok(None) => return unrecognized(),
        Err(error) => return error.into_response(),
    };

    // Mirror `federation_auth_middleware`: with federation ingress off there is
    // no server-server surface at all, so the proxied one must be absent too.
    if scheme == ProxyScheme::Federation && (!config.federation.enabled || !config.federation.allow_ingress) {
        return unrecognized();
    }

    let body_limit = config.federation.max_transaction_payload.max(64 * 1024) as usize;
    let body_bytes = match axum::body::to_bytes(body, body_limit).await {
        Ok(bytes) => bytes,
        Err(_) => return ApiError::bad_request("Invalid request body").into_response(),
    };

    let (injected_header, injected_value) = match scheme {
        ProxyScheme::Client => {
            let uri = parts.uri.to_string();
            let token = match crate::utils::auth::extract_token(&parts.headers, &uri) {
                Ok(token) => token,
                Err(error) => return error.into_response(),
            };
            match state.services.core.token_auth.validate_token(&token).await {
                Ok((user_id, ..)) => (USER_IDENTIFIER_HEADER, user_id),
                Err(error) => return error.into_response(),
            }
        }
        ProxyScheme::Federation => {
            let ctx = <FederationContext as FromRef<AppState>>::from_ref(&state);
            match authenticate_federation_request(&ctx, &parts, &body_bytes).await {
                Ok(auth) => (ORIGIN_HEADER, auth.origin),
                Err(response) => return response,
            }
        }
    };

    forward_proxied_request(&target, &parts, body_bytes, injected_header, injected_value).await
}

/// Forward an authenticated request to `target.proxy_url`, relaying the
/// application service's response back to the original caller.
async fn forward_proxied_request(
    target: &AppServiceProxyTarget,
    parts: &Parts,
    body_bytes: Bytes,
    injected_header: &'static str,
    injected_value: String,
) -> Response {
    // The MSC forwards to "the same path anchored on `proxy_url`". Our own
    // access token is not the application service's business, so it is dropped
    // from the forwarded query string.
    let path_and_query = match parts.uri.path_and_query() {
        Some(path_and_query) => strip_access_token(path_and_query.as_str()),
        None => parts.uri.path().to_owned(),
    };
    let target_url = format!("{}{}", target.proxy_url.trim_end_matches('/'), path_and_query);

    let mut builder = synapse_common::http_client::default_client().request(parts.method.clone(), &target_url);

    for (name, value) in parts.headers.iter() {
        let name_str = name.as_str();
        if crate::routes::app_service::is_hop_by_hop_header(name_str)
            || name_str.eq_ignore_ascii_case("authorization")
            || name_str.eq_ignore_ascii_case(injected_header)
        {
            continue;
        }
        builder = builder.header(name, value);
    }

    builder = builder.header(header::AUTHORIZATION, format!("Bearer {}", target.hs_token));
    if let Ok(value) = HeaderValue::from_str(&injected_value) {
        builder = builder.header(injected_header, value);
    }
    if !body_bytes.is_empty() {
        builder = builder.body(body_bytes);
    }

    let response = match builder.send().await {
        Ok(response) => response,
        Err(error) => {
            return ApiError::internal_with_cause(
                &format!("Failed to forward request to application service {}", target.as_id),
                error,
            )
            .into_response();
        }
    };

    let status = response.status();
    let response_headers = response.headers().clone();
    // `bytes()` rather than `text()`: the proxied surface is generic and may
    // carry non-UTF-8 payloads.
    let response_body = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            return ApiError::internal_with_cause("Failed to read application service response body", error)
                .into_response();
        }
    };

    let mut response_builder = Response::builder().status(status);
    for (name, value) in response_headers.iter() {
        if !crate::routes::app_service::is_hop_by_hop_header(name.as_str()) {
            response_builder = response_builder.header(name, value);
        }
    }

    match response_builder.body(Body::from(response_body)) {
        Ok(response) => response,
        Err(error) => ApiError::internal_with_cause("Failed to build proxy response", error).into_response(),
    }
}

/// Drop `access_token` from a `path?query` string, leaving everything else
/// untouched.
fn strip_access_token(target: &str) -> String {
    let Some((path, query)) = target.split_once('?') else {
        return target.to_owned();
    };
    let filtered: Vec<&str> = query
        .split('&')
        .filter(|param| match param.split_once('=') {
            Some((key, _)) => key != "access_token",
            None => *param != "access_token",
        })
        .collect();
    if filtered.is_empty() {
        path.to_owned()
    } else {
        format!("{}?{}", path, filtered.join("&"))
    }
}

/// MSC4512: have the homeserver sign and send a federation request on behalf
/// of an application service.
pub async fn fed_proxy(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(payload): Json<Value>,
) -> Result<Response, ApiError> {
    let Some(as_id) = auth.appservice_id.as_deref() else {
        return Err(ApiError::forbidden("This endpoint is only available to application services"));
    };

    let object = payload.as_object().ok_or_else(|| ApiError::invalid_param("Request body must be a JSON object"))?;

    let destination = required_string(object, "destination")?;
    let method = required_string(object, "method")?;
    let path = required_string(object, "path")?;

    if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "DELETE") {
        return Err(ApiError::invalid_param("`method` must be one of GET, POST, PUT or DELETE"));
    }
    if matches!(method.as_str(), "GET" | "DELETE") && object.get("body").is_some_and(|value| !value.is_null()) {
        return Err(ApiError::invalid_param("`body` must not be specified for GET or DELETE requests"));
    }

    let body = match object.get("body") {
        None | Some(Value::Null) => None,
        Some(value @ Value::Object(_)) => Some(
            serde_json::to_string(value)
                .map_err(|error| ApiError::invalid_param(format!("`body` could not be serialised: {}", error)))?,
        ),
        Some(_) => return Err(ApiError::invalid_param("`body` must be a JSON object")),
    };

    let query = match object.get("query") {
        None | Some(Value::Null) => None,
        Some(Value::Object(map)) => {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            for (key, value) in map {
                let Some(value) = value.as_str() else {
                    return Err(ApiError::invalid_param("`query` values must all be strings"));
                };
                serializer.append_pair(key, value);
            }
            Some(serializer.finish())
        }
        Some(_) => return Err(ApiError::invalid_param("`query` must be a JSON object")),
    };

    let federation_ctx = <FederationContext as FromRef<AppState>>::from_ref(&state);
    if is_local_federation_destination(&federation_ctx, &destination) {
        return Err(ApiError::fed_proxy_destination_denied("`destination` is this homeserver"));
    }

    if path.split('/').any(|segment| segment == "." || segment == "..") {
        return Err(ApiError::fed_proxy_path_not_allowed("`path` must not contain `.` or `..` segments"));
    }
    let Some((ProxyScheme::Federation, remainder)) = split_scheme_and_remainder(&path) else {
        return Err(ApiError::fed_proxy_path_not_allowed("`path` must be under /_matrix/federation/"));
    };

    let service = state.services.admin.modules.app_service_manager.get(as_id).await?;
    let Some(service) = service else {
        return Err(ApiError::forbidden("Application service not found"));
    };
    if !service.is_enabled {
        return Err(ApiError::forbidden("Application service is disabled"));
    }
    let Some(proxy_config) = proxy_config_from_value(&service.config)? else {
        return Err(ApiError::fed_proxy_path_not_allowed("Application service does not declare a proxy_prefix"));
    };
    if !prefix_matches(&proxy_config.proxy_prefix, remainder) {
        return Err(ApiError::fed_proxy_path_not_allowed(format!(
            "`path` is not under the application service's proxy_prefix '{}'",
            proxy_config.proxy_prefix
        )));
    }

    let request_path = match query {
        Some(query) if !query.is_empty() => format!("{}?{}", path, query),
        _ => path,
    };

    match state
        .services
        .federation
        .federation_client
        .proxy_federation_request(&destination, &method, &request_path, body.as_deref())
        .await
    {
        Ok((status, content)) => Ok(Json(json!({"status": status, "content": content})).into_response()),
        Err(error) => {
            warn!(destination = %destination, %error, "Application service fed_proxy request failed");
            Err(ApiError::fed_proxy_connection_failed(format!("Could not reach destination server '{}'", destination)))
        }
    }
}

/// Read a required, non-empty string field, distinguishing an absent field
/// (`M_MISSING_PARAM`) from a wrong-typed one (`M_INVALID_PARAM`).
fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, ApiError> {
    match object.get(key) {
        None => Err(ApiError::missing_param(format!("Missing required parameter: {}", key))),
        Some(Value::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(Value::String(_)) => Err(ApiError::invalid_param(format!("`{}` must not be empty", key))),
        Some(_) => Err(ApiError::invalid_param(format!("`{}` must be a string", key))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_access_token_from_query() {
        assert_eq!(strip_access_token("/p?a=1&access_token=t&b=2"), "/p?a=1&b=2");
    }

    #[test]
    fn drops_query_when_only_access_token_remains() {
        assert_eq!(strip_access_token("/p?access_token=t"), "/p");
    }

    #[test]
    fn leaves_query_without_access_token_untouched() {
        assert_eq!(strip_access_token("/p?a=1&b=2"), "/p?a=1&b=2");
        assert_eq!(strip_access_token("/p"), "/p");
    }

    #[test]
    fn required_string_distinguishes_missing_from_invalid() {
        let mut object = Map::new();
        object.insert("present".to_string(), json!("value"));
        object.insert("empty".to_string(), json!(""));
        object.insert("wrong".to_string(), json!(7));

        assert_eq!(required_string(&object, "present").unwrap(), "value");
        assert!(required_string(&object, "absent").unwrap_err().message().contains("Missing required parameter"));
        assert!(required_string(&object, "empty").unwrap_err().message().contains("must not be empty"));
        assert!(required_string(&object, "wrong").unwrap_err().message().contains("must be a string"));
    }
}
