use std::sync::OnceLock;
use std::time::Duration;

use reqwest::{Client, Response};
use tracing::debug;

use crate::error::KagiError;

const USER_AGENT: &str = concat!(
    "kagi-cli/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Microck/kagi-cli)"
);

static CLIENT_20S: OnceLock<Result<Client, String>> = OnceLock::new();
static CLIENT_30S: OnceLock<Result<Client, String>> = OnceLock::new();

pub fn client_20s() -> Result<Client, KagiError> {
    cached_client(&CLIENT_20S, Duration::from_secs(20))
}

pub fn client_30s() -> Result<Client, KagiError> {
    cached_client(&CLIENT_30S, Duration::from_secs(30))
}

pub fn map_transport_error(error: reqwest::Error) -> KagiError {
    if error.is_timeout() {
        return KagiError::Network("request to Kagi timed out".to_string());
    }

    if error.is_connect() {
        return KagiError::Network(format!("failed to connect to Kagi: {error}"));
    }

    KagiError::Network(format!("request to Kagi failed: {error}"))
}

pub async fn read_error_body(response: Response, surface: &str) -> String {
    match response.text().await {
        Ok(body) => body,
        Err(error) => {
            debug!(surface, error = %error, "failed to read error response body");
            String::new()
        }
    }
}

fn cached_client(
    slot: &OnceLock<Result<Client, String>>,
    timeout: Duration,
) -> Result<Client, KagiError> {
    let result = slot.get_or_init(|| {
        Client::builder()
            .user_agent(USER_AGENT)
            .timeout(timeout)
            .build()
            .map_err(|error| format!("failed to build HTTP client: {error}"))
    });

    result
        .as_ref()
        .cloned()
        .map_err(|error| KagiError::Network(error.clone()))
}
