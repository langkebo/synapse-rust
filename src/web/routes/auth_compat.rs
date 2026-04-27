use crate::common::ApiError;
use crate::web::extractors::{AuthenticatedUser, MatrixJson};
use crate::web::routes::AppState;
use crate::web::utils::admin_auth::enforce_admin_login_mfa;
use axum::{
    extract::{Query, State},
    Json,
};
use serde_json::{json, Value};

pub(crate) async fn register(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let auth = body.get("auth").cloned();
    let auth_type = auth
        .as_ref()
        .and_then(|a| a.get("type"))
        .and_then(|t| t.as_str());

    let username = body.get("username").and_then(|v| v.as_str());
    let password = body.get("password").and_then(|v| v.as_str());

    if username.is_none() || password.is_none() {
        if auth_type == Some("m.login.dummy") || auth_type == Some("m.login.password") {
            return Err(ApiError::bad_request(
                "Username and password required".to_string(),
            ));
        }
        return Ok(Json(json!({
            "flows": [
                { "stages": ["m.login.dummy"] },
                { "stages": ["m.login.password"] }
            ],
            "params": {},
            "session": uuid::Uuid::new_v4().to_string()
        })));
    }

    let username =
        username.ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;
    let password =
        password.ok_or_else(|| ApiError::bad_request("Password required".to_string()))?;

    state
        .services
        .auth_service
        .validator
        .validate_username(username)?;
    state
        .services
        .auth_service
        .validator
        .validate_password(password)?;

    let displayname = body.get("displayname").and_then(|v| v.as_str());

    Ok(Json(
        state
            .services
            .registration_service
            .register_user(username, password, displayname)
            .await?,
    ))
}

pub(crate) async fn check_username_availability(
    State(state): State<AppState>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = params
        .get("username")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;

    if let Err(e) = state
        .services
        .auth_service
        .validator
        .validate_username(username)
    {
        return Err(e.into());
    }

    let user_id = format!("@{}:{}", username, state.services.server_name);
    let exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "available": !exists,
        "username": username
    })))
}

pub(crate) async fn request_email_verification(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    request_email_verification_with_submit_path(
        &state,
        &body,
        "/_matrix/client/r0/register/email/submitToken",
        None,
        "register",
    )
    .await
}

pub(crate) async fn request_email_verification_with_submit_path(
    state: &AppState,
    body: &Value,
    submit_path: &str,
    user_id: Option<&str>,
    purpose: &str,
) -> Result<Json<Value>, ApiError> {
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Email is required".to_string()))?;

    if state
        .services
        .auth_service
        .validator
        .validate_email(email)
        .is_err()
    {
        return Err(ApiError::bad_request(
            "Invalid email address format".to_string(),
        ));
    }

    let client_secret = body
        .get("client_secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("client_secret is required".to_string()))?;

    let _send_attempt = body
        .get("send_attempt")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);

    let token = state
        .services
        .auth_service
        .generate_email_verification_token()
        .map_err(|e| {
            ::tracing::error!("Failed to generate email verification token: {}", e);
            ApiError::internal(
                "Failed to generate verification token. Please try again later.".to_string(),
            )
        })?;

    let session_data = serde_json::json!({
        "client_secret": client_secret,
        "purpose": purpose,
    });

    let token_id = state
        .services
        .email_verification_storage
        .create_verification_token(email, &token, 3600, user_id, Some(session_data))
        .await
        .map_err(|e| {
            ::tracing::error!("Failed to store email verification token: {}", e);
            ApiError::internal(
                "Failed to store verification token. Please try again later.".to_string(),
            )
        })?;

    let sid = format!("{}", token_id);

    let submit_url = format!(
        "https://{}:{}{}",
        state.services.config.server.host, state.services.config.server.port, submit_path
    );

    ::tracing::info!(
        "Email verification token created for {}: sid={}",
        email,
        sid
    );

    Ok(Json(json!({
        "sid": sid,
        "submit_url": submit_url,
        "expires_in": 3600
    })))
}

pub(crate) fn session_client_secret(session_data: Option<&Value>) -> Option<&str> {
    match session_data {
        Some(Value::String(secret)) => Some(secret.as_str()),
        Some(Value::Object(map)) => map.get("client_secret").and_then(|v| v.as_str()),
        _ => None,
    }
}

pub(crate) async fn submit_email_token(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let sid = body
        .get("sid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Session ID (sid) is required".to_string()))?;

    let client_secret = body
        .get("client_secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Client secret is required".to_string()))?;

    let token = body
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Verification token is required".to_string()))?;

    let sid_int: i64 = sid
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid session ID format".to_string()))?;

    let verification_token = state
        .services
        .email_verification_storage
        .get_verification_token_by_id(sid_int)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get verification token: {}", e)))?;

    let verification_token = match verification_token {
        Some(t) => t,
        None => {
            return Err(ApiError::bad_request(
                "Invalid session ID or session not found".to_string(),
            ))
        }
    };

    if verification_token.used {
        return Err(ApiError::bad_request(
            "Verification token has already been used".to_string(),
        ));
    }

    if verification_token.expires_at < chrono::Utc::now() {
        return Err(ApiError::bad_request(
            "Verification token has expired".to_string(),
        ));
    }

    if verification_token.token != token {
        return Err(ApiError::bad_request(
            "Invalid verification token".to_string(),
        ));
    }

    if session_client_secret(verification_token.session_data.as_ref()) != Some(client_secret) {
        return Err(ApiError::bad_request("Client secret mismatch".to_string()));
    }

    state
        .services
        .email_verification_storage
        .mark_token_used(sid_int)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to mark token as used: {}", e)))?;

    Ok(Json(json!({
        "success": true
    })))
}

pub(crate) async fn get_login_flows(State(state): State<AppState>) -> Json<Value> {
    let mut flows = vec![
        json!({"type": "m.login.password"}),
        json!({"type": "m.login.token"}),
    ];

    let mut sso_providers = Vec::new();

    // 检查 SAML SSO
    #[cfg(feature = "saml-sso")]
    {
        sso_providers.push(json!({
            "id": "saml",
            "name": "SAML",
            "brand": "saml"
        }));
    }

    // 检查 OIDC
    if state.services.oidc_service.is_some() {
        sso_providers.push(json!({
            "id": "oidc",
            "name": "OIDC",
            "brand": "oidc"
        }));
    }

    // 检查 CAS
    #[cfg(feature = "cas-sso")]
    {
        sso_providers.push(json!({
            "id": "cas",
            "name": "CAS",
            "brand": "cas"
        }));
        flows.push(json!({"type": "m.login.cas"}));
    }

    // 如果有任何 SSO 提供商，添加 m.login.sso 类型
    if !sso_providers.is_empty() {
        flows.push(json!({
            "type": "m.login.sso",
            "identity_providers": sso_providers
        }));
    }

    // 检查内置 OIDC Provider
    if state.services.builtin_oidc_provider.is_some() {
        flows.push(json!({"type": "m.login.oidc"}));
    }

    Json(json!({ "flows": flows }))
}

pub(crate) async fn get_register_flows() -> Json<Value> {
    Json(json!({
        "flows": [
            {"type": "m.login.dummy"},
            {"type": "m.login.password"}
        ],
        "params": {}
    }))
}

pub(crate) async fn login(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = body
        .get("identifier")
        .and_then(|id| id.get("user"))
        .or_else(|| body.get("user"))
        .or_else(|| body.get("username"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Password required".to_string()))?;

    if username.is_empty() || password.is_empty() {
        return Err(ApiError::bad_request(
            "Username and password are required".to_string(),
        ));
    }

    if username.len() > 255 {
        return Err(ApiError::bad_request("Username too long".to_string()));
    }

    if password.len() > 128 {
        return Err(ApiError::bad_request(
            "Password too long (max 128 characters)".to_string(),
        ));
    }

    let device_id = body.get("device_id").and_then(|v| v.as_str());
    let initial_display_name = body.get("initial_display_name").and_then(|v| v.as_str());
    let mfa_code = body.get("mfa_code").and_then(|v| v.as_str());

    enforce_admin_login_mfa(&state, username, mfa_code).await?;

    let (user, access_token, refresh_token, device_id) = state
        .services
        .auth_service
        .login(username, password, device_id, initial_display_name)
        .await?;

    Ok(Json(json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_in": state.services.auth_service.token_expiry,
        "device_id": device_id,
        "user_id": user.user_id(),
        "well_known": {
            "m.homeserver": {
                "base_url": format!(
                    "http://{}:{}",
                    state.services.config.server.host,
                    state.services.config.server.port
                )
            }
        }
    })))
}

pub(crate) async fn logout(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .logout(&auth_user.access_token, auth_user.device_id.as_deref())
        .await?;

    Ok(Json(json!({})))
}

pub(crate) async fn logout_all(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .logout_all(&auth_user.user_id)
        .await?;

    Ok(Json(json!({})))
}

pub(crate) async fn refresh_token(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let refresh_token = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Refresh token required".to_string()))?;

    let (new_access, new_refresh, device_id) = state
        .services
        .auth_service
        .refresh_token(refresh_token)
        .await?;

    Ok(Json(json!({
        "access_token": new_access,
        "refresh_token": new_refresh,
        "expires_in": state.services.auth_service.token_expiry,
        "device_id": device_id
    })))
}

pub(crate) async fn login_fallback_page(
    State(state): State<AppState>,
) -> Result<axum::response::Html<String>, ApiError> {
    let flows = get_login_flows(State(state)).await;
    let empty_vec = vec![];
    let flows_data = flows
        .0
        .get("flows")
        .and_then(|f| f.as_array())
        .unwrap_or(&empty_vec);

    let mut flows_html = String::new();

    for flow in flows_data {
        let flow_type = flow.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match flow_type {
            "m.login.password" => {
                flows_html.push_str(
                    r#"
                <div class="flow">
                    <h3>Password Login</h3>
                    <form method="POST" action="/_matrix/client/v3/login">
                        <input type="hidden" name="type" value="m.login.password">
                        <div>
                            <label>Username:</label>
                            <input type="text" name="identifier[user]" required>
                        </div>
                        <div>
                            <label>Password:</label>
                            <input type="password" name="password" required>
                        </div>
                        <button type="submit">Login</button>
                    </form>
                </div>
                "#,
                );
            }
            "m.login.sso" => {
                if let Some(providers) = flow.get("identity_providers").and_then(|p| p.as_array()) {
                    flows_html.push_str("<div class=\"flow\"><h3>SSO Login</h3>");
                    for provider in providers {
                        let id = provider.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let name = provider.get("name").and_then(|n| n.as_str()).unwrap_or(id);
                        flows_html.push_str(&format!(
                            r#"<a href="/_matrix/client/v3/login/sso/redirect?redirectUrl=/">Login with {}</a><br>"#,
                            name
                        ));
                    }
                    flows_html.push_str("</div>");
                }
            }
            "m.login.cas" => {
                flows_html.push_str(
                    r#"
                <div class="flow">
                    <h3>CAS Login</h3>
                    <a href="/cas/login?service=/">Login with CAS</a>
                </div>
                "#,
                );
            }
            _ => {}
        }
    }

    let html = format!(
        r#"<!doctype html>
<html>
<head>
    <meta charset="utf-8">
    <title>Login - Matrix</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            max-width: 600px;
            margin: 50px auto;
            padding: 20px;
        }}
        h1 {{
            color: #333;
        }}
        .flow {{
            margin: 20px 0;
            padding: 20px;
            border: 1px solid #ddd;
            border-radius: 8px;
        }}
        .flow h3 {{
            margin-top: 0;
        }}
        form div {{
            margin: 10px 0;
        }}
        label {{
            display: inline-block;
            width: 100px;
        }}
        input[type="text"], input[type="password"] {{
            padding: 8px;
            width: 300px;
            border: 1px solid #ddd;
            border-radius: 4px;
        }}
        button, a {{
            display: inline-block;
            padding: 10px 20px;
            background: #0066cc;
            color: white;
            text-decoration: none;
            border: none;
            border-radius: 4px;
            cursor: pointer;
        }}
        button:hover, a:hover {{
            background: #0052a3;
        }}
    </style>
</head>
<body>
    <h1>Login to Matrix</h1>
    {}
</body>
</html>"#,
        flows_html
    );

    Ok(axum::response::Html(html))
}
