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

static CLIENT_20S: OnceLock<Client> = OnceLock::new();
static CLIENT_30S: OnceLock<Client> = OnceLock::new();
static CLIENT_ASSISTANT_STREAM: OnceLock<Client> = OnceLock::new();

/// Returns a shared HTTP client with a 20-second timeout.
///
/// # Errors
/// Returns `KagiError::Network` if the client cannot be constructed.
pub fn client_20s() -> Result<Client, KagiError> {
    cached_client(&CLIENT_20S, Duration::from_secs(20))
}

/// Returns a shared HTTP client with a 30-second timeout.
///
/// # Errors
/// Returns `KagiError::Network` if the client cannot be constructed.
pub fn client_30s() -> Result<Client, KagiError> {
    cached_client(&CLIENT_30S, Duration::from_secs(30))
}

/// Returns a shared HTTP client for Kagi Assistant streams.
///
/// Assistant responses can legitimately take longer than the short API deadline while the
/// server continues streaming useful frames. Use connect and per-read timeouts instead of a
/// total request timeout so long completions are not cut off after 30 seconds.
///
/// # Errors
/// Returns `KagiError::Network` if the client cannot be constructed.
pub fn client_assistant_stream() -> Result<Client, KagiError> {
    if let Some(client) = CLIENT_ASSISTANT_STREAM.get() {
        return Ok(client.clone());
    }

    let client = Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| KagiError::Network(format!("failed to build HTTP client: {error}")))?;

    let _ = CLIENT_ASSISTANT_STREAM.set(client.clone());
    Ok(client)
}

/// Maps a `reqwest::Error` to a domain-specific `KagiError`.
///
/// # Arguments
/// * `error` - The transport-level error from reqwest.
///
/// # Returns
/// A `KagiError::Network` variant with a descriptive message.
pub fn map_transport_error(error: reqwest::Error) -> KagiError {
    let target = error.url().map(|url| url.as_str()).unwrap_or("unknown URL");

    if error.is_timeout() {
        return KagiError::Network(format!(
            "request to {target} timed out after the configured timeout"
        ));
    }

    if error.is_connect() {
        return KagiError::Network(format!("failed to connect to {target}: {error}"));
    }

    KagiError::Network(format!("request to {target} failed: {error}"))
}

/// Reads the response body text, returning a diagnostic placeholder on failure.
///
/// # Arguments
/// * `response` - The HTTP response to consume.
/// * `surface` - A label used in debug logging on read failure.
///
/// # Returns
/// The response body as a string, or a diagnostic placeholder if the body could not be read.
pub async fn read_error_body(response: Response, surface: &str) -> String {
    match response.text().await {
        Ok(body) => body,
        Err(error) => {
            debug!(surface, error = %error, "failed to read error response body");
            format!("<failed to read error body: {error}>")
        }
    }
}

/// Formats a response-body diagnostic suffix for HTTP status errors.
///
/// The suffix is intentionally short so a failed CLI command remains readable,
/// while still preserving the server's most useful diagnostic text.
pub fn error_body_suffix(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let normalized = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    let detail = match normalized.char_indices().nth(500) {
        Some((idx, _)) => format!("{}...", &normalized[..idx]),
        None => normalized,
    };
    format!("; response body: {detail}")
}

/// Builds a full Kagi API URL from a path, using the `KAGI_BASE_URL` env override or the default.
///
/// # Arguments
/// * `path` - API path (e.g. `"/api/v0/search"`). Absolute URLs are returned unchanged.
///
/// # Returns
/// The complete URL string.
pub fn kagi_url(path: &str) -> String {
    build_url(
        &base_url_from_env(KAGI_BASE_URL_ENV, DEFAULT_KAGI_BASE_URL),
        path,
    )
}

/// Builds a full Kagi News API URL from a path, using the `KAGI_NEWS_BASE_URL` env override or the default.
///
/// # Arguments
/// * `path` - API path (e.g. `"/api/batches/latest"`). Absolute URLs are returned unchanged.
///
/// # Returns
/// The complete URL string.
pub fn kagi_news_url(path: &str) -> String {
    build_url(
        &base_url_from_env(KAGI_NEWS_BASE_URL_ENV, DEFAULT_KAGI_NEWS_BASE_URL),
        path,
    )
}

/// Builds a full Kagi Translate API URL from a path, using the `KAGI_TRANSLATE_BASE_URL` env override or the default.
///
/// # Arguments
/// * `path` - API path. Absolute URLs are returned unchanged.
///
/// # Returns
/// The complete URL string.
pub fn kagi_translate_url(path: &str) -> String {
    build_url(
        &base_url_from_env(KAGI_TRANSLATE_BASE_URL_ENV, DEFAULT_KAGI_TRANSLATE_BASE_URL),
        path,
    )
}

fn cached_client(slot: &OnceLock<Client>, timeout: Duration) -> Result<Client, KagiError> {
    if let Some(client) = slot.get() {
        return Ok(client.clone());
    }

    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .build()
        .map_err(|error| KagiError::Network(format!("failed to build HTTP client: {error}")))?;

    let _ = slot.set(client.clone());
    Ok(client)
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
    use crate::test_support::lock_env;

    fn set_env_var(key: &str, value: &str) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env_var(key: &str) {
        unsafe { std::env::remove_var(key) }
    }

    #[test]
    fn builds_default_urls() {
        let _guard = lock_env();

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
        let _guard = lock_env();

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

    #[test]
    fn formats_http_error_body_suffix() {
        let suffix = super::error_body_suffix("  rate   limit exceeded\nretry later  ");
        assert_eq!(suffix, "; response body: rate limit exceeded retry later");
    }
}
