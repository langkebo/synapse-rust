use super::{
    cors_origins_regex, get_allowed_origins, is_dev_mode, is_localhost_bind, is_origin_allowed, same_origin,
    set_config_allowed_origins_once,
};
use axum::body::Body;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::response::Response;
use tracing::info;

#[derive(Debug, Clone)]
pub struct CorsSecurityReport {
    pub is_development_mode: bool,
    pub is_localhost_bind: bool,
    pub allows_any_origin: bool,
    pub allowed_origins: Vec<String>,
    pub has_pattern: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl CorsSecurityReport {
    pub fn has_issues(&self) -> bool {
        !self.errors.is_empty() || !self.warnings.is_empty()
    }
}

pub fn set_config_allowed_origins(origins: Vec<String>) {
    set_config_allowed_origins_once(origins);
}

pub fn check_cors_security() -> CorsSecurityReport {
    let is_dev = is_dev_mode();
    let allowed_origins = get_allowed_origins();
    let allows_any_origin = allowed_origins.iter().any(|o| o == "*");
    let has_pattern = cors_origins_regex().is_some();

    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    if is_dev {
        if is_localhost_bind() {
            warnings
                .push("⚠️  DEVELOPMENT MODE ENABLED - CORS allows all origins. DO NOT use in production!".to_string());
        } else {
            warnings.push(
                "⚠️  DEVELOPMENT MODE on non-localhost address - permissive CORS is DISABLED. \
                 Only same-origin requests will be allowed. \
                 Bind to 127.0.0.1/localhost to enable permissive CORS in development."
                    .to_string(),
            );
        }
    }

    if !is_dev && allows_any_origin {
        errors.push(
            "🚨 SECURITY ERROR: Production environment cannot use '*' as CORS origin. \
             Please configure ALLOWED_ORIGINS environment variable with specific domains."
                .to_string(),
        );
    }

    if !is_dev && allowed_origins.is_empty() && !has_pattern {
        errors.push(
            "🚨 SECURITY ERROR: No CORS origins configured in production. \
             Configure `cors.allowed_origins` in homeserver.yaml, \
             or set the ALLOWED_ORIGINS / CORS_ORIGIN_PATTERN environment variable."
                .to_string(),
        );
    }

    if !is_dev && allows_any_origin {
        warnings.push(
            "⚠️  CORS wildcard origin detected in production configuration. \
             This is a security risk and may expose your server to CSRF attacks."
                .to_string(),
        );
    }

    CorsSecurityReport {
        is_development_mode: is_dev,
        is_localhost_bind: is_localhost_bind(),
        allows_any_origin,
        allowed_origins,
        has_pattern,
        warnings,
        errors,
    }
}

pub fn log_cors_security_report(report: &CorsSecurityReport) {
    let mode = if report.is_development_mode { "DEVELOPMENT" } else { "PRODUCTION" };
    info!("CORS Security Configuration Check: mode={}", mode);

    if report.is_development_mode {
        if report.is_localhost_bind {
            info!("Development mode is ACTIVE — all CORS origins are permitted (NOT SAFE FOR PRODUCTION)");
        } else {
            info!("Non-localhost bind address detected — permissive CORS is DISABLED for non-localhost");
        }
    }

    if report.allows_any_origin {
        info!("CORS Origin: * (ANY ORIGIN)");
    } else if report.has_pattern {
        info!("CORS Origin: Pattern-based matching");
    } else if report.allowed_origins.is_empty() {
        info!("CORS Origin: NOT CONFIGURED");
    } else {
        info!("CORS Origins: {:?}", report.allowed_origins);
    }

    for error in &report.errors {
        tracing::error!("{}", error);
    }
    for warning in &report.warnings {
        tracing::warn!("{}", warning);
    }

    if !report.has_issues() {
        info!("CORS configuration looks secure");
    }
}

pub fn validate_cors_config_for_production() -> Result<(), String> {
    let report = check_cors_security();

    if !report.errors.is_empty() {
        return Err(report.errors.join("; "));
    }

    Ok(())
}

pub fn validate_bind_address_for_dev_mode(host: &str) -> Result<(), String> {
    if !is_dev_mode() {
        return Ok(());
    }

    let local_addresses = ["127.0.0.1", "localhost", "::1", "0.0.0.0", "::", "[::]"];

    let is_local = local_addresses
        .iter()
        .any(|&local| host.eq_ignore_ascii_case(local) || host.starts_with("127.") || host.starts_with("::1"));

    if !is_local {
        return Err(format!(
            "Development mode should only bind to local addresses. \
             Current bind address '{host}' is not local. \
             For development, use '127.0.0.1' or 'localhost'."
        ));
    }

    Ok(())
}

pub async fn cors_middleware(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let origin = request.headers().get("origin").and_then(|v| v.to_str().ok()).map(|s| s.to_string());

    let is_options = request.method() == "OPTIONS";

    let allow_origin = if is_dev_mode() && is_localhost_bind() {
        origin.as_deref().or(Some("*"))
    } else if is_dev_mode() && !is_localhost_bind() {
        if let Some(ref req_origin) = origin {
            if same_origin(req_origin, request.headers()) {
                Some(req_origin.as_str())
            } else {
                tracing::warn!("CORS origin rejected in dev mode (non-localhost bind): {}", req_origin);
                None
            }
        } else {
            None
        }
    } else if let Some(ref req_origin) = origin {
        if is_origin_allowed(req_origin) || same_origin(req_origin, request.headers()) {
            Some(req_origin.as_str())
        } else {
            tracing::debug!("CORS origin rejected: {}", req_origin);
            None
        }
    } else {
        None
    };

    let allow_credentials = allow_origin.is_some() && allow_origin != Some("*");

    if is_options {
        let mut response = Response::new(Body::empty());
        if let Some(allowed) = allow_origin {
            if let Ok(value) = HeaderValue::from_str(allowed) {
                response.headers_mut().insert("Access-Control-Allow-Origin", value);
            }
        }
        response
            .headers_mut()
            .insert("Access-Control-Allow-Methods", HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS, PATCH"));

        let request_headers = request
            .headers()
            .get("Access-Control-Request-Headers")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let allowed_header_set = [
            "content-type",
            "authorization",
            "x-requested-with",
            "x-request-id",
            "x-csrf-token",
            "x-matrix-auth",
            "accept",
            "origin",
            "x-matrix",
            "unstable-prefix",
            "accept-language",
        ];

        let allow_headers_value = if let Some(ref req_headers) = request_headers {
            let filtered: Vec<&str> = req_headers
                .split(',')
                .map(str::trim)
                .filter(|h| allowed_header_set.contains(&h.to_lowercase().as_str()))
                .collect();
            if filtered.is_empty() {
                "Content-Type, Authorization, X-Requested-With, X-Request-ID, X-CSRF-Token, X-Matrix-Auth, Accept, Origin".to_string()
            } else {
                filtered.join(", ")
            }
        } else {
            "Content-Type, Authorization, X-Requested-With, X-Request-ID, X-CSRF-Token, X-Matrix-Auth, Accept, Origin"
                .to_string()
        };
        if let Ok(value) = HeaderValue::from_str(&allow_headers_value) {
            response.headers_mut().insert("Access-Control-Allow-Headers", value);
        }

        response.headers_mut().insert(
            "Access-Control-Expose-Headers",
            HeaderValue::from_static(
                "X-Request-ID, X-CSRF-Token, X-Matrix-Error, X-Ratelimit-Limit, X-Ratelimit-Remaining, X-Ratelimit-Retry-After",
            ),
        );
        if allow_credentials {
            response.headers_mut().insert("Access-Control-Allow-Credentials", HeaderValue::from_static("true"));
        }
        response.headers_mut().insert("Vary", HeaderValue::from_static("Origin"));
        let max_age = std::env::var("CORS_MAX_AGE").ok().and_then(|s| s.parse().ok()).unwrap_or(86400);
        response.headers_mut().insert("Access-Control-Max-Age", HeaderValue::from(max_age));
        *response.status_mut() = StatusCode::NO_CONTENT;
        return response;
    }

    let mut response = next.run(request).await;
    if let Some(allowed) = allow_origin {
        if let Ok(value) = HeaderValue::from_str(allowed) {
            response.headers_mut().insert("Access-Control-Allow-Origin", value);
        }
    }

    response
        .headers_mut()
        .insert("Access-Control-Allow-Methods", HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS, PATCH"));

    response.headers_mut().insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static(
            "Content-Type, Authorization, X-Requested-With, X-Request-ID, X-CSRF-Token, X-Matrix-Auth, Accept, Origin",
        ),
    );

    response.headers_mut().insert(
        "Access-Control-Expose-Headers",
        HeaderValue::from_static(
            "X-Request-ID, X-CSRF-Token, X-Matrix-Error, X-Ratelimit-Limit, X-Ratelimit-Remaining, X-Ratelimit-Retry-After",
        ),
    );

    if allow_credentials {
        response.headers_mut().insert("Access-Control-Allow-Credentials", HeaderValue::from_static("true"));
    }

    response.headers_mut().insert("Vary", HeaderValue::from_static("Origin"));

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_services::test_utils::{env_lock, EnvGuard};

    #[test]
    fn test_cors_security_report_development_mode() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "development");
        env_guard.remove("ALLOWED_ORIGINS");

        let report = check_cors_security();

        assert!(report.is_development_mode);
        assert!(report.has_issues());
        assert!(!report.warnings.is_empty());
    }

    #[test]
    fn test_cors_security_report_production_with_wildcard() {
        std::thread::spawn(|| {
            let _env_lock = env_lock();
            let mut env_guard = EnvGuard::new();
            env_guard.set("RUST_ENV", "production");
            env_guard.set("ALLOWED_ORIGINS", "*");

            let report = check_cors_security();

            assert!(!report.is_development_mode, "Should not be in dev mode");
            assert!(report.allows_any_origin, "Should allow any origin with wildcard");
            assert!(!report.errors.is_empty(), "Should have errors with wildcard in production");

            let validation = validate_cors_config_for_production();
            assert!(validation.is_err(), "Validation should fail with wildcard origin in production: {validation:?}");
        })
        .join()
        .expect("Thread panicked");
    }

    #[test]
    fn test_cors_security_report_production_no_origins() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "production");
        env_guard.remove("ALLOWED_ORIGINS");
        env_guard.remove("CORS_ORIGIN_PATTERN");

        let report = check_cors_security();

        assert!(!report.is_development_mode);
        assert!(report.allowed_origins.is_empty());
        assert!(!report.has_pattern);
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn test_cors_security_report_production_with_specific_origins() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "production");
        env_guard.set("ALLOWED_ORIGINS", "https://example.com,https://app.example.com");

        let report = check_cors_security();

        assert!(!report.is_development_mode);
        assert!(!report.allows_any_origin);
        assert_eq!(report.allowed_origins.len(), 2);
        assert!(report.allowed_origins.contains(&"https://example.com".to_string()));
        assert!(report.allowed_origins.contains(&"https://app.example.com".to_string()));

        let validation = validate_cors_config_for_production();
        assert!(validation.is_ok());
    }

    #[test]
    fn test_validate_bind_address_for_dev_mode_local() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "development");

        assert!(validate_bind_address_for_dev_mode("127.0.0.1").is_ok());
        assert!(validate_bind_address_for_dev_mode("localhost").is_ok());
        assert!(validate_bind_address_for_dev_mode("::1").is_ok());
        assert!(validate_bind_address_for_dev_mode("0.0.0.0").is_ok());
        assert!(validate_bind_address_for_dev_mode("127.0.0.5").is_ok());
    }

    #[test]
    fn test_validate_bind_address_for_dev_mode_non_local() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "development");

        let result = validate_bind_address_for_dev_mode("192.168.1.1");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Development mode should only bind to local addresses"));

        let result = validate_bind_address_for_dev_mode("example.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_bind_address_for_production_mode() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "production");

        assert!(validate_bind_address_for_dev_mode("0.0.0.0").is_ok());
        assert!(validate_bind_address_for_dev_mode("192.168.1.1").is_ok());
        assert!(validate_bind_address_for_dev_mode("example.com").is_ok());
    }

    #[test]
    fn test_cors_security_report_has_issues() {
        let report_with_errors = CorsSecurityReport {
            is_development_mode: false,
            is_localhost_bind: true,
            allows_any_origin: true,
            allowed_origins: vec!["*".to_string()],
            has_pattern: false,
            warnings: vec![],
            errors: vec!["Test error".to_string()],
        };
        assert!(report_with_errors.has_issues());

        let report_with_warnings = CorsSecurityReport {
            is_development_mode: true,
            is_localhost_bind: true,
            allows_any_origin: true,
            allowed_origins: vec![],
            has_pattern: false,
            warnings: vec!["Test warning".to_string()],
            errors: vec![],
        };
        assert!(report_with_warnings.has_issues());

        let report_clean = CorsSecurityReport {
            is_development_mode: false,
            is_localhost_bind: true,
            allows_any_origin: false,
            allowed_origins: vec!["https://example.com".to_string()],
            has_pattern: false,
            warnings: vec![],
            errors: vec![],
        };
        assert!(!report_clean.has_issues());
    }
}
