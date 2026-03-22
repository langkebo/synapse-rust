//! 日志优化模块
//!
//! 提供结构化日志、日志级别过滤、日志指标等优化功能

use std::sync::atomic::{AtomicU64, Ordering};
use tracing::Level;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

/// 日志配置
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// 日志级别
    pub level: Level,
    /// 是否启用结构化日志 (JSON)
    pub structured: bool,
    /// 日志文件路径 (可选)
    pub file_path: Option<String>,
    /// 是否启用日志轮转
    pub rotation: bool,
    /// 最大日志文件数
    pub max_files: usize,
    /// 单个日志文件最大大小 (字节)
    pub max_file_size: usize,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: Level::INFO,
            structured: false,
            file_path: None,
            rotation: true,
            max_files: 5,
            max_file_size: 100 * 1024 * 1024, // 100MB
        }
    }
}

/// 日志级别过滤器
#[derive(Debug, Clone)]
pub struct LogLevelFilter {
    /// 全局默认级别
    default_level: Level,
    /// 模块级别覆盖
    module_levels: std::collections::HashMap<String, Level>,
}

impl LogLevelFilter {
    pub fn new(default_level: Level) -> Self {
        Self {
            default_level,
            module_levels: std::collections::HashMap::new(),
        }
    }

    /// 设置模块级别
    pub fn set_module_level(&mut self, module: impl Into<String>, level: Level) {
        self.module_levels.insert(module.into(), level);
    }

    /// 获取模块级别
    pub fn get_level(&self, module: &str) -> Level {
        self.module_levels
            .get(module)
            .copied()
            .unwrap_or(self.default_level)
    }

    /// 检查是否应该记录
    pub fn should_log(&self, module: &str, level: Level) -> bool {
        level <= self.get_level(module)
    }
}

/// 结构化日志字段
#[derive(Debug, Clone, serde::Serialize)]
pub struct StructuredLogFields {
    pub timestamp: i64,
    pub level: &'static str,
    pub target: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// 日志指标收集器
#[derive(Debug, Default)]
pub struct LogMetrics {
    pub total_logs: AtomicU64,
    pub error_count: AtomicU64,
    pub warn_count: AtomicU64,
    pub info_count: AtomicU64,
    pub debug_count: AtomicU64,
    pub trace_count: AtomicU64,
}

impl LogMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, level: Level) {
        self.total_logs.fetch_add(1, Ordering::Relaxed);
        match level {
            Level::ERROR => { let _ = self.error_count.fetch_add(1, Ordering::Relaxed); }
            Level::WARN => { let _ = self.warn_count.fetch_add(1, Ordering::Relaxed); }
            Level::INFO => { let _ = self.info_count.fetch_add(1, Ordering::Relaxed); }
            Level::DEBUG => { let _ = self.debug_count.fetch_add(1, Ordering::Relaxed); }
            Level::TRACE => { let _ = self.trace_count.fetch_add(1, Ordering::Relaxed); }
        }
    }

    pub fn get_summary(&self) -> LogMetricsSummary {
        LogMetricsSummary {
            total: self.total_logs.load(Ordering::Relaxed),
            errors: self.error_count.load(Ordering::Relaxed),
            warnings: self.warn_count.load(Ordering::Relaxed),
            info: self.info_count.load(Ordering::Relaxed),
            debug: self.debug_count.load(Ordering::Relaxed),
            trace: self.trace_count.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogMetricsSummary {
    pub total: u64,
    pub errors: u64,
    pub warnings: u64,
    pub info: u64,
    pub debug: u64,
    pub trace: u64,
}

/// 日志上下文（用于结构化日志）
#[derive(Debug, Clone)]
pub struct LogContext {
    pub request_id: Option<String>,
    pub user_id: Option<String>,
    pub room_id: Option<String>,
    pub extra: std::collections::HashMap<String, String>,
}

impl LogContext {
    pub fn new() -> Self {
        Self {
            request_id: None,
            user_id: None,
            room_id: None,
            extra: std::collections::HashMap::new(),
        }
    }

    pub fn with_request_id(mut self, id: String) -> Self {
        self.request_id = Some(id);
        self
    }

    pub fn with_user_id(mut self, id: String) -> Self {
        self.user_id = Some(id);
        self
    }

    pub fn with_room_id(mut self, id: String) -> Self {
        self.room_id = Some(id);
        self
    }

    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

impl Default for LogContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 初始化优化日志系统
pub fn init_logging(config: &LoggingConfig) {
    // 创建环境过滤器
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("{}", config.level)));

    // 创建 subscriber
    let subscriber = tracing_subscriber::registry().with(env_filter);

    if config.structured {
        // JSON 格式
        let layer = fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true);

        subscriber.with(layer).init();
    } else {
        // 人类可读格式
        let layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_file(true)
            .with_line_number(true);

        subscriber.with(layer).init();
    }

    tracing::info!(
        "Logging initialized: level={:?}, structured={}",
        config.level,
        config.structured
    );
}

/// 便捷宏：带上下文的结构化日志
#[macro_export]
macro_rules! log_with_context {
    ($ctx:expr, $level:expr, $($arg:tt)*) => {
        tracing::log!($level, target: "synapse", request_id = ?$ctx.request_id, user_id = ?$ctx.user_id, room_id = ?$ctx.room_id, $($arg)*)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_filter() {
        let mut filter = LogLevelFilter::new(Level::INFO);
        filter.set_module_level("cache", Level::DEBUG);
        
        assert_eq!(filter.get_level("cache"), Level::DEBUG);
        assert_eq!(filter.get_level("other"), Level::INFO);
        assert!(filter.should_log("cache", Level::DEBUG));
        assert!(!filter.should_log("cache", Level::TRACE));
    }

    #[test]
    fn test_log_context() {
        let ctx = LogContext::new()
            .with_request_id("req-123".to_string())
            .with_user_id("user-456".to_string())
            .with_room_id("room-789".to_string())
            .with_extra("key", "value");

        assert_eq!(ctx.request_id, Some("req-123".to_string()));
        assert_eq!(ctx.user_id, Some("user-456".to_string()));
        assert_eq!(ctx.room_id, Some("room-789".to_string()));
    }

    #[test]
    fn test_log_metrics() {
        let metrics = LogMetrics::new();
        metrics.record(Level::ERROR);
        metrics.record(Level::ERROR);
        metrics.record(Level::WARN);
        metrics.record(Level::INFO);

        let summary = metrics.get_summary();
        assert_eq!(summary.total, 4);
        assert_eq!(summary.errors, 2);
        assert_eq!(summary.warnings, 1);
        assert_eq!(summary.info, 1);
    }
}
