use reqwest::{Client, StatusCode, header};
use serde::Deserialize;

use crate::error::KagiError;
use crate::http;
use crate::parser::parse_search_results;
use crate::types::{SearchResponse, SearchResult};

const KAGI_SEARCH_URL: &str = "https://kagi.com/html/search";
const KAGI_API_SEARCH_URL: &str = "https://kagi.com/api/v0/search";
const UNAUTHENTICATED_MARKERS: [&str; 3] = [
    "<title>Kagi Search - A Premium Search Engine</title>",
    "Welcome to Kagi",
    "paid search engine that gives power back to the user",
];

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query: String,
    pub lens: Option<String>,
    pub region: Option<String>,
    pub time_filter: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub order: Option<String>,
    pub verbatim: Option<bool>,
    pub personalized: Option<bool>,
}

impl SearchRequest {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            lens: None,
            region: None,
            time_filter: None,
            from_date: None,
            to_date: None,
            order: None,
            verbatim: None,
            personalized: None,
        }
    }

    pub fn with_lens(mut self, lens: impl Into<String>) -> Self {
        self.lens = Some(lens.into());
        self
    }

    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    pub fn with_time_filter(mut self, time_filter: impl Into<String>) -> Self {
        self.time_filter = Some(time_filter.into());
        self
    }

    pub fn with_from_date(mut self, from_date: impl Into<String>) -> Self {
        self.from_date = Some(from_date.into());
        self
    }

    pub fn with_to_date(mut self, to_date: impl Into<String>) -> Self {
        self.to_date = Some(to_date.into());
        self
    }

    pub fn with_order(mut self, order: impl Into<String>) -> Self {
        self.order = Some(order.into());
        self
    }

    pub fn with_verbatim(mut self, verbatim: bool) -> Self {
        self.verbatim = Some(verbatim);
        self
    }

    pub fn with_personalized(mut self, personalized: bool) -> Self {
        self.personalized = Some(personalized);
        self
    }

    pub fn has_runtime_filters(&self) -> bool {
        self.region.is_some()
            || self.time_filter.is_some()
            || self.from_date.is_some()
            || self.to_date.is_some()
            || self.order.is_some()
            || self.verbatim.unwrap_or(false)
            || self.personalized.is_some()
    }

    pub fn requires_session_auth(&self) -> bool {
        self.lens.is_some() || self.has_runtime_filters()
    }

    pub fn validate(&self) -> Result<(), KagiError> {
        if self.query.trim().is_empty() {
            return Err(KagiError::Config(
                "search query cannot be empty".to_string(),
            ));
        }

        let lens = trimmed_optional(self.lens.as_deref());
        if self.lens.is_some() && lens.is_none() {
            return Err(KagiError::Config(
                "search --lens cannot be empty".to_string(),
            ));
        }
        if let Some(lens) = lens {
            validate_lens_value(lens)?;
        }

        let region = trimmed_optional(self.region.as_deref());
        if self.region.is_some() && region.is_none() {
            return Err(KagiError::Config(
                "search --region cannot be empty".to_string(),
            ));
        }

        let time_filter = trimmed_optional(self.time_filter.as_deref());
        if self.time_filter.is_some() && time_filter.is_none() {
            return Err(KagiError::Config(
                "search --time cannot be empty".to_string(),
            ));
        }

        let order = trimmed_optional(self.order.as_deref());
        if self.order.is_some() && order.is_none() {
            return Err(KagiError::Config(
                "search --order cannot be empty".to_string(),
            ));
        }

        let from_date = trimmed_optional(self.from_date.as_deref());
        if self.from_date.is_some() && from_date.is_none() {
            return Err(KagiError::Config(
                "search --from-date cannot be empty".to_string(),
            ));
        }

        let to_date = trimmed_optional(self.to_date.as_deref());
        if self.to_date.is_some() && to_date.is_none() {
            return Err(KagiError::Config(
                "search --to-date cannot be empty".to_string(),
            ));
        }

        if time_filter.is_some() && (from_date.is_some() || to_date.is_some()) {
            return Err(KagiError::Config(
                "search --time cannot be combined with --from-date or --to-date".to_string(),
            ));
        }

        if let Some(date) = from_date {
            validate_iso_date("search --from-date", date)?;
        }
        if let Some(date) = to_date {
            validate_iso_date("search --to-date", date)?;
        }
        if let (Some(from_date), Some(to_date)) = (from_date, to_date)
            && from_date > to_date
        {
            return Err(KagiError::Config(
                "search --from-date cannot be after --to-date".to_string(),
            ));
        }

        Ok(())
    }
}

pub fn validate_lens_value(lens: &str) -> Result<(), KagiError> {
    if lens.parse::<u32>().is_err() {
        return Err(KagiError::Config(format!(
            "lens '{}' must be a numeric index (e.g., '0', '1', '2'). \
             Visit https://kagi.com/settings/lenses to see your enabled lenses, \
             then use the index from the 'l=' parameter in your browser URL.",
            lens
        )));
    }

    Ok(())
}

pub async fn search_with_lens(request: &SearchRequest, token: &str) -> Result<String, KagiError> {
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi session token (expected KAGI_SESSION_TOKEN)".to_string(),
        ));
    }

    let client = build_client()?;
    let query_params = build_search_query_params(request)?;

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

    request.validate()?;

    if request.requires_session_auth() {
        return Err(KagiError::Config(api_session_requirement_message(request)));
    }

    let client = build_client()?;
    let response = client
        .get(KAGI_API_SEARCH_URL)
        .query(&[("q", request.query.trim())])
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

pub async fn execute_search(
    request: &SearchRequest,
    token: &str,
) -> Result<SearchResponse, KagiError> {
    let html = search_with_lens(request, token).await?;
    let data = parse_search_results(&html)?;
    Ok(SearchResponse { data })
}

fn build_search_query_params(
    request: &SearchRequest,
) -> Result<Vec<(&'static str, String)>, KagiError> {
    request.validate()?;

    let mut query_params = vec![("q", request.query.trim().to_string())];

    if let Some(lens) = trimmed_optional(request.lens.as_deref()) {
        query_params.push(("l", lens.to_string()));
    }
    if let Some(region) = trimmed_optional(request.region.as_deref()) {
        query_params.push(("r", region.to_string()));
    }
    if let Some(time_filter) = trimmed_optional(request.time_filter.as_deref()) {
        query_params.push(("dr", time_filter.to_string()));
    }
    if let Some(from_date) = trimmed_optional(request.from_date.as_deref()) {
        query_params.push(("from_date", from_date.to_string()));
    }
    if let Some(to_date) = trimmed_optional(request.to_date.as_deref()) {
        query_params.push(("to_date", to_date.to_string()));
    }
    if let Some(order) = trimmed_optional(request.order.as_deref())
        && !order.is_empty()
    {
        query_params.push(("order", order.to_string()));
    }
    if request.verbatim == Some(true) {
        query_params.push(("verbatim", "1".to_string()));
    }
    if let Some(personalized) = request.personalized {
        query_params.push((
            "personalized",
            if personalized { "1" } else { "0" }.to_string(),
        ));
    }

    Ok(query_params)
}

fn trimmed_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn validate_iso_date(label: &str, date: &str) -> Result<(), KagiError> {
    if !is_valid_iso_date(date) {
        return Err(KagiError::Config(format!(
            "{label} must use YYYY-MM-DD format"
        )));
    }

    Ok(())
}

fn is_valid_iso_date(date: &str) -> bool {
    if date.len() != 10 {
        return false;
    }

    let bytes = date.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }

    let year = match date[0..4].parse::<u32>() {
        Ok(year) => year,
        Err(_) => return false,
    };
    let month = match date[5..7].parse::<u32>() {
        Ok(month) => month,
        Err(_) => return false,
    };
    let day = match date[8..10].parse::<u32>() {
        Ok(day) => day,
        Err(_) => return false,
    };

    if month == 0 || month > 12 || day == 0 {
        return false;
    }

    day <= days_in_month(year, month)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn api_session_requirement_message(request: &SearchRequest) -> String {
    if request.lens.is_some() {
        "lens search requires KAGI_SESSION_TOKEN; the Kagi Search API only supports plain base search"
            .to_string()
    } else {
        "search filters require KAGI_SESSION_TOKEN; the Kagi Search API only supports plain base search"
            .to_string()
    }
}

fn looks_unauthenticated(body: &str) -> bool {
    UNAUTHENTICATED_MARKERS
        .iter()
        .all(|marker| body.contains(marker))
}

fn build_client() -> Result<Client, KagiError> {
    http::client_20s()
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
        assert!(!request.requires_session_auth());
    }

    #[test]
    fn search_request_with_lens_adds_lens() {
        let request = SearchRequest::new("rust lang").with_lens("2");
        assert_eq!(request.query, "rust lang");
        assert_eq!(request.lens, Some("2".to_string()));
        assert!(request.requires_session_auth());
    }

    #[test]
    fn search_request_with_filters_requires_session_auth() {
        let request = SearchRequest::new("rust lang")
            .with_region("us")
            .with_time_filter("2")
            .with_order("4")
            .with_verbatim(true)
            .with_personalized(false);

        assert!(request.has_runtime_filters());
        assert!(request.requires_session_auth());
    }

    #[test]
    fn validate_lens_value_rejects_non_numeric_indices() {
        let error = validate_lens_value("forums").expect_err("non-numeric lens should fail");
        assert!(matches!(error, KagiError::Config(_)));
    }

    #[test]
    fn reject_time_filter_with_date_range() {
        let error = SearchRequest::new("rust")
            .with_time_filter("2")
            .with_from_date("2026-03-01")
            .validate()
            .expect_err("time filter and custom date range should conflict");

        assert!(matches!(error, KagiError::Config(_)));
        assert!(error.to_string().contains("--time"));
    }

    #[test]
    fn rejects_invalid_from_date_format() {
        let error = SearchRequest::new("rust")
            .with_from_date("2026-2-1")
            .validate()
            .expect_err("invalid date should fail");

        assert!(matches!(error, KagiError::Config(_)));
        assert!(error.to_string().contains("YYYY-MM-DD"));
    }

    #[test]
    fn rejects_nonexistent_iso_dates() {
        let error = SearchRequest::new("rust")
            .with_to_date("2026-02-30")
            .validate()
            .expect_err("nonexistent date should fail");

        assert!(matches!(error, KagiError::Config(_)));
    }

    #[test]
    fn rejects_inverted_date_range() {
        let error = SearchRequest::new("rust")
            .with_from_date("2026-03-02")
            .with_to_date("2026-03-01")
            .validate()
            .expect_err("inverted date range should fail");

        assert!(matches!(error, KagiError::Config(_)));
        assert!(error.to_string().contains("cannot be after"));
    }

    #[test]
    fn builds_query_params_for_search_filters() {
        let request = SearchRequest::new("rust lang")
            .with_lens("2")
            .with_region("us")
            .with_order("4")
            .with_from_date("2026-03-01")
            .with_to_date("2026-03-02")
            .with_verbatim(true)
            .with_personalized(false);

        let params = build_search_query_params(&request).expect("query params should build");

        assert!(params.contains(&("q", "rust lang".to_string())));
        assert!(params.contains(&("l", "2".to_string())));
        assert!(params.contains(&("r", "us".to_string())));
        assert!(params.contains(&("order", "4".to_string())));
        assert!(params.contains(&("from_date", "2026-03-01".to_string())));
        assert!(params.contains(&("to_date", "2026-03-02".to_string())));
        assert!(params.contains(&("verbatim", "1".to_string())));
        assert!(params.contains(&("personalized", "0".to_string())));
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
    async fn execute_search_without_filters_attempts_transport() {
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

    #[tokio::test]
    async fn execute_api_search_rejects_filtered_requests() {
        let request = SearchRequest::new("test query").with_region("us");
        let result = execute_api_search(&request, "api-token").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, KagiError::Config(_)));
        assert!(
            err.to_string()
                .contains("search filters require KAGI_SESSION_TOKEN")
        );
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
