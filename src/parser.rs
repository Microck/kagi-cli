use scraper::{Html, Selector};

use crate::error::KagiError;
use crate::types::{AssistantThreadSummary, SearchResult};

/// Parse Kagi search results from HTML.
///
/// Handles two known result layouts:
/// - Primary results: `.search-result` with `.__sri_title_link` and `.__sri_desc`
/// - Grouped results: `.sr-group .__srgi` with `.__srgi-title a` and `.__sri_desc`
///
/// LENS NOTE: As of 2026-03-15, lens-filtered search results are expected to use the
/// same HTML structure as standard search. If live testing reveals different selectors
/// or additional layout variants for lens-scoped results, this parser should be extended
/// with lens-specific extraction paths. The current implementation assumes structural
/// parity between filtered and unfiltered results.
pub fn parse_search_results(html: &str) -> Result<Vec<SearchResult>, KagiError> {
    let document = Html::parse_document(html);

    let search_result_selector = selector(".search-result")?;
    let grouped_result_selector = selector(".sr-group .__srgi")?;
    let title_link_selector = selector(".__sri_title_link")?;
    let grouped_title_link_selector = selector(".__srgi-title a")?;
    let snippet_selector = selector(".__sri-desc")?;

    let mut results = Vec::new();

    for element in document.select(&search_result_selector) {
        if let Some(result) = extract_result(&element, &title_link_selector, &snippet_selector) {
            results.push(result);
        }
    }

    for element in document.select(&grouped_result_selector) {
        if let Some(result) =
            extract_result(&element, &grouped_title_link_selector, &snippet_selector)
        {
            results.push(result);
        }
    }

    Ok(results)
}

pub fn parse_assistant_thread_list(html: &str) -> Result<Vec<AssistantThreadSummary>, KagiError> {
    let document = Html::parse_fragment(html);
    let thread_selector = selector(".thread-list .thread")?;
    let anchor_selector = selector("a")?;
    let title_selector = selector(".title")?;
    let snippet_selector = selector(".excerpt")?;

    let mut threads = Vec::new();

    for element in document.select(&thread_selector) {
        let id = element
            .value()
            .attr("data-code")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                KagiError::Parse("assistant thread list item missing data-code".to_string())
            })?;
        let saved = element
            .value()
            .attr("data-saved")
            .map(|value| value == "true")
            .unwrap_or(false);
        let shared = element
            .value()
            .attr("data-public")
            .map(|value| value == "true")
            .unwrap_or(false);
        let tag_ids =
            serde_json::from_str::<Vec<String>>(element.value().attr("data-tags").unwrap_or("[]"))
                .map_err(|error| {
                    KagiError::Parse(format!("failed to parse assistant thread tag ids: {error}"))
                })?;
        let snippet = element
            .value()
            .attr("data-snippet")
            .map(str::trim)
            .unwrap_or_default()
            .to_string();

        let anchor = element.select(&anchor_selector).next().ok_or_else(|| {
            KagiError::Parse("assistant thread list item missing anchor".to_string())
        })?;
        let url = anchor
            .value()
            .attr("href")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| KagiError::Parse("assistant thread list item missing href".to_string()))?
            .to_string();
        let title = element
            .select(&title_selector)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                KagiError::Parse("assistant thread list item missing title".to_string())
            })?;
        let parsed_snippet = element
            .select(&snippet_selector)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or(snippet);

        threads.push(AssistantThreadSummary {
            id: id.to_string(),
            title,
            url,
            snippet: parsed_snippet,
            saved,
            shared,
            tag_ids,
        });
    }

    Ok(threads)
}

fn extract_result(
    element: &scraper::element_ref::ElementRef<'_>,
    title_selector: &Selector,
    snippet_selector: &Selector,
) -> Option<SearchResult> {
    let title_link = element.select(title_selector).next()?;
    let title = title_link.text().collect::<String>().trim().to_string();
    let url = title_link.value().attr("href")?.trim().to_string();
    let snippet = element
        .select(snippet_selector)
        .next()
        .map(|node| node.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    if title.is_empty() || url.is_empty() {
        return None;
    }

    Some(SearchResult {
        t: 0,
        rank: None,
        title,
        url,
        snippet,
        published: None,
    })
}

fn selector(value: &str) -> Result<Selector, KagiError> {
    Selector::parse(value)
        .map_err(|error| KagiError::Parse(format!("failed to parse selector `{value}`: {error:?}")))
}

#[cfg(test)]
mod tests {
    use super::{parse_assistant_thread_list, parse_search_results};

    #[test]
    fn parses_primary_and_grouped_results() {
        let html = r#"
        <html><body>
          <div class="search-result">
            <a class="__sri_title_link" href="https://example.com/one">One Result</a>
            <div class="__sri-desc">First snippet</div>
          </div>
          <div class="sr-group">
            <div class="__srgi">
              <div class="__srgi-title">
                <a href="https://example.com/two">Grouped Result</a>
              </div>
              <div class="__sri-desc">Second snippet</div>
            </div>
          </div>
        </body></html>
        "#;

        let results = parse_search_results(html).expect("parser should succeed");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].t, 0);
        assert_eq!(results[0].title, "One Result");
        assert_eq!(results[0].url, "https://example.com/one");
        assert_eq!(results[0].snippet, "First snippet");
        assert_eq!(results[1].t, 0);
        assert_eq!(results[1].title, "Grouped Result");
        assert_eq!(results[1].url, "https://example.com/two");
        assert_eq!(results[1].snippet, "Second snippet");
    }

    #[test]
    fn returns_empty_vec_when_no_matches_exist() {
        let html = "<html><body><div>No search results here</div></body></html>";
        let results = parse_search_results(html).expect("parser should succeed");
        assert!(results.is_empty());
    }

    #[test]
    fn parses_assistant_thread_list_items() {
        let html = r#"
        <div class="hide-if-no-threads">
          <ul class="thread-list">
            <li class="thread"
                data-code="thread-1"
                data-saved="true"
                data-public="false"
                data-tags='["tag-1"]'
                data-snippet="First snippet">
              <a href="/assistant/thread-1">
                <div class="title">First Thread</div>
                <div class="excerpt">First snippet</div>
              </a>
            </li>
          </ul>
        </div>
        "#;

        let threads = parse_assistant_thread_list(html).expect("thread list should parse");

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "thread-1");
        assert_eq!(threads[0].title, "First Thread");
        assert_eq!(threads[0].url, "/assistant/thread-1");
        assert_eq!(threads[0].snippet, "First snippet");
        assert!(threads[0].saved);
        assert!(!threads[0].shared);
        assert_eq!(threads[0].tag_ids, vec!["tag-1".to_string()]);
    }
}
