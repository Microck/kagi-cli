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
}
