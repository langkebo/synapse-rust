use crate::common::ApiError;
use crate::services::cas_service::CasValidationResponse;
use crate::storage::cas::{CasService as CasServiceModel, RegisterServiceRequest};
use crate::web::routes::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ServiceTicketQuery {
    service: String,
    renew: Option<bool>,
    gateway: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ValidateQuery {
    service: String,
    ticket: String,
    pgt_url: Option<String>,
    renew: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ProxyQuery {
    target_service: String,
    pgt: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ProxyValidateQuery {
    service: String,
    ticket: String,
    pgt_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct P3ServiceValidateQuery {
    service: String,
    ticket: String,
    pgt_url: Option<String>,
    renew: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RegisterServiceBody {
    service_id: String,
    name: String,
    description: Option<String>,
    service_url_pattern: String,
    allowed_attributes: Option<Vec<String>>,
    allowed_proxy_callbacks: Option<Vec<String>>,
    require_secure: Option<bool>,
    single_logout: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SetAttributeBody {
    attribute_name: String,
    attribute_value: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct ServiceTicketResponse {
    ticket: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct ProxyTicketResponse {
    ticket: String,
}

#[derive(Debug, Serialize)]
struct ServiceResponse {
    service_id: String,
    name: String,
    description: Option<String>,
    service_url_pattern: String,
    is_enabled: bool,
}

impl From<CasServiceModel> for ServiceResponse {
    fn from(s: CasServiceModel) -> Self {
        Self {
            service_id: s.service_id,
            name: s.name,
            description: s.description,
            service_url_pattern: s.service_url_pattern,
            is_enabled: s.is_enabled,
        }
    }
}

pub fn cas_routes() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_redirect))
        .route("/serviceValidate", get(service_validate))
        .route("/proxyValidate", get(proxy_validate))
        .route("/proxy", get(proxy))
        .route("/p3/serviceValidate", get(p3_service_validate))
        .route("/logout", get(logout))
        .route("/admin/services", post(register_service))
        .route("/admin/services", get(list_services))
        .route("/admin/services/{service_id}", delete(delete_service))
        .route(
            "/admin/users/{user_id}/attributes",
            post(set_user_attribute),
        )
        .route(
            "/admin/users/{user_id}/attributes",
            get(get_user_attributes),
        )
}

async fn login_redirect(
    State(state): State<AppState>,
    Query(query): Query<ServiceTicketQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let _service = state
        .services
        .cas_service
        .get_service_by_url(&query.service)
        .await?;

    let encoded_service: String =
        form_urlencoded::byte_serialize(query.service.as_bytes()).collect();

    Ok((
        StatusCode::FOUND,
        [(
            header::LOCATION,
            format!("/cas/login?service={}", encoded_service),
        )],
        Json(serde_json::json!({
            "redirect_url": format!("/cas/login?service={}", encoded_service)
        })),
    ))
}

async fn service_validate(
    State(state): State<AppState>,
    Query(query): Query<ValidateQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let result = state
        .services
        .cas_service
        .validate_service_ticket(&query.ticket, &query.service)
        .await?;

    match result {
        Some(ticket) => {
            let response = format!("yes\n{}\n", ticket.user_id);
            Ok(([(header::CONTENT_TYPE, "text/plain")], response))
        }
        None => Ok(([(header::CONTENT_TYPE, "text/plain")], "no\n\n".to_string())),
    }
}

async fn proxy_validate(
    State(state): State<AppState>,
    Query(query): Query<ProxyValidateQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let result = state
        .services
        .cas_service
        .validate_proxy_ticket(&query.ticket, &query.service)
        .await?;

    match result {
        Some(ticket) => {
            let response = CasValidationResponse::Success {
                user: ticket.user_id,
                attributes: std::collections::HashMap::new(),
                proxy_granting_ticket: None,
            };
            Ok((
                [(header::CONTENT_TYPE, "application/xml")],
                response.to_xml(),
            ))
        }
        None => {
            let response = CasValidationResponse::Failure {
                code: "INVALID_TICKET".to_string(),
                description: "Proxy ticket not found or invalid".to_string(),
            };
            Ok((
                [(header::CONTENT_TYPE, "application/xml")],
                response.to_xml(),
            ))
        }
    }
}

async fn proxy(
    State(state): State<AppState>,
    Query(query): Query<ProxyQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let ticket = state
        .services
        .cas_service
        .create_proxy_ticket(&query.pgt, &query.target_service)
        .await?;

    let response = CasValidationResponse::Success {
        user: ticket.user_id.clone(),
        attributes: std::collections::HashMap::new(),
        proxy_granting_ticket: Some(ticket.proxy_ticket_id.clone()),
    };

    Ok((
        [(header::CONTENT_TYPE, "application/xml")],
        response.to_xml(),
    ))
}

async fn p3_service_validate(
    State(state): State<AppState>,
    Query(query): Query<P3ServiceValidateQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let response = state
        .services
        .cas_service
        .validate_service_ticket_v3(
            &query.ticket,
            &query.service,
            query.pgt_url.as_deref(),
            query.renew.unwrap_or(false),
        )
        .await?;

    Ok((
        [(header::CONTENT_TYPE, "application/xml")],
        response.to_xml(),
    ))
}

async fn logout(
    State(_state): State<AppState>,
    Query(query): Query<ServiceTicketQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let _ = query;
    Ok((
        [(header::CONTENT_TYPE, "text/html")],
        "<html><body><h1>Logged out successfully</h1></body></html>",
    ))
}

async fn register_service(
    State(state): State<AppState>,
    Json(body): Json<RegisterServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = RegisterServiceRequest {
        service_id: body.service_id,
        name: body.name,
        description: body.description,
        service_url_pattern: body.service_url_pattern,
        allowed_attributes: body.allowed_attributes,
        allowed_proxy_callbacks: body.allowed_proxy_callbacks,
        require_secure: body.require_secure,
        single_logout: body.single_logout,
    };

    let service = state.services.cas_service.register_service(request).await?;

    Ok((StatusCode::CREATED, Json(ServiceResponse::from(service))))
}

async fn list_services(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let services = state.services.cas_service.list_services().await?;
    let response: Vec<ServiceResponse> = services.into_iter().map(ServiceResponse::from).collect();
    Ok(Json(response))
}

async fn delete_service(
    State(state): State<AppState>,
    Path(service_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let deleted = state
        .services
        .cas_service
        .delete_service(&service_id)
        .await?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("Service not found"))
    }
}

async fn set_user_attribute(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<SetAttributeBody>,
) -> Result<impl IntoResponse, ApiError> {
    let attr = state
        .services
        .cas_service
        .set_user_attribute(&user_id, &body.attribute_name, &body.attribute_value)
        .await?;

    Ok(Json(serde_json::json!({
        "user_id": attr.user_id,
        "attribute_name": attr.attribute_name,
        "attribute_value": attr.attribute_value,
    })))
}

async fn get_user_attributes(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let attrs = state
        .services
        .cas_service
        .get_user_attributes(&user_id)
        .await?;

    let response: Vec<serde_json::Value> = attrs
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "name": a.attribute_name,
                "value": a.attribute_value,
            })
        })
        .collect();

    Ok(Json(response))
}
