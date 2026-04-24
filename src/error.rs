//! Error types for the kagi-cli crate.
//!
//! All fallible operations return [`KagiError`], which covers network,
//! authentication, parsing, configuration, and batch processing failures.

use thiserror::Error;

/// Top-level error type for kagi-cli operations.
///
/// Each variant carries a human-readable description string. Convert specific
/// upstream errors (e.g. `serde_json::Error`) into the appropriate variant
/// using the provided `From` implementations.
#[derive(Debug, Error)]
/// Top-level error type for kagi-cli operations.
pub enum KagiError {
    /// A network-related failure (connection, timeout, DNS, HTTP status).
    #[error("network error: {0}")]
    Network(String),

    /// An authentication or authorization failure (missing/invalid API key).
    #[error("authentication error: {0}")]
    Auth(String),

    /// A data parsing or deserialization failure.
    #[error("parse error: {0}")]
    Parse(String),

    /// A configuration error (missing env var, invalid settings).
    #[error("configuration error: {0}")]
    Config(String),

    /// A batch operation error (parallel search failures).
    #[error("batch error: {0}")]
    Batch(String),
}

impl From<serde_json::Error> for KagiError {
    fn from(err: serde_json::Error) -> Self {
        Self::Parse(format!("JSON serialization error: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::KagiError;

    #[test]
    fn converts_serde_json_errors_to_parse_errors() {
        let serde_error = serde_json::from_str::<serde_json::Value>("{invalid json")
            .expect_err("invalid JSON should fail to deserialize");

        let error = KagiError::from(serde_error);

        assert!(matches!(error, KagiError::Parse(_)));
        assert!(error.to_string().contains("JSON serialization error"));
    }
}
