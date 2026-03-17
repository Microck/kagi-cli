use thiserror::Error;

#[derive(Debug, Error)]
pub enum KagiError {
    #[error("network error: {0}")]
    Network(String),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("batch error: {0}")]
    Batch(String),
}

impl From<serde_json::Error> for KagiError {
    fn from(err: serde_json::Error) -> Self {
        KagiError::Parse(format!("JSON serialization error: {}", err))
    }
}
