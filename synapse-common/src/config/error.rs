use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Config load error: {0}")]
    LoadError(String),
    #[error("Config parse error: {0}")]
    ParseError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
}
