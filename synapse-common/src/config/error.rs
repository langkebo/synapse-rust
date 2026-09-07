use thiserror::Error;

#[derive(Debug, Error)]
/// Represents ConfigError; see per-variant docs.
pub enum ConfigError {
    #[error("Config load error: {0}")]
    /// `LoadError` variant.
    LoadError(String),
    #[error("Config parse error: {0}")]
    /// `ParseError` variant.
    ParseError(String),
    #[error("Validation error: {0}")]
    /// `ValidationError` variant.
    ValidationError(String),
}
