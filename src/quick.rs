use reqwest::{Client, StatusCode, Url, header};
use scraper::Html;
use serde::Deserialize;
use tracing::debug;

use crate::error::KagiError;
use crate::http::{self, map_transport_error};
use crate::search::{SearchRequest, validate_lens_value};
use crate::types::{
    QuickMessage, QuickMeta, QuickReferenceCollection, QuickReferenceItem, QuickResponse,
};

const KAGI_QUICK_ANSWER_URL: &str = "https://kagi.com/mother/context";

/// Executes a Kagi Quick Answer request using session-token authentication.
/// 
/// # Arguments
/// * `request` - The search request containing the query and optional lens.
/// * `token` - The Kagi session token.
/// 
/// # Returns
/// A parsed `QuickResponse` with the answer, references, and follow-up questions.
/// 
/// # Errors
/// Returns `KagiError::Auth` if the token is missing or invalid,
/// `KagiError::Config` for invalid query parameters,
/// `KagiError::Network` for transport or server errors,
/// or `KagiError::Parse` if the response stream cannot be parsed.
pub async fn execute_quick(
    request: &SearchRequest,
    token: &str,
) -> Result<QuickResponse, KagiError> {
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi session token (expected KAGI_SESSION_TOKEN)".to_string(),
        ));
    }

    let query = request.query.trim();
    if query.is_empty() {
        return Err(KagiError::Config("quick query cannot be empty".to_string()));
    }

    let lens = request
        .lens
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if request.lens.is_some() && lens.is_none() {
        return Err(KagiError::Config(
            "quick --lens cannot be empty".to_string(),
        ));
    }
    if let Some(lens) = lens {
        validate_lens_value(lens)?;
    }

    let client = build_client()?;
    let mut query_params = vec![("q", query)];
    if let Some(lens) = lens {
        query_params.push(("l", lens));
    }

    let response = client
        .post(KAGI_QUICK_ANSWER_URL)
        .body(String::new())
        .query(&query_params)
        .header(header::COOKIE, format!("kagi_session={token}"))
        .header(header::ACCEPT, "application/vnd.kagi.stream")
        .header(header::CONTENT_LENGTH, "0")
        .header(header::CACHE_CONTROL, "no-store")
        .send()
        .await
        .map_err(map_transport_error)?;

    match response.status() {
        StatusCode::OK => {
            let body = response.text().await.map_err(|error| {
                KagiError::Network(format!(
                    "failed to read quick answer response body: {error}"
                ))
            })?;

            if looks_like_html_document(&body) {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }

            parse_quick_answer_stream(&body, query, lens)
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            debug!(status = %response.status(), "Kagi Quick Answer rejected the session token");
            Err(KagiError::Auth(
                "invalid or expired Kagi session token".to_string(),
            ))
        }
        status if status.is_client_error() => {
            let body = http::read_error_body(response, "quick answer").await;
            Err(KagiError::Config(format!(
                "Kagi Quick Answer request rejected: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi Quick Answer server error: HTTP {status}"
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi Quick Answer response status: HTTP {status}"
        ))),
    }
}

/// Formats a `QuickResponse` as a human-readable pretty-printed string with optional ANSI colors.
/// 
/// # Arguments
/// * `response` - The quick answer response to format.
/// * `use_color` - Whether to include ANSI color codes.
/// 
/// # Returns
/// A formatted string with sections for the answer, references, and follow-up questions.
pub fn format_quick_pretty(response: &QuickResponse, use_color: bool) -> String {
    let heading_color = if use_color { "\x1b[1;34m" } else { "" };
    let url_color = if use_color { "\x1b[36m" } else { "" };
    let reset_color = if use_color { "\x1b[0m" } else { "" };
    let answer = render_pretty_answer(response);

    let mut sections = Vec::new();
    sections.push(format!(
        "{heading_color}Quick Answer{reset_color}\n\n{answer}"
    ));

    if !response.references.items.is_empty() {
        let references = response
            .references
            .items
            .iter()
            .map(|reference| {
                let contribution = reference
                    .contribution_pct
                    .map(|value| format!(" ({value}%)"))
                    .unwrap_or_default();
                format!(
                    "{}{}. {}{}{}\n   {}{}{}",
                    heading_color,
                    reference.index,
                    reference.title,
                    contribution,
                    reset_color,
                    url_color,
                    reference.url,
                    reset_color
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        sections.push(format!(
            "{heading_color}References{reset_color}\n\n{references}"
        ));
    }

    if !response.followup_questions.is_empty() {
        let followups = response
            .followup_questions
            .iter()
            .map(|question| format!("- {question}"))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!(
            "{heading_color}Follow-up Questions{reset_color}\n\n{followups}"
        ));
    }

    sections.join("\n\n")
}

/// Formats a `QuickResponse` as Markdown.
/// 
/// # Arguments
/// * `response` - The quick answer response to format.
/// 
/// # Returns
/// A Markdown string with the answer body, references, and follow-up questions.
pub fn format_quick_markdown(response: &QuickResponse) -> String {
    let mut sections = Vec::new();
    sections.push(render_markdown_answer(response));

    if !response.references.markdown.trim().is_empty() {
        sections.push(response.references.markdown.trim().to_string());
    }

    if !response.followup_questions.is_empty() {
        let followups = response
            .followup_questions
            .iter()
            .map(|question| format!("- {question}"))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("## Follow-up Questions\n\n{followups}"));
    }

    sections
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn parse_quick_answer_stream(
    body: &str,
    query: &str,
    lens: Option<&str>,
) -> Result<QuickResponse, KagiError> {
    let mut meta = QuickMeta::default();
    let mut last_tokens_html = String::new();
    let mut message = None;

    for frame in body.split("\0\n").filter(|frame| !frame.trim().is_empty()) {
        let Some((tag, payload)) = frame.split_once(':') else {
            continue;
        };

        match tag {
            "hi" => {
                let hello: QuickHello = serde_json::from_str(payload).map_err(|error| {
                    KagiError::Parse(format!("failed to parse quick answer hello frame: {error}"))
                })?;
                meta.version = hello.v;
                meta.trace = hello.trace;
            }
            "tokens.json" => {
                let token_frame: QuickTokensFrame =
                    serde_json::from_str(payload).map_err(|error| {
                        KagiError::Parse(format!(
                            "failed to parse quick answer token frame: {error}"
                        ))
                    })?;
                last_tokens_html = token_frame.text;
            }
            "new_message.json" => {
                let payload: QuickMessagePayload =
                    serde_json::from_str(payload).map_err(|error| {
                        KagiError::Parse(format!(
                            "failed to parse quick answer message frame: {error}"
                        ))
                    })?;
                message = Some(payload);
            }
            "limit_notice.html" => {
                let detail = html_to_text(payload);
                return Err(KagiError::Config(if detail.is_empty() {
                    "Kagi Quick Answer is currently unavailable for this account or request"
                        .to_string()
                } else {
                    format!("Kagi Quick Answer is currently unavailable: {detail}")
                }));
            }
            "unauthorized" => {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }
            _ => {
                debug!(tag, "ignoring unknown Kagi Quick Answer stream frame");
            }
        }
    }

    let message = message.ok_or_else(|| {
        if last_tokens_html.is_empty() {
            KagiError::Parse(
                "quick answer response did not include a new_message.json frame".to_string(),
            )
        } else {
            KagiError::Parse(
                "quick answer response ended before the final new_message.json frame".to_string(),
            )
        }
    })?;

    if message.state == "error" {
        let detail = if message.md.trim().is_empty() {
            html_to_text(&message.reply)
        } else {
            message.md.trim().to_string()
        };
        return Err(KagiError::Network(if detail.is_empty() {
            "Kagi Quick Answer returned an error state".to_string()
        } else {
            format!("Kagi Quick Answer failed: {detail}")
        }));
    }

    Ok(QuickResponse {
        meta,
        query: query.to_string(),
        lens: lens.map(std::string::ToString::to_string),
        message: QuickMessage {
            id: message.id,
            thread_id: message.thread_id,
            created_at: message.created_at,
            state: message.state,
            prompt: message.prompt,
            html: if message.reply.trim().is_empty() {
                last_tokens_html
            } else {
                message.reply
            },
            markdown: message.md,
        },
        references: QuickReferenceCollection {
            markdown: message.references_md.clone(),
            items: parse_quick_reference_markdown(&message.references_md),
        },
        followup_questions: message.followup_questions,
    })
}

fn parse_quick_reference_markdown(markdown: &str) -> Vec<QuickReferenceItem> {
    markdown
        .lines()
        .filter_map(parse_quick_reference_line)
        .collect()
}

fn parse_quick_reference_line(line: &str) -> Option<QuickReferenceItem> {
    let line = line.trim();
    let rest = line.strip_prefix("[^")?;
    let (index_raw, rest) = rest.split_once("]: ")?;
    let index = index_raw.parse::<usize>().ok()?;
    let rest = rest.strip_prefix('[')?;
    let title_end = rest.find("](")?;
    let title = rest[..title_end].trim().to_string();
    let remainder = &rest[(title_end + 2)..];

    let (url, contribution_pct) = if let Some(split_index) = remainder.rfind(") (") {
        let url = remainder[..split_index].trim().to_string();
        let contribution = remainder[(split_index + 3)..]
            .trim_end_matches(')')
            .trim()
            .trim_end_matches('%')
            .parse::<u8>()
            .ok();
        (url, contribution)
    } else {
        (remainder.trim_end_matches(')').trim().to_string(), None)
    };

    Some(QuickReferenceItem {
        index,
        title,
        domain: Url::parse(&url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(std::string::ToString::to_string)),
        url,
        contribution_pct,
    })
}

fn render_pretty_answer(response: &QuickResponse) -> String {
    if response.message.markdown.trim().is_empty() {
        html_to_text(&response.message.html)
    } else {
        prettify_markdown(&response.message.markdown)
    }
}

fn render_markdown_answer(response: &QuickResponse) -> String {
    if response.message.markdown.trim().is_empty() {
        html_to_text(&response.message.html)
    } else {
        response.message.markdown.trim().to_string()
    }
}

fn prettify_markdown(markdown: &str) -> String {
    let stripped_lines = markdown
        .lines()
        .filter(|line| !line.trim_start().starts_with("[^"))
        .map(strip_inline_footnote_refs)
        .map(|line| {
            cleanup_spacing_before_punctuation(
                &line.replace("**", "").replace("__", "").replace('`', ""),
            )
        })
        .collect::<Vec<_>>();

    collapse_blank_lines(&stripped_lines.join("\n"))
}

fn strip_inline_footnote_refs(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut index = 0;

    while index < line.len() {
        let remainder = &line[index..];
        if let Some(rest) = remainder.strip_prefix("[^")
            && let Some(end_index) = rest.find(']')
        {
            let footnote_id = &rest[..end_index];
            if !footnote_id.is_empty() && footnote_id.chars().all(|ch| ch.is_ascii_digit()) {
                index += 2 + end_index + 1;
                continue;
            }
        }

        let ch = remainder
            .chars()
            .next()
            .expect("line slice should contain at least one char");
        output.push(ch);
        index += ch.len_utf8();
    }

    output.trim_end().to_string()
}

fn cleanup_spacing_before_punctuation(text: &str) -> String {
    text.replace(" .", ".")
        .replace(" ,", ",")
        .replace(" ;", ";")
        .replace(" :", ":")
        .replace(" !", "!")
        .replace(" ?", "?")
}

fn collapse_blank_lines(text: &str) -> String {
    let mut lines = Vec::new();
    let mut previous_blank = false;

    for line in text.lines().map(str::trim_end) {
        if line.trim().is_empty() {
            if !previous_blank {
                lines.push(String::new());
            }
            previous_blank = true;
        } else {
            lines.push(line.to_string());
            previous_blank = false;
        }
    }

    lines.join("\n").trim().to_string()
}

fn html_to_text(html: &str) -> String {
    let normalized = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n\n")
        .replace("</li>", "\n")
        .replace("<li>", "- ")
        .replace("</ul>", "\n")
        .replace("</ol>", "\n")
        .replace("</h1>", "\n\n")
        .replace("</h2>", "\n\n")
        .replace("</h3>", "\n\n")
        .replace("</h4>", "\n\n");
    let fragment = Html::parse_fragment(&normalized);
    let text = fragment.root_element().text().collect::<Vec<_>>().join("");

    collapse_blank_lines(&text)
}

fn looks_like_html_document(body: &str) -> bool {
    body.contains("<!DOCTYPE html") || body.contains("<html")
}

fn format_client_error_suffix(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return format!("; {payload}");
    }

    let detail = html_to_text(trimmed);
    if detail.is_empty() {
        String::new()
    } else {
        format!("; {detail}")
    }
}

fn build_client() -> Result<Client, KagiError> {
    http::client_30s()
}

#[derive(Debug, Deserialize)]
struct QuickHello {
    v: Option<String>,
    trace: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QuickTokensFrame {
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
struct QuickMessagePayload {
    id: String,
    thread_id: String,
    created_at: String,
    state: String,
    prompt: String,
    #[serde(default)]
    reply: String,
    #[serde(default)]
    md: String,
    #[serde(default)]
    references_md: String,
    #[serde(default)]
    followup_questions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        format_quick_markdown, format_quick_pretty, parse_quick_answer_stream,
        parse_quick_reference_markdown, strip_inline_footnote_refs,
    };
    use crate::auth::load_credential_inventory;
    use crate::error::KagiError;
    use crate::search::SearchRequest;
    use crate::types::{QuickMessage, QuickMeta, QuickReferenceCollection, QuickResponse};

    #[test]
    fn parses_quick_reference_markdown_items() {
        let references = parse_quick_reference_markdown(
            "[^1]: [Intro to Rust](https://www.rust-lang.org/learn) (26%)\n\
             [^2]: [Rust (programming language) - Wikipedia](https://en.wikipedia.org/wiki/Rust_(programming_language)) (12%)",
        );

        assert_eq!(references.len(), 2);
        assert_eq!(references[0].index, 1);
        assert_eq!(references[0].title, "Intro to Rust");
        assert_eq!(references[0].contribution_pct, Some(26));
        assert_eq!(references[1].index, 2);
        assert_eq!(
            references[1].url,
            "https://en.wikipedia.org/wiki/Rust_(programming_language)"
        );
    }

    #[test]
    fn parses_quick_answer_stream() {
        let raw = concat!(
            "hi:{\"v\":\"202603171911.stage.707e740\",\"trace\":\"trace-123\"}\0\n",
            "tokens.json:{\"text\":\"<p>Partial answer</p>\"}\0\n",
            "new_message.json:{",
            "\"id\":\"msg-1\",",
            "\"thread_id\":\"thread-1\",",
            "\"created_at\":\"2026-03-19T00:00:00Z\",",
            "\"state\":\"done\",",
            "\"prompt\":\"what is rust?\",",
            "\"reply\":\"<p>Rust is a systems programming language.</p>\",",
            "\"md\":\"Rust is a systems programming language.\",",
            "\"references_md\":\"[^1]: [Rust](https://www.rust-lang.org/) (26%)\",",
            "\"followup_questions\":[\"Why is Rust memory-safe?\"]",
            "}\0\n"
        );

        let parsed = parse_quick_answer_stream(raw, "what is rust?", Some("0"))
            .expect("quick stream parses");

        assert_eq!(parsed.meta.trace.as_deref(), Some("trace-123"));
        assert_eq!(parsed.lens.as_deref(), Some("0"));
        assert_eq!(parsed.message.id, "msg-1");
        assert_eq!(parsed.references.items.len(), 1);
        assert_eq!(
            parsed.followup_questions,
            vec!["Why is Rust memory-safe?".to_string()]
        );
    }

    #[test]
    fn rejects_quick_stream_without_final_message() {
        let raw = concat!(
            "hi:{\"v\":\"202603171911.stage.707e740\",\"trace\":\"trace-123\"}\0\n",
            "tokens.json:{\"text\":\"<p>Partial answer</p>\"}\0\n"
        );

        let error = parse_quick_answer_stream(raw, "what is rust?", None)
            .expect_err("stream without final message should fail");
        assert!(matches!(error, KagiError::Parse(_)));
    }

    #[test]
    fn format_quick_markdown_appends_followups() {
        let raw = concat!(
            "hi:{\"v\":\"202603171911.stage.707e740\",\"trace\":\"trace-123\"}\0\n",
            "new_message.json:{",
            "\"id\":\"msg-1\",",
            "\"thread_id\":\"thread-1\",",
            "\"created_at\":\"2026-03-19T00:00:00Z\",",
            "\"state\":\"done\",",
            "\"prompt\":\"what is rust?\",",
            "\"reply\":\"<p>Rust</p>\",",
            "\"md\":\"Rust answer\",",
            "\"references_md\":\"[^1]: [Rust](https://www.rust-lang.org/) (26%)\",",
            "\"followup_questions\":[\"Why is Rust memory-safe?\"]",
            "}\0\n"
        );
        let parsed =
            parse_quick_answer_stream(raw, "what is rust?", None).expect("quick stream parses");
        let markdown = format_quick_markdown(&parsed);
        assert!(markdown.contains("Rust answer"));
        assert!(markdown.contains("[^1]: [Rust]"));
        assert!(markdown.contains("## Follow-up Questions"));
    }

    #[test]
    fn format_quick_pretty_renders_sections() {
        let raw = concat!(
            "hi:{\"v\":\"202603171911.stage.707e740\",\"trace\":\"trace-123\"}\0\n",
            "new_message.json:{",
            "\"id\":\"msg-1\",",
            "\"thread_id\":\"thread-1\",",
            "\"created_at\":\"2026-03-19T00:00:00Z\",",
            "\"state\":\"done\",",
            "\"prompt\":\"what is rust?\",",
            "\"reply\":\"<p>Rust is fast.</p>\",",
            "\"md\":\"Rust is fast.\",",
            "\"references_md\":\"[^1]: [Rust](https://www.rust-lang.org/) (26%)\",",
            "\"followup_questions\":[\"Why is Rust memory-safe?\"]",
            "}\0\n"
        );
        let parsed =
            parse_quick_answer_stream(raw, "what is rust?", None).expect("quick stream parses");
        let pretty = format_quick_pretty(&parsed, false);
        assert!(pretty.contains("Quick Answer"));
        assert!(pretty.contains("Rust is fast."));
        assert!(pretty.contains("References"));
        assert!(pretty.contains("Follow-up Questions"));
    }

    #[test]
    fn format_quick_pretty_prefers_markdown_and_strips_footnotes() {
        let response = QuickResponse {
            meta: QuickMeta::default(),
            query: "what is rust".to_string(),
            lens: None,
            message: QuickMessage {
                id: "msg-1".to_string(),
                thread_id: "thread-1".to_string(),
                created_at: "2026-03-19T00:00:00Z".to_string(),
                state: "done".to_string(),
                prompt: "what is rust".to_string(),
                html: "<p>Rust source title noise</p>".to_string(),
                markdown: "Rust is **fast**[^1].\n\n- `safe`\n".to_string(),
            },
            references: QuickReferenceCollection {
                markdown: "[^1]: [Rust](https://www.rust-lang.org/) (26%)".to_string(),
                items: Vec::new(),
            },
            followup_questions: Vec::new(),
        };

        let pretty = format_quick_pretty(&response, false);

        assert!(pretty.contains("Rust is fast."));
        assert!(pretty.contains("- safe"));
        assert!(!pretty.contains("[^1]"));
        assert!(!pretty.contains("source title noise"));
    }

    #[test]
    fn strip_inline_footnote_refs_removes_numeric_markers() {
        assert_eq!(
            strip_inline_footnote_refs("Rust[^1] is safe[^23]."),
            "Rust is safe."
        );
        assert_eq!(
            strip_inline_footnote_refs("Leave [^alpha] alone."),
            "Leave [^alpha] alone."
        );
    }

    #[test]
    fn prettify_markdown_cleans_spacing_after_footnote_removal() {
        let response = QuickResponse {
            meta: QuickMeta::default(),
            query: "what is rust".to_string(),
            lens: None,
            message: QuickMessage {
                id: "msg-1".to_string(),
                thread_id: "thread-1".to_string(),
                created_at: "2026-03-19T00:00:00Z".to_string(),
                state: "done".to_string(),
                prompt: "what is rust".to_string(),
                html: String::new(),
                markdown: "Rust is reliable [^1].".to_string(),
            },
            references: QuickReferenceCollection {
                markdown: "[^1]: [Rust](https://www.rust-lang.org/) (26%)".to_string(),
                items: Vec::new(),
            },
            followup_questions: Vec::new(),
        };

        let pretty = format_quick_pretty(&response, false);
        assert!(pretty.contains("Rust is reliable."));
        assert!(!pretty.contains("reliable ."));
    }

    #[test]
    fn rejects_quick_limit_notice_stream() {
        let raw = "limit_notice.html:<p>Daily limit reached</p>\0\n";
        let error = parse_quick_answer_stream(raw, "what is rust?", None)
            .expect_err("limit notice should fail");
        assert!(matches!(error, KagiError::Config(_)));
    }

    #[test]
    fn rejects_quick_unauthorized_stream() {
        let raw = "unauthorized:\0\n";
        let error = parse_quick_answer_stream(raw, "what is rust?", None)
            .expect_err("unauthorized stream should fail");
        assert!(matches!(error, KagiError::Auth(_)));
    }

    fn live_session_token() -> Option<String> {
        load_credential_inventory()
            .ok()?
            .session_token
            .map(|credential| credential.value)
    }

    #[tokio::test]
    #[ignore = "requires live Kagi session token"]
    async fn live_quick_query_without_question_mark() {
        let token = live_session_token().expect("missing session token for live quick test");
        let response = super::execute_quick(&SearchRequest::new("what is rust"), &token)
            .await
            .expect("quick answer should succeed");

        assert_eq!(response.query, "what is rust");
        assert_eq!(response.message.state, "done");
        assert!(!response.message.markdown.trim().is_empty());
        assert!(!response.references.items.is_empty());
        assert!(!response.followup_questions.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires live Kagi session token"]
    async fn live_quick_query_with_question_mark() {
        let token = live_session_token().expect("missing session token for live quick test");
        let response = super::execute_quick(&SearchRequest::new("what is rust?"), &token)
            .await
            .expect("quick answer should succeed");

        assert_eq!(response.message.state, "done");
        assert!(!response.message.markdown.trim().is_empty());
    }

    #[tokio::test]
    #[ignore = "requires live Kagi session token"]
    async fn live_quick_query_with_lens() {
        let token = live_session_token().expect("missing session token for live quick test");
        let request = SearchRequest::new("best rust tutorials").with_lens("0".to_string());
        let response = super::execute_quick(&request, &token)
            .await
            .expect("quick answer should succeed with lens");

        assert_eq!(response.lens.as_deref(), Some("0"));
        assert_eq!(response.message.state, "done");
        assert!(!response.message.markdown.trim().is_empty());
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn live_quick_invalid_token_is_rejected() {
        let error = super::execute_quick(&SearchRequest::new("what is rust"), "bogus.invalid")
            .await
            .expect_err("invalid token should fail");

        assert!(matches!(error, KagiError::Auth(_) | KagiError::Config(_)));
    }
}
