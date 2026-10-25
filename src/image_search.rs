//! Subscriber-only image search. Never selects or falls back to a paid API.
use reqwest::{StatusCode, header};
use scraper::{Html, Selector};
use serde::Serialize;
use std::collections::HashSet;
use std::time::Duration;

use crate::error::KagiError;
use crate::http;
use crate::types::{ImageSearchResponse, ImageSearchResult};

#[derive(Debug, Clone, Serialize)]
/// Query and optional local cap for a single subscriber image-result page.
pub struct ImageSearchRequest {
    pub query: String,
    pub limit: Option<usize>,
}

impl ImageSearchRequest {
    /// Reject blank queries and unsupported result limits before any network request.
    pub fn validate(&self) -> Result<(), KagiError> {
        if self.query.trim().is_empty() {
            return Err(KagiError::Config("search query cannot be empty".into()));
        }
        if self.limit.is_some_and(|n| n == 0 || n > 1024) {
            return Err(KagiError::Config(
                "--limit must be between 1 and 1024".into(),
            ));
        }
        Ok(())
    }
}

/// Fetch and parse one subscriber image page without paid fallback or redirects.
///
/// # Errors
/// Returns validation, authentication, transport, or response-format errors.
pub async fn execute(
    request: &ImageSearchRequest,
    token: &str,
) -> Result<ImageSearchResponse, KagiError> {
    request.validate()?;
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "image search requires KAGI_SESSION_TOKEN; paid API fallback is disabled".into(),
        ));
    }
    // Never forward the subscriber cookie through a redirect. A sign-in redirect
    // is an authentication failure, not an empty result set.
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "kagi-cli/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/Microck/kagi-cli)"
        ))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| KagiError::Network("could not prepare image-search HTTP client".into()))?;
    let mut cookie = header::HeaderValue::from_str(&format!("kagi_session={token}"))
        .map_err(|_| KagiError::Auth("invalid Kagi session token format".into()))?;
    cookie.set_sensitive(true);
    let response = client
        .get(http::kagi_url("/images"))
        .query(&[("q", request.query.trim())])
        .header(header::COOKIE, cookie)
        .send()
        .await
        .map_err(|error| {
            use std::error::Error;
            let error = error.without_url();
            let mut details = Vec::new();
            let mut source = error.source();
            while let Some(cause) = source {
                details.push(cause.to_string());
                source = cause.source();
            }
            if details.is_empty() {
                details.push(error.to_string());
            }
            KagiError::Network(format!(
                "image-search request failed: {}",
                details.join(": ")
            ))
        })?;
    let status = response.status();
    if status.is_redirection() || matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
    {
        return Err(KagiError::Auth(format!(
            "Kagi image search rejected the session (HTTP {status}); check kagi auth"
        )));
    }
    if status != StatusCode::OK {
        return Err(KagiError::Network(format!(
            "Kagi image search returned HTTP {status}"
        )));
    }
    // Do not log response bodies: account markup can contain private data.
    let body = response
        .text()
        .await
        .map_err(|_| KagiError::Network("could not read image-search response".into()))?;
    let mut data = parse_results(&body)?;
    if let Some(limit) = request.limit {
        data.truncate(limit);
    }
    Ok(ImageSearchResponse {
        query: request.query.trim().into(),
        data,
    })
}

/// Compile an internal selector without exposing page data on failure.
fn selector(value: &str) -> Result<Selector, KagiError> {
    Selector::parse(value).map_err(|_| KagiError::Parse("invalid image-search selector".into()))
}

/// Accept only absolute HTTP(S) URLs without credentials or control characters.
fn http_url(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.chars().any(char::is_control) {
        return None;
    }
    let url = reqwest::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(value.to_owned())
}

/// Parse result cards, including the hidden HTML streamed after the page shell.
/// Contract observed on Kagi /images, 2026-10-25: `_0_image_item` data attributes
/// contain original dimensions/URLs; img width/height are thumbnail dimensions.
pub fn parse_results(body: &str) -> Result<Vec<ImageSearchResult>, KagiError> {
    let document = Html::parse_document(body);
    // Challenge words may legitimately appear in image titles or source URLs.
    // Only challenge-specific elements indicate browser verification.
    if document.select(&selector(
        "link[href*='turnstile_page'], .cf-turnstile, #challenge-form, script[src*='challenges.cloudflare.com/turnstile']"
    )?).next().is_some() {
        return Err(KagiError::Auth(
            "Kagi requires browser verification; no image search results were parsed".into(),
        ));
    }
    if document
        .select(&selector("input[type=password], form[action='/signin']")?)
        .next()
        .is_some()
    {
        return Err(KagiError::Auth(
            "Kagi session expired; sign in again".into(),
        ));
    }
    let scripts: Vec<_> = document
        .select(&selector("script")?)
        .map(|s| s.inner_html())
        .collect();
    let streaming = scripts.iter().any(|s| s.contains("scriptStreamCallback("));
    let images_complete = scripts
        .iter()
        .any(|s| s.contains("scriptStreamCallback(\"images\", \"html\")"));
    let stream_done = scripts
        .iter()
        .any(|s| s.contains("scriptStreamCallback(\"done\", \"text\")"));
    if streaming && (!images_complete || !stream_done) {
        return Err(KagiError::Parse(
            "incomplete Kagi image-search stream; retry the search".into(),
        ));
    }
    let cards: Vec<_> = document.select(&selector("._0_image_item")?).collect();
    if cards.is_empty() {
        // Only a completed image payload may legitimately be empty. The initial
        // page shell alone also contains an empty grid and must not look successful.
        if images_complete
            && stream_done
            && document
                .select(&selector("._0_img-results")?)
                .next()
                .is_some()
        {
            return Ok(vec![]);
        }
        return Err(KagiError::Parse(
            "unrecognized Kagi image-search response; expected image results".into(),
        ));
    }
    let thumbnail = selector("img._0_img_src")?;
    let mut seen = HashSet::new();
    let mut results = Vec::new();
    for card in cards {
        let attr = card.value();
        let image_url = http_url(attr.attr("data-content_url")).ok_or_else(|| {
            KagiError::Parse("image result is missing a valid original image URL".into())
        })?;
        let source_url = http_url(attr.attr("data-host_url")).ok_or_else(|| {
            KagiError::Parse("image result is missing a valid source page URL".into())
        })?;
        if !seen.insert((image_url.clone(), source_url.clone())) {
            continue;
        }
        let dimensions = |name| {
            attr.attr(name)
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|v| *v > 0)
        };
        results.push(ImageSearchResult {
            title: attr.attr("data-title").unwrap_or("").trim().to_owned(),
            image_url,
            source_url,
            thumbnail_url: card
                .select(&thumbnail)
                .next()
                .and_then(|img| http_url(img.value().attr("src"))),
            width: dimensions("data-width"),
            height: dimensions("data-height"),
        });
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../tests/fixtures/image-search.html");

    #[test]
    fn parses_streamed_cards_and_original_dimensions() {
        let results = parse_results(FIXTURE).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Otter & friends");
        assert_eq!(
            results[0].image_url,
            "https://images.example/otter.jpg?a=1&b=2"
        );
        assert_eq!(results[0].source_url, "https://example.org/otter");
        assert_eq!(results[0].width, Some(922));
        assert_eq!(results[0].height, Some(843));
        assert_eq!(results[1].width, None);
        assert_eq!(results[1].thumbnail_url, None);
    }

    #[test]
    fn fails_on_shell_challenge_login_and_truncated_stream() {
        for body in [
            "<div class='_0_img-results'></div>",
            "<link href='turnstile_page.scss.css'>",
            "<input type=password>",
            "<h1>Unknown response</h1>",
        ] {
            assert!(parse_results(body).is_err());
        }
        assert!(
            parse_results(&FIXTURE.replace("scriptStreamCallback(\"done\", \"text\")", ""))
                .is_err()
        );
    }

    #[test]
    fn verification_words_in_results_are_not_a_challenge() {
        for title in ["Verify you are human", "cf-turnstile", "turnstile_page"] {
            let body = FIXTURE.replace("Otter &amp; friends", title);
            let results = parse_results(&body).unwrap();
            assert_eq!(results.len(), 2);
            assert_eq!(results[0].title, title);
        }
        for markup in [
            "<div class='cf-turnstile'></div>",
            "<form id='challenge-form'></form>",
            "<script src='https://challenges.cloudflare.com/turnstile/v0/api.js'></script>",
        ] {
            assert!(matches!(parse_results(markup), Err(KagiError::Auth(_))));
        }
    }

    #[test]
    fn completed_empty_stream_is_empty() {
        let body = "<div class='_0_img-results'></div><script>scriptStreamCallback(\"images\", \"html\")</script><script>scriptStreamCallback(\"done\", \"text\")</script>";
        assert!(parse_results(body).unwrap().is_empty());
    }

    #[test]
    fn rejects_unsafe_or_missing_result_urls() {
        for bad in [
            "javascript:alert(1)",
            "file:///tmp/private",
            "https://user:secret@example.org/x",
            "",
        ] {
            assert!(parse_results(&FIXTURE.replace("https://example.org/otter", bad)).is_err());
        }
    }

    #[test]
    fn validates_requests() {
        for (query, limit) in [(" ", None), ("otter", Some(0)), ("otter", Some(1025))] {
            assert!(
                ImageSearchRequest {
                    query: query.into(),
                    limit
                }
                .validate()
                .is_err()
            );
        }
    }

    #[tokio::test]
    async fn requires_session_without_paid_fallback() {
        let err = execute(
            &ImageSearchRequest {
                query: "otter".into(),
                limit: None,
            },
            "",
        )
        .await
        .unwrap_err();
        assert!(matches!(err, KagiError::Auth(_)));
    }
}
