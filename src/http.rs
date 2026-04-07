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
const DEFAULT_KAGI_BASE_URL: &str = "https://kagi.com";
const DEFAULT_KAGI_NEWS_BASE_URL: &str = "https://news.kagi.com";
const DEFAULT_KAGI_TRANSLATE_BASE_URL: &str = "https://translate.kagi.com";

pub const KAGI_BASE_URL_ENV: &str = "KAGI_BASE_URL";
pub const KAGI_NEWS_BASE_URL_ENV: &str = "KAGI_NEWS_BASE_URL";
pub const KAGI_TRANSLATE_BASE_URL_ENV: &str = "KAGI_TRANSLATE_BASE_URL";

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

pub fn kagi_url(path: &str) -> String {
    build_url(
        &base_url_from_env(KAGI_BASE_URL_ENV, DEFAULT_KAGI_BASE_URL),
        path,
    )
}

pub fn kagi_news_url(path: &str) -> String {
    build_url(
        &base_url_from_env(KAGI_NEWS_BASE_URL_ENV, DEFAULT_KAGI_NEWS_BASE_URL),
        path,
    )
}

pub fn kagi_translate_url(path: &str) -> String {
    build_url(
        &base_url_from_env(KAGI_TRANSLATE_BASE_URL_ENV, DEFAULT_KAGI_TRANSLATE_BASE_URL),
        path,
    )
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

fn base_url_from_env(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn build_url(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }

    if path.starts_with('/') {
        format!("{}{}", base.trim_end_matches('/'), path)
    } else {
        format!("{}/{}", base.trim_end_matches('/'), path)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        KAGI_BASE_URL_ENV, KAGI_NEWS_BASE_URL_ENV, KAGI_TRANSLATE_BASE_URL_ENV, kagi_news_url,
        kagi_translate_url, kagi_url,
    };

    fn set_env_var(key: &str, value: &str) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env_var(key: &str) {
        unsafe { std::env::remove_var(key) }
    }

    #[test]
    fn builds_default_urls() {
        remove_env_var(KAGI_BASE_URL_ENV);
        remove_env_var(KAGI_NEWS_BASE_URL_ENV);
        remove_env_var(KAGI_TRANSLATE_BASE_URL_ENV);

        assert_eq!(kagi_url("/api/v0/search"), "https://kagi.com/api/v0/search");
        assert_eq!(
            kagi_news_url("/api/batches/latest"),
            "https://news.kagi.com/api/batches/latest"
        );
        assert_eq!(
            kagi_translate_url("/api/translate"),
            "https://translate.kagi.com/api/translate"
        );
    }

    #[test]
    fn honors_base_url_overrides() {
        set_env_var(KAGI_BASE_URL_ENV, "http://127.0.0.1:9000/");
        set_env_var(KAGI_NEWS_BASE_URL_ENV, "http://127.0.0.1:9001/");
        set_env_var(KAGI_TRANSLATE_BASE_URL_ENV, "http://127.0.0.1:9002/");

        assert_eq!(
            kagi_url("/api/v0/search"),
            "http://127.0.0.1:9000/api/v0/search"
        );
        assert_eq!(
            kagi_news_url("/api/batches/latest"),
            "http://127.0.0.1:9001/api/batches/latest"
        );
        assert_eq!(
            kagi_translate_url("/api/translate"),
            "http://127.0.0.1:9002/api/translate"
        );

        remove_env_var(KAGI_BASE_URL_ENV);
        remove_env_var(KAGI_NEWS_BASE_URL_ENV);
        remove_env_var(KAGI_TRANSLATE_BASE_URL_ENV);
    }
}
