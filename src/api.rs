use std::collections::HashMap;

use reqwest::{Client, StatusCode, Url, header};
use scraper::Html;
use serde::Deserialize;
#[cfg(test)]
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::json;

use crate::error::KagiError;
use crate::parser::parse_assistant_thread_list;
#[cfg(test)]
use crate::types::ApiMeta;
use crate::types::{
    AssistantMessage, AssistantMeta, AssistantPromptRequest, AssistantPromptResponse,
    AssistantThread, AssistantThreadDeleteResponse, AssistantThreadExportResponse,
    AssistantThreadListResponse, AssistantThreadOpenResponse, AssistantThreadPagination,
    EnrichResponse, FastGptRequest, FastGptResponse, NewsBatchCategories, NewsBatchCategory,
    NewsCategoriesResponse, NewsCategoryMetadata, NewsCategoryMetadataList, NewsChaos,
    NewsChaosResponse, NewsLatestBatch, NewsResolvedCategory, NewsStoriesPayload,
    NewsStoriesResponse, SmallWebFeed, SubscriberSummarization, SubscriberSummarizeMeta,
    SubscriberSummarizeRequest, SubscriberSummarizeResponse, SummarizeRequest, SummarizeResponse,
};

const USER_AGENT: &str = "kagi-cli/0.1.0 (+https://github.com/)";
const KAGI_SUMMARIZE_URL: &str = "https://kagi.com/api/v0/summarize";
const KAGI_SUBSCRIBER_SUMMARIZE_URL: &str = "https://kagi.com/mother/summary_labs";
const KAGI_NEWS_LATEST_URL: &str = "https://news.kagi.com/api/batches/latest";
const KAGI_NEWS_CATEGORIES_METADATA_URL: &str = "https://news.kagi.com/api/categories/metadata";
const KAGI_NEWS_BATCH_CATEGORIES_URL: &str = "https://news.kagi.com/api/batches";
const KAGI_ASSISTANT_PROMPT_URL: &str = "https://kagi.com/assistant/prompt";
const KAGI_ASSISTANT_THREAD_OPEN_URL: &str = "https://kagi.com/assistant/thread_open";
const KAGI_ASSISTANT_THREAD_LIST_URL: &str = "https://kagi.com/assistant/thread_list";
const KAGI_ASSISTANT_THREAD_DELETE_URL: &str = "https://kagi.com/assistant/thread_delete";
const KAGI_FASTGPT_URL: &str = "https://kagi.com/api/v0/fastgpt";
const KAGI_ENRICH_WEB_URL: &str = "https://kagi.com/api/v0/enrich/web";
const KAGI_ENRICH_NEWS_URL: &str = "https://kagi.com/api/v0/enrich/news";
const KAGI_SMALLWEB_FEED_URL: &str = "https://kagi.com/api/v1/smallweb/feed/";
const ASSISTANT_ZERO_BRANCH_UUID: &str = "00000000-0000-4000-0000-000000000000";

pub async fn execute_summarize(
    request: &SummarizeRequest,
    token: &str,
) -> Result<SummarizeResponse, KagiError> {
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi API token (expected KAGI_API_TOKEN)".to_string(),
        ));
    }

    if request.url.is_some() == request.text.is_some() {
        return Err(KagiError::Config(
            "summarize requires exactly one of --url or --text".to_string(),
        ));
    }

    let client = build_client()?;
    let response = client
        .post(KAGI_SUMMARIZE_URL)
        .header(header::AUTHORIZATION, format!("Bot {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .json(request)
        .send()
        .await
        .map_err(map_transport_error)?;

    decode_kagi_json(response, "summarizer").await
}

pub async fn execute_subscriber_summarize(
    request: &SubscriberSummarizeRequest,
    token: &str,
) -> Result<SubscriberSummarizeResponse, KagiError> {
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi session token (expected KAGI_SESSION_TOKEN)".to_string(),
        ));
    }

    let (field_name, source_value) = normalize_subscriber_summary_input(request)?;
    let summary_type = normalize_subscriber_summary_type(request.summary_type.as_deref())?;
    let summary_length = normalize_subscriber_summary_length(request.length.as_deref())?;
    let target_language = request
        .target_language
        .as_deref()
        .map(str::trim)
        .unwrap_or("");

    let client = build_client()?;
    let response = client
        .get(KAGI_SUBSCRIBER_SUMMARIZE_URL)
        .query(&[
            (field_name, source_value.as_str()),
            ("stream", "1"),
            ("target_language", target_language),
            ("summary_type", summary_type.as_str()),
            ("summary_length", summary_length.as_str()),
        ])
        .header(header::COOKIE, format!("kagi_session={token}"))
        .header(header::ACCEPT, "application/vnd.kagi.stream")
        .header(header::CACHE_CONTROL, "no-store")
        .send()
        .await
        .map_err(map_transport_error)?;

    match response.status() {
        StatusCode::OK => {
            let body = response.text().await.map_err(|error| {
                KagiError::Network(format!(
                    "failed to read subscriber summarizer response body: {error}"
                ))
            })?;

            if looks_like_html_document(&body) {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }

            parse_subscriber_summarize_stream(&body)
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(KagiError::Auth(
            "invalid or expired Kagi session token".to_string(),
        )),
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi subscriber summarizer server error: HTTP {status}"
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi subscriber summarizer response status: HTTP {status}"
        ))),
    }
}

pub async fn execute_news(
    category: &str,
    limit: u32,
    lang: &str,
) -> Result<NewsStoriesResponse, KagiError> {
    if limit == 0 {
        return Err(KagiError::Config(
            "news --limit must be greater than 0".to_string(),
        ));
    }

    let client = build_client()?;
    let normalized_lang = normalize_news_lang(lang);
    let latest_batch: NewsLatestBatch = decode_kagi_free_json(
        client
            .get(KAGI_NEWS_LATEST_URL)
            .query(&[("lang", normalized_lang.as_str())])
            .send()
            .await
            .map_err(map_transport_error)?,
        "news latest batch",
    )
    .await?;
    let metadata: NewsCategoryMetadataList = decode_kagi_free_json(
        client
            .get(KAGI_NEWS_CATEGORIES_METADATA_URL)
            .send()
            .await
            .map_err(map_transport_error)?,
        "news category metadata",
    )
    .await?;
    let batch_categories: NewsBatchCategories = decode_kagi_free_json(
        client
            .get(format!(
                "{}/{}/categories",
                KAGI_NEWS_BATCH_CATEGORIES_URL, latest_batch.id
            ))
            .query(&[("lang", normalized_lang.as_str())])
            .send()
            .await
            .map_err(map_transport_error)?,
        "news batch categories",
    )
    .await?;
    let category =
        resolve_news_category(&batch_categories.categories, &metadata.categories, category)?;
    let payload: NewsStoriesPayload = decode_kagi_free_json(
        client
            .get(format!(
                "{}/{}/categories/{}/stories",
                KAGI_NEWS_BATCH_CATEGORIES_URL, latest_batch.id, category.id
            ))
            .query(&[
                ("limit", limit.to_string()),
                ("lang", normalized_lang.clone()),
            ])
            .send()
            .await
            .map_err(map_transport_error)?,
        "news stories",
    )
    .await?;

    Ok(NewsStoriesResponse {
        latest_batch,
        category,
        stories: payload.stories,
        total_stories: payload.total_stories,
        domains: payload.domains,
        read_count: payload.read_count,
    })
}

pub async fn execute_news_categories(lang: &str) -> Result<NewsCategoriesResponse, KagiError> {
    let client = build_client()?;
    let normalized_lang = normalize_news_lang(lang);
    let latest_batch: NewsLatestBatch = decode_kagi_free_json(
        client
            .get(KAGI_NEWS_LATEST_URL)
            .query(&[("lang", normalized_lang.as_str())])
            .send()
            .await
            .map_err(map_transport_error)?,
        "news latest batch",
    )
    .await?;
    let metadata: NewsCategoryMetadataList = decode_kagi_free_json(
        client
            .get(KAGI_NEWS_CATEGORIES_METADATA_URL)
            .send()
            .await
            .map_err(map_transport_error)?,
        "news category metadata",
    )
    .await?;
    let batch_categories: NewsBatchCategories = decode_kagi_free_json(
        client
            .get(format!(
                "{}/{}/categories",
                KAGI_NEWS_BATCH_CATEGORIES_URL, latest_batch.id
            ))
            .query(&[("lang", normalized_lang.as_str())])
            .send()
            .await
            .map_err(map_transport_error)?,
        "news batch categories",
    )
    .await?;
    let metadata_map = metadata
        .categories
        .into_iter()
        .map(|entry| (entry.category_id.clone(), entry))
        .collect::<HashMap<_, _>>();
    let categories = batch_categories
        .categories
        .into_iter()
        .map(|category| {
            let metadata = metadata_map.get(&category.category_id).cloned();
            merge_news_category(category, metadata)
        })
        .collect();

    Ok(NewsCategoriesResponse {
        latest_batch,
        categories,
    })
}

pub async fn execute_news_chaos(lang: &str) -> Result<NewsChaosResponse, KagiError> {
    let client = build_client()?;
    let normalized_lang = normalize_news_lang(lang);
    let latest_batch: NewsLatestBatch = decode_kagi_free_json(
        client
            .get(KAGI_NEWS_LATEST_URL)
            .query(&[("lang", normalized_lang.as_str())])
            .send()
            .await
            .map_err(map_transport_error)?,
        "news latest batch",
    )
    .await?;
    let chaos: NewsChaos = decode_kagi_free_json(
        client
            .get(format!(
                "{}/{}/chaos",
                KAGI_NEWS_BATCH_CATEGORIES_URL, latest_batch.id
            ))
            .query(&[("lang", normalized_lang.as_str())])
            .send()
            .await
            .map_err(map_transport_error)?,
        "news chaos",
    )
    .await?;

    Ok(NewsChaosResponse {
        latest_batch,
        chaos,
    })
}

pub async fn execute_assistant_prompt(
    request: &AssistantPromptRequest,
    token: &str,
) -> Result<AssistantPromptResponse, KagiError> {
    let query = normalize_assistant_query(&request.query)?;
    let thread_id = normalize_assistant_thread_id(request.thread_id.as_deref())?;
    let profile = assistant_profile_payload(request);
    let body = execute_assistant_stream(
        KAGI_ASSISTANT_PROMPT_URL,
        &json!({
            "focus": {
                "thread_id": thread_id,
                "branch_id": ASSISTANT_ZERO_BRANCH_UUID,
                "prompt": query,
                "message_id": Value::Null,
            },
            "profile": profile,
        }),
        token,
        "Assistant prompt",
    )
    .await?;

    parse_assistant_prompt_stream(&body)
}

pub async fn execute_assistant_thread_list(
    token: &str,
) -> Result<AssistantThreadListResponse, KagiError> {
    let body = execute_assistant_stream(
        KAGI_ASSISTANT_THREAD_LIST_URL,
        &json!({ "limit": 100 }),
        token,
        "Assistant thread list",
    )
    .await?;

    parse_assistant_thread_list_stream(&body)
}

pub async fn execute_assistant_thread_get(
    thread_id: &str,
    token: &str,
) -> Result<AssistantThreadOpenResponse, KagiError> {
    let thread_id = normalize_assistant_thread_id(Some(thread_id))?
        .ok_or_else(|| KagiError::Config("assistant thread id cannot be empty".to_string()))?;
    let body = execute_assistant_stream(
        KAGI_ASSISTANT_THREAD_OPEN_URL,
        &json!({
            "focus": {
                "thread_id": thread_id,
                "branch_id": ASSISTANT_ZERO_BRANCH_UUID,
            }
        }),
        token,
        "Assistant thread open",
    )
    .await?;

    parse_assistant_thread_open_stream(&body)
}

pub async fn execute_assistant_thread_delete(
    thread_id: &str,
    token: &str,
) -> Result<AssistantThreadDeleteResponse, KagiError> {
    let thread = execute_assistant_thread_get(thread_id, token).await?.thread;
    let body = execute_assistant_stream(
        KAGI_ASSISTANT_THREAD_DELETE_URL,
        &json!({
            "threads": [{
                "id": thread.id,
                "title": thread.title,
                "saved": thread.saved,
                "shared": thread.shared,
                "tag_ids": thread.tag_ids,
            }]
        }),
        token,
        "Assistant thread delete",
    )
    .await?;

    parse_assistant_thread_delete_stream(&body, thread_id)
}

pub async fn execute_assistant_thread_export(
    thread_id: &str,
    token: &str,
) -> Result<AssistantThreadExportResponse, KagiError> {
    let thread_id = normalize_assistant_thread_id(Some(thread_id))?
        .ok_or_else(|| KagiError::Config("assistant thread id cannot be empty".to_string()))?;
    let client = build_client()?;
    let response = client
        .get(format!("https://kagi.com/assistant/{thread_id}/download"))
        .header(header::COOKIE, format!("kagi_session={token}"))
        .send()
        .await
        .map_err(map_transport_error)?;

    match response.status() {
        StatusCode::OK => {
            let filename = response
                .headers()
                .get(header::CONTENT_DISPOSITION)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_content_disposition_filename);
            let markdown = response.text().await.map_err(|error| {
                KagiError::Network(format!("failed to read Assistant export body: {error}"))
            })?;
            if looks_like_html_document(&markdown) {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }
            Ok(AssistantThreadExportResponse {
                thread_id,
                filename,
                markdown,
            })
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(KagiError::Auth(
            "invalid or expired Kagi session token".to_string(),
        )),
        status if status.is_client_error() => {
            let body = response.text().await.unwrap_or_else(|_| String::new());
            Err(KagiError::Config(format!(
                "Kagi Assistant export request rejected: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi Assistant export server error: HTTP {status}"
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi Assistant export response status: HTTP {status}"
        ))),
    }
}

pub async fn execute_fastgpt(
    request: &FastGptRequest,
    token: &str,
) -> Result<FastGptResponse, KagiError> {
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi API token (expected KAGI_API_TOKEN)".to_string(),
        ));
    }

    let client = build_client()?;
    let response = client
        .post(KAGI_FASTGPT_URL)
        .header(header::AUTHORIZATION, format!("Bot {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .json(request)
        .send()
        .await
        .map_err(map_transport_error)?;

    decode_kagi_json(response, "FastGPT").await
}

pub async fn execute_enrich_web(query: &str, token: &str) -> Result<EnrichResponse, KagiError> {
    execute_enrich(KAGI_ENRICH_WEB_URL, query, token, "web enrichment").await
}

pub async fn execute_enrich_news(query: &str, token: &str) -> Result<EnrichResponse, KagiError> {
    execute_enrich(KAGI_ENRICH_NEWS_URL, query, token, "news enrichment").await
}

pub async fn execute_smallweb(limit: Option<u32>) -> Result<SmallWebFeed, KagiError> {
    let client = build_client()?;
    let mut request = client.get(KAGI_SMALLWEB_FEED_URL);
    if let Some(limit) = limit {
        request = request.query(&[("limit", limit)]);
    }

    let response = request.send().await.map_err(map_transport_error)?;
    match response.status() {
        StatusCode::OK => response
            .text()
            .await
            .map(|xml| SmallWebFeed { xml })
            .map_err(|error| {
                KagiError::Network(format!("failed to read Small Web feed body: {error}"))
            }),
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi Small Web feed server error: HTTP {status}"
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi Small Web feed status: HTTP {status}"
        ))),
    }
}

async fn execute_enrich(
    url: &str,
    query: &str,
    token: &str,
    surface: &str,
) -> Result<EnrichResponse, KagiError> {
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi API token (expected KAGI_API_TOKEN)".to_string(),
        ));
    }

    let client = build_client()?;
    let response = client
        .get(url)
        .header(header::AUTHORIZATION, format!("Bot {token}"))
        .query(&[("q", query)])
        .send()
        .await
        .map_err(map_transport_error)?;

    decode_kagi_json(response, surface).await
}

fn normalize_subscriber_summary_input(
    request: &SubscriberSummarizeRequest,
) -> Result<(&'static str, String), KagiError> {
    match (request.url.as_deref(), request.text.as_deref()) {
        (Some(url), None) => {
            let normalized = url.trim();
            if normalized.is_empty() {
                return Err(KagiError::Config(
                    "subscriber summarize URL cannot be empty".to_string(),
                ));
            }
            Ok(("url", normalized.to_string()))
        }
        (None, Some(text)) => {
            let normalized = text.trim();
            if normalized.is_empty() {
                return Err(KagiError::Config(
                    "subscriber summarize text cannot be empty".to_string(),
                ));
            }
            Ok(("text", normalized.to_string()))
        }
        _ => Err(KagiError::Config(
            "subscriber summarize requires exactly one of --url or --text".to_string(),
        )),
    }
}

fn normalize_subscriber_summary_type(raw: Option<&str>) -> Result<String, KagiError> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("summary") => Ok("article".to_string()),
        Some("keypoints") => Ok("keypoints".to_string()),
        Some("eli5") => Ok("eli5".to_string()),
        Some(value) => Err(KagiError::Config(format!(
            "subscriber summarize --summary-type must be one of: summary, keypoints, eli5; got '{value}'"
        ))),
    }
}

fn normalize_subscriber_summary_length(raw: Option<&str>) -> Result<String, KagiError> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok("medium".to_string()),
        Some("headline") => Ok("headline".to_string()),
        Some("overview") => Ok("overview".to_string()),
        Some("digest") => Ok("digest".to_string()),
        Some("medium") => Ok("medium".to_string()),
        Some("long") => Ok("long".to_string()),
        Some(value) => Err(KagiError::Config(format!(
            "subscriber summarize --length must be one of: headline, overview, digest, medium, long; got '{value}'"
        ))),
    }
}

fn looks_like_html_document(body: &str) -> bool {
    body.contains("<!DOCTYPE html") || body.contains("<html")
}

fn parse_subscriber_summarize_stream(body: &str) -> Result<SubscriberSummarizeResponse, KagiError> {
    let mut meta = SubscriberSummarizeMeta::default();
    let mut last_message: Option<SubscriberSummaryStreamMessage> = None;

    for frame in body.split("\0\n").filter(|frame| !frame.trim().is_empty()) {
        let Some((tag, payload)) = frame.split_once(':') else {
            continue;
        };

        match tag {
            "hi" => {
                let hello: SubscriberSummaryHello =
                    serde_json::from_str(payload).map_err(|error| {
                        KagiError::Parse(format!(
                            "failed to parse subscriber summarizer hello frame: {error}"
                        ))
                    })?;
                meta.version = hello.v;
                meta.trace = hello.trace;
            }
            "new_message.json" => {
                let message: SubscriberSummaryStreamMessage = serde_json::from_str(payload)
                    .map_err(|error| {
                        KagiError::Parse(format!(
                            "failed to parse subscriber summarizer message frame: {error}"
                        ))
                    })?;
                last_message = Some(message);
            }
            _ => {}
        }
    }

    let message = last_message.ok_or_else(|| {
        KagiError::Parse(
            "subscriber summarizer response did not include a new_message.json frame".to_string(),
        )
    })?;

    if message.state == "error" {
        let detail = if message.reply.trim().is_empty() {
            "Kagi subscriber summarizer returned an error state".to_string()
        } else {
            format!(
                "Kagi subscriber summarizer failed: {}",
                message.reply.trim()
            )
        };
        return Err(KagiError::Network(detail));
    }

    Ok(SubscriberSummarizeResponse {
        meta,
        data: SubscriberSummarization {
            id: message.id,
            thread_id: message.thread_id,
            created_at: message.created_at,
            state: message.state,
            prompt: message.prompt,
            output: message.reply,
            markdown: message.md,
            metadata_html: message.metadata,
            documents: message.documents,
        },
    })
}

fn normalize_news_lang(raw: &str) -> String {
    let normalized = raw.trim();
    if normalized.is_empty() {
        "default".to_string()
    } else {
        normalized.to_string()
    }
}

fn merge_news_category(
    category: NewsBatchCategory,
    metadata: Option<NewsCategoryMetadata>,
) -> NewsResolvedCategory {
    NewsResolvedCategory {
        id: category.id,
        category_id: category.category_id,
        category_name: category.category_name,
        source_language: category.source_language,
        timestamp: category.timestamp,
        read_count: category.read_count,
        cluster_count: category.cluster_count,
        metadata,
    }
}

fn resolve_news_category(
    batch_categories: &[NewsBatchCategory],
    metadata: &[NewsCategoryMetadata],
    requested_category: &str,
) -> Result<NewsResolvedCategory, KagiError> {
    let requested = requested_category.trim();
    if requested.is_empty() {
        return Err(KagiError::Config(
            "news category cannot be empty".to_string(),
        ));
    }

    let metadata_map = metadata
        .iter()
        .cloned()
        .map(|entry| (entry.category_id.clone(), entry))
        .collect::<HashMap<_, _>>();
    if let Some(category) = batch_categories.iter().find(|category| {
        category.category_id.eq_ignore_ascii_case(requested)
            || category.category_name.eq_ignore_ascii_case(requested)
            || metadata_map
                .get(&category.category_id)
                .map(|entry| entry.display_name.eq_ignore_ascii_case(requested))
                .unwrap_or(false)
    }) {
        return Ok(merge_news_category(
            category.clone(),
            metadata_map.get(&category.category_id).cloned(),
        ));
    }

    Err(KagiError::Config(format!(
        "unknown news category '{requested}'. Run `kagi news --list-categories` to inspect current categories."
    )))
}

fn normalize_assistant_query(raw: &str) -> Result<String, KagiError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(KagiError::Config(
            "assistant query cannot be empty".to_string(),
        ));
    }

    Ok(normalized.to_string())
}

fn normalize_assistant_thread_id(raw: Option<&str>) -> Result<Option<String>, KagiError> {
    match raw {
        None => Ok(None),
        Some(value) => {
            let normalized = value.trim();
            if normalized.is_empty() {
                return Err(KagiError::Config(
                    "assistant thread id cannot be empty".to_string(),
                ));
            }
            Ok(Some(normalized.to_string()))
        }
    }
}

fn assistant_profile_payload(request: &AssistantPromptRequest) -> Value {
    let mut payload = serde_json::Map::new();

    if let Some(model) = request
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("model".to_string(), Value::String(model.to_string()));
    }

    if let Some(lens_id) = request.lens_id {
        payload.insert("lens_id".to_string(), json!(lens_id));
    }

    if let Some(internet_access) = request.internet_access {
        payload.insert("internet_access".to_string(), Value::Bool(internet_access));
    }

    if let Some(personalizations) = request.personalizations {
        payload.insert(
            "personalizations".to_string(),
            Value::Bool(personalizations),
        );
    }

    Value::Object(payload)
}

async fn execute_assistant_stream(
    url: &str,
    payload: &Value,
    token: &str,
    surface: &str,
) -> Result<String, KagiError> {
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi session token (expected KAGI_SESSION_TOKEN)".to_string(),
        ));
    }

    let client = build_client()?;
    let response = client
        .post(url)
        .header(header::COOKIE, format!("kagi_session={token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/vnd.kagi.stream")
        .json(payload)
        .send()
        .await
        .map_err(map_transport_error)?;

    match response.status() {
        StatusCode::OK => {
            let body = response.text().await.map_err(|error| {
                KagiError::Network(format!("failed to read {surface} response body: {error}"))
            })?;

            if looks_like_html_document(&body) {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }

            Ok(body)
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(KagiError::Auth(
            "invalid or expired Kagi session token".to_string(),
        )),
        status if status.is_client_error() => {
            let body = response.text().await.unwrap_or_else(|_| String::new());
            Err(KagiError::Config(format!(
                "Kagi {surface} request rejected: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi {surface} server error: HTTP {status}{}",
            {
                let body = response.text().await.unwrap_or_else(|_| String::new());
                if body.trim().is_empty() {
                    String::new()
                } else if looks_like_html_document(&body) {
                    let stripped = strip_html_to_text(&body);
                    let normalized_surface = surface.to_ascii_lowercase();
                    if normalized_surface.contains("thread") {
                        "; the thread id may be invalid or no longer available".to_string()
                    } else if stripped.is_empty() {
                        String::new()
                    } else {
                        format!("; {stripped}")
                    }
                } else {
                    format_client_error_suffix(&body)
                }
            }
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi {surface} response status: HTTP {status}"
        ))),
    }
}

fn parse_assistant_prompt_stream(body: &str) -> Result<AssistantPromptResponse, KagiError> {
    let mut meta = AssistantMeta::default();
    let mut thread = None;
    let mut message = None;

    for frame in body.split("\0\n").filter(|frame| !frame.trim().is_empty()) {
        let Some((tag, payload)) = frame.split_once(':') else {
            continue;
        };

        match tag {
            "hi" => {
                let hello: AssistantHello = serde_json::from_str(payload).map_err(|error| {
                    KagiError::Parse(format!("failed to parse assistant hello frame: {error}"))
                })?;
                meta.version = hello.v;
                meta.trace = hello.trace;
            }
            "thread.json" => {
                let payload: AssistantThreadPayload =
                    serde_json::from_str(payload).map_err(|error| {
                        KagiError::Parse(format!("failed to parse assistant thread frame: {error}"))
                    })?;
                thread = Some(AssistantThread::from(payload));
            }
            "new_message.json" => {
                let payload: AssistantMessagePayload =
                    serde_json::from_str(payload).map_err(|error| {
                        KagiError::Parse(format!(
                            "failed to parse assistant message frame: {error}"
                        ))
                    })?;
                message = Some(assistant_message_from_payload(payload));
            }
            "limit_notice.html" => {
                let detail = strip_html_to_text(payload);
                return Err(KagiError::Config(if detail.is_empty() {
                    "Kagi Assistant rate limited this request".to_string()
                } else {
                    detail
                }));
            }
            "unauthorized" => {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }
            _ => {}
        }
    }

    let thread = thread.ok_or_else(|| {
        KagiError::Parse("assistant response did not include a thread.json frame".to_string())
    })?;
    let message = message.ok_or_else(|| {
        KagiError::Parse("assistant response did not include a new_message.json frame".to_string())
    })?;

    if message.state == "error" {
        return Err(KagiError::Network(
            message
                .markdown
                .as_deref()
                .or(message.reply_html.as_deref())
                .unwrap_or("Kagi Assistant returned an error state")
                .to_string(),
        ));
    }

    Ok(AssistantPromptResponse {
        meta,
        thread,
        message,
    })
}

fn parse_assistant_thread_open_stream(
    body: &str,
) -> Result<AssistantThreadOpenResponse, KagiError> {
    let mut meta = AssistantMeta::default();
    let mut tags = Vec::new();
    let mut thread = None;
    let mut messages = None;

    for frame in body.split("\0\n").filter(|frame| !frame.trim().is_empty()) {
        let Some((tag, payload)) = frame.split_once(':') else {
            continue;
        };

        match tag {
            "hi" => {
                let hello: AssistantHello = serde_json::from_str(payload).map_err(|error| {
                    KagiError::Parse(format!("failed to parse assistant hello frame: {error}"))
                })?;
                meta.version = hello.v;
                meta.trace = hello.trace;
            }
            "tags.json" => {
                tags = serde_json::from_str(payload).map_err(|error| {
                    KagiError::Parse(format!("failed to parse assistant tags frame: {error}"))
                })?;
            }
            "thread.json" => {
                let payload: AssistantThreadPayload =
                    serde_json::from_str(payload).map_err(|error| {
                        KagiError::Parse(format!("failed to parse assistant thread frame: {error}"))
                    })?;
                thread = Some(AssistantThread::from(payload));
            }
            "messages.json" => {
                let payloads: Vec<AssistantMessagePayload> = serde_json::from_str(payload)
                    .map_err(|error| {
                        KagiError::Parse(format!(
                            "failed to parse assistant messages frame: {error}"
                        ))
                    })?;
                messages = Some(
                    payloads
                        .into_iter()
                        .map(assistant_message_from_payload)
                        .collect(),
                );
            }
            "limit_notice.html" => {
                let detail = strip_html_to_text(payload);
                return Err(KagiError::Config(if detail.is_empty() {
                    "Kagi Assistant rate limited this request".to_string()
                } else {
                    detail
                }));
            }
            "unauthorized" => {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }
            _ => {}
        }
    }

    Ok(AssistantThreadOpenResponse {
        meta,
        tags,
        thread: thread.ok_or_else(|| {
            KagiError::Parse(
                "assistant thread open response did not include a thread.json frame".to_string(),
            )
        })?,
        messages: messages.ok_or_else(|| {
            KagiError::Parse(
                "assistant thread open response did not include a messages.json frame".to_string(),
            )
        })?,
    })
}

fn parse_assistant_thread_list_stream(
    body: &str,
) -> Result<AssistantThreadListResponse, KagiError> {
    let mut meta = AssistantMeta::default();
    let mut tags = Vec::new();
    let mut threads = Vec::new();
    let mut pagination = None;

    for frame in body.split("\0\n").filter(|frame| !frame.trim().is_empty()) {
        let Some((tag, payload)) = frame.split_once(':') else {
            continue;
        };

        match tag {
            "hi" => {
                let hello: AssistantHello = serde_json::from_str(payload).map_err(|error| {
                    KagiError::Parse(format!("failed to parse assistant hello frame: {error}"))
                })?;
                meta.version = hello.v;
                meta.trace = hello.trace;
            }
            "tags.json" => {
                tags = serde_json::from_str(payload).map_err(|error| {
                    KagiError::Parse(format!("failed to parse assistant tags frame: {error}"))
                })?;
            }
            "thread_list.html" => {
                let payload: AssistantThreadListPayload =
                    serde_json::from_str(payload).map_err(|error| {
                        KagiError::Parse(format!(
                            "failed to parse assistant thread list frame: {error}"
                        ))
                    })?;
                threads = parse_assistant_thread_list(&payload.html)?;
                pagination = Some(AssistantThreadPagination {
                    next_cursor: payload.next_cursor,
                    has_more: payload.has_more,
                    count: payload.count,
                    total_counts: payload.total_counts,
                });
            }
            "limit_notice.html" => {
                let detail = strip_html_to_text(payload);
                return Err(KagiError::Config(if detail.is_empty() {
                    "Kagi Assistant rate limited this request".to_string()
                } else {
                    detail
                }));
            }
            "unauthorized" => {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }
            _ => {}
        }
    }

    Ok(AssistantThreadListResponse {
        meta,
        tags,
        threads,
        pagination: pagination.ok_or_else(|| {
            KagiError::Parse(
                "assistant thread list response did not include a thread_list.html frame"
                    .to_string(),
            )
        })?,
    })
}

fn parse_assistant_thread_delete_stream(
    body: &str,
    thread_id: &str,
) -> Result<AssistantThreadDeleteResponse, KagiError> {
    for frame in body.split("\0\n").filter(|frame| !frame.trim().is_empty()) {
        let Some((tag, payload)) = frame.split_once(':') else {
            continue;
        };

        match tag {
            "ok" => {
                let value: Option<Value> = serde_json::from_str(payload).map_err(|error| {
                    KagiError::Parse(format!("failed to parse assistant delete frame: {error}"))
                })?;
                if value.is_none() {
                    return Ok(AssistantThreadDeleteResponse {
                        deleted_thread_ids: vec![thread_id.to_string()],
                    });
                }
            }
            "limit_notice.html" => {
                let detail = strip_html_to_text(payload);
                return Err(KagiError::Config(if detail.is_empty() {
                    "Kagi Assistant rate limited this request".to_string()
                } else {
                    detail
                }));
            }
            "unauthorized" => {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }
            _ => {}
        }
    }

    Err(KagiError::Parse(
        "assistant thread delete response did not include an ok frame".to_string(),
    ))
}

fn assistant_message_from_payload(payload: AssistantMessagePayload) -> AssistantMessage {
    AssistantMessage {
        id: payload.id,
        thread_id: payload.thread_id,
        created_at: payload.created_at,
        branch_list: payload.branch_list,
        state: payload.state,
        prompt: payload.prompt,
        reply_html: payload.reply,
        markdown: payload.md,
        references_html: payload.references_html,
        references_markdown: payload.references_md,
        metadata_html: payload.metadata,
        documents: payload.documents,
        profile: payload.profile,
        trace_id: payload.trace_id,
    }
}

fn strip_html_to_text(html: &str) -> String {
    Html::parse_fragment(html)
        .root_element()
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_content_disposition_filename(header_value: &str) -> Option<String> {
    for segment in header_value.split(';').map(str::trim) {
        if let Some(encoded) = segment.strip_prefix("filename*=utf-8''") {
            let decoded = Url::parse(&format!("https://example.com/?filename={encoded}"))
                .ok()?
                .query_pairs()
                .find_map(|(key, value)| (key == "filename").then(|| value.into_owned()))?;
            return Some(decoded);
        }

        if let Some(raw) = segment.strip_prefix("filename=") {
            return Some(raw.trim_matches('"').to_string());
        }
    }

    None
}

fn format_client_error_suffix(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Ok(payload) = serde_json::from_str::<Value>(trimmed) {
        return format!("; {}", payload);
    }

    format!("; {trimmed}")
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    #[allow(dead_code)]
    meta: Option<serde_json::Value>,
    error: Option<Vec<ApiErrorItem>>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorItem {
    msg: String,
}

#[derive(Debug, Deserialize)]
struct SubscriberSummaryHello {
    v: Option<String>,
    trace: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SubscriberSummaryStreamMessage {
    id: String,
    thread_id: String,
    created_at: String,
    state: String,
    prompt: String,
    reply: String,
    #[serde(default)]
    md: String,
    #[serde(default)]
    metadata: String,
    #[serde(default)]
    documents: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct AssistantHello {
    v: Option<String>,
    trace: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssistantThreadPayload {
    id: String,
    title: String,
    ack: String,
    created_at: String,
    expires_at: String,
    saved: bool,
    shared: bool,
    branch_id: String,
    #[serde(default)]
    tag_ids: Vec<String>,
}

impl From<AssistantThreadPayload> for AssistantThread {
    fn from(payload: AssistantThreadPayload) -> Self {
        Self {
            id: payload.id,
            title: payload.title,
            ack: payload.ack,
            created_at: payload.created_at,
            expires_at: payload.expires_at,
            saved: payload.saved,
            shared: payload.shared,
            branch_id: payload.branch_id,
            tag_ids: payload.tag_ids,
        }
    }
}

#[derive(Debug, Deserialize)]
struct AssistantMessagePayload {
    id: String,
    thread_id: String,
    created_at: String,
    #[serde(default)]
    branch_list: Vec<String>,
    state: String,
    prompt: String,
    #[serde(default)]
    reply: Option<String>,
    #[serde(default)]
    md: Option<String>,
    #[serde(default)]
    references_html: Option<String>,
    #[serde(default)]
    references_md: Option<String>,
    #[serde(default)]
    metadata: Option<String>,
    #[serde(default)]
    documents: Vec<Value>,
    #[serde(default)]
    profile: Option<Value>,
    #[serde(default)]
    trace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssistantThreadListPayload {
    html: String,
    #[serde(default)]
    next_cursor: Option<String>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    count: u64,
    #[serde(default)]
    total_counts: HashMap<String, u64>,
}

async fn decode_kagi_json<T>(response: reqwest::Response, surface: &str) -> Result<T, KagiError>
where
    T: DeserializeOwned,
{
    match response.status() {
        StatusCode::OK => {
            let body = response.text().await.map_err(|error| {
                KagiError::Network(format!("failed to read {surface} response body: {error}"))
            })?;
            serde_json::from_str(&body).map_err(|error| {
                KagiError::Parse(format!("failed to parse {surface} response: {error}"))
            })
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(KagiError::Auth(format!(
            "invalid Kagi API token or access is not enabled for {surface}"
        ))),
        status if status.is_client_error() => {
            let body = response.text().await.unwrap_or_else(|_| String::new());
            let parsed_error = serde_json::from_str::<ApiErrorBody>(&body)
                .ok()
                .and_then(|payload| payload.error)
                .and_then(|errors| errors.into_iter().next())
                .map(|error| error.msg);
            Err(KagiError::Auth(format!(
                "Kagi {surface} request rejected: HTTP {status}{}",
                match parsed_error {
                    Some(message) => format!("; {message}"),
                    None if body.trim().is_empty() => String::new(),
                    None => format!("; {body}"),
                }
            )))
        }
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi {surface} server error: HTTP {status}"
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi {surface} response status: HTTP {status}"
        ))),
    }
}

async fn decode_kagi_free_json<T>(
    response: reqwest::Response,
    surface: &str,
) -> Result<T, KagiError>
where
    T: DeserializeOwned,
{
    match response.status() {
        StatusCode::OK => {
            let body = response.text().await.map_err(|error| {
                KagiError::Network(format!("failed to read {surface} response body: {error}"))
            })?;
            serde_json::from_str(&body).map_err(|error| {
                KagiError::Parse(format!("failed to parse {surface} response: {error}"))
            })
        }
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi {surface} server error: HTTP {status}"
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi {surface} response status: HTTP {status}"
        ))),
    }
}

fn build_client() -> Result<Client, KagiError> {
    Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
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

#[cfg(test)]
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct KagiEnvelope<T> {
    pub meta: ApiMeta,
    pub data: T,
}

#[cfg(test)]
mod tests {
    use super::{
        ApiErrorBody, KagiEnvelope, normalize_assistant_query, normalize_assistant_thread_id,
        normalize_subscriber_summary_input, normalize_subscriber_summary_length,
        normalize_subscriber_summary_type, parse_assistant_prompt_stream,
        parse_assistant_thread_delete_stream, parse_assistant_thread_list_stream,
        parse_assistant_thread_open_stream, parse_content_disposition_filename,
        parse_subscriber_summarize_stream, resolve_news_category,
    };
    use crate::api::{
        execute_assistant_prompt, execute_assistant_thread_delete, execute_assistant_thread_export,
        execute_assistant_thread_get, execute_assistant_thread_list,
    };
    use crate::auth::{SESSION_TOKEN_ENV, load_credential_inventory};
    use crate::types::SubscriberSummarizeRequest;
    use crate::types::{
        AssistantPromptRequest, FastGptAnswer, NewsBatchCategory, NewsCategoryMetadata, Reference,
        Summarization,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_summarize_envelope() {
        let raw = r#"{
            "meta": { "id": "1", "node": "us-east", "ms": 10 },
            "data": { "output": "summary", "tokens": 42 }
        }"#;
        let parsed: KagiEnvelope<Summarization> =
            serde_json::from_str(raw).expect("summarize envelope parses");
        assert_eq!(parsed.data.output, "summary");
        assert_eq!(parsed.data.tokens, 42);
    }

    #[test]
    fn parses_fastgpt_envelope() {
        let raw = r#"{
            "meta": { "id": "1", "node": "us-east", "ms": 10 },
            "data": {
                "output": "answer",
                "tokens": 12,
                "references": [{ "title": "Doc", "snippet": "...", "url": "https://example.com" }]
            }
        }"#;
        let parsed: KagiEnvelope<FastGptAnswer> =
            serde_json::from_str(raw).expect("fastgpt envelope parses");
        assert_eq!(parsed.data.output, "answer");
        assert_eq!(
            parsed.data.references,
            vec![Reference {
                title: "Doc".to_string(),
                snippet: "...".to_string(),
                url: "https://example.com".to_string(),
            }]
        );
    }

    #[test]
    fn parses_api_error_message() {
        let raw = r#"{
            "meta": { "id": "1" },
            "data": null,
            "error": [{ "code": 101, "msg": "Insufficient credit to perform this request.", "ref": null }]
        }"#;
        let parsed: ApiErrorBody = serde_json::from_str(raw).expect("api error parses");
        let message = parsed
            .error
            .expect("error list present")
            .into_iter()
            .next()
            .expect("first error")
            .msg;
        assert_eq!(message, "Insufficient credit to perform this request.");
    }

    #[test]
    fn normalizes_subscriber_summary_type_values() {
        assert_eq!(
            normalize_subscriber_summary_type(None).expect("default type"),
            "article"
        );
        assert_eq!(
            normalize_subscriber_summary_type(Some("summary")).expect("summary type"),
            "article"
        );
        assert_eq!(
            normalize_subscriber_summary_type(Some("keypoints")).expect("keypoints type"),
            "keypoints"
        );
        assert_eq!(
            normalize_subscriber_summary_type(Some("eli5")).expect("eli5 type"),
            "eli5"
        );
    }

    #[test]
    fn rejects_invalid_subscriber_summary_type() {
        let error = normalize_subscriber_summary_type(Some("takeaway"))
            .expect_err("invalid subscriber type should fail");
        assert!(error.to_string().contains("summary, keypoints, eli5"));
    }

    #[test]
    fn normalizes_subscriber_summary_length_values() {
        assert_eq!(
            normalize_subscriber_summary_length(None).expect("default length"),
            "medium"
        );
        assert_eq!(
            normalize_subscriber_summary_length(Some("digest")).expect("digest length"),
            "digest"
        );
    }

    #[test]
    fn rejects_invalid_subscriber_summary_length() {
        let error = normalize_subscriber_summary_length(Some("short"))
            .expect_err("invalid subscriber length should fail");
        assert!(
            error
                .to_string()
                .contains("headline, overview, digest, medium, long")
        );
    }

    #[test]
    fn normalizes_subscriber_summary_input() {
        let url_request = SubscriberSummarizeRequest {
            url: Some("https://example.com".to_string()),
            text: None,
            summary_type: None,
            target_language: None,
            length: None,
        };
        let text_request = SubscriberSummarizeRequest {
            url: None,
            text: Some("hello world".to_string()),
            summary_type: None,
            target_language: None,
            length: None,
        };

        assert_eq!(
            normalize_subscriber_summary_input(&url_request).expect("url input"),
            ("url", "https://example.com".to_string())
        );
        assert_eq!(
            normalize_subscriber_summary_input(&text_request).expect("text input"),
            ("text", "hello world".to_string())
        );
    }

    #[test]
    fn rejects_invalid_subscriber_summary_input_shape() {
        let request = SubscriberSummarizeRequest {
            url: Some("https://example.com".to_string()),
            text: Some("hello world".to_string()),
            summary_type: None,
            target_language: None,
            length: None,
        };

        let error =
            normalize_subscriber_summary_input(&request).expect_err("mixed input should fail");
        assert!(error.to_string().contains("exactly one of --url or --text"));
    }

    #[test]
    fn parses_subscriber_summarize_stream() {
        let raw = "hi:{\"v\":\"202603091651.stage.c128588\",\"trace\":\"abc123\"}\0\nnew_message.json:{\"id\":\"msg-1\",\"thread_id\":\"thread-1\",\"created_at\":\"2026-03-16T05:17:57Z\",\"state\":\"done\",\"prompt\":\"hello\",\"reply\":\"summary output\",\"md\":\"summary output\",\"metadata\":\"<li>meta</li>\",\"documents\":[{\"url\":\"https://example.com\"}]}\0\n";

        let parsed = parse_subscriber_summarize_stream(raw).expect("stream parses");
        assert_eq!(
            parsed.meta.version.as_deref(),
            Some("202603091651.stage.c128588")
        );
        assert_eq!(parsed.meta.trace.as_deref(), Some("abc123"));
        assert_eq!(parsed.data.thread_id, "thread-1");
        assert_eq!(parsed.data.output, "summary output");
        assert_eq!(parsed.data.documents.len(), 1);
    }

    #[test]
    fn rejects_error_state_in_subscriber_summarize_stream() {
        let raw = "new_message.json:{\"id\":\"msg-1\",\"thread_id\":\"thread-1\",\"created_at\":\"2026-03-16T05:17:57Z\",\"state\":\"error\",\"prompt\":\"hello\",\"reply\":\"We are sorry, we are not able to extract the source.\",\"md\":\"\",\"metadata\":\"\",\"documents\":[]}\0\n";

        let error = parse_subscriber_summarize_stream(raw).expect_err("error state should fail");
        assert!(
            error
                .to_string()
                .contains("We are sorry, we are not able to extract the source.")
        );
    }

    #[test]
    fn resolves_news_category_by_display_name() {
        let batch_categories = vec![NewsBatchCategory {
            id: "batch-world".to_string(),
            category_id: "world".to_string(),
            category_name: "World".to_string(),
            source_language: "en".to_string(),
            timestamp: 1,
            read_count: 2,
            cluster_count: 3,
        }];
        let metadata = vec![NewsCategoryMetadata {
            category_id: "world".to_string(),
            category_type: "core".to_string(),
            display_name: "World".to_string(),
            is_core: true,
            source_language: "en".to_string(),
        }];

        let resolved = resolve_news_category(&batch_categories, &metadata, "World")
            .expect("category should resolve");
        assert_eq!(resolved.id, "batch-world");
        assert_eq!(resolved.category_id, "world");
        assert_eq!(resolved.metadata.expect("metadata").category_type, "core");
    }

    #[test]
    fn parses_assistant_prompt_stream() {
        let raw = concat!(
            "hi:{\"v\":\"202603091651.stage.c128588\",\"trace\":\"trace-123\"}\0\n",
            "thread.json:{\"id\":\"thread-1\",\"title\":\"Greeting\",\"ack\":\"2026-03-16T06:19:07Z\",\"created_at\":\"2026-03-16T06:19:07Z\",\"expires_at\":\"2026-03-16T07:19:07Z\",\"saved\":false,\"shared\":false,\"branch_id\":\"00000000-0000-4000-0000-000000000000\",\"tag_ids\":[]}\0\n",
            "new_message.json:{\"id\":\"msg-1\",\"thread_id\":\"thread-1\",\"created_at\":\"2026-03-16T06:19:07Z\",\"branch_list\":[\"00000000-0000-4000-0000-000000000000\"],\"state\":\"done\",\"prompt\":\"Hello\",\"reply\":\"<p>Hi</p>\",\"md\":\"Hi\",\"references_html\":\"<ol><li>Doc</li></ol>\",\"references_md\":\"1. [Doc](https://example.com)\",\"metadata\":\"<li>meta</li>\",\"documents\":[],\"trace_id\":\"trace-message-1\"}\0\n"
        );

        let parsed = parse_assistant_prompt_stream(raw).expect("assistant stream parses");
        assert_eq!(parsed.meta.trace.as_deref(), Some("trace-123"));
        assert_eq!(parsed.thread.id, "thread-1");
        assert_eq!(parsed.message.markdown.as_deref(), Some("Hi"));
        assert_eq!(
            parsed.message.references_markdown.as_deref(),
            Some("1. [Doc](https://example.com)")
        );
        assert_eq!(
            parsed.message.branch_list,
            vec!["00000000-0000-4000-0000-000000000000".to_string()]
        );
        assert_eq!(parsed.message.trace_id.as_deref(), Some("trace-message-1"));
    }

    #[test]
    fn normalizes_assistant_query_and_thread_id() {
        assert_eq!(
            normalize_assistant_query("  hello  ").expect("query trims"),
            "hello"
        );
        assert_eq!(
            normalize_assistant_thread_id(Some("  thread-1  ")).expect("thread id trims"),
            Some("thread-1".to_string())
        );
        assert_eq!(
            normalize_assistant_thread_id(None).expect("missing thread id stays none"),
            None
        );
    }

    #[test]
    fn rejects_empty_assistant_query_and_thread_id() {
        let query_error = normalize_assistant_query("   ").expect_err("blank query should fail");
        assert!(
            query_error
                .to_string()
                .contains("assistant query cannot be empty")
        );

        let thread_error =
            normalize_assistant_thread_id(Some("   ")).expect_err("blank thread id should fail");
        assert!(
            thread_error
                .to_string()
                .contains("assistant thread id cannot be empty")
        );
    }

    #[test]
    fn parses_assistant_thread_open_stream() {
        let raw = concat!(
            "hi:{\"v\":\"202603171911.stage.707e740\",\"trace\":\"trace-open\"}\0\n",
            "tags.json:[]\0\n",
            "thread.json:{\"id\":\"thread-1\",\"title\":\"Greeting\",\"ack\":\"2026-03-16T06:19:07Z\",\"created_at\":\"2026-03-16T06:19:07Z\",\"expires_at\":\"2026-03-16T07:19:07Z\",\"saved\":false,\"shared\":false,\"branch_id\":\"00000000-0000-4000-0000-000000000000\",\"tag_ids\":[]}\0\n",
            "messages.json:[{\"id\":\"msg-1\",\"thread_id\":\"thread-1\",\"created_at\":\"2026-03-16T06:19:07Z\",\"branch_list\":[],\"state\":\"done\",\"prompt\":\"Hello\",\"reply\":\"<p>Hi</p>\",\"md\":\"Hi\",\"metadata\":\"\",\"documents\":[],\"trace_id\":\"trace-msg\"}]\0\n"
        );

        let parsed = parse_assistant_thread_open_stream(raw).expect("thread open parses");
        assert_eq!(parsed.meta.trace.as_deref(), Some("trace-open"));
        assert_eq!(parsed.thread.id, "thread-1");
        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.messages[0].trace_id.as_deref(), Some("trace-msg"));
    }

    #[test]
    fn parses_assistant_thread_list_stream() {
        let raw = concat!(
            "hi:{\"v\":\"202603171911.stage.707e740\",\"trace\":\"trace-list\"}\0\n",
            "tags.json:[]\0\n",
            "thread_list.html:{\"html\":\"<div class=\\\"hide-if-no-threads\\\"><ul class=\\\"thread-list\\\"><li class=\\\"thread\\\" data-code=\\\"thread-1\\\" data-saved=\\\"true\\\" data-public=\\\"false\\\" data-tags='[&quot;tag-1&quot;]' data-snippet=\\\"First snippet\\\"><a href=\\\"/assistant/thread-1\\\"><div class=\\\"title\\\">First Thread</div><div class=\\\"excerpt\\\">First snippet</div></a></li></ul></div>\",\"next_cursor\":null,\"has_more\":false,\"count\":1,\"total_counts\":{\"all\":1}}\0\n"
        );

        let parsed = parse_assistant_thread_list_stream(raw).expect("thread list parses");
        assert_eq!(parsed.meta.trace.as_deref(), Some("trace-list"));
        assert_eq!(parsed.threads.len(), 1);
        assert_eq!(parsed.threads[0].id, "thread-1");
        assert_eq!(parsed.pagination.count, 1);
        assert_eq!(parsed.pagination.total_counts.get("all"), Some(&1));
    }

    #[test]
    fn parses_assistant_thread_delete_stream() {
        let parsed =
            parse_assistant_thread_delete_stream("hi:{\"v\":\"x\"}\0\nok:null\0\n", "thread-1")
                .expect("delete stream parses");
        assert_eq!(parsed.deleted_thread_ids, vec!["thread-1".to_string()]);
    }

    #[test]
    fn parses_content_disposition_filename() {
        assert_eq!(
            parse_content_disposition_filename(
                "attachment; filename*=utf-8''Say%20Hi%20In%20Five%20Words.md"
            ),
            Some("Say Hi In Five Words.md".to_string())
        );
        assert_eq!(
            parse_content_disposition_filename("attachment; filename=\"thread.md\""),
            Some("thread.md".to_string())
        );
    }

    fn live_session_token() -> Option<String> {
        load_credential_inventory()
            .ok()
            .and_then(|inventory| inventory.session_token.map(|credential| credential.value))
    }

    fn live_nonce() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos()
    }

    #[tokio::test]
    #[ignore]
    async fn live_assistant_thread_roundtrip() {
        let Some(token) = live_session_token() else {
            eprintln!("skipping live assistant test because {SESSION_TOKEN_ENV} is not set");
            return;
        };

        let request = AssistantPromptRequest {
            query: format!("Reply with exactly: assistant-v2-smoke-{}", live_nonce()),
            thread_id: None,
            model: Some("gpt-5-mini".to_string()),
            lens_id: None,
            internet_access: Some(true),
            personalizations: Some(false),
        };

        let prompt = execute_assistant_prompt(&request, &token)
            .await
            .expect("assistant prompt should succeed");
        assert_eq!(prompt.message.state, "done");
        assert_eq!(
            prompt
                .message
                .profile
                .as_ref()
                .and_then(|v| v.get("model"))
                .and_then(|v| v.as_str()),
            Some("gpt-5-mini")
        );

        let thread_id = prompt.thread.id.clone();

        let fetched = execute_assistant_thread_get(&thread_id, &token)
            .await
            .expect("assistant thread get should succeed");
        assert_eq!(fetched.thread.id, thread_id);
        assert!(!fetched.messages.is_empty());

        let listed = execute_assistant_thread_list(&token)
            .await
            .expect("assistant thread list should succeed");
        assert!(listed.threads.iter().any(|thread| thread.id == thread_id));

        let exported = execute_assistant_thread_export(&thread_id, &token)
            .await
            .expect("assistant thread export should succeed");
        assert!(exported.markdown.contains("assistant-v2-smoke-"));

        let deleted = execute_assistant_thread_delete(&thread_id, &token)
            .await
            .expect("assistant thread delete should succeed");
        assert_eq!(deleted.deleted_thread_ids, vec![thread_id]);
    }
}
