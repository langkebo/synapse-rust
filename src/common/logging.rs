use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "TRACE"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogContext {
    pub request_id: Option<String>,
    pub user_id: Option<String>,
    pub room_id: Option<String>,
    pub device_id: Option<String>,
    pub session_id: Option<String>,
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMetadata {
    pub timestamp: i64,
    pub level: LogLevel,
    pub target: String,
    pub context: LogContext,
    pub module: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub metadata: LogMetadata,
    pub message: String,
    pub error: Option<String>,
    pub duration_ms: Option<u64>,
    pub extra: Option<serde_json::Value>,
}

impl LogEntry {
    pub fn new(level: LogLevel, target: &str, message: &str) -> Self {
        Self {
            metadata: LogMetadata {
                timestamp: chrono::Utc::now().timestamp_millis(),
                level,
                target: target.to_string(),
                context: LogContext {
                    request_id: None,
                    user_id: None,
                    room_id: None,
                    device_id: None,
                    session_id: None,
                    trace_id: None,
                },
                module: None,
                file: None,
                line: None,
            },
            message: message.to_string(),
            error: None,
            duration_ms: None,
            extra: None,
        }
    }

    pub fn with_context(mut self, context: LogContext) -> Self {
        self.metadata.context = context;
        self
    }

    pub fn with_request_id(mut self, request_id: &str) -> Self {
        self.metadata.context.request_id = Some(request_id.to_string());
        self
    }

    pub fn with_user_id(mut self, user_id: &str) -> Self {
        self.metadata.context.user_id = Some(user_id.to_string());
        self
    }

    pub fn with_room_id(mut self, room_id: &str) -> Self {
        self.metadata.context.room_id = Some(room_id.to_string());
        self
    }

    pub fn with_device_id(mut self, device_id: &str) -> Self {
        self.metadata.context.device_id = Some(device_id.to_string());
        self
    }

    pub fn with_session_id(mut self, session_id: &str) -> Self {
        self.metadata.context.session_id = Some(session_id.to_string());
        self
    }

    pub fn with_trace_id(mut self, trace_id: &str) -> Self {
        self.metadata.context.trace_id = Some(trace_id.to_string());
        self
    }

    pub fn with_module(mut self, module: &str) -> Self {
        self.metadata.module = Some(module.to_string());
        self
    }

    pub fn with_location(mut self, file: &str, line: u32) -> Self {
        self.metadata.file = Some(file.to_string());
        self.metadata.line = Some(line);
        self
    }

    pub fn with_error(mut self, error: &str) -> Self {
        self.error = Some(error.to_string());
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn with_extra(mut self, extra: serde_json::Value) -> Self {
        self.extra = Some(extra);
        self
    }
}

impl fmt::Display for LogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp(
            self.metadata.timestamp / 1000,
            (self.metadata.timestamp % 1000) as u32,
        )
        .unwrap_or_default()
        .format("%Y-%m-%d %H:%M:%S%.3f");

        write!(
            f,
            "{} [{}] {} - {}",
            timestamp, self.metadata.level, self.metadata.target, self.message
        )?;

        if let Some(request_id) = &self.metadata.context.request_id {
            write!(f, " [request_id={}]", request_id)?;
        }

        if let Some(user_id) = &self.metadata.context.user_id {
            write!(f, " [user_id={}]", user_id)?;
        }

        if let Some(room_id) = &self.metadata.context.room_id {
            write!(f, " [room_id={}]", room_id)?;
        }

        if let Some(duration) = self.duration_ms {
            write!(f, " [duration={}ms]", duration)?;
        }

        if let Some(error) = &self.error {
            write!(f, " [error={}]", error)?;
        }

        Ok(())
    }
}

pub fn log_trace(target: &str, message: &str) -> LogEntry {
    LogEntry::new(LogLevel::Trace, target, message)
}

pub fn log_debug(target: &str, message: &str) -> LogEntry {
    LogEntry::new(LogLevel::Debug, target, message)
}

pub fn log_info(target: &str, message: &str) -> LogEntry {
    LogEntry::new(LogLevel::Info, target, message)
}

pub fn log_warn(target: &str, message: &str) -> LogEntry {
    LogEntry::new(LogLevel::Warn, target, message)
}

pub fn log_error(target: &str, message: &str) -> LogEntry {
    LogEntry::new(LogLevel::Error, target, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_display() {
        assert_eq!(format!("{}", LogLevel::Trace), "TRACE");
        assert_eq!(format!("{}", LogLevel::Debug), "DEBUG");
        assert_eq!(format!("{}", LogLevel::Info), "INFO");
        assert_eq!(format!("{}", LogLevel::Warn), "WARN");
        assert_eq!(format!("{}", LogLevel::Error), "ERROR");
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry::new(LogLevel::Info, "test_module", "Test message");
        assert_eq!(entry.metadata.level, LogLevel::Info);
        assert_eq!(entry.metadata.target, "test_module");
        assert_eq!(entry.message, "Test message");
    }

    #[test]
    fn test_log_entry_with_context() {
        let entry = LogEntry::new(LogLevel::Info, "test_module", "Test message")
            .with_request_id("req-123")
            .with_user_id("@user:server")
            .with_room_id("!room:server");

        assert_eq!(
            entry.metadata.context.request_id,
            Some("req-123".to_string())
        );
        assert_eq!(
            entry.metadata.context.user_id,
            Some("@user:server".to_string())
        );
        assert_eq!(
            entry.metadata.context.room_id,
            Some("!room:server".to_string())
        );
    }

    #[test]
    fn test_log_entry_with_duration() {
        let entry = LogEntry::new(LogLevel::Info, "test_module", "Test message").with_duration(100);

        assert_eq!(entry.duration_ms, Some(100));
    }

    #[test]
    fn test_log_entry_with_error() {
        let entry =
            LogEntry::new(LogLevel::Error, "test_module", "Test message").with_error("Test error");

        assert_eq!(entry.error, Some("Test error".to_string()));
    }

    #[test]
    fn test_log_entry_display() {
        let entry = LogEntry::new(LogLevel::Info, "test_module", "Test message")
            .with_request_id("req-123")
            .with_user_id("@user:server")
            .with_duration(50);

        let display = format!("{}", entry);
        assert!(display.contains("INFO"));
        assert!(display.contains("test_module"));
        assert!(display.contains("Test message"));
        assert!(display.contains("req-123"));
        assert!(display.contains("@user:server"));
        assert!(display.contains("50ms"));
    }
}
