//! Account billing and usage reporting.
//!
//! Fetches the authenticated Kagi billing settings page with session-token auth
//! and parses the current plan, AI-cost allowance, renewal metadata, and daily
//! calendar-month usage.

use reqwest::{StatusCode, header};
use scraper::{Html, Selector};
use serde::Serialize;

use crate::error::KagiError;
use crate::http;

const KAGI_BILLING_PATH: &str = "/settings/billing";
const TEXT_CANDIDATE_LIMIT: usize = 512;
const AI_COST_LABEL: &str = "total ai cost this period";
const AI_USAGE_LABEL: &str = "ai usage (usd)";
const ACCOUNT_BALANCE_LABEL: &str = "account balance";
const NEXT_RENEWAL_LABEL: &str = "next renewal is";

/// Account billing and calendar-month usage reported by Kagi.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UsageReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub ai_cost: AiCostUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_balance_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_renewal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_period: Option<String>,
    pub daily_usage: Vec<DailyUsage>,
}

/// AI cost consumed and included allowance, in USD.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AiCostUsage {
    pub used_usd: f64,
    pub limit_usd: f64,
}

/// One row from the billing page's calendar-month usage table.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DailyUsage {
    pub date: String,
    pub searches: u64,
    pub ai_cost_usd: f64,
}

/// Fetches and parses the authenticated Kagi billing page.
///
/// # Errors
///
/// Returns authentication errors for a missing or expired session token,
/// network errors for failed HTTP requests, and parse errors when Kagi's page
/// no longer exposes the required AI cost values.
pub async fn execute_usage(session_token: &str) -> Result<UsageReport, KagiError> {
    let session_token = session_token.trim();
    if session_token.is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi session token (expected KAGI_SESSION_TOKEN)".to_string(),
        ));
    }

    let response = http::client_20s()?
        .get(http::kagi_url(KAGI_BILLING_PATH))
        .header(header::COOKIE, format!("kagi_session={session_token}"))
        .header(header::ACCEPT, "text/html")
        .send()
        .await
        .map_err(http::map_transport_error)?;

    let status = response.status();
    let final_path = response.url().path().to_ascii_lowercase();
    if final_path.contains("/login") || final_path.contains("/signin") {
        return Err(KagiError::Auth(
            "invalid or expired Kagi session token for billing usage".to_string(),
        ));
    }

    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return Err(KagiError::Auth(format!(
            "invalid or expired Kagi session token for billing usage: HTTP {status}"
        )));
    }

    if !status.is_success() {
        let body = http::read_error_body(response, "billing usage").await;
        let redacted_body = body.replace(session_token, "<redacted>");
        let suffix = http::error_body_suffix(&redacted_body);
        let message = if status.is_server_error() {
            format!("Kagi billing server error: HTTP {status}{suffix}")
        } else {
            format!("unexpected Kagi billing response status: HTTP {status}{suffix}")
        };
        return Err(KagiError::Network(message));
    }

    let body = response.text().await.map_err(|error| {
        KagiError::Network(format!(
            "failed to read Kagi billing response body: {error}"
        ))
    })?;

    // Reuse the shared AND-based detector: the real billing page contains
    // "Welcome to Kagi" in its footer, so an OR over markers would reject
    // every successful authenticated response.
    if crate::api::looks_like_logged_out_page(&body) {
        return Err(KagiError::Auth(
            "invalid or expired Kagi session token for billing usage".to_string(),
        ));
    }

    parse_usage_html(&body)
}

/// Formats a usage report for human-readable terminal output.
pub fn format_pretty(report: &UsageReport) -> String {
    let mut lines = Vec::new();

    if let Some(plan) = report.plan.as_deref() {
        lines.push(format!("Plan: {plan}"));
    }
    lines.push(format!(
        "AI cost: ${:.2} / ${:.2}",
        report.ai_cost.used_usd, report.ai_cost.limit_usd
    ));
    if let Some(balance) = report.account_balance_usd {
        lines.push(format!("Account balance: ${balance:.2}"));
    }
    if let Some(next_renewal) = report.next_renewal.as_deref() {
        lines.push(format!("Next renewal: {next_renewal}"));
    }
    if let Some(period) = report.usage_period.as_deref() {
        lines.push(format!("Usage period: {period}"));
    }

    if !report.daily_usage.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "{:<10}  {:>10}  {:>13}",
            "Date", "Searches", "AI cost (USD)"
        ));
        for row in &report.daily_usage {
            lines.push(format!(
                "{:<10}  {:>10}  {:>13.3}",
                row.date, row.searches, row.ai_cost_usd
            ));
        }
    }

    lines.join("\n")
}

/// Parses billing-page HTML into a structured usage report.
fn parse_usage_html(html: &str) -> Result<UsageReport, KagiError> {
    let document = Html::parse_document(html);
    let candidates = text_candidates(&document);
    let ai_cost = extract_ai_cost(&candidates).ok_or_else(|| {
        KagiError::Parse(
            "Kagi billing page did not contain the current AI cost and limit; the page layout may have changed"
                .to_string(),
        )
    })?;

    Ok(UsageReport {
        plan: extract_plan(&candidates),
        ai_cost,
        account_balance_usd: extract_first_decimal_after_label(&candidates, ACCOUNT_BALANCE_LABEL),
        next_renewal: extract_text_after_label(&candidates, NEXT_RENEWAL_LABEL)
            .and_then(|text| extract_iso_date(&text)),
        usage_period: extract_usage_period(&candidates),
        daily_usage: parse_daily_usage(&document),
    })
}

/// Collects short, normalized visible-text candidates from the page.
fn text_candidates(document: &Html) -> Vec<String> {
    let selector = Selector::parse("body *").expect("static selector should parse");
    let mut candidates = Vec::new();

    for element in document.select(&selector) {
        if matches!(
            element.value().name(),
            "script" | "style" | "noscript" | "svg" | "path"
        ) {
            continue;
        }

        let text = normalize_space(&element.text().collect::<Vec<_>>().join(" "));
        if text.is_empty() || text.len() > TEXT_CANDIDATE_LIMIT || candidates.contains(&text) {
            continue;
        }
        candidates.push(text);
    }

    candidates.sort_by_key(String::len);
    candidates
}

/// Extracts the current AI cost and included limit from text candidates.
///
/// Handles both known page layouts: the legacy
/// `Total AI cost this period (USD) $X / $Y` text and the newer
/// `AI usage (USD)` box whose `$X` and `/Y` render in separate elements but
/// aggregate into one ancestor candidate.
fn extract_ai_cost(candidates: &[String]) -> Option<AiCostUsage> {
    candidates.iter().find_map(|candidate| {
        let tail = slice_after_label(candidate, AI_COST_LABEL)
            .or_else(|| slice_after_label(candidate, AI_USAGE_LABEL))?;
        let values = decimal_values(tail);
        if values.len() < 2 {
            return None;
        }
        Some(AiCostUsage {
            used_usd: values[0],
            limit_usd: values[1],
        })
    })
}

/// Extracts the first decimal value following a case-insensitive label.
fn extract_first_decimal_after_label(candidates: &[String], label: &str) -> Option<f64> {
    candidates.iter().find_map(|candidate| {
        slice_after_label(candidate, label).and_then(|tail| decimal_values(tail).into_iter().next())
    })
}

/// Extracts non-empty text following a case-insensitive label.
fn extract_text_after_label(candidates: &[String], label: &str) -> Option<String> {
    candidates.iter().find_map(|candidate| {
        let tail = slice_after_label(candidate, label)?.trim();
        (!tail.is_empty()).then_some(tail.to_string())
    })
}

/// Returns the portion of text after a case-insensitive ASCII label.
fn slice_after_label<'a>(text: &'a str, label: &str) -> Option<&'a str> {
    let lower = text.to_ascii_lowercase();
    let offset = lower.find(label)?;
    Some(&text[offset + label.len()..])
}

/// Extracts the plan name from a monthly or yearly price description.
fn extract_plan(candidates: &[String]) -> Option<String> {
    candidates.iter().find_map(|candidate| {
        let lower = candidate.to_ascii_lowercase();
        if !lower.contains("per month") && !lower.contains("per year") {
            return None;
        }

        let boundary = candidate
            .char_indices()
            .find_map(|(index, character)| {
                (character.is_ascii_digit() || matches!(character, '$' | '€' | '£'))
                    .then_some(index)
            })
            .unwrap_or(candidate.len());
        let mut plan = candidate[..boundary]
            .trim_matches(|character: char| {
                character.is_whitespace() || matches!(character, ':' | '-' | '–' | '—')
            })
            .to_string();

        for prefix in ["Switch Plan", "Current Plan"] {
            if let Some(stripped) = plan.strip_prefix(prefix) {
                plan = stripped.trim().to_string();
            }
        }

        (!plan.is_empty()).then_some(plan)
    })
}

/// Extracts a calendar-month heading such as `December 2025`.
fn extract_usage_period(candidates: &[String]) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];

    candidates.iter().find_map(|candidate| {
        let words = candidate.split_whitespace().collect::<Vec<_>>();
        if words.len() != 2
            || !MONTHS
                .iter()
                .any(|month| words[0].eq_ignore_ascii_case(month))
            || words[1].len() != 4
            || !words[1].chars().all(|character| character.is_ascii_digit())
        {
            return None;
        }
        Some(candidate.clone())
    })
}

/// Parses the first table containing date, search, and AI-cost columns.
fn parse_daily_usage(document: &Html) -> Vec<DailyUsage> {
    let table_selector = Selector::parse("table").expect("static selector should parse");
    let row_selector = Selector::parse("tr").expect("static selector should parse");
    let cell_selector = Selector::parse("th, td").expect("static selector should parse");

    for table in document.select(&table_selector) {
        let rows = table
            .select(&row_selector)
            .map(|row| {
                row.select(&cell_selector)
                    .map(|cell| normalize_space(&cell.text().collect::<Vec<_>>().join(" ")))
                    .collect::<Vec<_>>()
            })
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>();

        let Some((header_index, date_index, searches_index, cost_index)) =
            rows.iter().enumerate().find_map(|(row_index, row)| {
                let date_index = row
                    .iter()
                    .position(|cell| cell.to_ascii_lowercase().contains("date"))?;
                let searches_index = row
                    .iter()
                    .position(|cell| cell.to_ascii_lowercase().contains("search"))?;
                let cost_index = row.iter().position(|cell| {
                    let lower = cell.to_ascii_lowercase();
                    lower.contains("ai cost") || lower == "cost" || lower.contains("cost (usd)")
                })?;
                Some((row_index, date_index, searches_index, cost_index))
            })
        else {
            continue;
        };

        let required_index = date_index.max(searches_index).max(cost_index);
        let parsed = rows
            .iter()
            .skip(header_index + 1)
            .filter_map(|row| {
                if row.len() <= required_index {
                    return None;
                }
                let date = extract_iso_date(&row[date_index])?;
                let searches = parse_search_count(&row[searches_index])?;
                let ai_cost_usd = decimal_values(&row[cost_index]).into_iter().next()?;
                Some(DailyUsage {
                    date,
                    searches,
                    ai_cost_usd,
                })
            })
            .collect::<Vec<_>>();

        if !parsed.is_empty() {
            return parsed;
        }
    }

    Vec::new()
}

/// Parses a search count while ignoring locale-specific grouping separators.
fn parse_search_count(text: &str) -> Option<u64> {
    let digits = text
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// Extracts decimal values from free-form text.
fn decimal_values(text: &str) -> Vec<f64> {
    let mut values = Vec::new();
    let mut token = String::new();

    for character in text.chars().chain(std::iter::once(' ')) {
        if character.is_ascii_digit()
            || matches!(character, '.' | ',' | '\'' | '\u{00a0}' | '\u{202f}')
        {
            token.push(character);
            continue;
        }

        if let Some(value) = parse_decimal_token(&token) {
            values.push(value);
        }
        token.clear();
    }

    values
}

/// Parses one decimal token with comma, period, or grouped separators.
fn parse_decimal_token(token: &str) -> Option<f64> {
    let compact = token
        .chars()
        .filter(|character| character.is_ascii_digit() || matches!(character, '.' | ','))
        .collect::<String>();
    if compact.is_empty() || !compact.chars().any(|character| character.is_ascii_digit()) {
        return None;
    }

    let last_separator = compact
        .char_indices()
        .rev()
        .find(|(_, character)| matches!(character, '.' | ','));
    let normalized = if let Some((separator_index, separator)) = last_separator {
        let fractional_digits = compact[separator_index + separator.len_utf8()..]
            .chars()
            .filter(char::is_ascii_digit)
            .count();
        let has_prior_same_separator = compact[..separator_index]
            .chars()
            .any(|character| character == separator);
        let has_other_separator = compact
            .chars()
            .any(|character| matches!(character, '.' | ',') && character != separator);

        if (1..=3).contains(&fractional_digits)
            && !(has_prior_same_separator && !has_other_separator)
        {
            compact
                .char_indices()
                .filter_map(|(index, character)| {
                    if character.is_ascii_digit() {
                        Some(character)
                    } else if index == separator_index {
                        Some('.')
                    } else {
                        None
                    }
                })
                .collect::<String>()
        } else {
            compact
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<String>()
        }
    } else {
        compact
    };

    normalized.parse().ok()
}

/// Extracts the first `YYYY-MM-DD` date from text.
fn extract_iso_date(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.len() < 10 {
        return None;
    }

    for start in 0..=bytes.len() - 10 {
        let candidate = &bytes[start..start + 10];
        if candidate[0..4].iter().all(u8::is_ascii_digit)
            && candidate[4] == b'-'
            && candidate[5..7].iter().all(u8::is_ascii_digit)
            && candidate[7] == b'-'
            && candidate[8..10].iter().all(u8::is_ascii_digit)
        {
            return Some(text[start..start + 10].to_string());
        }
    }

    None
}

/// Collapses all Unicode whitespace runs to single ASCII spaces.
fn normalize_space(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{AiCostUsage, DailyUsage, format_pretty, parse_decimal_token, parse_usage_html};

    /// Returns representative localized billing HTML for parser tests.
    fn billing_fixture() -> &'static str {
        r#"
        <html>
          <body>
            <section class="plan">
              <span>Ultimate</span>
              <span>$25 (+tax) per month</span>
            </section>
            <section class="summary">
              <div>
                <span>Total AI cost this period (USD)</span>
                <strong>$0,00/25,00</strong>
              </div>
              <div>
                <span>Account balance</span>
                <strong>$5.00</strong>
              </div>
            </section>
            <p>Next renewal is <strong>2026-01-28</strong></p>
            <div class="usage-heading">December 2025</div>
            <table>
              <thead>
                <tr>
                  <th>Date (UTC)</th>
                  <th>Searches</th>
                  <th>AI Cost (USD)</th>
                </tr>
              </thead>
              <tbody>
                <tr><td>2025-12-29</td><td>2</td><td>0.000</td></tr>
                <tr><td>2025-12-28</td><td>1,234</td><td>1.125</td></tr>
              </tbody>
            </table>
          </body>
        </html>
        "#
    }

    /// Verifies summary fields and daily rows from representative billing HTML.
    #[test]
    fn parses_billing_summary_and_daily_usage() {
        let report = parse_usage_html(billing_fixture()).expect("fixture should parse");

        assert_eq!(report.plan.as_deref(), Some("Ultimate"));
        assert_eq!(
            report.ai_cost,
            AiCostUsage {
                used_usd: 0.0,
                limit_usd: 25.0,
            }
        );
        assert_eq!(report.account_balance_usd, Some(5.0));
        assert_eq!(report.next_renewal.as_deref(), Some("2026-01-28"));
        assert_eq!(report.usage_period.as_deref(), Some("December 2025"));
        assert_eq!(
            report.daily_usage,
            vec![
                DailyUsage {
                    date: "2025-12-29".to_string(),
                    searches: 2,
                    ai_cost_usd: 0.0,
                },
                DailyUsage {
                    date: "2025-12-28".to_string(),
                    searches: 1234,
                    ai_cost_usd: 1.125,
                },
            ]
        );
    }

    /// Verifies daily columns are selected by header rather than position.
    #[test]
    fn maps_daily_columns_by_header_name() {
        let html = r#"
        <html><body>
          <div>Total AI cost this period (USD) $1.25 / $25.00</div>
          <table>
            <tr><th>Date</th><th>AI Tokens</th><th>AI Cost</th><th>Searches</th></tr>
            <tr><td>2026-08-21</td><td>42,000</td><td>0,125</td><td>17</td></tr>
          </table>
        </body></html>
        "#;

        let report = parse_usage_html(html).expect("alternate table should parse");
        assert_eq!(report.daily_usage.len(), 1);
        assert_eq!(report.daily_usage[0].searches, 17);
        assert!((report.daily_usage[0].ai_cost_usd - 0.125).abs() < f64::EPSILON);
    }

    /// Verifies decimal and grouping conventions used by localized billing pages.
    #[test]
    fn parses_common_decimal_conventions() {
        assert_eq!(parse_decimal_token("0,00"), Some(0.0));
        assert_eq!(parse_decimal_token("25.00"), Some(25.0));
        assert_eq!(parse_decimal_token("1.234,56"), Some(1234.56));
        assert_eq!(parse_decimal_token("1,234.56"), Some(1234.56));
        assert_eq!(parse_decimal_token("1,234,567"), Some(1_234_567.0));
        assert_eq!(parse_decimal_token("1.234.567"), Some(1_234_567.0));
    }

    /// Verifies missing required cost values produce a layout-change error.
    #[test]
    fn reports_layout_changes_when_cost_pair_is_missing() {
        let error = parse_usage_html("<html><body>Billing Details</body></html>")
            .expect_err("missing cost pair should fail");
        assert!(
            error
                .to_string()
                .contains("did not contain the current AI cost and limit")
        );
    }

    /// Regression: the live billing page renders the newer `AI usage (USD)`
    /// box whose `$used` and `/limit` live in separate child elements, and its
    /// footer legitimately contains "Welcome to Kagi". Both must parse as a
    /// successful authenticated response instead of a logged-out rejection.
    #[test]
    fn parses_current_ai_usage_box_without_logged_out_false_positive() {
        // Mirrors the live page structure observed 2026-08-22.
        let html = r#"
        <html><body>
          <div class="billing_box">
            <div class="billing_box_body">
              <div class="billing_box_count_box">
                <div class="billing_box_count_title">AI usage (USD)</div>
                <div class="billing_box_count_num"><span>$12.76</span>/20.00</div>
              </div>
            </div>
          </div>
          <footer>Welcome to Kagi</footer>
        </body></html>
        "#;

        let report = parse_usage_html(html).expect("current layout should parse");
        assert_eq!(report.ai_cost.used_usd, 12.76);
        assert_eq!(report.ai_cost.limit_usd, 20.0);
        assert_eq!(report.plan, None);
        assert_eq!(report.account_balance_usd, None);
        assert_eq!(report.next_renewal, None);
        assert_eq!(report.usage_period, None);
        assert!(report.daily_usage.is_empty());
    }

    /// Guards the shared AND-based logged-out detector: pages carrying any
    /// single marker (e.g. footer text) must not be treated as logged out.
    #[test]
    fn logged_out_detection_requires_all_markers() {
        use crate::api::looks_like_logged_out_page;

        let partial = "<html><body>Welcome to Kagi</body></html>";
        assert!(!looks_like_logged_out_page(partial));

        let logged_out = "<html><head><title>Kagi Search - A Premium Search Engine</title></head>\
             <body>Welcome to Kagi, the paid search engine that gives power back to the user</body></html>";
        assert!(looks_like_logged_out_page(logged_out));
    }

    /// Verifies terminal output includes the key summary and usage fields.
    #[test]
    fn formats_pretty_output() {
        let report = parse_usage_html(billing_fixture()).expect("fixture should parse");
        let output = format_pretty(&report);
        assert!(output.contains("Plan: Ultimate"));
        assert!(output.contains("AI cost: $0.00 / $25.00"));
        assert!(output.contains("2025-12-29"));
    }
}
