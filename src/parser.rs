use scraper::{Html, Selector};

use crate::error::KagiError;
use crate::types::SearchResult;

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
    use super::parse_search_results;

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
}
