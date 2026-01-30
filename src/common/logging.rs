use std::time::Duration;
use tracing::{debug, error, info, trace, warn, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub struct LoggingConfig {
    pub level: Level,
    pub format: LogFormat,
    pub file_path: Option<String>,
    pub rotation: LogRotation,
    pub retention_days: u32,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LogFormat {
    Json,
    Pretty,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LogRotation {
    Daily,
    Weekly,
    Never,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: Level::INFO,
            format: LogFormat::Json,
            file_path: None,
            rotation: LogRotation::Daily,
            retention_days: 30,
        }
    }
}

pub fn init_tracing(config: &LoggingConfig) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("synapse_rust={}", config.level)));

    if let Some(file_path) = &config.file_path {
        let file_appender = tracing_appender::rolling::daily(file_path, "synapse-rust.log");

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json().with_writer(file_appender))
            .init();
    } else {
        match config.format {
            LogFormat::Json => {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(
                        fmt::layer()
                            .json()
                            .with_target(false)
                            .with_thread_ids(true)
                            .with_file(true)
                            .with_line_number(true),
                    )
                    .init();
            }
            LogFormat::Pretty => {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(
                        fmt::layer()
                            .pretty()
                            .with_target(false)
                            .with_thread_ids(true)
                            .with_file(true)
                            .with_line_number(true),
                    )
                    .init();
            }
        }
    }
}

pub fn log_request(method: &str, path: &str, status: u16, duration_ms: u64, user_id: Option<&str>) {
    info!(
        method = method,
        path = path,
        status = status,
        duration_ms = duration_ms,
        user_id = user_id,
        "HTTP request"
    );
}

pub fn log_error(
    error: &dyn std::error::Error,
    context: &str,
    user_id: Option<&str>,
    request_id: Option<&str>,
) {
    error!(
        error = %error,
        context = context,
        user_id = user_id,
        request_id = request_id,
        "Error occurred"
    );
}

pub fn log_warning(message: &str, context: &str, user_id: Option<&str>) {
    warn!(
        message = message,
        context = context,
        user_id = user_id,
        "Warning"
    );
}

pub fn log_business_event(event_type: &str, user_id: &str, details: Option<&str>) {
    info!(
        event_type = event_type,
        user_id = user_id,
        details = details,
        "Business event"
    );
}

pub fn log_debug(message: &str, context: &str, data: Option<&str>) {
    debug!(message = message, context = context, data = data, "Debug");
}

pub fn log_trace(message: &str, context: &str, data: Option<&str>) {
    trace!(message = message, context = context, data = data, "Trace");
}

pub fn sanitize_for_logging(data: &str) -> String {
    let sensitive_patterns = vec![
        ("password", "[REDACTED: PASSWORD]"),
        ("token", "[REDACTED: TOKEN]"),
        ("secret", "[REDACTED: SECRET]"),
        ("key", "[REDACTED: KEY]"),
        ("authorization", "[REDACTED: AUTHORIZATION]"),
        ("cookie", "[REDACTED: COOKIE]"),
    ];

    let mut sanitized = data.to_string();
    for (pattern, replacement) in &sensitive_patterns {
        let pattern_with_quotes = format!(r#""{}":"#, pattern);
        if sanitized.to_lowercase().contains(&pattern_with_quotes) {
            let regex_pattern = format!(r#"(?i)"{}"\s*:\s*"[^"]*""#, pattern);
            if let Ok(re) = regex::Regex::new(&regex_pattern) {
                sanitized = re
                    .replace_all(&sanitized, format!(r#""{}": {}"#, pattern, replacement))
                    .to_string();
            }
        }
    }
    sanitized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, Level::INFO);
        assert_eq!(config.format, LogFormat::Json);
        assert!(config.file_path.is_none());
        assert_eq!(config.retention_days, 30);
    }

    #[test]
    fn test_sanitize_for_logging() {
        let data = r#"{"password": "secret123", "token": "abc123"}"#;
        let sanitized = sanitize_for_logging(data);
        assert!(sanitized.contains("[REDACTED: PASSWORD]"));
        assert!(sanitized.contains("[REDACTED: TOKEN]"));
        assert!(!sanitized.contains("secret123"));
        assert!(!sanitized.contains("abc123"));
    }

    #[test]
    fn test_sanitize_no_sensitive_data() {
        let data = r#"{"username": "john", "email": "john@example.com"}"#;
        let sanitized = sanitize_for_logging(data);
        assert_eq!(sanitized, data);
    }
}
