//! HTML parsing for Kagi search result pages.
//!
//! Extracts structured [`SearchResult`] values from the HTML markup returned
//! by Kagi's web search endpoint. Also parses assistant profiles, threads,
//! custom bangs, and lens details from their respective HTML pages.

use scraper::{Html, Selector};

use crate::error::KagiError;
use crate::types::{
    AssistantProfileDetails, AssistantProfileSummary, AssistantThreadSummary, CustomBangDetails,
    CustomBangSummary, LensDetails, LensSummary, RedirectRuleDetails, RedirectRuleSummary,
    SearchResult,
};

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

/// Parses a list of assistant threads from the Kagi settings HTML.
///
/// # Arguments
/// * `html` - The HTML content of the assistant thread list page.
///
/// # Returns
/// A vector of `AssistantThreadSummary` entries.
///
/// # Errors
/// Returns `KagiError::Parse` if expected elements or attributes are missing.
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
            .is_some_and(|value| value == "true");
        let shared = element
            .value()
            .attr("data-public")
            .is_some_and(|value| value == "true");
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

/// Parses a list of assistant profiles from the Kagi settings HTML.
///
/// # Arguments
/// * `html` - The HTML content of the assistant profiles page.
///
/// # Returns
/// A vector of `AssistantProfileSummary` entries.
///
/// # Errors
/// Returns `KagiError::Parse` if expected elements or attributes are missing.
pub fn parse_assistant_profile_list(html: &str) -> Result<Vec<AssistantProfileSummary>, KagiError> {
    let document = Html::parse_document(html);
    let item_selector = selector("#custom_mode_table #items_p .item")?;
    let name_selector = selector(".item-name a")?;
    let edit_selector = selector(".edit a")?;
    let detail_block_selector = selector(".item-details > div")?;
    let dd_selector = selector("dd")?;

    let mut assistants = Vec::new();

    for item in document.select(&item_selector) {
        let id = item
            .value()
            .attr("id")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| KagiError::Parse("assistant item missing id".to_string()))?
            .to_string();

        let anchor = item
            .select(&name_selector)
            .next()
            .ok_or_else(|| KagiError::Parse("assistant item missing name link".to_string()))?;
        let name = anchor.text().collect::<String>().trim().to_string();
        let href = anchor
            .value()
            .attr("href")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| KagiError::Parse("assistant item missing invoke link".to_string()))?;

        let detail_blocks = item.select(&detail_block_selector).collect::<Vec<_>>();
        let model = detail_blocks
            .first()
            .and_then(|block| block.select(&dd_selector).next())
            .map(|node| node.text().collect::<String>().trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| KagiError::Parse(format!("assistant '{name}' missing model")))?;
        let bang_trigger = detail_blocks
            .get(1)
            .map(|block| block.text().collect::<String>().trim().to_string())
            .filter(|value| !value.is_empty());
        let internet_access = detail_blocks
            .last()
            .and_then(|block| block.select(&dd_selector).next())
            .map(|node| node.text().collect::<String>())
            .is_some_and(|value| parse_toggle_text(&value));
        let edit_url = item
            .select(&edit_selector)
            .next()
            .and_then(|node| node.value().attr("href"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let invoke_profile = parse_query_value(href, "profile").unwrap_or_else(|| id.clone());

        assistants.push(AssistantProfileSummary {
            id,
            name,
            invoke_profile,
            model,
            bang_trigger,
            internet_access,
            built_in: edit_url.is_none(),
            edit_url,
        });
    }

    Ok(assistants)
}

/// Parses the assistant profile edit form from HTML to extract field values.
///
/// # Arguments
/// * `html` - The HTML content of the assistant profile form.
///
/// # Returns
/// An `AssistantProfileDetails` with all form field values.
///
/// # Errors
/// Returns `KagiError::Parse` if required fields are missing from the form.
pub fn parse_assistant_profile_form(html: &str) -> Result<AssistantProfileDetails, KagiError> {
    let document = Html::parse_document(html);
    let profile_id = extract_input_value(&document, "profile_id");
    let name = extract_input_value(&document, "name")
        .ok_or_else(|| KagiError::Parse("assistant form missing name".to_string()))?;
    let bang_trigger =
        extract_input_value(&document, "bang_trigger").filter(|value| !value.is_empty());
    let selected_lens = extract_checked_radio_value(&document, "selected_lens")
        .ok_or_else(|| KagiError::Parse("assistant form missing selected lens".to_string()))?;
    let base_model = extract_checked_radio_value(&document, "base_model").unwrap_or_default();
    let custom_instructions =
        extract_textarea_value(&document, "custom_instructions").unwrap_or_default();
    let delete_supported = document
        .select(&selector(
            r#"form[action="/settings/ast/profiles/delete"]"#,
        )?)
        .next()
        .is_some();

    Ok(AssistantProfileDetails {
        profile_id,
        name,
        bang_trigger,
        internet_access: extract_checkbox_checked(&document, "internet_access"),
        selected_lens,
        personalizations: extract_checkbox_checked(&document, "personalizations"),
        base_model,
        custom_instructions,
        delete_supported,
    })
}

/// Parses a list of Kagi lenses from the settings HTML.
///
/// # Arguments
/// * `html` - The HTML content of the lenses list page.
///
/// # Returns
/// A vector of `LensSummary` entries.
///
/// # Errors
/// Returns `KagiError::Parse` if expected elements or attributes are missing.
pub fn parse_lens_list(html: &str) -> Result<Vec<LensSummary>, KagiError> {
    let document = Html::parse_document(html);
    let item_selector = selector("form.__lens_item")?;
    let name_selector = selector(".lens_title div")?;
    let description_selector = selector(".lens_desc")?;
    let edit_selector = selector(r#".lens_edit_lens a[aria-label="Edit lens"]"#)?;

    let mut lenses = Vec::new();

    for item in document.select(&item_selector) {
        let id = extract_input_value_from(&item, "lens_id")
            .ok_or_else(|| KagiError::Parse("lens item missing lens_id".to_string()))?;
        let active_index = extract_input_value_from(&item, "active_index");
        let next_index = extract_input_value_from(&item, "next_index");
        let (enabled, position, toggle_field, toggle_value) = if let Some(index) = active_index {
            (
                true,
                index.parse::<u32>().ok(),
                "active_index".to_string(),
                index,
            )
        } else if let Some(index) = next_index {
            (false, None, "next_index".to_string(), index)
        } else {
            return Err(KagiError::Parse(format!(
                "lens '{id}' missing toggle index payload"
            )));
        };
        let name = item
            .select(&name_selector)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| KagiError::Parse(format!("lens '{id}' missing name")))?;
        let description = item
            .select(&description_selector)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_string())
            .filter(|value| !value.is_empty());
        let edit_url = item
            .select(&edit_selector)
            .next()
            .and_then(|node| node.value().attr("href"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| KagiError::Parse(format!("lens '{id}' missing edit url")))?
            .to_string();

        lenses.push(LensSummary {
            id,
            name,
            description,
            enabled,
            position,
            edit_url,
            toggle_field,
            toggle_value,
        });
    }

    Ok(lenses)
}

/// Parses the lens edit form from HTML to extract field values.
///
/// # Arguments
/// * `html` - The HTML content of the lens form.
///
/// # Returns
/// A `LensDetails` with all form field values.
///
/// # Errors
/// Returns `KagiError::Parse` if the lens name field is missing.
pub fn parse_lens_form(html: &str) -> Result<LensDetails, KagiError> {
    let document = Html::parse_document(html);

    Ok(LensDetails {
        id: extract_input_value(&document, "id"),
        name: extract_input_value(&document, "name")
            .ok_or_else(|| KagiError::Parse("lens form missing name".to_string()))?,
        included_sites: extract_input_value(&document, "included_sites").unwrap_or_default(),
        included_keywords: extract_input_value(&document, "included_keywords").unwrap_or_default(),
        description: extract_input_value(&document, "description").unwrap_or_default(),
        search_region: extract_checked_radio_value(&document, "search_region")
            .unwrap_or_else(|| "no_region".to_string()),
        before_time: normalize_optional_form_value(extract_input_value(&document, "before_time")),
        after_time: normalize_optional_form_value(extract_input_value(&document, "after_time")),
        excluded_sites: extract_input_value(&document, "excluded_sites").unwrap_or_default(),
        excluded_keywords: extract_input_value(&document, "excluded_keywords").unwrap_or_default(),
        shortcut_keyword: extract_input_value(&document, "shortcut_keyword").unwrap_or_default(),
        autocomplete_keywords: extract_checkbox_checked(&document, "autocomplete_keywords"),
        template: extract_checked_radio_value(&document, "template")
            .unwrap_or_else(|| "0".to_string()),
        file_type: extract_input_value(&document, "file_type").unwrap_or_default(),
        share_with_team: extract_checkbox_checked(&document, "share_with_team"),
        share_copy_code: extract_checkbox_checked(&document, "share_copy_code"),
    })
}

/// Parses a list of custom bangs from the settings HTML table.
///
/// # Arguments
/// * `html` - The HTML content of the custom bangs page.
///
/// # Returns
/// A vector of `CustomBangSummary` entries.
///
/// # Errors
/// Returns `KagiError::Parse` if expected elements are missing.
pub fn parse_custom_bang_list(html: &str) -> Result<Vec<CustomBangSummary>, KagiError> {
    let document = Html::parse_document(html);
    let row_selector = selector("table.custom_bangs_table tbody tr")?;
    let cell_selector = selector("td")?;
    let edit_selector = selector("a.s-edit-btn")?;

    let mut bangs = Vec::new();

    for row in document.select(&row_selector) {
        let cells = row.select(&cell_selector).collect::<Vec<_>>();
        if cells.len() < 3 {
            continue;
        }

        let edit_url = row
            .select(&edit_selector)
            .next()
            .and_then(|node| node.value().attr("href"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| KagiError::Parse("custom bang row missing edit url".to_string()))?
            .to_string();
        let id = parse_query_value(&edit_url, "bang_id")
            .ok_or_else(|| KagiError::Parse("custom bang edit url missing bang_id".to_string()))?;

        bangs.push(CustomBangSummary {
            id,
            name: cells[0].text().collect::<String>().trim().to_string(),
            trigger: cells[1].text().collect::<String>().trim().to_string(),
            shortcut_menu: parse_toggle_text(&cells[2].text().collect::<String>()),
            edit_url,
        });
    }

    Ok(bangs)
}

/// Parses the custom bang edit form from HTML to extract field values.
///
/// # Arguments
/// * `html` - The HTML content of the custom bang form.
///
/// # Returns
/// A `CustomBangDetails` with all form field values.
///
/// # Errors
/// Returns `KagiError::Parse` if the name or trigger field is missing.
pub fn parse_custom_bang_form(html: &str) -> Result<CustomBangDetails, KagiError> {
    let document = Html::parse_document(html);

    Ok(CustomBangDetails {
        bang_id: extract_input_value(&document, "bang_id"),
        name: extract_input_value(&document, "name")
            .ok_or_else(|| KagiError::Parse("custom bang form missing name".to_string()))?,
        trigger: extract_input_value(&document, "trigger")
            .ok_or_else(|| KagiError::Parse("custom bang form missing trigger".to_string()))?,
        template: extract_input_value(&document, "template").unwrap_or_default(),
        snap_domain: extract_input_value(&document, "snap_domain").unwrap_or_default(),
        regex_pattern: extract_input_value(&document, "regex_pattern").unwrap_or_default(),
        shortcut_menu: extract_checkbox_checked(&document, "shortcut_menu"),
        fmt_open_snap_domain: extract_checkbox_checked(&document, "fmt_open_snap_domain"),
        fmt_open_base_path: extract_checkbox_checked(&document, "fmt_open_base_path"),
        fmt_url_encode_placeholder: extract_checkbox_checked(
            &document,
            "fmt_url_encode_placeholder",
        ),
        fmt_url_encode_space_to_plus: extract_checkbox_checked(
            &document,
            "fmt_url_encode_space_to_plus",
        ),
    })
}

/// Parses a list of redirect rules from the settings HTML table.
///
/// # Arguments
/// * `html` - The HTML content of the redirect rules page.
///
/// # Returns
/// A vector of `RedirectRuleSummary` entries.
///
/// # Errors
/// Returns `KagiError::Parse` if expected elements are missing.
pub fn parse_redirect_list(html: &str) -> Result<Vec<RedirectRuleSummary>, KagiError> {
    let document = Html::parse_document(html);
    let row_selector = selector("table tbody tr")?;
    let cell_selector = selector("td")?;
    let edit_selector = selector(r#"a[href*="/settings/redirects_form?rule_id="]"#)?;
    let toggle_form_selector = selector(r#"form[action="/rewrite_rules/toggle"]"#)?;
    let delete_form_selector = selector(r#"form[action="/rewrite_rules/delete"]"#)?;
    let button_selector = selector("button")?;

    let mut redirects = Vec::new();

    for row in document.select(&row_selector) {
        let edit_url = row
            .select(&edit_selector)
            .next()
            .and_then(|node| node.value().attr("href"))
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let toggle_form = row.select(&toggle_form_selector).next();
        let delete_form = row.select(&delete_form_selector).next();

        if edit_url.is_none() || toggle_form.is_none() || delete_form.is_none() {
            continue;
        }

        let edit_url = edit_url
            .ok_or_else(|| KagiError::Parse("redirect row missing edit URL".to_string()))?
            .to_string();
        let id = parse_query_value(&edit_url, "rule_id")
            .or_else(|| toggle_form.and_then(|form| extract_input_value_from(&form, "rule_id")))
            .ok_or_else(|| KagiError::Parse("redirect row missing rule_id".to_string()))?;
        let cells = row.select(&cell_selector).collect::<Vec<_>>();
        let rule = cells
            .first()
            .map(|cell| cell.text().collect::<String>().trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| KagiError::Parse(format!("redirect '{id}' missing rule text")))?;
        let enabled = toggle_form
            .and_then(|form| form.select(&button_selector).next())
            .and_then(|button| button.value().attr("class"))
            .is_some_and(|class_name| class_name.contains("--enabled"));

        redirects.push(RedirectRuleSummary {
            id,
            rule,
            enabled,
            edit_url,
        });
    }

    Ok(redirects)
}

/// Parses the redirect rule edit form from HTML to extract field values.
///
/// # Arguments
/// * `html` - The HTML content of the redirect rule form.
///
/// # Returns
/// A `RedirectRuleDetails` with the form field values.
///
/// # Errors
/// Returns `KagiError::Parse` if the rule field is missing.
pub fn parse_redirect_form(html: &str) -> Result<RedirectRuleDetails, KagiError> {
    let document = Html::parse_document(html);

    Ok(RedirectRuleDetails {
        rule_id: extract_input_value(&document, "rule_id"),
        rule: extract_input_value(&document, "regex")
            .ok_or_else(|| KagiError::Parse("redirect form missing rule".to_string()))?,
        enabled: None,
    })
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

fn extract_input_value(document: &Html, name: &str) -> Option<String> {
    let selector = selector(&format!(r#"input[name="{name}"]"#)).ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|node| node.value().attr("value"))
        .map(str::to_string)
}

fn extract_textarea_value(document: &Html, name: &str) -> Option<String> {
    let selector = selector(&format!(r#"textarea[name="{name}"]"#)).ok()?;
    document
        .select(&selector)
        .next()
        .map(|node| node.text().collect::<String>())
}

fn extract_checked_radio_value(document: &Html, name: &str) -> Option<String> {
    let selector = selector(&format!(r#"input[type="radio"][name="{name}"]"#)).ok()?;
    document.select(&selector).find_map(|node| {
        node.value()
            .attr("checked")
            .map(|_| ())
            .and_then(|()| node.value().attr("value"))
            .map(str::to_string)
    })
}

fn extract_checkbox_checked(document: &Html, name: &str) -> bool {
    let selector = match selector(&format!(r#"input[type="checkbox"][name="{name}"]"#)) {
        Ok(value) => value,
        Err(_) => return false,
    };
    document
        .select(&selector)
        .any(|node| node.value().attr("checked").is_some())
}

fn extract_input_value_from(
    element: &scraper::element_ref::ElementRef<'_>,
    name: &str,
) -> Option<String> {
    let selector = selector(&format!(r#"input[name="{name}"]"#)).ok()?;
    element
        .select(&selector)
        .next()
        .and_then(|node| node.value().attr("value"))
        .map(str::to_string)
}

fn parse_toggle_text(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "yes" | "on" | "true" | "enabled")
}

fn normalize_optional_form_value(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_query_value(href: &str, key: &str) -> Option<String> {
    let (_, query) = href.split_once('?')?;
    for pair in query.split('&') {
        let (candidate_key, candidate_value) = pair.split_once('=')?;
        if candidate_key == key {
            return Some(candidate_value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        parse_assistant_profile_form, parse_assistant_profile_list, parse_assistant_thread_list,
        parse_custom_bang_form, parse_custom_bang_list, parse_lens_form, parse_lens_list,
        parse_redirect_form, parse_redirect_list, parse_search_results,
    };

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

    #[test]
    fn parses_assistant_profile_list_items() {
        let html = r#"
        <div id="custom_mode_table">
          <ul id="items_p">
            <li class="item" id="profile-1">
              <div class="item-name">
                <a href="/assistant?profile=code">Code</a>
              </div>
              <dl class="item-details">
                <div><dt>Model:</dt><dd>Quick</dd></div>
                <div></div>
                <div><dt>Internet Access:</dt><dd>On</dd></div>
              </dl>
            </li>
            <li class="item" id="profile-2">
              <div class="item-name">
                <a href="/assistant?profile=profile-2">Writer</a>
              </div>
              <dl class="item-details">
                <div><dt>Model:</dt><dd>GPT 5 Mini</dd></div>
                <div>!write</div>
                <div><dt>Internet Access:</dt><dd>Off</dd></div>
              </dl>
              <div class="edit">
                <a href="/settings/custom_assistant?id=profile-2">Edit</a>
              </div>
            </li>
          </ul>
        </div>
        "#;

        let assistants = parse_assistant_profile_list(html).expect("assistant list should parse");

        assert_eq!(assistants.len(), 2);
        assert_eq!(assistants[0].invoke_profile, "code");
        assert!(assistants[0].built_in);
        assert_eq!(assistants[1].bang_trigger.as_deref(), Some("!write"));
        assert!(!assistants[1].internet_access);
        assert_eq!(
            assistants[1].edit_url.as_deref(),
            Some("/settings/custom_assistant?id=profile-2")
        );
    }

    #[test]
    fn parses_assistant_profile_form_fields() {
        let html = r#"
        <form class="s-form" action="/settings/ast/profiles/update" method="POST">
          <input type="hidden" name="profile_id" value="profile-2">
          <input type="text" name="name" value="Writer">
          <input type="text" name="bang_trigger" value="write">
          <input type="checkbox" name="internet_access" checked value="on">
          <input type="hidden" name="internet_access" value="false">
          <input type="radio" name="selected_lens" value="0" class="hidden">
          <input type="radio" name="selected_lens" value="15" checked class="hidden">
          <input type="checkbox" name="personalizations" value="on">
          <input type="hidden" name="personalizations" value="false">
          <input type="radio" name="base_model" value="gpt-5-mini" checked class="hidden">
          <textarea name="custom_instructions">Write clearly.</textarea>
        </form>
        <form action="/settings/ast/profiles/delete" method="POST"></form>
        "#;

        let details = parse_assistant_profile_form(html).expect("assistant form should parse");

        assert_eq!(details.profile_id.as_deref(), Some("profile-2"));
        assert_eq!(details.name, "Writer");
        assert_eq!(details.bang_trigger.as_deref(), Some("write"));
        assert!(details.internet_access);
        assert_eq!(details.selected_lens, "15");
        assert!(!details.personalizations);
        assert_eq!(details.base_model, "gpt-5-mini");
        assert_eq!(details.custom_instructions, "Write clearly.");
        assert!(details.delete_supported);
    }

    #[test]
    fn allows_custom_assistant_create_form_without_selected_model() {
        let html = r#"
        <form class="s-form" action="/settings/ast/profiles/update" method="POST">
          <input type="hidden" name="profile_id" value="">
          <input type="text" name="name" value="">
          <input type="text" name="bang_trigger" value="">
          <input type="checkbox" name="internet_access" checked value="on">
          <input type="hidden" name="internet_access" value="false">
          <input type="radio" name="selected_lens" value="0" checked class="hidden">
          <input type="checkbox" name="personalizations" checked value="on">
          <input type="hidden" name="personalizations" value="false">
          <input type="radio" name="base_model" value="gpt-5-mini" class="hidden">
          <textarea name="custom_instructions"></textarea>
        </form>
        "#;

        let details = parse_assistant_profile_form(html).expect("create form should parse");

        assert_eq!(details.base_model, "");
        assert!(details.internet_access);
        assert_eq!(details.selected_lens, "0");
    }

    #[test]
    fn parses_lens_list_items() {
        let html = r#"
        <form class="__lens_item" action="/lenses/move" method="POST">
          <input type="hidden" name="lens_id" value="22524">
          <input type="hidden" name="active_index" value="0">
          <a class="lens_title" href="/settings/update_lens?id=22524"><div>Reddit</div></a>
          <button formaction="/lenses/subscribe" type="submit"></button>
          <div class="lens_edit_lens">
            <a aria-label="Edit lens" href="/settings/update_lens?id=22524">Edit</a>
          </div>
        </form>
        <form class="__lens_item" action="/lenses/move" method="POST">
          <input type="hidden" name="lens_id" value="1">
          <input type="hidden" name="next_index" value="7">
          <div class="lens_title">
            <div>Forums</div>
            <div class="desc_box"><span class="lens_desc">Forum discussions</span></div>
          </div>
          <button formaction="/lenses/subscribe" type="submit"></button>
          <div class="lens_edit_lens">
            <a aria-label="Edit lens" href="/settings/update_lens?id=1">Edit</a>
          </div>
        </form>
        "#;

        let lenses = parse_lens_list(html).expect("lens list should parse");

        assert_eq!(lenses.len(), 2);
        assert!(lenses[0].enabled);
        assert_eq!(lenses[0].position, Some(0));
        assert_eq!(lenses[1].description.as_deref(), Some("Forum discussions"));
        assert_eq!(lenses[1].toggle_field, "next_index");
    }

    #[test]
    fn parses_lens_form_fields() {
        let html = r#"
        <form class="s-form" action="/lenses/update" method="POST">
          <input type="hidden" name="id" value="22524">
          <input type="text" name="name" value="Reddit">
          <input type="text" name="included_sites" value="reddit.com">
          <input type="text" name="included_keywords" value="">
          <input type="text" name="description" value="">
          <input type="radio" name="search_region" value="no_region" checked class="hidden">
          <input type="date" name="before_time" value="none">
          <input type="date" name="after_time" value="">
          <input type="text" name="excluded_sites" value="">
          <input type="text" name="excluded_keywords" value="">
          <input type="text" name="shortcut_keyword" value="reddit">
          <input type="checkbox" name="autocomplete_keywords" value="true" checked>
          <input type="hidden" name="autocomplete_keywords" value="false">
          <input type="radio" name="template" value="0" checked class="hidden">
          <input type="text" name="file_type" value="pdf">
          <input type="checkbox" name="share_with_team" value="true">
          <input type="hidden" name="share_with_team" value="false">
          <input type="checkbox" name="share_copy_code" value="true" checked>
          <input type="hidden" name="share_copy_code" value="false">
        </form>
        "#;

        let details = parse_lens_form(html).expect("lens form should parse");

        assert_eq!(details.id.as_deref(), Some("22524"));
        assert_eq!(details.name, "Reddit");
        assert_eq!(details.included_sites, "reddit.com");
        assert_eq!(details.before_time, None);
        assert_eq!(details.after_time, None);
        assert!(details.autocomplete_keywords);
        assert!(!details.share_with_team);
        assert!(details.share_copy_code);
    }

    #[test]
    fn parses_custom_bang_list_rows() {
        let html = r#"
        <table class="custom_bangs_table">
          <tbody>
            <tr>
              <td>Google</td>
              <td>!g</td>
              <td>Yes</td>
              <td><a class="s-edit-btn" href="/settings/custom_bangs_form?bang_id=1">Edit</a></td>
              <td><a href="/settings/delete_custom_bang?bang_id=1">Delete</a></td>
            </tr>
          </tbody>
        </table>
        "#;

        let bangs = parse_custom_bang_list(html).expect("custom bang list should parse");

        assert_eq!(bangs.len(), 1);
        assert_eq!(bangs[0].id, "1");
        assert_eq!(bangs[0].trigger, "!g");
        assert!(bangs[0].shortcut_menu);
    }

    #[test]
    fn parses_custom_bang_form_fields() {
        let html = r#"
        <form class="s-form" action="/bangs/modify" method="POST">
          <input type="hidden" name="bang_id" value="1">
          <input type="text" name="name" value="Google">
          <input type="text" name="trigger" value="g">
          <input type="text" name="template" value="https://google.com/search?q=%s">
          <input type="text" name="snap_domain" value="google.com">
          <input type="text" name="regex_pattern" value="">
          <input type="checkbox" name="shortcut_menu" checked value="true">
          <input type="hidden" name="shortcut_menu" value="false">
          <input type="checkbox" name="fmt_open_snap_domain" value="true">
          <input type="hidden" name="fmt_open_snap_domain" value="false">
          <input type="checkbox" name="fmt_open_base_path" checked value="true">
          <input type="hidden" name="fmt_open_base_path" value="false">
          <input type="checkbox" name="fmt_url_encode_placeholder" value="true">
          <input type="hidden" name="fmt_url_encode_placeholder" value="false">
          <input type="checkbox" name="fmt_url_encode_space_to_plus" checked value="true">
          <input type="hidden" name="fmt_url_encode_space_to_plus" value="false">
        </form>
        "#;

        let details = parse_custom_bang_form(html).expect("custom bang form should parse");

        assert_eq!(details.bang_id.as_deref(), Some("1"));
        assert_eq!(details.trigger, "g");
        assert!(details.shortcut_menu);
        assert!(details.fmt_open_base_path);
        assert!(details.fmt_url_encode_space_to_plus);
        assert!(!details.fmt_open_snap_domain);
    }

    #[test]
    fn parses_redirect_list_rows() {
        let html = r#"
        <table>
          <tbody>
            <tr>
              <td>^https://www.reddit.com|https://old.reddit.com</td>
              <td class="s-row-icon">
                <a href="/settings/redirects_form?rule_id=16641">Edit</a>
              </td>
              <td class="s-row-icon">
                <form action="/rewrite_rules/delete" method="POST">
                  <input type="hidden" name="rule_id" value="16641">
                </form>
              </td>
              <td class="s-row-icon">
                <form action="/rewrite_rules/toggle" method="POST" class="_0_form_as">
                  <input type="hidden" name="rule_id" value="16641">
                  <button type="submit" class="_0_k_ui_toggle_switch k_ui_toggle_switch --enabled"></button>
                </form>
              </td>
            </tr>
          </tbody>
        </table>
        "#;

        let redirects = parse_redirect_list(html).expect("redirect list should parse");

        assert_eq!(redirects.len(), 1);
        assert_eq!(redirects[0].id, "16641");
        assert!(redirects[0].enabled);
    }

    #[test]
    fn parses_redirect_form_fields() {
        let html = r#"
        <form class="s-form" action="/rewrite_rules" method="POST">
          <input type="hidden" name="rule_id" value="16641">
          <input type="text" name="regex" value="^https://www.reddit.com|https://old.reddit.com">
        </form>
        "#;

        let details = parse_redirect_form(html).expect("redirect form should parse");

        assert_eq!(details.rule_id.as_deref(), Some("16641"));
        assert_eq!(
            details.rule,
            "^https://www.reddit.com|https://old.reddit.com"
        );
    }
}
