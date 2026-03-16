use reqwest::{Client, StatusCode, header};
use serde::Deserialize;

use crate::error::KagiError;
use crate::parser::parse_search_results;
use crate::types::{SearchResponse, SearchResult};

const KAGI_SEARCH_URL: &str = "https://kagi.com/html/search";
const KAGI_API_SEARCH_URL: &str = "https://kagi.com/api/v0/search";
const USER_AGENT: &str = "kagi-cli/0.1.0 (+https://github.com/)";
const UNAUTHENTICATED_MARKERS: [&str; 3] = [
    "<title>Kagi Search - A Premium Search Engine</title>",
    "Welcome to Kagi",
    "paid search engine that gives power back to the user",
];

/// Typed search request that can carry optional lens scoping.
///
/// LENS FORMAT: The lens value should be a numeric index (e.g., "0", "1", "2").
/// Lens indices are user-specific and correspond to the order of enabled lenses
/// in your Kagi account settings. Use `kagi search --help` to see how to discover
/// your available lens indices.
///
/// To find your lens indices:
/// 1. Visit https://kagi.com/settings/lenses to see your enabled lenses
/// 2. Perform a search in the Kagi web UI and note the `l=` parameter in the URL
/// 3. The index corresponds to the position in your lens dropdown (0-indexed)
#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query: String,
    pub lens: Option<String>,
}

impl SearchRequest {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            lens: None,
        }
    }

    pub fn with_lens(mut self, lens: impl Into<String>) -> Self {
        self.lens = Some(lens.into());
        self
    }
}

/// Perform a search request against Kagi's HTML endpoint.
///
/// If a lens is specified in the request, it will be passed as the `l` query parameter.
pub async fn search_with_lens(request: &SearchRequest, token: &str) -> Result<String, KagiError> {
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi session token (expected KAGI_SESSION_TOKEN)".to_string(),
        ));
    }

    let client = build_client()?;

    let mut query_params = vec![("q", request.query.as_str())];

    let lens_value: String;
    if let Some(ref lens) = request.lens {
        if lens.parse::<u32>().is_err() {
            return Err(KagiError::Config(format!(
                "lens '{}' must be a numeric index (e.g., '0', '1', '2'). \
                 Visit https://kagi.com/settings/lenses to see your enabled lenses, \
                 then use the index from the 'l=' parameter in your browser URL.",
                lens
            )));
        }
        lens_value = lens.clone();
        query_params.push(("l", lens_value.as_str()));
    }

    let response = client
        .get(KAGI_SEARCH_URL)
        .query(&query_params)
        .header(header::COOKIE, format!("kagi_session={token}"))
        .send()
        .await
        .map_err(map_transport_error)?;

    match response.status() {
        StatusCode::OK => {
            let body = response.text().await.map_err(|error| {
                KagiError::Network(format!("failed to read response body: {error}"))
            })?;

            if looks_unauthenticated(&body) {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }

            Ok(body)
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(KagiError::Auth(
            "invalid or expired Kagi session token".to_string(),
        )),
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi server error: HTTP {status}"
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi response status: HTTP {status}"
        ))),
    }
}

pub async fn execute_api_search(
    request: &SearchRequest,
    token: &str,
) -> Result<SearchResponse, KagiError> {
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi API token (expected KAGI_API_TOKEN)".to_string(),
        ));
    }

    if request.lens.is_some() {
        return Err(KagiError::Config(
            "lens search requires KAGI_SESSION_TOKEN; Kagi API token search is currently base-search only"
                .to_string(),
        ));
    }

    let client = build_client()?;
    let response = client
        .get(KAGI_API_SEARCH_URL)
        .query(&[("q", request.query.as_str())])
        .header(header::AUTHORIZATION, format!("Bot {token}"))
        .send()
        .await
        .map_err(map_transport_error)?;

    match response.status() {
        StatusCode::OK => {
            let body = response.text().await.map_err(|error| {
                KagiError::Network(format!("failed to read response body: {error}"))
            })?;
            let api_response: ApiSearchResponse = serde_json::from_str(&body).map_err(|error| {
                KagiError::Parse(format!("failed to parse Kagi API response: {error}"))
            })?;
            Ok(SearchResponse {
                data: api_response.data,
            })
        }
        status if status.is_client_error() => {
            let body = response.text().await.unwrap_or_else(|_| String::new());
            Err(KagiError::Auth(format!(
                "Kagi Search API request rejected: HTTP {status}{}",
                format_api_error_suffix(&body)
            )))
        }
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi API server error: HTTP {status}"
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi API response status: HTTP {status}"
        ))),
    }
}

/// Legacy search function for backward compatibility.
/// Consider using `search_with_lens` for lens support.
#[allow(dead_code)]
pub async fn search(query: &str, token: &str) -> Result<String, KagiError> {
    let request = SearchRequest::new(query);
    search_with_lens(&request, token).await
}

pub async fn execute_search(
    request: &SearchRequest,
    token: &str,
) -> Result<SearchResponse, KagiError> {
    let html = search_with_lens(request, token).await?;
    let data = parse_search_results(&html)?;
    Ok(SearchResponse { data })
}

fn looks_unauthenticated(body: &str) -> bool {
    UNAUTHENTICATED_MARKERS
        .iter()
        .all(|marker| body.contains(marker))
}

fn build_client() -> Result<Client, KagiError> {
    Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| KagiError::Network(format!("failed to build HTTP client: {error}")))
}

fn map_transport_error(error: reqwest::Error) -> KagiError {
    if error.is_timeout() {
        return KagiError::Network("request to Kagi timed out".to_string());
    }

    if error.is_connect() {
        return KagiError::Network(format!("failed to connect to Kagi: {error}"));
    }

    KagiError::Network(format!("request to Kagi failed: {error}"))
}

#[derive(Debug, Deserialize)]
struct ApiSearchResponse {
    data: Vec<SearchResult>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    error: Option<Vec<ApiErrorItem>>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorItem {
    msg: String,
}

fn format_api_error_suffix(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let parsed_error = serde_json::from_str::<ApiErrorBody>(trimmed)
        .ok()
        .and_then(|payload| payload.error)
        .and_then(|errors| errors.into_iter().next())
        .map(|error| error.msg);

    match parsed_error {
        Some(message) => format!("; {message}"),
        None => format!("; {trimmed}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_request_builder_creates_base_request() {
        let request = SearchRequest::new("rust lang");
        assert_eq!(request.query, "rust lang");
        assert!(request.lens.is_none());
    }

    #[test]
    fn search_request_with_lens_adds_lens() {
        let request = SearchRequest::new("rust lang").with_lens("2");
        assert_eq!(request.query, "rust lang");
        assert_eq!(request.lens, Some("2".to_string()));
    }

    #[test]
    fn search_request_with_lens_can_be_chained() {
        let request = SearchRequest::new("test query")
            .with_lens("1")
            .with_lens("2");
        assert_eq!(request.lens, Some("2".to_string()));
    }

    #[tokio::test]
    async fn execute_search_rejects_non_numeric_lens() {
        let request = SearchRequest::new("rust lang").with_lens("forums");
        let result = execute_search(&request, "dummy-token").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, KagiError::Config(_)));
        assert!(err.to_string().contains("must be a numeric index"));
    }

    #[tokio::test]
    async fn execute_search_accepts_numeric_lens() {
        let request = SearchRequest::new("test").with_lens("2");
        let result = execute_search(&request, "").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, KagiError::Auth(_)));
    }

    #[tokio::test]
    async fn execute_search_without_lens_attempts_transport() {
        let request = SearchRequest::new("test query");
        let result = execute_search(&request, "").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, KagiError::Auth(_)));
    }

    #[tokio::test]
    async fn execute_api_search_requires_token() {
        let request = SearchRequest::new("test query");
        let result = execute_api_search(&request, "").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, KagiError::Auth(_)));
        assert!(err.to_string().contains("KAGI_API_TOKEN"));
    }

    #[tokio::test]
    async fn execute_api_search_rejects_lens_requests() {
        let request = SearchRequest::new("test query").with_lens("2");
        let result = execute_api_search(&request, "api-token").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, KagiError::Config(_)));
        assert!(err.to_string().contains("requires KAGI_SESSION_TOKEN"));
    }

    #[test]
    fn parses_api_response_shape_into_search_response() {
        let raw = r#"{
            "meta": { "id": "abc", "node": "us", "ms": 10 },
            "data": [
                {
                    "t": 0,
                    "url": "https://example.com",
                    "title": "Example",
                    "snippet": "Example snippet"
                }
            ]
        }"#;

        let parsed: ApiSearchResponse = serde_json::from_str(raw).expect("api response parses");
        assert_eq!(parsed.data.len(), 1);
        assert_eq!(parsed.data[0].title, "Example");
    }

    #[test]
    fn formats_search_api_error_suffix_from_error_payload() {
        let raw = r#"{
            "meta": { "id": "abc", "api_balance": 0.0 },
            "data": null,
            "error": [{ "code": 101, "msg": "Insufficient credit to perform this request.", "ref": null }]
        }"#;

        assert_eq!(
            format_api_error_suffix(raw),
            "; Insufficient credit to perform this request."
        );
    }
}
