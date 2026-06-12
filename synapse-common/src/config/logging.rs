use serde::Deserialize;

/// 日志配置。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct LoggingConfig {
    /// 日志级别
    pub level: String,
    /// 日志格式
    pub format: String,
    /// 日志文件路径
    pub log_file: Option<String>,
    /// 日志目录
    pub log_dir: Option<String>,
}
