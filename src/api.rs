use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::multipart;
use reqwest::{Client, Method, StatusCode, Url, header};
use scraper::Html;
#[cfg(test)]
use scraper::Selector;
use serde::Deserialize;
#[cfg(test)]
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use serde_json::{Map, Number, Value};
use tokio::time::sleep;
use tracing::debug;

use crate::cli::{NewsFilterMode, NewsFilterScope};
use crate::error::KagiError;
use crate::http::{self, map_transport_error};
#[cfg(test)]
use crate::parser::parse_assistant_thread_list;
use crate::parser::{
    parse_custom_bang_form, parse_custom_bang_list, parse_lens_form, parse_lens_list,
    parse_redirect_form, parse_redirect_list,
};
#[cfg(test)]
use crate::types::ApiMeta;
use crate::types::{
    AlternativeTranslationsResponse, AskPageRequest, AskPageResponse, AskPageSource,
    AssistantMessage, AssistantMeta, AssistantModelCatalog, AssistantModelOption,
    AssistantProfileCreateRequest, AssistantProfileDetails, AssistantProfileSummary,
    AssistantProfileUpdateRequest, AssistantPromptRequest, AssistantPromptResponse,
    AssistantPromptStreamEvent, AssistantThread, AssistantThreadDeleteResponse,
    AssistantThreadExportResponse, AssistantThreadListResponse, AssistantThreadOpenResponse,
    AssistantThreadPagination, AssistantThreadSummary, AssistantUsage, CustomBangCreateRequest,
    CustomBangDetails, CustomBangSummary, CustomBangUpdateRequest, DeletedResourceResponse,
    EnrichResponse, ExtractPageInput, ExtractRequest, ExtractResponse, FastGptRequest,
    FastGptResponse, LensCreateRequest, LensDetails, LensSummary, LensUpdateRequest,
    NewsBatchCategories, NewsBatchCategory, NewsCategoriesResponse, NewsCategoryMetadata,
    NewsCategoryMetadataList, NewsChaos, NewsChaosResponse, NewsContentFilterSummary,
    NewsFilterPresetListEntry, NewsFilterPresetListResponse, NewsLatestBatch, NewsResolvedCategory,
    NewsStoriesPayload, NewsStoriesResponse, NewsStoryContentFilterSummary,
    RedirectRuleCreateRequest, RedirectRuleDetails, RedirectRuleSummary, RedirectRuleUpdateRequest,
    SmallWebFeed, SubscriberSummarization, SubscriberSummarizeMeta, SubscriberSummarizeRequest,
    SubscriberSummarizeResponse, SummarizeRequest, SummarizeResponse, TextAlignmentsResponse,
    ToggleResourceResponse, TranslateBootstrapMetadata, TranslateCommandRequest,
    TranslateDetectedLanguage, TranslateOptionState, TranslateResponse, TranslateTextResponse,
    TranslateWarning, TranslationSuggestionsResponse, WordInsightsResponse,
};

const KAGI_SUMMARIZE_PATH: &str = "/api/v0/summarize";
const KAGI_EXTRACT_PATH: &str = "/api/v1/extract";
const KAGI_SUBSCRIBER_SUMMARIZE_PATH: &str = "/mother/summary_labs";
const KAGI_NEWS_LATEST_PATH: &str = "/api/batches/latest";
const KAGI_NEWS_CATEGORIES_METADATA_PATH: &str = "/api/categories/metadata";
const KAGI_NEWS_BATCH_CATEGORIES_PATH: &str = "/api/batches";
const NEWS_FILTER_PRESETS_JSON: &str = include_str!("../data/news-filter-presets.json");
const DEBUG_BODY_PREVIEW_LIMIT: usize = 256;
const KAGI_ASSISTANT_CONVERSATIONS_PATH: &str = "/api/conversations";
const KAGI_ASSISTANT_INIT_PATH: &str = "/api/init";
const KAGI_ASSISTANTS_PATH: &str = "/api/assistants";
const KAGI_ASSISTANT_CUSTOM_EDITOR_PATH: &str = "/assistant/custom-assistants";
const KAGI_SETTINGS_LENSES_PATH: &str = "/html/settings/lenses";
const KAGI_SETTINGS_CREATE_LENS_PATH: &str = "/settings/create_lens";
const KAGI_LENSES_CREATE_PATH: &str = "/lenses/create";
const KAGI_LENSES_UPDATE_PATH: &str = "/lenses/update";
const KAGI_LENSES_DELETE_PATH: &str = "/lenses/delete";
const KAGI_LENSES_SUBSCRIBE_PATH: &str = "/lenses/subscribe";
const KAGI_SETTINGS_CUSTOM_BANGS_PATH: &str = "/settings/custom_bangs";
const KAGI_SETTINGS_CUSTOM_BANG_FORM_PATH: &str = "/settings/custom_bangs_form";
const KAGI_CUSTOM_BANGS_MODIFY_PATH: &str = "/bangs/modify";
const KAGI_SETTINGS_REDIRECTS_PATH: &str = "/settings/redirects";
const KAGI_REDIRECTS_CREATE_UPDATE_PATH: &str = "/rewrite_rules";
const KAGI_REDIRECTS_DELETE_PATH: &str = "/rewrite_rules/delete";
const KAGI_REDIRECTS_TOGGLE_PATH: &str = "/rewrite_rules/toggle";
const KAGI_FASTGPT_PATH: &str = "/api/v0/fastgpt";
const KAGI_ENRICH_WEB_PATH: &str = "/api/v0/enrich/web";
const KAGI_ENRICH_NEWS_PATH: &str = "/api/v0/enrich/news";
const KAGI_SMALLWEB_FEED_PATH: &str = "/api/v1/smallweb/feed/";
const KAGI_TRANSLATE_DETECT_PATH: &str = "/api/detect";
const KAGI_TRANSLATE_PATH: &str = "/api/translate";
const KAGI_TRANSLATE_ALTERNATIVES_PATH: &str = "/api/alternative-translations";
const KAGI_TRANSLATE_ALIGNMENTS_PATH: &str = "/api/text-alignments";
const KAGI_TRANSLATE_SUGGESTIONS_PATH: &str = "/api/translation-suggestions";
const KAGI_TRANSLATE_WORD_INSIGHTS_PATH: &str = "/api/word-insights";

const TRANSLATE_BOOTSTRAP_MAX_ATTEMPTS: usize = 3;
const TRANSLATE_BOOTSTRAP_MISSING_COOKIE_ERROR: &str =
    "translate bootstrap did not mint a translate_session cookie";
const KAGI_LOGGED_OUT_MARKERS: [&str; 3] = [
    "<title>Kagi Search - A Premium Search Engine</title>",
    "Welcome to Kagi",
    "paid search engine that gives power back to the user",
];
const LENS_MUTATION_LOOKUP_ATTEMPTS: usize = 10;
const LENS_MUTATION_LOOKUP_DELAY_MS: u64 = 500;
const LENS_TOGGLE_VERIFY_ATTEMPTS: usize = 5;
const KAGI_LENS_NAME_MAX_CHARS: usize = 33;

#[derive(Debug, Clone)]
/// Filter parameters for the Kagi News API.
pub struct NewsFilterRequest {
    pub preset_ids: Vec<String>,
    pub keywords: Vec<String>,
    pub mode: NewsFilterMode,
    pub scope: NewsFilterScope,
}

/// Summarizes a URL or text using the Kagi public Summarizer API with API-token auth.
///
/// # Arguments
/// * `request` - The summarize request (must have exactly one of `url` or `text`).
/// * `token` - The Kagi API token.
///
/// # Returns
/// A `SummarizeResponse` with the summarization output.
///
/// # Errors
/// Returns `KagiError::Auth` if the token is missing, `KagiError::Config` if both or neither
/// URL and text are provided, and network/parse errors on failure.
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
        .post(http::kagi_url(KAGI_SUMMARIZE_PATH))
        .header(header::AUTHORIZATION, format!("Bot {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .json(request)
        .send()
        .await
        .map_err(map_transport_error)?;

    decode_kagi_json(response, "summarizer").await
}

/// Extracts a web page using Kagi's v1 Extract API with API-key auth.
///
/// # Arguments
/// * `url` - The HTTPS URL to extract.
/// * `token` - The Kagi API key.
///
/// # Returns
/// Full Extract API response, including markdown and retained link metadata.
///
/// # Errors
/// Returns `KagiError::Auth` if the token is missing, `KagiError::Config` if the
/// URL does not satisfy the Extract API contract, and network/parse errors on failure.
pub async fn execute_extract_response(
    url: &str,
    token: &str,
) -> Result<ExtractResponse, KagiError> {
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi API key (expected KAGI_API_KEY)".to_string(),
        ));
    }

    let url = normalize_extract_url(url)?;
    let request = ExtractRequest {
        pages: vec![ExtractPageInput { url }],
        format: "json".to_string(),
    };

    let client = build_client()?;
    let response = client
        .post(http::kagi_url(KAGI_EXTRACT_PATH))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .json(&request)
        .send()
        .await
        .map_err(map_transport_error)?;

    decode_kagi_json(response, "Extract").await
}

/// Extracts a web page as markdown using Kagi's v1 Extract API with API-key auth.
///
/// # Errors
/// Returns auth/config/network errors from [`execute_extract_response`] and
/// parse errors when the response contains no markdown content.
pub async fn execute_extract(url: &str, token: &str) -> Result<String, KagiError> {
    let response = execute_extract_response(url, token).await?;
    extract_first_markdown(response)
}

/// Summarizes a URL or text using the subscriber web Summarizer with session-token auth.
///
/// # Arguments
/// * `request` - The subscriber summarize request.
/// * `token` - The Kagi session token.
///
/// # Returns
/// A `SubscriberSummarizeResponse` with the streamed summarization result.
///
/// # Errors
/// Returns `KagiError::Auth` if the token is missing or expired,
/// `KagiError::Config` for invalid parameters,
/// `KagiError::Network` for transport errors,
/// `KagiError::Parse` if the stream cannot be parsed.
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
    let target_language = request.target_language.as_deref().map_or("", str::trim);

    let client = build_client()?;
    let response = client
        .post(http::kagi_assistant_api_url(KAGI_SUBSCRIBER_SUMMARIZE_PATH))
        .form(&[
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
        status @ (StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => {
            let body = http::read_error_body(response, "subscriber summarizer").await;
            Err(KagiError::Auth(format!(
                "invalid or expired Kagi session token for subscriber summarizer: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_server_error() => {
            let body = http::read_error_body(response, "subscriber summarizer").await;
            Err(KagiError::Network(format!(
                "Kagi subscriber summarizer server error: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status => {
            let body = http::read_error_body(response, "subscriber summarizer").await;
            Err(KagiError::Network(format!(
                "unexpected Kagi subscriber summarizer response status: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
    }
}

fn normalize_extract_url(url: &str) -> Result<String, KagiError> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(KagiError::Config("extract requires a URL".to_string()));
    }

    let parsed = Url::parse(trimmed)
        .map_err(|error| KagiError::Config(format!("extract URL is invalid: {error}")))?;
    if parsed.scheme() != "https" {
        return Err(KagiError::Config(
            "extract URL must use the https scheme".to_string(),
        ));
    }
    if parsed.host_str().is_none() {
        return Err(KagiError::Config(
            "extract URL must include a valid host".to_string(),
        ));
    }

    Ok(trimmed.to_string())
}

fn extract_first_markdown(response: ExtractResponse) -> Result<String, KagiError> {
    if let Some(markdown) = response
        .data
        .first()
        .and_then(|page| page.markdown.as_deref())
        .filter(|markdown| !markdown.is_empty())
    {
        return Ok(markdown.to_string());
    }

    let suffix = response
        .meta
        .trace
        .as_deref()
        .map(|trace| format!(" (trace id: {trace})"))
        .unwrap_or_default();

    if let Some(errors) = response.errors.filter(|errors| !errors.is_empty()) {
        return Err(KagiError::Network(format!(
            "Kagi Extract API error: {}{}",
            serde_json::to_string(&errors).unwrap_or_else(|_| format!("{errors:?}")),
            suffix
        )));
    }

    Err(KagiError::Parse(format!(
        "Kagi Extract API returned no content{suffix}"
    )))
}

/// Fetches Kagi News stories for a given category with optional content filtering.
///
/// # Arguments
/// * `category` - The category slug (e.g. `"world"`, `"tech"`).
/// * `limit` - Maximum number of stories to return (must be > 0).
/// * `lang` - Language code.
/// * `filter_request` - Optional content filter configuration.
///
/// # Returns
/// A `NewsStoriesResponse` with the latest batch, category, and filtered stories.
///
/// # Errors
/// Returns `KagiError::Config` if `limit` is 0, or network/parse errors on failure.
pub async fn execute_news(
    category: &str,
    limit: u32,
    lang: &str,
    filter_request: Option<&NewsFilterRequest>,
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
            .get(http::kagi_news_url(KAGI_NEWS_LATEST_PATH))
            .query(&[("lang", normalized_lang.as_str())])
            .send()
            .await
            .map_err(map_transport_error)?,
        "news latest batch",
    )
    .await?;
    let metadata: NewsCategoryMetadataList = decode_kagi_free_json(
        client
            .get(http::kagi_news_url(KAGI_NEWS_CATEGORIES_METADATA_PATH))
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
                http::kagi_news_url(KAGI_NEWS_BATCH_CATEGORIES_PATH),
                latest_batch.id
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
                http::kagi_news_url(KAGI_NEWS_BATCH_CATEGORIES_PATH),
                latest_batch.id,
                category.id
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
    let (stories, content_filter) = match filter_request {
        Some(request) => {
            let filtered = apply_news_content_filters(payload.stories, request, &normalized_lang)?;
            (filtered.stories, Some(filtered.summary))
        }
        None => (payload.stories, None),
    };

    Ok(NewsStoriesResponse {
        latest_batch,
        category,
        stories,
        total_stories: payload.total_stories,
        domains: payload.domains,
        read_count: payload.read_count,
        content_filter,
    })
}

/// Returns the built-in news filter presets for a given language.
///
/// # Arguments
/// * `lang` - Language code (e.g. `"en"`, `"default"`).
///
/// # Returns
/// A `NewsFilterPresetListResponse` with available presets and their keywords.
///
/// # Errors
/// Returns `KagiError::Parse` if the embedded preset data cannot be loaded.
pub fn execute_news_filter_presets(lang: &str) -> Result<NewsFilterPresetListResponse, KagiError> {
    let normalized_lang = normalize_news_lang(lang);
    let presets = load_news_filter_presets()?;

    Ok(NewsFilterPresetListResponse {
        language: normalized_lang.clone(),
        presets: presets
            .into_iter()
            .map(|preset| {
                let keywords = preset.resolve_keywords(&normalized_lang);
                NewsFilterPresetListEntry {
                    id: preset.id,
                    label: preset.label,
                    keywords,
                }
            })
            .collect(),
    })
}

/// Fetches the list of available Kagi News categories with metadata.
///
/// # Arguments
/// * `lang` - Language code.
///
/// # Returns
/// A `NewsCategoriesResponse` with the latest batch and resolved categories.
///
/// # Errors
/// Returns network/parse errors on failure.
pub async fn execute_news_categories(lang: &str) -> Result<NewsCategoriesResponse, KagiError> {
    let client = build_client()?;
    let normalized_lang = normalize_news_lang(lang);
    let latest_batch: NewsLatestBatch = decode_kagi_free_json(
        client
            .get(http::kagi_news_url(KAGI_NEWS_LATEST_PATH))
            .query(&[("lang", normalized_lang.as_str())])
            .send()
            .await
            .map_err(map_transport_error)?,
        "news latest batch",
    )
    .await?;
    let metadata: NewsCategoryMetadataList = decode_kagi_free_json(
        client
            .get(http::kagi_news_url(KAGI_NEWS_CATEGORIES_METADATA_PATH))
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
                http::kagi_news_url(KAGI_NEWS_BATCH_CATEGORIES_PATH),
                latest_batch.id
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

/// Fetches the current Kagi News chaos index.
///
/// # Arguments
/// * `lang` - Language code.
///
/// # Returns
/// A `NewsChaosResponse` with the latest batch and chaos data.
///
/// # Errors
/// Returns network/parse errors on failure.
pub async fn execute_news_chaos(lang: &str) -> Result<NewsChaosResponse, KagiError> {
    let client = build_client()?;
    let normalized_lang = normalize_news_lang(lang);
    let latest_batch: NewsLatestBatch = decode_kagi_free_json(
        client
            .get(http::kagi_news_url(KAGI_NEWS_LATEST_PATH))
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
                http::kagi_news_url(KAGI_NEWS_BATCH_CATEGORIES_PATH),
                latest_batch.id
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

/// Sends a prompt to Kagi Assistant and returns the response.
///
/// # Arguments
/// * `request` - The assistant prompt request with query and optional thread/profile settings.
/// * `token` - The Kagi session token.
///
/// # Returns
/// An `AssistantPromptResponse` with the thread and generated message.
///
/// # Errors
/// Returns `KagiError::Config` if the query is empty, or auth/network/parse errors on failure.
pub async fn execute_assistant_prompt(
    request: &AssistantPromptRequest,
    token: &str,
) -> Result<AssistantPromptResponse, KagiError> {
    execute_current_assistant_prompt(request, token, &mut |_| Ok(())).await
}

/// Sends a prompt to Kagi Assistant and calls `on_event` for every message update.
///
/// The returned value is the same final response produced by [`execute_assistant_prompt`].
pub async fn execute_assistant_prompt_stream<F>(
    request: &AssistantPromptRequest,
    token: &str,
    mut on_event: F,
) -> Result<AssistantPromptResponse, KagiError>
where
    F: FnMut(&AssistantPromptStreamEvent) -> Result<(), KagiError>,
{
    execute_current_assistant_prompt(request, token, &mut on_event).await
}

async fn execute_current_assistant_prompt<F>(
    request: &AssistantPromptRequest,
    token: &str,
    on_event: &mut F,
) -> Result<AssistantPromptResponse, KagiError>
where
    F: FnMut(&AssistantPromptStreamEvent) -> Result<(), KagiError>,
{
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi session token (expected KAGI_SESSION_TOKEN)".to_string(),
        ));
    }

    let query = normalize_assistant_query(&request.query)?;
    let thread_id = normalize_assistant_thread_id(request.thread_id.as_deref())?;
    let attachments = request
        .attachments
        .iter()
        .map(|path| load_assistant_attachment(path))
        .collect::<Result<Vec<_>, _>>()?;
    let (profile_id, selected_profile) = if let Some(selector) = request.profile_id.as_deref() {
        let assistants = execute_custom_assistant_list(token).await?;
        let assistant = resolve_custom_assistant_ref(&assistants, selector, false)?;
        (Some(assistant.id.clone()), Some(json!(assistant)))
    } else {
        (
            None,
            request
                .model
                .as_ref()
                .map(|model| json!({ "model_name": model })),
        )
    };
    let (_conversation, branch) =
        prepare_current_assistant_conversation(thread_id.as_deref(), request, token).await?;
    let attachment_uuids = upload_current_assistant_attachments(attachments, token).await?;

    let mut payload = Map::new();
    payload.insert("message".to_string(), Value::String(query.clone()));
    if !attachment_uuids.is_empty() {
        payload.insert(
            "attachment_uuids".to_string(),
            Value::Array(attachment_uuids.into_iter().map(Value::String).collect()),
        );
    }
    if let Some(profile_id) = profile_id {
        payload.insert("profile_uuid".to_string(), Value::String(profile_id));
    }
    if let Some(model) = request.model.as_deref() {
        payload.insert("model_name".to_string(), Value::String(model.to_string()));
    }
    if let Some(lens_id) = request.lens_id {
        payload.insert("lens_id".to_string(), Value::from(lens_id));
    }
    if let Some(internet_access) = request.internet_access {
        payload.insert("enable_search".to_string(), Value::Bool(internet_access));
    }
    if let Some(personalizations) = request.personalizations {
        payload.insert("personalization".to_string(), Value::Bool(personalizations));
    }

    let client = http::client_assistant_stream()?;
    let response = client
        .post(http::kagi_assistant_url(&format!(
            "/api/branches/{}/messages",
            branch.uuid
        )))
        .header(header::COOKIE, format!("kagi_session={token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .json(&Value::Object(payload))
        .send()
        .await
        .map_err(map_transport_error)?;
    let sent: CurrentAssistantPromptMessageResponse =
        read_current_assistant_json_response(response, "Assistant prompt").await?;

    let response = client
        .get(http::kagi_assistant_url(&format!(
            "/api/branches/{}/stream",
            sent.branch.uuid
        )))
        .header(header::COOKIE, format!("kagi_session={token}"))
        .header(header::ACCEPT, "text/event-stream")
        .send()
        .await
        .map_err(map_transport_error)?;

    handle_current_assistant_prompt_stream(
        response,
        sent.conversation,
        sent.branch,
        sent.user_message,
        query,
        selected_profile,
        on_event,
    )
    .await
}

async fn prepare_current_assistant_conversation(
    thread_id: Option<&str>,
    request: &AssistantPromptRequest,
    token: &str,
) -> Result<(CurrentAssistantConversation, CurrentAssistantBranch), KagiError> {
    let client = http::client_assistant_stream()?;
    if let Some(thread_id) = thread_id {
        validate_current_assistant_path_id("assistant thread id", thread_id)?;
        let response = client
            .get(http::kagi_assistant_url(&format!(
                "/api/conversations/{thread_id}/init"
            )))
            .header(header::COOKIE, format!("kagi_session={token}"))
            .header(header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(map_transport_error)?;
        let initialized: CurrentAssistantConversationInitResponse =
            read_current_assistant_json_response(response, "Assistant conversation init").await?;
        let branch = initialized.selected_branch().ok_or_else(|| {
            KagiError::Parse("Assistant conversation did not include an active branch".to_string())
        })?;
        return Ok((initialized.conversation, branch));
    }

    let mut payload = Map::new();
    if let Some(model) = request.model.as_deref() {
        payload.insert("model_name".to_string(), Value::String(model.to_string()));
    }
    let response = client
        .post(http::kagi_assistant_url(KAGI_ASSISTANT_CONVERSATIONS_PATH))
        .header(header::COOKIE, format!("kagi_session={token}"))
        .header(header::ACCEPT, "application/json")
        .header(header::CONTENT_TYPE, "application/json")
        .json(&Value::Object(payload))
        .send()
        .await
        .map_err(map_transport_error)?;
    let created: CurrentAssistantConversationCreateResponse =
        read_current_assistant_json_response(response, "Assistant conversation create").await?;
    Ok((created.conversation, created.default_branch))
}

async fn upload_current_assistant_attachments(
    attachments: Vec<AssistantAttachmentPayload>,
    token: &str,
) -> Result<Vec<String>, KagiError> {
    let client = http::client_assistant_stream()?;
    let mut uuids = Vec::with_capacity(attachments.len());
    for attachment in attachments {
        let part = multipart::Part::bytes(attachment.bytes)
            .file_name(attachment.filename)
            .mime_str(&attachment.content_type)
            .map_err(|error| {
                KagiError::Config(format!(
                    "invalid assistant attachment content type: {error}"
                ))
            })?;
        let response = client
            .post(http::kagi_assistant_url("/api/upload"))
            .header(header::COOKIE, format!("kagi_session={token}"))
            .multipart(multipart::Form::new().part("file", part))
            .send()
            .await
            .map_err(map_transport_error)?;
        let uploaded: CurrentAssistantUploadResponse =
            read_current_assistant_json_response(response, "Assistant attachment upload").await?;
        uuids.push(uploaded.uuid);
    }
    Ok(uuids)
}

async fn handle_current_assistant_prompt_stream<F>(
    response: reqwest::Response,
    conversation: CurrentAssistantConversation,
    branch: CurrentAssistantBranch,
    user_message: CurrentAssistantMessage,
    prompt: String,
    selected_profile: Option<Value>,
    on_event: &mut F,
) -> Result<AssistantPromptResponse, KagiError>
where
    F: FnMut(&AssistantPromptStreamEvent) -> Result<(), KagiError>,
{
    if !response.status().is_success() {
        let status = response.status();
        let body = http::read_error_body(response, "Assistant prompt stream").await;
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(KagiError::Auth(format!(
                "invalid or expired Kagi session token for Assistant prompt stream: HTTP {status}{}",
                assistant_error_suffix(&body)
            )));
        }
        return Err(KagiError::Network(format!(
            "Kagi Assistant prompt stream failed: HTTP {status}{}",
            assistant_error_suffix(&body)
        )));
    }

    let mut parser = CurrentAssistantPromptParser::new(
        conversation,
        branch,
        user_message,
        prompt,
        selected_profile,
    );
    let mut pending = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            KagiError::Network(format!(
                "failed to read Assistant prompt stream response body: {error}"
            ))
        })?;
        pending.extend_from_slice(&chunk);
        while let Some(frame) = take_next_assistant_sse_frame(&mut pending)? {
            if let Some(event) = parser.process_sse_frame(&frame)? {
                on_event(&event)?;
            }
        }
    }
    if !pending.is_empty() {
        let tail = String::from_utf8(pending).map_err(|error| {
            KagiError::Parse(format!(
                "Assistant prompt stream ended with invalid UTF-8: {error}"
            ))
        })?;
        if !tail.trim().is_empty()
            && let Some(event) = parser.process_sse_frame(&tail)?
        {
            on_event(&event)?;
        }
    }
    parser.finish()
}

fn take_next_assistant_sse_frame(pending: &mut Vec<u8>) -> Result<Option<String>, KagiError> {
    let lf = pending
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| (index, 2));
    let crlf = pending
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| (index, 4));
    let delimiter = match (lf, crlf) {
        (Some(lf), Some(crlf)) => Some(if lf.0 < crlf.0 { lf } else { crlf }),
        (Some(lf), None) => Some(lf),
        (None, Some(crlf)) => Some(crlf),
        (None, None) => None,
    };
    let Some((index, delimiter_len)) = delimiter else {
        return Ok(None);
    };

    let frame = String::from_utf8(pending[..index].to_vec()).map_err(|error| {
        KagiError::Parse(format!(
            "Assistant prompt stream frame contained invalid UTF-8: {error}"
        ))
    })?;
    pending.drain(..index + delimiter_len);
    Ok(Some(frame))
}

/// Lists every Assistant base model available to the authenticated account.
pub async fn execute_assistant_model_catalog(
    token: &str,
) -> Result<AssistantModelCatalog, KagiError> {
    let response = fetch_current_assistant_json::<CurrentAssistantInitResponse>(
        KAGI_ASSISTANT_INIT_PATH,
        token,
        "Assistant initialization",
    )
    .await?;
    Ok(response.models.into())
}

/// Lists all Kagi Assistant threads for the authenticated user.
///
/// # Arguments
/// * `token` - The Kagi session token.
///
/// # Returns
/// An `AssistantThreadListResponse` with threads and pagination info.
///
/// # Errors
/// Returns auth/network/parse errors on failure.
pub async fn execute_assistant_thread_list(
    token: &str,
) -> Result<AssistantThreadListResponse, KagiError> {
    let folders =
        fetch_current_assistant_json::<Vec<Value>>("/api/folders", token, "Assistant folders")
            .await
            .unwrap_or_default();
    let mut all_threads = Vec::new();
    let mut after_id = None;

    loop {
        let path = current_assistant_conversations_path(after_id.as_deref());
        let response = fetch_current_assistant_json::<CurrentAssistantConversationListResponse>(
            &path,
            token,
            "Assistant conversation list",
        )
        .await?;

        let next_after_id = response
            .items
            .last()
            .map(CurrentAssistantConversationItem::conversation_uuid);
        all_threads.extend(response.items.into_iter().map(AssistantThreadSummary::from));

        if !response.has_more || next_after_id.is_none() {
            after_id = None;
            break;
        }

        after_id = next_after_id;
    }

    let count = all_threads.len() as u64;
    Ok(AssistantThreadListResponse {
        meta: AssistantMeta::default(),
        folders,
        threads: all_threads,
        pagination: AssistantThreadPagination {
            next_cursor: after_id,
            has_more: false,
            count,
            total_counts: HashMap::from([(String::from("all"), count)]),
        },
    })
}

/// Opens a specific Kagi Assistant thread and returns its messages.
///
/// # Arguments
/// * `thread_id` - The thread identifier.
/// * `token` - The Kagi session token.
///
/// # Returns
/// An `AssistantThreadOpenResponse` with the thread and its messages.
///
/// # Errors
/// Returns `KagiError::Config` if the thread ID is empty, or auth/network/parse errors on failure.
pub async fn execute_assistant_thread_get(
    thread_id: &str,
    token: &str,
) -> Result<AssistantThreadOpenResponse, KagiError> {
    let thread_ref = normalize_assistant_thread_ref(thread_id)?;
    let folders =
        fetch_current_assistant_json::<Vec<Value>>("/api/folders", token, "Assistant folders")
            .await
            .unwrap_or_default();
    let mut response = fetch_current_assistant_json::<CurrentAssistantConversationInitResponse>(
        &current_assistant_conversation_init_path(&thread_ref),
        token,
        "Assistant conversation init",
    )
    .await?;
    let branch = response.selected_branch();
    let messages = fetch_current_assistant_messages(
        token,
        branch.as_ref(),
        std::mem::take(&mut response.messages),
    )
    .await?;

    Ok(response.into_thread_open_response(folders, branch, messages))
}

/// Deletes a Kagi Assistant thread.
///
/// # Arguments
/// * `thread_id` - The thread identifier.
/// * `token` - The Kagi session token.
///
/// # Returns
/// An `AssistantThreadDeleteResponse` confirming deletion.
///
/// # Errors
/// Returns auth/network/parse errors on failure.
pub async fn execute_assistant_thread_delete(
    thread_id: &str,
    token: &str,
) -> Result<AssistantThreadDeleteResponse, KagiError> {
    let thread_id = normalize_assistant_thread_id(Some(thread_id))?
        .ok_or_else(|| KagiError::Config("assistant thread id cannot be empty".to_string()))?;
    delete_current_assistant_resource(
        &format!("/api/conversations/{thread_id}"),
        token,
        "Assistant conversation delete",
    )
    .await?;

    Ok(AssistantThreadDeleteResponse {
        deleted_thread_ids: vec![thread_id],
    })
}

/// Exports a Kagi Assistant thread as Markdown.
///
/// # Arguments
/// * `thread_id` - The thread identifier.
/// * `token` - The Kagi session token.
///
/// # Returns
/// An `AssistantThreadExportResponse` with the exported markdown and optional filename.
///
/// # Errors
/// Returns `KagiError::Auth` if the token is expired, or network/parse errors on failure.
pub async fn execute_assistant_thread_export(
    thread_id: &str,
    token: &str,
) -> Result<AssistantThreadExportResponse, KagiError> {
    let thread_ref = normalize_assistant_thread_ref(thread_id)?;
    let response = execute_assistant_thread_get(thread_id, token).await?;

    Ok(AssistantThreadExportResponse {
        thread_id: thread_ref.thread_id,
        filename: Some(assistant_thread_export_filename(&response.thread.title)),
        markdown: format_assistant_thread_markdown(&response),
    })
}

/// Lists all custom assistant profiles for the authenticated user.
///
/// # Arguments
/// * `token` - The Kagi session token.
///
/// # Returns
/// A vector of `AssistantProfileSummary` entries.
///
/// # Errors
/// Returns auth/network/parse errors on failure.
pub async fn execute_custom_assistant_list(
    token: &str,
) -> Result<Vec<AssistantProfileSummary>, KagiError> {
    let response = fetch_current_assistant_profiles(token, "Assistant profiles").await?;
    Ok(response.assistant_summaries())
}

/// Gets the details of a specific custom assistant profile.
///
/// # Arguments
/// * `target` - The assistant name or ID.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The `AssistantProfileDetails` for the resolved assistant.
///
/// # Errors
/// Returns `KagiError::Config` if the assistant is not found or not editable.
pub async fn execute_custom_assistant_get(
    target: &str,
    token: &str,
) -> Result<AssistantProfileDetails, KagiError> {
    let response = fetch_current_assistant_profiles(token, "custom assistant get").await?;
    Ok(current_custom_assistant_for_target(&response, target)?.details())
}

/// Creates a new custom assistant profile.
///
/// # Arguments
/// * `request` - The creation request with profile settings.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The `AssistantProfileDetails` of the newly created assistant.
///
/// # Errors
/// Returns `KagiError::Config` for invalid names or settings, or auth/network/parse errors.
pub async fn execute_custom_assistant_create(
    request: &AssistantProfileCreateRequest,
    token: &str,
) -> Result<AssistantProfileDetails, KagiError> {
    let name = normalize_named_target(&request.name, "assistant name")?;
    let base_model = match trimmed_optional(request.base_model.as_deref()) {
        Some(value) => value.to_string(),
        None => {
            fetch_current_assistant_profiles(token, "Assistant initialization")
                .await?
                .models
                .default
        }
    };
    let details = AssistantProfileDetails {
        profile_id: None,
        name,
        bang_trigger: trimmed_optional(request.bang_trigger.as_deref()).map(str::to_string),
        internet_access: request.internet_access.unwrap_or(true),
        selected_lens: trimmed_optional(request.selected_lens.as_deref())
            .unwrap_or_default()
            .to_string(),
        personalizations: request.personalizations.unwrap_or(true),
        base_model,
        custom_instructions: request.custom_instructions.clone().unwrap_or_default(),
        delete_supported: true,
    };
    let created = send_current_assistant_json::<CurrentAssistantCustomAssistant>(
        Method::POST,
        KAGI_ASSISTANTS_PATH,
        &current_assistant_profile_payload(&details),
        token,
        "custom assistant create",
    )
    .await?;

    Ok(created.details())
}

/// Updates an existing custom assistant profile.
///
/// # Arguments
/// * `request` - The update request with target identifier and fields to change.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The updated `AssistantProfileDetails`.
///
/// # Errors
/// Returns `KagiError::Config` if the target is not found, or auth/network/parse errors.
pub async fn execute_custom_assistant_update(
    request: &AssistantProfileUpdateRequest,
    token: &str,
) -> Result<AssistantProfileDetails, KagiError> {
    let response = fetch_current_assistant_profiles(token, "custom assistant update").await?;
    let assistant = current_custom_assistant_for_target(&response, &request.target)?;
    let name = match request.name.as_deref() {
        Some(value) => normalize_named_target(value, "assistant name")?,
        None => assistant.name.clone(),
    };
    let bang_trigger = match request.bang_trigger.as_deref() {
        Some(value) => trimmed_optional(Some(value)).map(str::to_string),
        None => assistant.bang_trigger.clone(),
    };
    let selected_lens = match request.selected_lens.as_deref() {
        Some(value) => trimmed_optional(Some(value)).map(str::to_string),
        None => assistant.lens_id.clone(),
    };
    let custom_instructions = match request.custom_instructions.as_deref() {
        Some("") => None,
        Some(value) => Some(value.to_string()),
        None => assistant.instructions.clone(),
    };
    let base_model = trimmed_optional(request.base_model.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| assistant.llm_id.clone());
    let details = AssistantProfileDetails {
        profile_id: Some(assistant.uuid.clone()),
        name,
        bang_trigger,
        internet_access: request.internet_access.unwrap_or(assistant.internet_access),
        selected_lens: selected_lens.unwrap_or_default(),
        personalizations: request
            .personalizations
            .unwrap_or(assistant.personalizations),
        base_model,
        custom_instructions: custom_instructions.unwrap_or_default(),
        delete_supported: true,
    };
    let updated = send_current_assistant_json::<CurrentAssistantCustomAssistant>(
        Method::PATCH,
        &format!("{KAGI_ASSISTANTS_PATH}/{}", assistant.uuid),
        &current_assistant_profile_payload(&details),
        token,
        "custom assistant update",
    )
    .await?;

    Ok(updated.details())
}

/// Deletes a custom assistant profile.
///
/// # Arguments
/// * `target` - The assistant name or ID.
/// * `token` - The Kagi session token.
///
/// # Returns
/// A `DeletedResourceResponse` confirming deletion.
///
/// # Errors
/// Returns `KagiError::Config` if the target is not found or is built-in.
pub async fn execute_custom_assistant_delete(
    target: &str,
    token: &str,
) -> Result<DeletedResourceResponse, KagiError> {
    let response = fetch_current_assistant_profiles(token, "custom assistant delete").await?;
    let assistant = current_custom_assistant_for_target(&response, target)?;
    delete_current_assistant_resource(
        &format!("{KAGI_ASSISTANTS_PATH}/{}", assistant.uuid),
        token,
        "custom assistant delete",
    )
    .await?;
    Ok(DeletedResourceResponse {
        id: assistant.uuid.clone(),
    })
}

/// Lists all Kagi search lenses for the authenticated user.
///
/// # Arguments
/// * `token` - The Kagi session token.
///
/// # Returns
/// A vector of `LensSummary` entries.
///
/// # Errors
/// Returns auth/network/parse errors on failure.
pub async fn execute_lens_list(token: &str) -> Result<Vec<LensSummary>, KagiError> {
    let html = fetch_authenticated_html(
        &http::kagi_url(KAGI_SETTINGS_LENSES_PATH),
        token,
        "lens settings page",
    )
    .await?;
    parse_lens_list(&html)
}

/// Gets the details of a specific lens.
///
/// # Arguments
/// * `target` - The lens name or ID.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The `LensDetails` for the resolved lens.
///
/// # Errors
/// Returns `KagiError::Config` if the lens is not found, or auth/network/parse errors.
pub async fn execute_lens_get(target: &str, token: &str) -> Result<LensDetails, KagiError> {
    let lenses = execute_lens_list(token).await?;
    let lens = resolve_lens_ref(&lenses, target)?;
    let html =
        fetch_authenticated_html(&absolute_kagi_url(&lens.edit_url), token, "lens form").await?;
    parse_lens_form(&html)
}

/// Creates a new Kagi search lens.
///
/// # Arguments
/// * `request` - The creation request with lens settings.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The `LensDetails` of the newly created lens.
///
/// # Errors
/// Returns `KagiError::Config` for invalid settings, or auth/network/parse errors.
pub async fn execute_lens_create(
    request: &LensCreateRequest,
    token: &str,
) -> Result<LensDetails, KagiError> {
    let mut details = parse_lens_form(
        &fetch_authenticated_html(
            &http::kagi_url(KAGI_SETTINGS_CREATE_LENS_PATH),
            token,
            "new lens form",
        )
        .await?,
    )?;
    apply_lens_create_request(&mut details, request)?;

    let (url, _) = post_authenticated_form(
        &http::kagi_url(KAGI_LENSES_CREATE_PATH),
        &build_lens_form(&details),
        token,
        "lens create",
    )
    .await?;
    let created_id = match url_query_value(&url, "id") {
        Some(id) => id,
        None => resolve_lens_id_by_name_after_mutation(&details.name, token, "create").await?,
    };
    execute_lens_get_after_mutation(&created_id, token, "create").await
}

/// Updates an existing Kagi search lens.
///
/// # Arguments
/// * `request` - The update request with target identifier and fields to change.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The updated `LensDetails`.
///
/// # Errors
/// Returns `KagiError::Config` if the target is not found, or auth/network/parse errors.
pub async fn execute_lens_update(
    request: &LensUpdateRequest,
    token: &str,
) -> Result<LensDetails, KagiError> {
    let lenses = execute_lens_list(token).await?;
    let lens = resolve_lens_ref(&lenses, &request.target)?;
    let mut details = execute_lens_get(&lens.id, token).await?;
    apply_lens_update_request(&mut details, request)?;

    post_authenticated_form(
        &http::kagi_url(KAGI_LENSES_UPDATE_PATH),
        &build_lens_form(&details),
        token,
        "lens update",
    )
    .await?;
    execute_lens_get_after_mutation(&lens.id, token, "update").await
}

/// Deletes a Kagi search lens.
///
/// # Arguments
/// * `target` - The lens name or ID.
/// * `token` - The Kagi session token.
///
/// # Returns
/// A `DeletedResourceResponse` confirming deletion.
///
/// # Errors
/// Returns `KagiError::Config` if the lens is not found, or auth/network/parse errors.
pub async fn execute_lens_delete(
    target: &str,
    token: &str,
) -> Result<DeletedResourceResponse, KagiError> {
    let lenses = execute_lens_list(token).await?;
    let lens = resolve_lens_ref(&lenses, target)?;
    post_authenticated_form(
        &http::kagi_url(KAGI_LENSES_DELETE_PATH),
        &[("id".to_string(), lens.id.clone())],
        token,
        "lens delete",
    )
    .await?;
    Ok(DeletedResourceResponse {
        id: lens.id.clone(),
    })
}

/// Enables or disables a Kagi search lens.
///
/// # Arguments
/// * `target` - The lens name or ID.
/// * `enabled` - Whether to enable (`true`) or disable (`false`) the lens.
/// * `token` - The Kagi session token.
///
/// # Returns
/// A `ToggleResourceResponse` with the final enabled state.
///
/// # Errors
/// Returns `KagiError::Config` if the lens is not found, or auth/network/parse errors.
pub async fn execute_lens_set_enabled(
    target: &str,
    enabled: bool,
    token: &str,
) -> Result<ToggleResourceResponse, KagiError> {
    let lenses = execute_lens_list(token).await?;
    let lens = resolve_lens_ref(&lenses, target)?;
    if lens.enabled == enabled {
        return Ok(ToggleResourceResponse {
            id: lens.id.clone(),
            enabled: lens.enabled,
        });
    }

    post_authenticated_form(
        &http::kagi_url(KAGI_LENSES_SUBSCRIBE_PATH),
        &[
            ("lens_id".to_string(), lens.id.clone()),
            (lens.toggle_field.clone(), lens.toggle_value.clone()),
        ],
        token,
        if enabled {
            "lens enable"
        } else {
            "lens disable"
        },
    )
    .await?;

    for attempt in 0..LENS_TOGGLE_VERIFY_ATTEMPTS {
        match execute_lens_list(token).await {
            Ok(refreshed) => {
                if let Ok(lens) = resolve_lens_ref(&refreshed, &lens.id) {
                    return Ok(ToggleResourceResponse {
                        id: lens.id.clone(),
                        enabled: lens.enabled,
                    });
                }
            }
            Err(error)
                if attempt + 1 < LENS_TOGGLE_VERIFY_ATTEMPTS
                    && should_retry_lens_mutation_lookup(&error) => {}
            Err(error) => return Err(error),
        }

        if attempt + 1 < LENS_TOGGLE_VERIFY_ATTEMPTS {
            sleep(Duration::from_secs(1)).await;
        }
    }

    Err(KagiError::Config(format!(
        "lens '{}' was toggled but did not reappear in settings within 5 seconds",
        lens.id
    )))
}

/// Lists all custom bangs for the authenticated user.
///
/// # Arguments
/// * `token` - The Kagi session token.
///
/// # Returns
/// A vector of `CustomBangSummary` entries.
///
/// # Errors
/// Returns auth/network/parse errors on failure.
pub async fn execute_custom_bang_list(token: &str) -> Result<Vec<CustomBangSummary>, KagiError> {
    let html = fetch_authenticated_html(
        &http::kagi_url(KAGI_SETTINGS_CUSTOM_BANGS_PATH),
        token,
        "custom bangs page",
    )
    .await?;
    parse_custom_bang_list(&html)
}

/// Gets the details of a specific custom bang.
///
/// # Arguments
/// * `target` - The bang trigger or ID.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The `CustomBangDetails` for the resolved bang.
///
/// # Errors
/// Returns `KagiError::Config` if the bang is not found, or auth/network/parse errors.
pub async fn execute_custom_bang_get(
    target: &str,
    token: &str,
) -> Result<CustomBangDetails, KagiError> {
    let bangs = execute_custom_bang_list(token).await?;
    let bang = resolve_custom_bang_ref(&bangs, target)?;
    let html = fetch_authenticated_html(
        &absolute_kagi_url(&bang.edit_url),
        token,
        "custom bang form",
    )
    .await?;
    parse_custom_bang_form(&html)
}

/// Creates a new custom bang.
///
/// # Arguments
/// * `request` - The creation request with bang settings.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The `CustomBangDetails` of the newly created bang.
///
/// # Errors
/// Returns `KagiError::Config` for invalid settings, or auth/network/parse errors.
pub async fn execute_custom_bang_create(
    request: &CustomBangCreateRequest,
    token: &str,
) -> Result<CustomBangDetails, KagiError> {
    let mut details = parse_custom_bang_form(
        &fetch_authenticated_html(
            &http::kagi_url(KAGI_SETTINGS_CUSTOM_BANG_FORM_PATH),
            token,
            "new custom bang form",
        )
        .await?,
    )?;
    apply_custom_bang_create_request(&mut details, request)?;

    let (url, _) = post_authenticated_form(
        &http::kagi_url(KAGI_CUSTOM_BANGS_MODIFY_PATH),
        &build_custom_bang_form(&details, false),
        token,
        "custom bang create",
    )
    .await?;
    let created_id = match url_query_value(&url, "bang_id") {
        Some(id) => id,
        None => resolve_custom_bang_id_by_trigger(&details.trigger, token).await?,
    };
    execute_custom_bang_get(&created_id, token).await
}

/// Updates an existing custom bang.
///
/// # Arguments
/// * `request` - The update request with target identifier and fields to change.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The updated `CustomBangDetails`.
///
/// # Errors
/// Returns `KagiError::Config` if the target is not found, or auth/network/parse errors.
pub async fn execute_custom_bang_update(
    request: &CustomBangUpdateRequest,
    token: &str,
) -> Result<CustomBangDetails, KagiError> {
    let bangs = execute_custom_bang_list(token).await?;
    let bang = resolve_custom_bang_ref(&bangs, &request.target)?;
    let mut details = execute_custom_bang_get(&bang.id, token).await?;
    apply_custom_bang_update_request(&mut details, request)?;

    post_authenticated_form(
        &http::kagi_url(KAGI_CUSTOM_BANGS_MODIFY_PATH),
        &build_custom_bang_form(&details, false),
        token,
        "custom bang update",
    )
    .await?;
    execute_custom_bang_get(&bang.id, token).await
}

/// Deletes a custom bang.
///
/// # Arguments
/// * `target` - The bang trigger or ID.
/// * `token` - The Kagi session token.
///
/// # Returns
/// A `DeletedResourceResponse` confirming deletion.
///
/// # Errors
/// Returns `KagiError::Config` if the target is not found, or auth/network/parse errors.
pub async fn execute_custom_bang_delete(
    target: &str,
    token: &str,
) -> Result<DeletedResourceResponse, KagiError> {
    let bangs = execute_custom_bang_list(token).await?;
    let bang = resolve_custom_bang_ref(&bangs, target)?;
    let details = execute_custom_bang_get(&bang.id, token).await?;

    post_authenticated_form(
        &http::kagi_url(KAGI_CUSTOM_BANGS_MODIFY_PATH),
        &build_custom_bang_form(&details, true),
        token,
        "custom bang delete",
    )
    .await?;
    Ok(DeletedResourceResponse {
        id: bang.id.clone(),
    })
}

/// Lists all search redirect rules for the authenticated user.
///
/// # Arguments
/// * `token` - The Kagi session token.
///
/// # Returns
/// A vector of `RedirectRuleSummary` entries.
///
/// # Errors
/// Returns auth/network/parse errors on failure.
pub async fn execute_redirect_list(token: &str) -> Result<Vec<RedirectRuleSummary>, KagiError> {
    let html = fetch_authenticated_html(
        &http::kagi_url(KAGI_SETTINGS_REDIRECTS_PATH),
        token,
        "redirects page",
    )
    .await?;
    parse_redirect_list(&html)
}

/// Gets the details of a specific redirect rule.
///
/// # Arguments
/// * `target` - The redirect rule text or ID.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The `RedirectRuleDetails` for the resolved rule.
///
/// # Errors
/// Returns `KagiError::Config` if the rule is not found, or auth/network/parse errors.
pub async fn execute_redirect_get(
    target: &str,
    token: &str,
) -> Result<RedirectRuleDetails, KagiError> {
    let redirects = execute_redirect_list(token).await?;
    let redirect = resolve_redirect_ref(&redirects, target)?;
    let html = fetch_authenticated_html(
        &absolute_kagi_url(&redirect.edit_url),
        token,
        "redirect form",
    )
    .await?;
    let mut details = parse_redirect_form(&html)?;
    details.enabled = Some(redirect.enabled);
    Ok(details)
}

/// Creates a new search redirect rule.
///
/// # Arguments
/// * `request` - The creation request with the rule pattern.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The `RedirectRuleDetails` of the newly created rule.
///
/// # Errors
/// Returns `KagiError::Config` if the rule is empty, or auth/network/parse errors.
pub async fn execute_redirect_create(
    request: &RedirectRuleCreateRequest,
    token: &str,
) -> Result<RedirectRuleDetails, KagiError> {
    let rule = normalize_redirect_rule(&request.rule)?;
    let (url, _) = post_authenticated_form(
        &http::kagi_url(KAGI_REDIRECTS_CREATE_UPDATE_PATH),
        &[("regex".to_string(), rule.clone())],
        token,
        "redirect create",
    )
    .await?;
    if let Some(created_id) = url_query_value(&url, "rule_id") {
        return execute_redirect_get(&created_id, token).await;
    }
    execute_redirect_get(&rule, token).await
}

/// Updates an existing search redirect rule.
///
/// # Arguments
/// * `request` - The update request with target identifier and new rule pattern.
/// * `token` - The Kagi session token.
///
/// # Returns
/// The updated `RedirectRuleDetails`.
///
/// # Errors
/// Returns `KagiError::Config` if the target is not found, or auth/network/parse errors.
pub async fn execute_redirect_update(
    request: &RedirectRuleUpdateRequest,
    token: &str,
) -> Result<RedirectRuleDetails, KagiError> {
    let redirects = execute_redirect_list(token).await?;
    let redirect = resolve_redirect_ref(&redirects, &request.target)?;
    let rule = normalize_redirect_rule(&request.rule)?;
    post_authenticated_form(
        &http::kagi_url(KAGI_REDIRECTS_CREATE_UPDATE_PATH),
        &[
            ("rule_id".to_string(), redirect.id.clone()),
            ("regex".to_string(), rule),
        ],
        token,
        "redirect update",
    )
    .await?;
    execute_redirect_get(&redirect.id, token).await
}

/// Deletes a search redirect rule.
///
/// # Arguments
/// * `target` - The redirect rule text or ID.
/// * `token` - The Kagi session token.
///
/// # Returns
/// A `DeletedResourceResponse` confirming deletion.
///
/// # Errors
/// Returns `KagiError::Config` if the target is not found, or auth/network/parse errors.
pub async fn execute_redirect_delete(
    target: &str,
    token: &str,
) -> Result<DeletedResourceResponse, KagiError> {
    let redirects = execute_redirect_list(token).await?;
    let redirect = resolve_redirect_ref(&redirects, target)?;
    post_authenticated_form(
        &http::kagi_url(KAGI_REDIRECTS_DELETE_PATH),
        &[("rule_id".to_string(), redirect.id.clone())],
        token,
        "redirect delete",
    )
    .await?;
    Ok(DeletedResourceResponse {
        id: redirect.id.clone(),
    })
}

/// Enables or disables a search redirect rule.
///
/// # Arguments
/// * `target` - The redirect rule text or ID.
/// * `enabled` - Whether to enable (`true`) or disable (`false`) the rule.
/// * `token` - The Kagi session token.
///
/// # Returns
/// A `ToggleResourceResponse` with the final enabled state.
///
/// # Errors
/// Returns `KagiError::Config` if the rule is not found, or auth/network/parse errors.
pub async fn execute_redirect_set_enabled(
    target: &str,
    enabled: bool,
    token: &str,
) -> Result<ToggleResourceResponse, KagiError> {
    let redirects = execute_redirect_list(token).await?;
    let redirect = resolve_redirect_ref(&redirects, target)?;
    if redirect.enabled == enabled {
        return Ok(ToggleResourceResponse {
            id: redirect.id.clone(),
            enabled: redirect.enabled,
        });
    }

    post_authenticated_form(
        &http::kagi_url(KAGI_REDIRECTS_TOGGLE_PATH),
        &[("rule_id".to_string(), redirect.id.clone())],
        token,
        if enabled {
            "redirect enable"
        } else {
            "redirect disable"
        },
    )
    .await?;

    let refreshed = execute_redirect_list(token).await?;
    let redirect = resolve_redirect_ref(&refreshed, &redirect.id)?;
    Ok(ToggleResourceResponse {
        id: redirect.id.clone(),
        enabled: redirect.enabled,
    })
}

/// Asks Kagi Assistant a question about a specific web page.
///
/// # Arguments
/// * `request` - The ask-page request with URL and question.
/// * `token` - The Kagi session token.
///
/// # Returns
/// An `AskPageResponse` with the source, thread, and assistant message.
///
/// # Errors
/// Returns `KagiError::Config` if the URL or question is empty, or auth/network/parse errors.
pub async fn execute_ask_page(
    request: &AskPageRequest,
    token: &str,
) -> Result<AskPageResponse, KagiError> {
    let source_url = normalize_ask_page_url(&request.url)?;
    let question = normalize_ask_page_question(&request.question)?;
    let assistant = execute_assistant_prompt(
        &AssistantPromptRequest {
            query: build_ask_page_prompt(&source_url, &question),
            thread_id: None,
            attachments: Vec::new(),
            profile_id: None,
            model: None,
            lens_id: None,
            internet_access: None,
            personalizations: None,
        },
        token,
    )
    .await?;

    Ok(AskPageResponse {
        meta: assistant.meta,
        source: AskPageSource {
            url: source_url,
            question,
        },
        thread: assistant.thread,
        message: assistant.message,
    })
}

/// Translates text using Kagi Translate with session-token authentication.
///
/// Handles language detection, translation, and optional fetching of alternatives,
/// alignments, suggestions, and word insights.
///
/// # Arguments
/// * `request` - The translate command request with text, source/target languages, and options.
/// * `session_token` - The Kagi session token.
///
/// # Returns
/// A `TranslateResponse` with detection, translation, and optional supplementary data.
///
/// # Errors
/// Returns `KagiError::Auth` if the token is missing, `KagiError::Config` for invalid parameters,
/// or network/parse errors on failure.
pub async fn execute_translate(
    request: &TranslateCommandRequest,
    session_token: &str,
) -> Result<TranslateResponse, KagiError> {
    if session_token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi session token (expected KAGI_SESSION_TOKEN)".to_string(),
        ));
    }

    validate_translate_request(request)?;

    let bootstrap = bootstrap_translate_session(session_token).await?;
    let client = build_client()?;
    let cookie_header = build_translate_cookie_header(session_token, &bootstrap.translate_session);
    let detected_language =
        execute_translate_detect(&client, &cookie_header, request.text.trim()).await?;
    let effective_source_language =
        effective_translate_source_language(&request.from, &detected_language);
    let translation = execute_translate_text(
        &client,
        &cookie_header,
        request,
        &bootstrap.translate_session,
        &effective_source_language,
    )
    .await?;
    let target_language = request.to.clone();
    let translation = finalize_translate_text_response(
        translation,
        &detected_language,
        &effective_source_language,
        &target_language,
    );
    let translation_options = build_translate_option_state(request);
    let translated_text = translation.translation.clone();
    let translate_session = bootstrap.translate_session.clone();

    let (alternatives_result, alignments_result, suggestions_result, insights_result) = tokio::join!(
        capture_optional_translate_section(
            "alternatives",
            request.fetch_alternatives,
            execute_translate_alternatives(
                &client,
                &cookie_header,
                &translate_session,
                request,
                &effective_source_language,
                &translated_text,
                translation_options.as_ref(),
            ),
        ),
        capture_optional_translate_section(
            "text_alignments",
            request.fetch_alignments,
            execute_translate_text_alignments(
                &client,
                &cookie_header,
                &translate_session,
                request.text.trim(),
                &translated_text,
            ),
        ),
        capture_optional_translate_section(
            "translation_suggestions",
            request.fetch_suggestions,
            execute_translate_suggestions(
                &client,
                &cookie_header,
                &translate_session,
                TranslateSuggestionContext {
                    source_text: request.text.trim(),
                    target_text: &translated_text,
                    source_language: &effective_source_language,
                    target_language: &target_language,
                    translation_options: translation_options.as_ref(),
                },
            ),
        ),
        capture_optional_translate_section(
            "word_insights",
            request.fetch_word_insights,
            execute_translate_word_insights(
                &client,
                &cookie_header,
                &translate_session,
                request.text.trim(),
                &translated_text,
                &target_language,
                translation_options.as_ref(),
            ),
        ),
    );

    let (alternatives, alternatives_warning) = alternatives_result;
    let (text_alignments, alignments_warning) = alignments_result;
    let (translation_suggestions, suggestions_warning) = suggestions_result;
    let (word_insights, insights_warning) = insights_result;

    let warnings = vec![
        alternatives_warning,
        alignments_warning,
        suggestions_warning,
        insights_warning,
    ]
    .into_iter()
    .flatten()
    .collect();

    Ok(TranslateResponse {
        bootstrap: TranslateBootstrapMetadata {
            method: bootstrap.method,
            authenticated: true,
        },
        detected_language,
        translation,
        alternatives,
        text_alignments,
        translation_suggestions,
        word_insights,
        warnings,
    })
}

/// Answers a query using Kagi's FastGPT API with API-token authentication.
///
/// # Arguments
/// * `request` - The FastGPT request with query and optional parameters.
/// * `token` - The Kagi API token.
///
/// # Returns
/// A `FastGptResponse` with the answer.
///
/// # Errors
/// Returns `KagiError::Auth` if the token is missing, or network/parse errors on failure.
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
        .post(http::kagi_url(KAGI_FASTGPT_PATH))
        .header(header::AUTHORIZATION, format!("Bot {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .json(request)
        .send()
        .await
        .map_err(map_transport_error)?;

    decode_kagi_json(response, "FastGPT").await
}

/// Queries Kagi's web enrichment API.
///
/// # Arguments
/// * `query` - The enrichment query.
/// * `token` - The Kagi API token.
///
/// # Returns
/// An `EnrichResponse` with enrichment data.
///
/// # Errors
/// Returns network/parse errors on failure.
pub async fn execute_enrich_web(query: &str, token: &str) -> Result<EnrichResponse, KagiError> {
    execute_enrich(
        &http::kagi_url(KAGI_ENRICH_WEB_PATH),
        query,
        token,
        "web enrichment",
    )
    .await
}

/// Queries Kagi's news enrichment API.
///
/// # Arguments
/// * `query` - The enrichment query.
/// * `token` - The Kagi API token.
///
/// # Returns
/// An `EnrichResponse` with enrichment data.
///
/// # Errors
/// Returns network/parse errors on failure.
pub async fn execute_enrich_news(query: &str, token: &str) -> Result<EnrichResponse, KagiError> {
    execute_enrich(
        &http::kagi_url(KAGI_ENRICH_NEWS_PATH),
        query,
        token,
        "news enrichment",
    )
    .await
}

/// Fetches the Kagi Small Web feed.
///
/// # Arguments
/// * `limit` - Optional maximum number of entries to return.
///
/// # Returns
/// A `SmallWebFeed` with the feed entries.
///
/// # Errors
/// Returns network/parse errors on failure.
pub async fn execute_smallweb(limit: Option<u32>) -> Result<SmallWebFeed, KagiError> {
    let client = build_client()?;
    let mut request = client.get(http::kagi_url(KAGI_SMALLWEB_FEED_PATH));
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
        status if status.is_server_error() => {
            let body = http::read_error_body(response, "small web feed").await;
            Err(KagiError::Network(format!(
                "Kagi Small Web feed server error: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status => {
            let body = http::read_error_body(response, "small web feed").await;
            Err(KagiError::Network(format!(
                "unexpected Kagi Small Web feed status: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
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

async fn bootstrap_translate_session(
    session_token: &str,
) -> Result<TranslateBootstrapResult, KagiError> {
    let client = build_client()?;
    let mut last_error = None;

    for attempt in 0..TRANSLATE_BOOTSTRAP_MAX_ATTEMPTS {
        let response = client
            .get(http::kagi_translate_url("/"))
            .header(header::COOKIE, format!("kagi_session={session_token}"))
            .send()
            .await
            .map_err(map_transport_error)?;

        match resolve_translate_bootstrap(response.status(), response.headers()) {
            Ok(result) => return Ok(result),
            Err(error)
                if attempt + 1 < TRANSLATE_BOOTSTRAP_MAX_ATTEMPTS
                    && should_retry_translate_bootstrap(&error) =>
            {
                last_error = Some(error);
                sleep(Duration::from_millis(250 * (attempt as u64 + 1))).await;
            }
            Err(error) => return Err(error),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        KagiError::Network("Kagi Translate bootstrap failed after retries".to_string())
    }))
}

async fn execute_translate_detect(
    client: &Client,
    cookie_header: &str,
    text: &str,
) -> Result<TranslateDetectedLanguage, KagiError> {
    let response = client
        .post(http::kagi_translate_url(KAGI_TRANSLATE_DETECT_PATH))
        .header(header::COOKIE, cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&json!({
            "text": text,
            "include_alternatives": true,
        }))
        .send()
        .await
        .map_err(map_transport_error)?;

    let value: Value = decode_translate_json(response, "language detection").await?;
    parse_translate_detect_value(value)
}

async fn execute_translate_text(
    client: &Client,
    cookie_header: &str,
    request: &TranslateCommandRequest,
    translate_session: &str,
    effective_source_language: &str,
) -> Result<TranslateTextResponse, KagiError> {
    let response = client
        .post(http::kagi_translate_url(KAGI_TRANSLATE_PATH))
        .header(header::COOKIE, cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&build_translate_payload(
            request,
            translate_session,
            effective_source_language,
        ))
        .send()
        .await
        .map_err(map_transport_error)?;

    decode_translate_json(response, "translation").await
}

async fn execute_translate_alternatives(
    client: &Client,
    cookie_header: &str,
    translate_session: &str,
    request: &TranslateCommandRequest,
    effective_source_language: &str,
    translated_text: &str,
    translation_options: Option<&TranslateOptionState>,
) -> Result<AlternativeTranslationsResponse, KagiError> {
    let mut payload = Map::new();
    payload.insert(
        "original_text".to_string(),
        Value::String(request.text.clone()),
    );
    payload.insert(
        "existing_translation".to_string(),
        Value::String(translated_text.to_string()),
    );
    payload.insert(
        "source_lang".to_string(),
        Value::String(effective_source_language.to_string()),
    );
    payload.insert("target_lang".to_string(), Value::String(request.to.clone()));
    payload.insert(
        "session_token".to_string(),
        Value::String(translate_session.to_string()),
    );

    if let Some(quality) = normalize_aux_quality(request.quality.as_deref()) {
        payload.insert("quality".to_string(), Value::String(quality));
    }

    if let Some(options) = translation_options {
        payload.insert(
            "translation_options".to_string(),
            serde_json::to_value(options).map_err(|error| {
                KagiError::Parse(format!(
                    "failed to serialize translate alternatives options: {error}"
                ))
            })?,
        );
    }

    let response = client
        .post(http::kagi_translate_url(KAGI_TRANSLATE_ALTERNATIVES_PATH))
        .header(header::COOKIE, cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&Value::Object(payload))
        .send()
        .await
        .map_err(map_transport_error)?;

    decode_translate_json(response, "alternative translations").await
}

async fn execute_translate_text_alignments(
    client: &Client,
    cookie_header: &str,
    translate_session: &str,
    source_text: &str,
    target_text: &str,
) -> Result<TextAlignmentsResponse, KagiError> {
    let response = client
        .post(http::kagi_translate_url(KAGI_TRANSLATE_ALIGNMENTS_PATH))
        .header(header::COOKIE, cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&json!({
            "source_text": source_text,
            "target_text": target_text,
            "session_token": translate_session,
        }))
        .send()
        .await
        .map_err(map_transport_error)?;

    decode_translate_json(response, "text alignments").await
}

struct TranslateSuggestionContext<'a> {
    source_text: &'a str,
    target_text: &'a str,
    source_language: &'a str,
    target_language: &'a str,
    translation_options: Option<&'a TranslateOptionState>,
}

async fn execute_translate_suggestions(
    client: &Client,
    cookie_header: &str,
    translate_session: &str,
    context: TranslateSuggestionContext<'_>,
) -> Result<TranslationSuggestionsResponse, KagiError> {
    let response = client
        .post(http::kagi_translate_url(KAGI_TRANSLATE_SUGGESTIONS_PATH))
        .header(header::COOKIE, cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&build_translate_suggestions_payload(context, translate_session).map(Value::Object)?)
        .send()
        .await
        .map_err(map_transport_error)?;

    decode_translate_json(response, "translation suggestions").await
}

async fn execute_translate_word_insights(
    client: &Client,
    cookie_header: &str,
    translate_session: &str,
    source_text: &str,
    target_text: &str,
    explanation_language: &str,
    translation_options: Option<&TranslateOptionState>,
) -> Result<WordInsightsResponse, KagiError> {
    let response = client
        .post(http::kagi_translate_url(KAGI_TRANSLATE_WORD_INSIGHTS_PATH))
        .header(header::COOKIE, cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .json(
            &build_translate_word_insights_payload(
                source_text,
                target_text,
                explanation_language,
                translate_session,
                translation_options,
            )
            .map(Value::Object)?,
        )
        .send()
        .await
        .map_err(map_transport_error)?;

    decode_translate_json(response, "word insights").await
}

async fn capture_optional_translate_section<T, F>(
    section: &'static str,
    enabled: bool,
    future: F,
) -> (Option<T>, Option<TranslateWarning>)
where
    F: Future<Output = Result<T, KagiError>>,
{
    if !enabled {
        return (None, None);
    }

    match future.await {
        Ok(value) => (Some(value), None),
        Err(error) => (
            None,
            Some(TranslateWarning {
                section: section.to_string(),
                message: error.to_string(),
            }),
        ),
    }
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
    let mut final_output: Option<String> = None;
    let mut final_error: Option<String> = None;

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
            "final" => {
                let message: SubscriberSummaryAssistantFrame = serde_json::from_str(payload)
                    .map_err(|error| {
                        KagiError::Parse(format!(
                            "failed to parse subscriber summarizer final frame: {error}"
                        ))
                    })?;
                if message.error {
                    let detail = message.output_text.trim();
                    final_error = Some(if detail.is_empty() {
                        "Kagi subscriber summarizer returned an error state".to_string()
                    } else {
                        format!("Kagi subscriber summarizer failed: {detail}")
                    });
                } else if !message.output_text.trim().is_empty() {
                    final_output = Some(message.output_text);
                }
            }
            _ => {
                debug!(tag, "ignoring unknown subscriber summarizer stream frame");
            }
        }
    }

    let Some(message) = last_message else {
        if let Some(detail) = final_error {
            return Err(KagiError::Network(detail));
        }
        let output = final_output.ok_or_else(|| {
            KagiError::Parse(
                "subscriber summarizer response did not include a final summary frame".to_string(),
            )
        })?;
        return Ok(SubscriberSummarizeResponse {
            meta,
            data: SubscriberSummarization {
                id: String::new(),
                thread_id: String::new(),
                created_at: String::new(),
                state: "done".to_string(),
                prompt: String::new(),
                markdown: output.clone(),
                output,
                metadata_html: String::new(),
                documents: Vec::new(),
            },
        });
    };

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

fn load_news_filter_presets() -> Result<Vec<NewsFilterPresetDefinition>, KagiError> {
    serde_json::from_str::<NewsFilterPresetFile>(NEWS_FILTER_PRESETS_JSON)
        .map(|payload| payload.filters)
        .map_err(|error| {
            KagiError::Parse(format!(
                "failed to parse vendored news filter presets: {error}"
            ))
        })
}

fn apply_news_content_filters(
    stories: Vec<Value>,
    request: &NewsFilterRequest,
    lang: &str,
) -> Result<AppliedNewsFilter, KagiError> {
    let resolved = resolve_news_filter_request(request, lang)?;
    let mut visible_stories = Vec::with_capacity(stories.len());
    let mut filtered_count = 0_u64;

    for mut story in stories {
        let matched_keywords =
            match_news_story_keywords(&story, &resolved.active_keywords, resolved.scope);
        if matched_keywords.is_empty() {
            visible_stories.push(story);
            continue;
        }

        filtered_count += 1;
        match resolved.mode {
            NewsFilterMode::Hide => {}
            NewsFilterMode::Blur => {
                annotate_news_story_with_filter(&mut story, &matched_keywords);
                visible_stories.push(story);
            }
        }
    }

    Ok(AppliedNewsFilter {
        stories: visible_stories,
        summary: NewsContentFilterSummary {
            mode: resolved.mode.as_str().to_string(),
            scope: resolved.scope.as_str().to_string(),
            active_presets: resolved.active_presets,
            custom_keywords: resolved.custom_keywords,
            active_keywords: resolved.active_keywords,
            filtered_count,
        },
    })
}

fn resolve_news_filter_request(
    request: &NewsFilterRequest,
    lang: &str,
) -> Result<ResolvedNewsFilter, KagiError> {
    if request.preset_ids.is_empty() && request.keywords.is_empty() {
        return Err(KagiError::Config(
            "news filters require at least one --filter-preset or --filter-keyword".to_string(),
        ));
    }

    let presets = load_news_filter_presets()?;
    let valid_ids = presets
        .iter()
        .map(|preset| preset.id.as_str())
        .collect::<Vec<_>>();
    let mut active_presets = Vec::new();
    let mut active_keywords = Vec::new();

    for preset_id in &request.preset_ids {
        let normalized_preset_id = preset_id.trim();
        if normalized_preset_id.is_empty() {
            return Err(KagiError::Config(
                "news filter preset id cannot be empty".to_string(),
            ));
        }

        let preset = presets
            .iter()
            .find(|preset| preset.id.eq_ignore_ascii_case(normalized_preset_id))
            .ok_or_else(|| {
                KagiError::Config(format!(
                    "unknown news filter preset '{normalized_preset_id}'. Run `kagi news --list-filter-presets` to inspect available presets. Valid preset ids: {}",
                    valid_ids.join(", ")
                ))
            })?;

        push_unique_string(&mut active_presets, preset.id.clone());
        for keyword in preset.resolve_keywords(lang) {
            if let Some(normalized_keyword) = normalize_news_filter_keyword(&keyword) {
                push_unique_string(&mut active_keywords, normalized_keyword);
            }
        }
    }

    let mut custom_keywords = Vec::new();
    for keyword in &request.keywords {
        if let Some(normalized_keyword) = normalize_news_filter_keyword(keyword) {
            push_unique_string(&mut custom_keywords, normalized_keyword.clone());
            push_unique_string(&mut active_keywords, normalized_keyword);
        }
    }

    if active_keywords.is_empty() {
        return Err(KagiError::Config(
            "news filters require at least one non-empty keyword".to_string(),
        ));
    }

    Ok(ResolvedNewsFilter {
        active_presets,
        custom_keywords,
        active_keywords,
        mode: request.mode,
        scope: request.scope,
    })
}

fn normalize_news_filter_keyword(raw: &str) -> Option<String> {
    let normalized = raw.trim().to_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn push_unique_string(values: &mut Vec<String>, candidate: String) {
    if !values.contains(&candidate) {
        values.push(candidate);
    }
}

fn match_news_story_keywords(
    story: &Value,
    keywords: &[String],
    scope: NewsFilterScope,
) -> Vec<String> {
    if keywords.is_empty() {
        return vec![];
    }

    let searchable_text = collect_news_story_text(story, scope);
    if searchable_text.is_empty() {
        return vec![];
    }
    let lowered_text = searchable_text.to_lowercase();

    keywords
        .iter()
        .filter(|keyword| text_contains_news_filter_keyword(&lowered_text, keyword))
        .cloned()
        .collect()
}

fn collect_news_story_text(story: &Value, scope: NewsFilterScope) -> String {
    let mut parts = Vec::new();

    if matches!(scope, NewsFilterScope::Title | NewsFilterScope::All) {
        push_story_string_field(story, "title", &mut parts);
        push_story_string_field(story, "category", &mut parts);
    }

    if matches!(scope, NewsFilterScope::Summary | NewsFilterScope::All) {
        push_story_string_field(story, "short_summary", &mut parts);
    }

    if matches!(scope, NewsFilterScope::All) {
        if let Some(perspectives) = story.get("perspectives").and_then(Value::as_array) {
            for perspective in perspectives {
                push_story_string_field(perspective, "text", &mut parts);
                if let Some(sources) = perspective.get("sources").and_then(Value::as_array) {
                    for source in sources {
                        push_story_string_field(source, "name", &mut parts);
                    }
                }
            }
        }

        if let Some(domains) = story.get("domains").and_then(Value::as_array) {
            for domain in domains {
                push_story_string_field(domain, "name", &mut parts);
            }
        }

        if let Some(articles) = story.get("articles").and_then(Value::as_array) {
            for article in articles {
                push_story_string_field(article, "link", &mut parts);
                push_story_string_field(article, "domain", &mut parts);
            }
        }
    }

    parts.join(" ")
}

fn push_story_string_field(value: &Value, field: &str, parts: &mut Vec<String>) {
    if let Some(text) = value.get(field).and_then(Value::as_str) {
        let normalized = text.trim();
        if !normalized.is_empty() {
            parts.push(normalized.to_string());
        }
    }
}

fn text_contains_news_filter_keyword(text: &str, keyword: &str) -> bool {
    if keyword.is_empty() {
        return false;
    }

    for (index, _) in text.match_indices(keyword) {
        let start_ok = text[..index]
            .chars()
            .next_back()
            .is_none_or(|ch| !is_news_filter_word_char(ch));
        let end_index = index + keyword.len();
        let end_ok = text[end_index..]
            .chars()
            .next()
            .is_none_or(|ch| !is_news_filter_word_char(ch));

        if start_ok && end_ok {
            return true;
        }
    }

    false
}

const fn is_news_filter_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn annotate_news_story_with_filter(story: &mut Value, matched_keywords: &[String]) {
    let Some(object) = story.as_object_mut() else {
        return;
    };

    object.insert(
        "_content_filter".to_string(),
        serde_json::to_value(NewsStoryContentFilterSummary {
            mode: NewsFilterMode::Blur.as_str().to_string(),
            matched_keywords: matched_keywords.to_vec(),
        })
        .expect("news story filter metadata should serialize"),
    );
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
                .is_some_and(|entry| entry.display_name.eq_ignore_ascii_case(requested))
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
            let thread_id =
                assistant_thread_id_from_url(normalized).unwrap_or_else(|| normalized.to_string());
            validate_current_assistant_path_id("assistant thread id", &thread_id)?;
            Ok(Some(thread_id))
        }
    }
}

struct AssistantThreadRef {
    thread_id: String,
    branch_id: Option<String>,
}

fn normalize_assistant_thread_ref(raw: &str) -> Result<AssistantThreadRef, KagiError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(KagiError::Config(
            "assistant thread id cannot be empty".to_string(),
        ));
    }

    let thread_ref =
        assistant_thread_ref_from_url(normalized).unwrap_or_else(|| AssistantThreadRef {
            thread_id: normalized.to_string(),
            branch_id: None,
        });
    validate_current_assistant_path_id("assistant thread id", &thread_ref.thread_id)?;
    if let Some(branch_id) = &thread_ref.branch_id {
        validate_current_assistant_path_id("assistant branch id", branch_id)?;
    }
    Ok(thread_ref)
}

fn validate_current_assistant_path_id(label: &str, value: &str) -> Result<(), KagiError> {
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Ok(());
    }

    Err(KagiError::Config(format!(
        "{label} may only contain ASCII letters, digits, '-' or '_'"
    )))
}

fn assistant_thread_id_from_url(value: &str) -> Option<String> {
    assistant_thread_ref_from_url(value).map(|thread_ref| thread_ref.thread_id)
}

fn assistant_thread_ref_from_url(value: &str) -> Option<AssistantThreadRef> {
    let url = Url::parse(value).ok()?;
    let host = url.host_str()?;
    if !matches!(host, "assistant.kagi.com" | "kagi.com" | "www.kagi.com") {
        return None;
    }

    let segments = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let (thread_id, branch_id) = match segments.as_slice() {
        ["chat", thread_id] => (*thread_id, None),
        ["chat", thread_id, branch_id] => (*thread_id, Some(*branch_id)),
        ["assistant", "chat", thread_id] => (*thread_id, None),
        ["assistant", "thread", thread_id] => (*thread_id, None),
        ["assistant", thread_id] => (*thread_id, None),
        _ => return None,
    };

    (!thread_id.trim().is_empty()).then(|| AssistantThreadRef {
        thread_id: thread_id.to_string(),
        branch_id: branch_id.map(str::to_string),
    })
}

fn normalize_named_target(raw: &str, label: &str) -> Result<String, KagiError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(KagiError::Config(format!("{label} cannot be empty")));
    }

    Ok(normalized.to_string())
}

fn normalize_lens_name(raw: &str) -> Result<String, KagiError> {
    let normalized = normalize_named_target(raw, "lens name")?;
    let char_count = normalized.chars().count();
    if char_count > KAGI_LENS_NAME_MAX_CHARS {
        return Err(KagiError::Config(format!(
            "lens name must be at most {KAGI_LENS_NAME_MAX_CHARS} characters; Kagi truncates longer names"
        )));
    }

    Ok(normalized)
}

fn normalize_custom_bang_trigger(raw: &str) -> Result<String, KagiError> {
    let normalized = raw.trim().trim_start_matches('!').trim();
    if normalized.is_empty() {
        return Err(KagiError::Config(
            "custom bang trigger cannot be empty".to_string(),
        ));
    }

    Ok(normalized.to_string())
}

fn normalize_redirect_rule(raw: &str) -> Result<String, KagiError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(KagiError::Config(
            "redirect rule cannot be empty".to_string(),
        ));
    }
    if !normalized.contains('|') {
        return Err(KagiError::Config(
            "redirect rule must use the form regex|replacement".to_string(),
        ));
    }

    Ok(normalized.to_string())
}

fn trimmed_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn debug_body_preview(body: &str) -> &str {
    match body.char_indices().nth(DEBUG_BODY_PREVIEW_LIMIT) {
        Some((idx, _)) => &body[..idx],
        None => body,
    }
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

fn absolute_kagi_url(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        http::kagi_url(path)
    }
}

fn url_query_value(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.into_owned())
}

fn build_lens_form(details: &LensDetails) -> Vec<(String, String)> {
    let mut form = vec![
        ("name".to_string(), details.name.clone()),
        ("included_sites".to_string(), details.included_sites.clone()),
        (
            "included_keywords".to_string(),
            details.included_keywords.clone(),
        ),
        ("description".to_string(), details.description.clone()),
        ("search_region".to_string(), details.search_region.clone()),
        ("date_range".to_string(), "0".to_string()),
        (
            "before_time".to_string(),
            details.before_time.clone().unwrap_or_default(),
        ),
        (
            "after_time".to_string(),
            details.after_time.clone().unwrap_or_default(),
        ),
        ("excluded_sites".to_string(), details.excluded_sites.clone()),
        (
            "excluded_keywords".to_string(),
            details.excluded_keywords.clone(),
        ),
        (
            "shortcut_keyword".to_string(),
            details.shortcut_keyword.clone(),
        ),
        (
            "autocomplete_keywords".to_string(),
            if details.autocomplete_keywords {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
        ("template".to_string(), details.template.clone()),
        ("file_type".to_string(), details.file_type.clone()),
        (
            "share_with_team".to_string(),
            if details.share_with_team {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
        (
            "share_copy_code".to_string(),
            if details.share_copy_code {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
    ];

    if let Some(id) = details.id.as_ref() {
        form.push(("id".to_string(), id.clone()));
    }

    form
}

fn build_custom_bang_form(details: &CustomBangDetails, delete: bool) -> Vec<(String, String)> {
    let mut form = Vec::new();
    if let Some(id) = details.bang_id.as_ref() {
        form.push(("bang_id".to_string(), id.clone()));
    }
    if delete {
        form.push(("delete".to_string(), "1".to_string()));
        return form;
    }

    form.extend([
        ("name".to_string(), details.name.clone()),
        ("trigger".to_string(), details.trigger.clone()),
        ("template".to_string(), details.template.clone()),
        ("snap_domain".to_string(), details.snap_domain.clone()),
        ("regex_pattern".to_string(), details.regex_pattern.clone()),
        (
            "shortcut_menu".to_string(),
            if details.shortcut_menu {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
        (
            "fmt_open_snap_domain".to_string(),
            if details.fmt_open_snap_domain {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
        (
            "fmt_open_base_path".to_string(),
            if details.fmt_open_base_path {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
        (
            "fmt_url_encode_placeholder".to_string(),
            if details.fmt_url_encode_placeholder {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
        (
            "fmt_url_encode_space_to_plus".to_string(),
            if details.fmt_url_encode_space_to_plus {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
    ]);

    form
}

fn apply_lens_create_request(
    details: &mut LensDetails,
    request: &LensCreateRequest,
) -> Result<(), KagiError> {
    details.name = normalize_lens_name(&request.name)?;
    if let Some(value) = request.included_sites.as_ref() {
        details.included_sites = value.clone();
    }
    if let Some(value) = request.included_keywords.as_ref() {
        details.included_keywords = value.clone();
    }
    if let Some(value) = request.description.as_ref() {
        details.description = value.clone();
    }
    if let Some(value) = trimmed_optional(request.search_region.as_deref()) {
        details.search_region = value.to_string();
    }
    if let Some(value) = request.before_time.as_ref() {
        details.before_time = normalize_optional_form_value(Some(value.clone()));
    }
    if let Some(value) = request.after_time.as_ref() {
        details.after_time = normalize_optional_form_value(Some(value.clone()));
    }
    if let Some(value) = request.excluded_sites.as_ref() {
        details.excluded_sites = value.clone();
    }
    if let Some(value) = request.excluded_keywords.as_ref() {
        details.excluded_keywords = value.clone();
    }
    if let Some(value) = request.shortcut_keyword.as_ref() {
        details.shortcut_keyword = value.clone();
    }
    if let Some(value) = request.autocomplete_keywords {
        details.autocomplete_keywords = value;
    }
    if let Some(value) = request.template.as_ref() {
        details.template = value.clone();
    }
    if let Some(value) = request.file_type.as_ref() {
        details.file_type = value.clone();
    }
    if let Some(value) = request.share_with_team {
        details.share_with_team = value;
    }
    if let Some(value) = request.share_copy_code {
        details.share_copy_code = value;
    }

    Ok(())
}

fn apply_lens_update_request(
    details: &mut LensDetails,
    request: &LensUpdateRequest,
) -> Result<(), KagiError> {
    if let Some(value) = request.name.as_deref() {
        details.name = normalize_lens_name(value)?;
    }
    if let Some(value) = request.included_sites.as_ref() {
        details.included_sites = value.clone();
    }
    if let Some(value) = request.included_keywords.as_ref() {
        details.included_keywords = value.clone();
    }
    if let Some(value) = request.description.as_ref() {
        details.description = value.clone();
    }
    if let Some(value) = trimmed_optional(request.search_region.as_deref()) {
        details.search_region = value.to_string();
    }
    if let Some(value) = request.before_time.as_ref() {
        details.before_time = normalize_optional_form_value(Some(value.clone()));
    }
    if let Some(value) = request.after_time.as_ref() {
        details.after_time = normalize_optional_form_value(Some(value.clone()));
    }
    if let Some(value) = request.excluded_sites.as_ref() {
        details.excluded_sites = value.clone();
    }
    if let Some(value) = request.excluded_keywords.as_ref() {
        details.excluded_keywords = value.clone();
    }
    if let Some(value) = request.shortcut_keyword.as_ref() {
        details.shortcut_keyword = value.clone();
    }
    if let Some(value) = request.autocomplete_keywords {
        details.autocomplete_keywords = value;
    }
    if let Some(value) = request.template.as_ref() {
        details.template = value.clone();
    }
    if let Some(value) = request.file_type.as_ref() {
        details.file_type = value.clone();
    }
    if let Some(value) = request.share_with_team {
        details.share_with_team = value;
    }
    if let Some(value) = request.share_copy_code {
        details.share_copy_code = value;
    }

    Ok(())
}

fn apply_custom_bang_create_request(
    details: &mut CustomBangDetails,
    request: &CustomBangCreateRequest,
) -> Result<(), KagiError> {
    details.name = normalize_named_target(&request.name, "custom bang name")?;
    details.trigger = normalize_custom_bang_trigger(&request.trigger)?;
    if let Some(value) = request.template.as_ref() {
        details.template = value.clone();
    }
    if let Some(value) = request.snap_domain.as_ref() {
        details.snap_domain = value.clone();
    }
    if let Some(value) = request.regex_pattern.as_ref() {
        details.regex_pattern = value.clone();
    }
    if let Some(value) = request.shortcut_menu {
        details.shortcut_menu = value;
    }
    if let Some(value) = request.fmt_open_snap_domain {
        details.fmt_open_snap_domain = value;
    }
    if let Some(value) = request.fmt_open_base_path {
        details.fmt_open_base_path = value;
    }
    if let Some(value) = request.fmt_url_encode_placeholder {
        details.fmt_url_encode_placeholder = value;
    }
    if let Some(value) = request.fmt_url_encode_space_to_plus {
        details.fmt_url_encode_space_to_plus = value;
    }

    Ok(())
}

fn apply_custom_bang_update_request(
    details: &mut CustomBangDetails,
    request: &CustomBangUpdateRequest,
) -> Result<(), KagiError> {
    if let Some(value) = request.name.as_deref() {
        details.name = normalize_named_target(value, "custom bang name")?;
    }
    if let Some(value) = request.trigger.as_deref() {
        details.trigger = normalize_custom_bang_trigger(value)?;
    }
    if let Some(value) = request.template.as_ref() {
        details.template = value.clone();
    }
    if let Some(value) = request.snap_domain.as_ref() {
        details.snap_domain = value.clone();
    }
    if let Some(value) = request.regex_pattern.as_ref() {
        details.regex_pattern = value.clone();
    }
    if let Some(value) = request.shortcut_menu {
        details.shortcut_menu = value;
    }
    if let Some(value) = request.fmt_open_snap_domain {
        details.fmt_open_snap_domain = value;
    }
    if let Some(value) = request.fmt_open_base_path {
        details.fmt_open_base_path = value;
    }
    if let Some(value) = request.fmt_url_encode_placeholder {
        details.fmt_url_encode_placeholder = value;
    }
    if let Some(value) = request.fmt_url_encode_space_to_plus {
        details.fmt_url_encode_space_to_plus = value;
    }

    Ok(())
}

fn resolve_custom_assistant_ref<'a>(
    assistants: &'a [AssistantProfileSummary],
    target: &str,
    require_editable: bool,
) -> Result<&'a AssistantProfileSummary, KagiError> {
    let target = normalize_named_target(target, "assistant target")?;
    let assistant = assistants
        .iter()
        .find(|assistant| {
            assistant.id == target || assistant.invoke_profile.eq_ignore_ascii_case(&target)
        })
        .or_else(|| {
            let matches = assistants
                .iter()
                .filter(|assistant| assistant.name.eq_ignore_ascii_case(&target))
                .collect::<Vec<_>>();
            if matches.len() == 1 {
                matches.into_iter().next()
            } else {
                None
            }
        })
        .ok_or_else(|| KagiError::Config(format!("no assistant matched '{target}'")))?;

    if require_editable && assistant.edit_url.is_none() {
        return Err(KagiError::Config(format!(
            "assistant '{}' is built in and cannot be modified through custom-assistant commands",
            assistant.name
        )));
    }

    Ok(assistant)
}

fn current_custom_assistant_for_target<'a>(
    response: &'a CurrentAssistantInitResponse,
    target: &str,
) -> Result<&'a CurrentAssistantCustomAssistant, KagiError> {
    let assistant_id = resolve_custom_assistant_ref(&response.assistant_summaries(), target, true)?
        .id
        .clone();
    response
        .custom_assistants
        .iter()
        .find(|assistant| assistant.uuid == assistant_id)
        .ok_or_else(|| {
            KagiError::Parse(format!(
                "custom assistant '{}' was listed without a definition",
                assistant_id
            ))
        })
}

fn current_assistant_profile_payload(details: &AssistantProfileDetails) -> Value {
    let instructions =
        (!details.custom_instructions.is_empty()).then_some(details.custom_instructions.as_str());
    let selected_lens = trimmed_optional(Some(&details.selected_lens));
    json!({
        "name": details.name,
        "llm_id": details.base_model,
        "instructions": instructions,
        "internet_access": details.internet_access,
        "personalizations": details.personalizations,
        "lens_id": selected_lens,
        "bang_trigger": details.bang_trigger,
    })
}

fn resolve_lens_ref<'a>(
    lenses: &'a [LensSummary],
    target: &str,
) -> Result<&'a LensSummary, KagiError> {
    let target = normalize_named_target(target, "lens target")?;
    lenses
        .iter()
        .find(|lens| lens.id == target || lens.name.eq_ignore_ascii_case(&target))
        .ok_or_else(|| KagiError::Config(format!("no lens matched '{target}'")))
}

async fn execute_lens_get_after_mutation(
    target: &str,
    token: &str,
    operation: &str,
) -> Result<LensDetails, KagiError> {
    let mut last_retryable_error = None;

    // Kagi can redirect from a successful lens mutation before the settings
    // page reflects the new/updated lens. Retry only that narrow lookup gap.
    for attempt in 0..LENS_MUTATION_LOOKUP_ATTEMPTS {
        match execute_lens_get(target, token).await {
            Ok(details) => return Ok(details),
            Err(error)
                if attempt + 1 < LENS_MUTATION_LOOKUP_ATTEMPTS
                    && should_retry_lens_mutation_lookup(&error) =>
            {
                last_retryable_error = Some(error);
                sleep(Duration::from_millis(LENS_MUTATION_LOOKUP_DELAY_MS)).await;
            }
            Err(error) => return Err(error),
        }
    }

    Err(lens_mutation_lookup_timeout_error(
        target,
        operation,
        last_retryable_error,
    ))
}

async fn resolve_lens_id_by_name_after_mutation(
    name: &str,
    token: &str,
    operation: &str,
) -> Result<String, KagiError> {
    let mut last_retryable_error = None;

    for attempt in 0..LENS_MUTATION_LOOKUP_ATTEMPTS {
        match resolve_lens_id_by_name(name, token).await {
            Ok(id) => return Ok(id),
            Err(error)
                if attempt + 1 < LENS_MUTATION_LOOKUP_ATTEMPTS
                    && should_retry_lens_mutation_lookup(&error) =>
            {
                last_retryable_error = Some(error);
                sleep(Duration::from_millis(LENS_MUTATION_LOOKUP_DELAY_MS)).await;
            }
            Err(error) => return Err(error),
        }
    }

    Err(lens_mutation_lookup_timeout_error(
        name,
        operation,
        last_retryable_error,
    ))
}

fn lens_mutation_lookup_timeout_error(
    target: &str,
    operation: &str,
    last_error: Option<KagiError>,
) -> KagiError {
    match last_error {
        Some(error) => KagiError::Config(format!(
            "lens '{target}' did not become visible after {operation} within {LENS_MUTATION_LOOKUP_ATTEMPTS} attempts: {error}"
        )),
        None => KagiError::Config(format!(
            "lens '{target}' did not become visible after {operation} within {LENS_MUTATION_LOOKUP_ATTEMPTS} attempts"
        )),
    }
}

fn should_retry_lens_mutation_lookup(error: &KagiError) -> bool {
    match error {
        KagiError::Config(message) => message.starts_with("no lens matched "),
        KagiError::Network(message) => {
            message.contains("error decoding response body")
                && (message.contains("lens settings page") || message.contains("lens form"))
        }
        _ => false,
    }
}

fn resolve_custom_bang_ref<'a>(
    bangs: &'a [CustomBangSummary],
    target: &str,
) -> Result<&'a CustomBangSummary, KagiError> {
    let target = normalize_named_target(target, "custom bang target")?;
    let normalized_trigger = target.trim_start_matches('!');
    bangs
        .iter()
        .find(|bang| {
            bang.id == target
                || bang.name.eq_ignore_ascii_case(&target)
                || bang
                    .trigger
                    .trim_start_matches('!')
                    .eq_ignore_ascii_case(normalized_trigger)
        })
        .ok_or_else(|| KagiError::Config(format!("no custom bang matched '{target}'")))
}

fn resolve_redirect_ref<'a>(
    redirects: &'a [RedirectRuleSummary],
    target: &str,
) -> Result<&'a RedirectRuleSummary, KagiError> {
    let target = normalize_named_target(target, "redirect target")?;
    redirects
        .iter()
        .find(|redirect| redirect.id == target || redirect.rule == target)
        .ok_or_else(|| KagiError::Config(format!("no redirect matched '{target}'")))
}

async fn resolve_lens_id_by_name(name: &str, token: &str) -> Result<String, KagiError> {
    let lenses = execute_lens_list(token).await?;
    resolve_lens_ref(&lenses, name).map(|lens| lens.id.clone())
}

async fn resolve_custom_bang_id_by_trigger(
    trigger: &str,
    token: &str,
) -> Result<String, KagiError> {
    let bangs = execute_custom_bang_list(token).await?;
    resolve_custom_bang_ref(&bangs, trigger).map(|bang| bang.id.clone())
}

async fn fetch_authenticated_html(
    url: &str,
    token: &str,
    surface: &str,
) -> Result<String, KagiError> {
    let client = build_client()?;
    let response = client
        .get(url)
        .header(header::COOKIE, format!("kagi_session={token}"))
        .send()
        .await
        .map_err(map_transport_error)?;
    let (_, body) = read_authenticated_html_response(response, surface).await?;
    Ok(body)
}

async fn post_authenticated_form(
    url: &str,
    form: &[(String, String)],
    token: &str,
    surface: &str,
) -> Result<(Url, String), KagiError> {
    let client = build_client()?;
    let response = client
        .post(url)
        .header(header::COOKIE, format!("kagi_session={token}"))
        .form(form)
        .send()
        .await
        .map_err(map_transport_error)?;
    read_authenticated_html_response(response, surface).await
}

async fn read_authenticated_html_response(
    response: reqwest::Response,
    surface: &str,
) -> Result<(Url, String), KagiError> {
    let status = response.status();
    let final_url = response.url().clone();

    match status {
        status if status.is_success() => {
            let body = response.text().await.map_err(|error| {
                KagiError::Network(format!("failed to read {surface} response body: {error}"))
            })?;
            if looks_like_logged_out_page(&body) {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }
            Ok((final_url, body))
        }
        status @ (StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Auth(format!(
                "invalid or expired Kagi session token for {surface}: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_client_error() => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Config(format!(
                "Kagi {surface} request rejected: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_server_error() => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Network(format!(
                "Kagi {surface} server error: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Network(format!(
                "unexpected Kagi {surface} response status: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
    }
}

fn looks_like_logged_out_page(body: &str) -> bool {
    KAGI_LOGGED_OUT_MARKERS
        .iter()
        .all(|marker| body.contains(marker))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssistantAttachmentPayload {
    path: PathBuf,
    filename: String,
    content_type: String,
    bytes: Vec<u8>,
}

fn load_assistant_attachment(path: &Path) -> Result<AssistantAttachmentPayload, KagiError> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            KagiError::Config(format!(
                "assistant attachment '{}' must include a file name",
                path.display()
            ))
        })?
        .to_string();

    let bytes = fs::read(path).map_err(|error| {
        KagiError::Config(format!(
            "failed to read assistant attachment '{}': {error}",
            path.display()
        ))
    })?;

    Ok(AssistantAttachmentPayload {
        path: path.to_path_buf(),
        filename,
        content_type: mime_guess::from_path(path)
            .first_or_octet_stream()
            .essence_str()
            .to_string(),
        bytes,
    })
}

async fn fetch_current_assistant_json<T>(
    path: &str,
    token: &str,
    surface: &str,
) -> Result<T, KagiError>
where
    T: DeserializeOwned,
{
    let client = build_client()?;
    let response = client
        .get(http::kagi_assistant_url(path))
        .header(header::COOKIE, format!("kagi_session={token}"))
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(map_transport_error)?;

    read_current_assistant_json_response(response, surface).await
}

async fn fetch_current_assistant_profiles(
    token: &str,
    surface: &str,
) -> Result<CurrentAssistantInitResponse, KagiError> {
    fetch_current_assistant_json(KAGI_ASSISTANT_INIT_PATH, token, surface).await
}

async fn send_current_assistant_json<T>(
    method: Method,
    path: &str,
    payload: &Value,
    token: &str,
    surface: &str,
) -> Result<T, KagiError>
where
    T: DeserializeOwned,
{
    let client = build_client()?;
    let response = client
        .request(method, http::kagi_assistant_url(path))
        .header(header::COOKIE, format!("kagi_session={token}"))
        .header(header::ACCEPT, "application/json")
        .header(header::CONTENT_TYPE, "application/json")
        .json(payload)
        .send()
        .await
        .map_err(map_transport_error)?;

    read_current_assistant_json_response(response, surface).await
}

async fn delete_current_assistant_resource(
    path: &str,
    token: &str,
    surface: &str,
) -> Result<(), KagiError> {
    let client = build_client()?;
    let response = client
        .delete(http::kagi_assistant_url(path))
        .header(header::COOKIE, format!("kagi_session={token}"))
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(map_transport_error)?;

    let status = response.status();
    if status == StatusCode::NO_CONTENT {
        return Ok(());
    }

    let body = response.text().await.map_err(|error| {
        KagiError::Network(format!("failed to read {surface} response body: {error}"))
    })?;

    if looks_like_html_document(&body) {
        return Err(KagiError::Auth(
            "invalid or expired Kagi session token".to_string(),
        ));
    }

    if status.is_success() {
        return Ok(());
    }

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(KagiError::Auth(format!(
            "invalid or expired Kagi session token for {surface}: HTTP {status}{}",
            assistant_error_suffix(&body)
        )));
    }

    if status.is_client_error() {
        return Err(KagiError::Config(format!(
            "Kagi {surface} request rejected: HTTP {status}{}",
            assistant_error_suffix(&body)
        )));
    }

    if status.is_server_error() {
        return Err(KagiError::Network(format!(
            "Kagi {surface} server error: HTTP {status}{}",
            assistant_error_suffix(&body)
        )));
    }

    Err(KagiError::Network(format!(
        "unexpected Kagi {surface} response status: HTTP {status}{}",
        assistant_error_suffix(&body)
    )))
}

async fn read_current_assistant_json_response<T>(
    response: reqwest::Response,
    surface: &str,
) -> Result<T, KagiError>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        KagiError::Network(format!("failed to read {surface} response body: {error}"))
    })?;

    if looks_like_html_document(&body) {
        return Err(KagiError::Auth(
            "invalid or expired Kagi session token".to_string(),
        ));
    }

    if status.is_success() {
        return serde_json::from_str(&body).map_err(|error| {
            assistant_json_error(&body, surface).unwrap_or_else(|| {
                KagiError::Parse(format!("failed to parse {surface} response: {error}"))
            })
        });
    }

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(KagiError::Auth(format!(
            "invalid or expired Kagi session token for {surface}: HTTP {status}{}",
            assistant_error_suffix(&body)
        )));
    }

    if status.is_client_error() {
        return Err(KagiError::Config(format!(
            "Kagi {surface} request rejected: HTTP {status}{}",
            assistant_error_suffix(&body)
        )));
    }

    if status.is_server_error() {
        return Err(KagiError::Network(format!(
            "Kagi {surface} server error: HTTP {status}{}",
            assistant_error_suffix(&body)
        )));
    }

    Err(KagiError::Network(format!(
        "unexpected Kagi {surface} response status: HTTP {status}{}",
        assistant_error_suffix(&body)
    )))
}

fn current_assistant_conversations_path(after_id: Option<&str>) -> String {
    let mut path = String::from("/api/conversations?limit=100&all=true");
    if let Some(after_id) = after_id {
        path.push_str("&after_id=");
        path.push_str(after_id);
    }
    path
}

fn current_assistant_conversation_init_path(thread_ref: &AssistantThreadRef) -> String {
    let mut path = format!("/api/conversations/{}/init", thread_ref.thread_id);
    if let Some(branch_id) = &thread_ref.branch_id {
        path.push_str("?branch_uuid=");
        path.push_str(branch_id);
    }
    path
}

async fn fetch_current_assistant_messages(
    token: &str,
    branch: Option<&CurrentAssistantBranch>,
    page: CurrentAssistantMessagesPage,
) -> Result<Vec<CurrentAssistantMessage>, KagiError> {
    let mut messages = page.items;
    let mut has_more = page.has_more;

    while has_more {
        let branch = branch.ok_or_else(|| {
            KagiError::Parse(
                "Assistant conversation has more messages but no active branch".to_string(),
            )
        })?;
        let after_id = messages
            .first()
            .and_then(|message| message.uuid.as_deref())
            .ok_or_else(|| {
                KagiError::Parse(
                    "Assistant conversation has more messages but no message cursor".to_string(),
                )
            })?;
        let path = current_assistant_branch_messages_path(&branch.uuid, after_id)?;
        let response = fetch_current_assistant_json::<CurrentAssistantBranchMessagesResponse>(
            &path,
            token,
            "Assistant branch messages",
        )
        .await?;

        let mut older_messages = response.messages.items;
        older_messages.reverse();
        older_messages.extend(messages);
        messages = older_messages;
        has_more = response.messages.has_more;
    }

    Ok(messages)
}

fn current_assistant_branch_messages_path(
    branch_id: &str,
    after_id: &str,
) -> Result<String, KagiError> {
    validate_current_assistant_path_id("assistant branch id", branch_id)?;
    validate_current_assistant_path_id("assistant message id", after_id)?;
    Ok(format!(
        "/api/branches/{branch_id}/messages?limit=100&after_id={after_id}"
    ))
}

fn assistant_json_error(body: &str, surface: &str) -> Option<KagiError> {
    let value = serde_json::from_str::<Value>(body).ok()?;
    assistant_error_detail(&value)
        .map(|detail| KagiError::Config(format!("Kagi {surface} returned an error: {detail}")))
}

fn assistant_error_suffix(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| assistant_error_detail(&value))
        .map(|detail| format!("; response body: {detail}"))
        .unwrap_or_else(|| format_client_error_suffix(body))
}

fn assistant_error_detail(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => nonempty_detail(value),
        Value::Object(object) => {
            for key in ["message", "error", "detail", "description", "title", "code"] {
                if let Some(detail) = object.get(key).and_then(assistant_error_detail) {
                    return Some(detail);
                }
            }
            None
        }
        Value::Array(values) => values.iter().find_map(assistant_error_detail),
        _ => None,
    }
}

fn nonempty_detail(value: &str) -> Option<String> {
    let detail = strip_html_to_text(value);
    if detail.is_empty() {
        None
    } else {
        Some(detail)
    }
}

#[cfg(test)]
fn parse_assistant_thread_open_stream(
    body: &str,
) -> Result<AssistantThreadOpenResponse, KagiError> {
    let mut meta = AssistantMeta::default();
    let mut folders = Vec::new();
    let mut thread = None;
    let mut thread_html = None;
    let mut messages = None;
    let mut received_tags = Vec::new();

    for frame in body.split("\0\n").filter(|frame| !frame.trim().is_empty()) {
        let Some((tag, payload)) = frame.split_once(':') else {
            continue;
        };
        received_tags.push(tag.to_string());

        match tag {
            "hi" => {
                let hello: AssistantHello = serde_json::from_str(payload).map_err(|error| {
                    KagiError::Parse(format!("failed to parse assistant hello frame: {error}"))
                })?;
                meta.version = hello.v;
                meta.trace = hello.trace;
            }
            "folders.json" => {
                folders = serde_json::from_str(payload).map_err(|error| {
                    KagiError::Parse(format!("failed to parse assistant folders frame: {error}"))
                })?;
            }
            "thread.json" => {
                let payload: AssistantThreadPayload =
                    serde_json::from_str(payload).map_err(|error| {
                        KagiError::Parse(format!("failed to parse assistant thread frame: {error}"))
                    })?;
                thread = Some(AssistantThread::from(payload));
            }
            "thread.html" => {
                thread_html = Some(payload.to_string());
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
            "error.json" => {
                return Err(
                    assistant_json_error(payload, "Assistant thread open").unwrap_or_else(|| {
                        KagiError::Config(String::from(
                            "Kagi Assistant thread open returned an error",
                        ))
                    }),
                );
            }
            "unauthorized" => {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }
            _ => {
                debug!(tag, "ignoring unknown assistant thread-open stream frame");
            }
        }
    }

    let thread = match (thread, thread_html) {
        (Some(thread), _) => Ok(thread),
        (None, Some(html)) => parse_assistant_thread_open_html(&html),
        (None, None) => Err(KagiError::Parse(format!(
            "assistant thread open response did not include a thread.json or thread.html frame (received tags: {})",
            format_received_frame_tags(&received_tags)
        ))),
    }?;

    Ok(AssistantThreadOpenResponse {
        meta,
        folders,
        thread,
        messages: messages.ok_or_else(|| {
            KagiError::Parse(
                "assistant thread open response did not include a messages.json frame".to_string(),
            )
        })?,
    })
}

#[cfg(test)]
fn parse_assistant_thread_open_html(html: &str) -> Result<AssistantThread, KagiError> {
    let document = Html::parse_fragment(html);
    let thread_selector = Selector::parse(".thread").expect("valid CSS selector");
    let title_selector = Selector::parse(".title").expect("valid CSS selector");

    let element = document
        .select(&thread_selector)
        .next()
        .ok_or_else(|| KagiError::Parse("assistant thread html missing thread item".to_string()))?;
    let id = element
        .value()
        .attr("data-code")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| KagiError::Parse("assistant thread html missing data-code".to_string()))?
        .to_string();
    let title = element
        .select(&title_selector)
        .next()
        .map(|node| node.text().collect::<String>().trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| KagiError::Parse("assistant thread html missing title".to_string()))?;

    let folder_ids =
        serde_json::from_str::<Vec<String>>(element.value().attr("data-folders").unwrap_or("[]"))
            .unwrap_or_default();

    Ok(AssistantThread {
        id,
        title,
        ack: String::new(),
        created_at: String::new(),
        expires_at: String::new(),
        saved: element
            .value()
            .attr("data-saved")
            .is_some_and(|value| value == "true"),
        shared: element
            .value()
            .attr("data-public")
            .is_some_and(|value| value == "true"),
        branch_id: String::new(),
        folder_ids,
    })
}

#[cfg(test)]
fn format_received_frame_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        "<none>".to_string()
    } else {
        tags.join(", ")
    }
}

#[cfg(test)]
fn parse_assistant_thread_list_stream(
    body: &str,
) -> Result<AssistantThreadListResponse, KagiError> {
    let mut meta = AssistantMeta::default();
    let mut folders = Vec::new();
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
            "folders.json" => {
                folders = serde_json::from_str(payload).map_err(|error| {
                    KagiError::Parse(format!("failed to parse assistant folders frame: {error}"))
                })?;
            }
            "thread_list.html" => {
                let payload: Value = serde_json::from_str(payload).map_err(|error| {
                    KagiError::Parse(format!(
                        "failed to parse assistant thread list frame: {error}"
                    ))
                })?;
                let html = assistant_thread_list_html(&payload)?;
                threads = parse_assistant_thread_list(html)?;
                pagination = Some(AssistantThreadPagination {
                    next_cursor: assistant_thread_list_next_cursor(&payload),
                    has_more: payload
                        .get("has_more")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    count: payload.get("count").and_then(Value::as_u64).unwrap_or(0),
                    total_counts: assistant_thread_list_total_counts(&payload),
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
            "error.json" => {
                return Err(
                    assistant_json_error(payload, "Assistant thread list").unwrap_or_else(|| {
                        KagiError::Config(String::from(
                            "Kagi Assistant thread list returned an error",
                        ))
                    }),
                );
            }
            "unauthorized" => {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }
            _ => {
                debug!(tag, "ignoring unknown assistant thread-list stream frame");
            }
        }
    }

    Ok(AssistantThreadListResponse {
        meta,
        folders,
        threads,
        pagination: pagination.ok_or_else(|| {
            KagiError::Parse(
                "assistant thread list response did not include a thread_list.html frame"
                    .to_string(),
            )
        })?,
    })
}

#[cfg(test)]
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
            "error.json" => {
                return Err(assistant_json_error(payload, "Assistant thread delete")
                    .unwrap_or_else(|| {
                        KagiError::Config(String::from(
                            "Kagi Assistant thread delete returned an error",
                        ))
                    }));
            }
            "unauthorized" => {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }
            _ => {
                debug!(tag, "ignoring unknown assistant thread-delete stream frame");
            }
        }
    }

    Err(KagiError::Parse(
        "assistant thread delete response did not include an ok frame".to_string(),
    ))
}

#[cfg(test)]
fn assistant_thread_list_html(payload: &Value) -> Result<&str, KagiError> {
    let html = payload.get("html").ok_or_else(|| {
        KagiError::Parse("assistant thread list payload missing html".to_string())
    })?;

    if let Some(html) = html.as_str() {
        return Ok(html);
    }

    html.get("html").and_then(Value::as_str).ok_or_else(|| {
        KagiError::Parse("assistant thread list payload missing html string".to_string())
    })
}

#[cfg(test)]
fn assistant_thread_list_next_cursor(payload: &Value) -> Option<String> {
    payload.get("next_cursor").and_then(|cursor| {
        if cursor.is_null() {
            None
        } else {
            Some(cursor.to_string())
        }
    })
}

#[cfg(test)]
fn assistant_thread_list_total_counts(payload: &Value) -> HashMap<String, u64> {
    payload
        .get("total_counts")
        .and_then(Value::as_object)
        .map(|counts| {
            counts
                .iter()
                .filter_map(|(key, value)| value.as_u64().map(|value| (key.clone(), value)))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
fn parse_assistant_thread_cursor(cursor: &str) -> Option<Value> {
    serde_json::from_str::<Value>(cursor)
        .ok()
        .or_else(|| Some(json!({ "id": cursor })))
}

fn format_assistant_thread_markdown(response: &AssistantThreadOpenResponse) -> String {
    let mut markdown = format!("# {}\n", response.thread.title);

    for message in &response.messages {
        if !message.prompt.trim().is_empty() {
            markdown.push_str("\n## User\n\n");
            markdown.push_str(message.prompt.trim());
            markdown.push('\n');
        }

        let reply = message
            .markdown
            .as_deref()
            .or(message.reply_html.as_deref())
            .map(str::trim)
            .unwrap_or("");
        if !reply.is_empty() {
            markdown.push_str("\n## Assistant\n\n");
            markdown.push_str(reply);
            markdown.push('\n');
        }

        if let Some(references) = message
            .references_markdown
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            markdown.push('\n');
            markdown.push_str(references);
            markdown.push('\n');
        }
    }

    markdown
}

fn assistant_thread_export_filename(title: &str) -> String {
    let stem = title
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => ' ',
            _ => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let stem = if stem.is_empty() {
        String::from("conversation")
    } else {
        stem.chars().take(100).collect()
    };

    format!("{stem}.md")
}

fn strip_html_to_text(html: &str) -> String {
    Html::parse_fragment(html)
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
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
        return format!("; {}", truncate_error_detail(&payload.to_string()));
    }

    let detail = if looks_like_html_document(trimmed) {
        strip_html_to_text(trimmed)
    } else {
        trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
    };

    if detail.is_empty() {
        String::new()
    } else {
        format!("; {}", truncate_error_detail(&detail))
    }
}

fn truncate_error_detail(detail: &str) -> String {
    match detail.char_indices().nth(500) {
        Some((idx, _)) => format!("{}...", &detail[..idx]),
        None => detail.to_string(),
    }
}

fn build_translate_cookie_header(session_token: &str, translate_session: &str) -> String {
    format!("kagi_session={session_token}; translate_session={translate_session}")
}

fn validate_translate_request(request: &TranslateCommandRequest) -> Result<(), KagiError> {
    if request.text.trim().is_empty() {
        return Err(KagiError::Config(
            "translate text cannot be empty".to_string(),
        ));
    }

    if request.from.trim().is_empty() {
        return Err(KagiError::Config(
            "translate --from cannot be empty".to_string(),
        ));
    }

    if request.to.trim().is_empty() {
        return Err(KagiError::Config(
            "translate --to cannot be empty".to_string(),
        ));
    }

    if request.to.eq_ignore_ascii_case("auto") {
        return Err(KagiError::Config(
            "translate --to cannot be 'auto'; pass an explicit target language code".to_string(),
        ));
    }

    Ok(())
}

fn effective_translate_source_language(
    requested_from: &str,
    detected_language: &TranslateDetectedLanguage,
) -> String {
    if requested_from.eq_ignore_ascii_case("auto") && !detected_language.iso.trim().is_empty() {
        detected_language.iso.clone()
    } else {
        requested_from.to_string()
    }
}

fn finalize_translate_text_response(
    mut translation: TranslateTextResponse,
    detected_language: &TranslateDetectedLanguage,
    effective_source_language: &str,
    target_language: &str,
) -> TranslateTextResponse {
    if translation.detected_language.is_none() {
        translation.detected_language = Some(detected_language.clone());
    }
    translation.source_language = Some(effective_source_language.to_string());
    translation.target_language = Some(target_language.to_string());
    translation
}

fn build_translate_option_state(request: &TranslateCommandRequest) -> Option<TranslateOptionState> {
    let options = TranslateOptionState {
        formality: request.formality.clone(),
        speaker_gender: request.speaker_gender.clone(),
        addressee_gender: request.addressee_gender.clone(),
        language_complexity: request.language_complexity.clone(),
        style: request.translation_style.clone(),
        context: request.context.clone(),
    };

    if options.formality.is_none()
        && options.speaker_gender.is_none()
        && options.addressee_gender.is_none()
        && options.language_complexity.is_none()
        && options.style.is_none()
        && options.context.is_none()
    {
        None
    } else {
        Some(options)
    }
}

fn build_translate_payload(
    request: &TranslateCommandRequest,
    translate_session: &str,
    effective_source_language: &str,
) -> Value {
    let mut payload = Map::new();
    payload.insert("text".to_string(), Value::String(request.text.clone()));
    payload.insert(
        "from".to_string(),
        Value::String(effective_source_language.to_string()),
    );
    payload.insert("to".to_string(), Value::String(request.to.clone()));
    payload.insert("stream".to_string(), Value::Bool(false));
    payload.insert(
        "session_token".to_string(),
        Value::String(translate_session.to_string()),
    );

    insert_optional_string(&mut payload, "quality", request.quality.as_deref());
    insert_optional_string(&mut payload, "model", request.model.as_deref());
    insert_optional_string(&mut payload, "prediction", request.prediction.as_deref());
    insert_optional_string(
        &mut payload,
        "predicted_language",
        request.predicted_language.as_deref(),
    );
    insert_optional_string(&mut payload, "formality", request.formality.as_deref());
    insert_optional_string(
        &mut payload,
        "speaker_gender",
        request.speaker_gender.as_deref(),
    );
    insert_optional_string(
        &mut payload,
        "addressee_gender",
        request.addressee_gender.as_deref(),
    );
    insert_optional_string(
        &mut payload,
        "language_complexity",
        request.language_complexity.as_deref(),
    );
    insert_optional_string(
        &mut payload,
        "translation_style",
        request.translation_style.as_deref(),
    );
    insert_optional_string(&mut payload, "context", request.context.as_deref());
    insert_optional_string(
        &mut payload,
        "dictionary_language",
        request.dictionary_language.as_deref(),
    );
    insert_optional_string(&mut payload, "time_format", request.time_format.as_deref());
    insert_optional_bool(
        &mut payload,
        "use_definition_context",
        request.use_definition_context,
    );
    insert_optional_bool(
        &mut payload,
        "enable_language_features",
        request.enable_language_features,
    );
    insert_optional_bool(
        &mut payload,
        "preserve_formatting",
        request.preserve_formatting,
    );

    if let Some(context_memory) = &request.context_memory {
        payload.insert(
            "context_memory".to_string(),
            Value::Array(context_memory.clone()),
        );
    }

    Value::Object(payload)
}

fn build_translate_suggestions_payload(
    context: TranslateSuggestionContext<'_>,
    translate_session: &str,
) -> Result<Map<String, Value>, KagiError> {
    let mut payload = Map::new();
    payload.insert(
        "originalText".to_string(),
        Value::String(context.source_text.to_string()),
    );
    payload.insert(
        "translatedText".to_string(),
        Value::String(context.target_text.to_string()),
    );
    payload.insert(
        "sourceLanguage".to_string(),
        Value::String(context.source_language.to_string()),
    );
    payload.insert(
        "targetLanguage".to_string(),
        Value::String(context.target_language.to_string()),
    );
    payload.insert(
        "language".to_string(),
        Value::String(context.target_language.to_string()),
    );
    payload.insert(
        "session_token".to_string(),
        Value::String(translate_session.to_string()),
    );

    if let Some(options) = context.translation_options {
        payload.insert(
            "translationOptions".to_string(),
            serde_json::to_value(options).map_err(|error| {
                KagiError::Parse(format!(
                    "failed to serialize translate suggestion options: {error}"
                ))
            })?,
        );
    }

    Ok(payload)
}

fn build_translate_word_insights_payload(
    source_text: &str,
    target_text: &str,
    explanation_language: &str,
    translate_session: &str,
    translation_options: Option<&TranslateOptionState>,
) -> Result<Map<String, Value>, KagiError> {
    let mut payload = Map::new();
    payload.insert(
        "original_text".to_string(),
        Value::String(source_text.to_string()),
    );
    payload.insert(
        "translated_text".to_string(),
        Value::String(target_text.to_string()),
    );
    payload.insert(
        "target_explanation_language".to_string(),
        Value::String(explanation_language.to_string()),
    );
    payload.insert(
        "session_token".to_string(),
        Value::String(translate_session.to_string()),
    );

    if let Some(options) = translation_options {
        payload.insert(
            "translation_options".to_string(),
            serde_json::to_value(options).map_err(|error| {
                KagiError::Parse(format!(
                    "failed to serialize translate word-insight options: {error}"
                ))
            })?,
        );
    }

    Ok(payload)
}

fn insert_optional_string(payload: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        payload.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn insert_optional_bool(payload: &mut Map<String, Value>, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        payload.insert(key.to_string(), Value::Bool(value));
    }
}

fn normalize_aux_quality(raw: Option<&str>) -> Option<String> {
    raw.map(|value| {
        if value == "best" || value.starts_with("deep_") {
            "best".to_string()
        } else {
            "standard".to_string()
        }
    })
}

fn parse_translate_detect_value(value: Value) -> Result<TranslateDetectedLanguage, KagiError> {
    let candidate = match value {
        Value::Array(mut values) => values.drain(..).next().ok_or_else(|| {
            KagiError::Parse(
                "failed to parse translate language detection response: empty array".to_string(),
            )
        })?,
        Value::Object(_) => value,
        other => {
            return Err(KagiError::Parse(format!(
                "failed to parse translate language detection response: unexpected payload {other}"
            )));
        }
    };

    serde_json::from_value(candidate).map_err(|error| {
        KagiError::Parse(format!(
            "failed to parse translate language detection response: {error}"
        ))
    })
}

fn extract_set_cookie_value(headers: &header::HeaderMap, name: &str) -> Option<String> {
    let prefix = format!("{name}=");

    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            let raw = value.to_str().ok()?;
            let cookie = raw.strip_prefix(&prefix)?;
            cookie
                .split(';')
                .next()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

fn resolve_translate_bootstrap(
    status: StatusCode,
    headers: &header::HeaderMap,
) -> Result<TranslateBootstrapResult, KagiError> {
    match status {
        StatusCode::OK => {
            let translate_session = extract_set_cookie_value(headers, "translate_session")
                .ok_or_else(|| {
                    KagiError::Auth(TRANSLATE_BOOTSTRAP_MISSING_COOKIE_ERROR.to_string())
                })?;

            Ok(TranslateBootstrapResult {
                translate_session,
                method: "reqwest(set-cookie bootstrap)".to_string(),
            })
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(KagiError::Auth(
            "invalid or expired Kagi session token, or inactive Kagi subscription, for Kagi Translate"
                .to_string(),
        )),
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi Translate bootstrap server error: HTTP {status}"
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi Translate bootstrap response status: HTTP {status}"
        ))),
    }
}

fn should_retry_translate_bootstrap(error: &KagiError) -> bool {
    match error {
        KagiError::Auth(message) => message == TRANSLATE_BOOTSTRAP_MISSING_COOKIE_ERROR,
        KagiError::Network(_) => true,
        _ => false,
    }
}

fn normalize_ask_page_url(raw: &str) -> Result<String, KagiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(KagiError::Config(
            "ask-page URL cannot be empty".to_string(),
        ));
    }

    let url = Url::parse(trimmed)
        .map_err(|error| KagiError::Config(format!("invalid ask-page URL: {error}")))?;
    match url.scheme() {
        "http" | "https" => Ok(url.to_string()),
        scheme => Err(KagiError::Config(format!(
            "ask-page URL must use http or https, got `{scheme}`"
        ))),
    }
}

fn normalize_ask_page_question(raw: &str) -> Result<String, KagiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(KagiError::Config(
            "ask-page question cannot be empty".to_string(),
        ));
    }

    Ok(trimmed.to_string())
}

fn build_ask_page_prompt(url: &str, question: &str) -> String {
    format!("{url}\n{question}")
}

#[cfg(test)]
fn fake_header_map(set_cookies: &[&str]) -> header::HeaderMap {
    let mut headers = header::HeaderMap::new();
    for value in set_cookies {
        headers.append(
            header::SET_COOKIE,
            header::HeaderValue::from_str(value).expect("header value should parse"),
        );
    }
    headers
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
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
struct SubscriberSummaryAssistantFrame {
    #[serde(default)]
    error: bool,
    #[serde(default)]
    output_text: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
struct AssistantHello {
    v: Option<String>,
    trace: Option<String>,
}

#[cfg(test)]
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

#[cfg(test)]
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
        usage: None,
    }
}

#[derive(Debug, Deserialize)]
struct AssistantThreadPayload {
    id: String,
    title: String,
    ack: String,
    created_at: String,
    #[serde(default)]
    expires_at: Option<String>,
    saved: bool,
    shared: bool,
    branch_id: String,
    #[serde(default)]
    folder_ids: Vec<String>,
}

impl From<AssistantThreadPayload> for AssistantThread {
    fn from(payload: AssistantThreadPayload) -> Self {
        Self {
            id: payload.id,
            title: payload.title,
            ack: payload.ack,
            created_at: payload.created_at,
            expires_at: payload.expires_at.unwrap_or_default(),
            saved: payload.saved,
            shared: payload.shared,
            branch_id: payload.branch_id,
            folder_ids: payload.folder_ids,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantConversationListResponse {
    #[serde(default)]
    items: Vec<CurrentAssistantConversationItem>,
    #[serde(default)]
    has_more: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CurrentAssistantConversationItem {
    Wrapped {
        conversation: CurrentAssistantConversation,
        #[serde(default)]
        snippet: Option<String>,
    },
    Raw(CurrentAssistantConversation),
}

impl CurrentAssistantConversationItem {
    fn conversation_uuid(&self) -> String {
        match self {
            Self::Wrapped { conversation, .. } => conversation.uuid.clone(),
            Self::Raw(conversation) => conversation.uuid.clone(),
        }
    }
}

impl From<CurrentAssistantConversationItem> for AssistantThreadSummary {
    fn from(item: CurrentAssistantConversationItem) -> Self {
        let (conversation, snippet) = match item {
            CurrentAssistantConversationItem::Wrapped {
                conversation,
                snippet,
            } => (conversation, snippet),
            CurrentAssistantConversationItem::Raw(conversation) => (conversation, None),
        };
        let folder_ids = conversation
            .folder_uuid
            .clone()
            .into_iter()
            .collect::<Vec<_>>();
        Self {
            id: conversation.uuid.clone(),
            title: conversation.title,
            url: format!("/chat/{}", conversation.uuid),
            snippet: snippet.unwrap_or_default(),
            saved: conversation.is_saved,
            shared: conversation.is_shared,
            folder_ids,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantConversationInitResponse {
    conversation: CurrentAssistantConversation,
    #[serde(default)]
    active_branch: Option<CurrentAssistantBranch>,
    #[serde(default)]
    branches: Vec<CurrentAssistantBranch>,
    messages: CurrentAssistantMessagesPage,
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantInitResponse {
    models: CurrentAssistantModelCatalog,
    #[serde(default)]
    custom_assistants: Vec<CurrentAssistantCustomAssistant>,
}

impl CurrentAssistantInitResponse {
    fn assistant_summaries(&self) -> Vec<AssistantProfileSummary> {
        let mut assistants = self
            .models
            .assistants
            .iter()
            .map(|assistant| assistant.summary(&self.models))
            .collect::<Vec<_>>();
        assistants.extend(
            self.custom_assistants
                .iter()
                .map(|assistant| assistant.summary(&self.models)),
        );
        assistants
    }
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantModelCatalog {
    models: Vec<CurrentAssistantModel>,
    default: String,
    #[serde(default)]
    assistants: Vec<CurrentAssistantBuiltinAssistant>,
}

impl From<CurrentAssistantModelCatalog> for AssistantModelCatalog {
    fn from(catalog: CurrentAssistantModelCatalog) -> Self {
        Self {
            models: catalog.models.into_iter().map(Into::into).collect(),
            default: catalog.default,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantModel {
    id: String,
    display_name: String,
}

impl From<CurrentAssistantModel> for AssistantModelOption {
    fn from(model: CurrentAssistantModel) -> Self {
        Self {
            id: model.id,
            label: model.display_name,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantBuiltinAssistant {
    id: String,
    name: String,
    #[serde(default)]
    underlying_model: String,
    internet_access: bool,
}

impl CurrentAssistantBuiltinAssistant {
    fn summary(&self, models: &CurrentAssistantModelCatalog) -> AssistantProfileSummary {
        let invoke_profile = self
            .id
            .strip_prefix("assistant:")
            .unwrap_or(&self.id)
            .to_string();
        AssistantProfileSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            invoke_profile,
            model: models.model_label(&self.underlying_model),
            bang_trigger: None,
            internet_access: self.internet_access,
            built_in: true,
            edit_url: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantCustomAssistant {
    uuid: String,
    name: String,
    #[serde(default)]
    bang_trigger: Option<String>,
    #[serde(default)]
    instructions: Option<String>,
    internet_access: bool,
    personalizations: bool,
    #[serde(default)]
    lens_id: Option<String>,
    llm_id: String,
}

impl CurrentAssistantCustomAssistant {
    fn summary(&self, models: &CurrentAssistantModelCatalog) -> AssistantProfileSummary {
        AssistantProfileSummary {
            id: self.uuid.clone(),
            name: self.name.clone(),
            invoke_profile: self.uuid.clone(),
            model: models.model_label(&self.llm_id),
            bang_trigger: self.bang_trigger.clone(),
            internet_access: self.internet_access,
            built_in: false,
            edit_url: Some(format!("{KAGI_ASSISTANT_CUSTOM_EDITOR_PATH}/{}", self.uuid)),
        }
    }

    fn details(&self) -> AssistantProfileDetails {
        AssistantProfileDetails {
            profile_id: Some(self.uuid.clone()),
            name: self.name.clone(),
            bang_trigger: self.bang_trigger.clone(),
            internet_access: self.internet_access,
            selected_lens: self.lens_id.clone().unwrap_or_default(),
            personalizations: self.personalizations,
            base_model: self.llm_id.clone(),
            custom_instructions: self.instructions.clone().unwrap_or_default(),
            delete_supported: true,
        }
    }
}

impl CurrentAssistantModelCatalog {
    fn model_label(&self, model_id: &str) -> String {
        self.models
            .iter()
            .find(|model| model.id == model_id)
            .map(|model| model.display_name.clone())
            .unwrap_or_else(|| model_id.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantConversationCreateResponse {
    conversation: CurrentAssistantConversation,
    default_branch: CurrentAssistantBranch,
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantPromptMessageResponse {
    conversation: CurrentAssistantConversation,
    branch: CurrentAssistantBranch,
    user_message: CurrentAssistantMessage,
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantUploadResponse {
    uuid: String,
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantPromptStreamFrame {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    html_content: Option<String>,
    #[serde(default)]
    conversation_title: Option<String>,
    #[serde(default)]
    assistant_message_uuid: Option<String>,
    #[serde(default)]
    trace_id: Option<String>,
    #[serde(default)]
    references: Vec<Value>,
    #[serde(default)]
    usage: Option<CurrentAssistantUsage>,
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    error: Option<String>,
}

struct CurrentAssistantPromptParser {
    meta: AssistantMeta,
    thread: AssistantThread,
    message: AssistantMessage,
    previous_markdown: String,
    received_final: bool,
}

impl CurrentAssistantPromptParser {
    fn new(
        conversation: CurrentAssistantConversation,
        branch: CurrentAssistantBranch,
        user_message: CurrentAssistantMessage,
        prompt: String,
        selected_profile: Option<Value>,
    ) -> Self {
        let profile = match (
            selected_profile,
            current_assistant_message_profile(&user_message),
        ) {
            (Some(Value::Object(mut selected)), Some(Value::Object(current))) => {
                selected.extend(current);
                Some(Value::Object(selected))
            }
            (Some(selected), _) => Some(selected),
            (None, current) => current,
        };
        let created_at = user_message
            .created_at
            .unwrap_or_else(|| conversation.created_at.clone());
        let thread = AssistantThread {
            id: conversation.uuid.clone(),
            title: conversation.title,
            ack: conversation.updated_at,
            created_at: conversation.created_at,
            expires_at: String::new(),
            saved: conversation.is_saved,
            shared: conversation.is_shared,
            branch_id: branch.uuid,
            folder_ids: conversation.folder_uuid.into_iter().collect(),
        };
        let message = AssistantMessage {
            id: String::new(),
            thread_id: thread.id.clone(),
            created_at,
            branch_list: Vec::new(),
            state: "streaming".to_string(),
            prompt,
            reply_html: None,
            markdown: Some(String::new()),
            references_html: None,
            references_markdown: None,
            metadata_html: None,
            documents: Vec::new(),
            profile,
            trace_id: None,
            usage: None,
        };
        Self {
            meta: AssistantMeta::default(),
            thread,
            message,
            previous_markdown: String::new(),
            received_final: false,
        }
    }

    fn process_sse_frame(
        &mut self,
        frame: &str,
    ) -> Result<Option<AssistantPromptStreamEvent>, KagiError> {
        let payload = frame
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if payload.is_empty() || payload == "[DONE]" {
            return Ok(None);
        }
        let frame: CurrentAssistantPromptStreamFrame =
            serde_json::from_str(&payload).map_err(|error| {
                KagiError::Parse(format!(
                    "failed to parse Assistant prompt stream frame: {error}"
                ))
            })?;
        if let Some(error) = frame.error.filter(|error| !error.trim().is_empty()) {
            return Err(KagiError::Config(error));
        }
        if let Some(title) = frame.conversation_title {
            self.thread.title = title;
        }
        if let Some(trace_id) = frame.trace_id {
            self.meta.trace = Some(trace_id.clone());
            self.message.trace_id = Some(trace_id);
        }
        if let Some(message_id) = frame.assistant_message_uuid {
            self.message.id = message_id;
        }
        if let Some(html) = frame.html_content {
            self.message.reply_html = Some(html);
        }
        if !frame.references.is_empty() {
            self.message.references_markdown =
                current_assistant_references_markdown(&frame.references);
        }
        if let Some(usage) = frame.usage {
            self.message.usage = Some(usage.into());
        }
        if let Some(markdown) = frame.text {
            self.message.markdown = Some(markdown);
        }
        self.received_final |= frame.is_final;
        self.message.state = if self.received_final {
            "done".to_string()
        } else {
            "streaming".to_string()
        };

        let markdown = self.message.markdown.as_deref().unwrap_or("");
        let md_delta = markdown
            .strip_prefix(&self.previous_markdown)
            .unwrap_or(markdown)
            .to_string();
        let has_update =
            !md_delta.is_empty() || frame.is_final || self.message.reply_html.is_some();
        self.previous_markdown = markdown.to_string();
        if !has_update {
            return Ok(None);
        }
        Ok(Some(AssistantPromptStreamEvent {
            meta: self.meta.clone(),
            thread: Some(self.thread.clone()),
            message: self.message.clone(),
            md_delta,
        }))
    }

    fn finish(self) -> Result<AssistantPromptResponse, KagiError> {
        if !self.received_final {
            return Err(KagiError::Parse(
                "Assistant prompt stream ended before a final response".to_string(),
            ));
        }
        Ok(AssistantPromptResponse {
            meta: self.meta,
            thread: self.thread,
            message: self.message,
        })
    }
}

impl CurrentAssistantConversationInitResponse {
    fn selected_branch(&self) -> Option<CurrentAssistantBranch> {
        self.active_branch
            .clone()
            .or_else(|| {
                self.branches
                    .iter()
                    .find(|branch| branch.is_default)
                    .cloned()
            })
            .or_else(|| self.branches.first().cloned())
    }

    fn into_thread_open_response(
        self,
        folders: Vec<Value>,
        branch: Option<CurrentAssistantBranch>,
        messages: Vec<CurrentAssistantMessage>,
    ) -> AssistantThreadOpenResponse {
        let thread = self.conversation.to_assistant_thread(branch.as_ref());
        let messages = current_assistant_messages_to_legacy_turns(&thread.id, messages);
        AssistantThreadOpenResponse {
            meta: AssistantMeta::default(),
            folders,
            thread,
            messages,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CurrentAssistantBranch {
    uuid: String,
    #[serde(default)]
    is_default: bool,
}

#[derive(Debug, Default, Deserialize)]
struct CurrentAssistantMessagesPage {
    #[serde(default)]
    items: Vec<CurrentAssistantMessage>,
    #[serde(default)]
    has_more: bool,
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantBranchMessagesResponse {
    messages: CurrentAssistantMessagesPage,
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantConversation {
    uuid: String,
    title: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    is_saved: bool,
    #[serde(default)]
    is_shared: bool,
    #[serde(default)]
    folder_uuid: Option<String>,
}

impl CurrentAssistantConversation {
    fn to_assistant_thread(&self, branch: Option<&CurrentAssistantBranch>) -> AssistantThread {
        AssistantThread {
            id: self.uuid.clone(),
            title: self.title.clone(),
            ack: self.updated_at.clone(),
            created_at: self.created_at.clone(),
            expires_at: String::new(),
            saved: self.is_saved,
            shared: self.is_shared,
            branch_id: branch.map(|branch| branch.uuid.clone()).unwrap_or_default(),
            folder_ids: self.folder_uuid.clone().into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CurrentAssistantMessage {
    #[serde(default)]
    uuid: Option<String>,
    role: String,
    content: String,
    #[serde(default)]
    html_content: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    references: Vec<Value>,
    #[serde(default)]
    attachments: Vec<Value>,
    #[serde(default)]
    model_name: Option<String>,
    #[serde(default)]
    model_version: Option<String>,
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cost_usd: Option<Number>,
}

#[derive(Debug, Deserialize)]
struct CurrentAssistantUsage {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    cost_usd: Number,
}

impl From<CurrentAssistantUsage> for AssistantUsage {
    fn from(usage: CurrentAssistantUsage) -> Self {
        Self {
            prompt_tokens: usage.input_tokens,
            completion_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
            cost_usd: usage.cost_usd,
        }
    }
}

fn current_assistant_messages_to_legacy_turns(
    thread_id: &str,
    messages: Vec<CurrentAssistantMessage>,
) -> Vec<AssistantMessage> {
    let mut turns = Vec::new();
    let mut pending_user = None;

    for message in messages {
        if message.role == "user" {
            if let Some(user) = pending_user.take() {
                turns.push(current_user_message_to_assistant_message(thread_id, user));
            }
            pending_user = Some(message);
        } else if message.role == "assistant" {
            if let Some(user) = pending_user.take() {
                turns.push(current_message_pair_to_assistant_message(
                    thread_id, user, message,
                ));
            } else {
                turns.push(current_assistant_message_to_assistant_message(
                    thread_id, message,
                ));
            }
        }
    }

    if let Some(user) = pending_user {
        turns.push(current_user_message_to_assistant_message(thread_id, user));
    }

    turns
}

fn current_message_pair_to_assistant_message(
    thread_id: &str,
    user: CurrentAssistantMessage,
    assistant: CurrentAssistantMessage,
) -> AssistantMessage {
    let profile = current_assistant_message_profile(&assistant);
    let usage = current_assistant_message_usage(&assistant);
    AssistantMessage {
        id: assistant.uuid.or(user.uuid).unwrap_or_default(),
        thread_id: thread_id.to_string(),
        created_at: assistant.created_at.or(user.created_at).unwrap_or_default(),
        branch_list: Vec::new(),
        state: String::from("done"),
        prompt: user.content,
        reply_html: assistant.html_content,
        markdown: Some(assistant.content),
        references_html: None,
        references_markdown: current_assistant_references_markdown(&assistant.references),
        metadata_html: None,
        documents: assistant.attachments,
        profile,
        trace_id: None,
        usage,
    }
}

fn current_user_message_to_assistant_message(
    thread_id: &str,
    user: CurrentAssistantMessage,
) -> AssistantMessage {
    AssistantMessage {
        id: user.uuid.unwrap_or_default(),
        thread_id: thread_id.to_string(),
        created_at: user.created_at.unwrap_or_default(),
        branch_list: Vec::new(),
        state: String::from("user"),
        prompt: user.content,
        reply_html: None,
        markdown: None,
        references_html: None,
        references_markdown: None,
        metadata_html: None,
        documents: user.attachments,
        profile: None,
        trace_id: None,
        usage: None,
    }
}

fn current_assistant_message_to_assistant_message(
    thread_id: &str,
    assistant: CurrentAssistantMessage,
) -> AssistantMessage {
    let profile = current_assistant_message_profile(&assistant);
    let usage = current_assistant_message_usage(&assistant);
    AssistantMessage {
        id: assistant.uuid.unwrap_or_default(),
        thread_id: thread_id.to_string(),
        created_at: assistant.created_at.unwrap_or_default(),
        branch_list: Vec::new(),
        state: String::from("assistant"),
        prompt: String::new(),
        reply_html: assistant.html_content,
        markdown: Some(assistant.content),
        references_html: None,
        references_markdown: current_assistant_references_markdown(&assistant.references),
        metadata_html: None,
        documents: assistant.attachments,
        profile,
        trace_id: None,
        usage,
    }
}

fn current_assistant_message_usage(message: &CurrentAssistantMessage) -> Option<AssistantUsage> {
    let prompt_tokens = message.input_tokens?;
    let completion_tokens = message.output_tokens?;
    let cost_usd = message.cost_usd.clone()?;
    Some(AssistantUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
        cost_usd,
    })
}

fn current_assistant_message_profile(message: &CurrentAssistantMessage) -> Option<Value> {
    if message.model_name.is_none() && message.model_version.is_none() {
        return None;
    }

    Some(json!({
        "model_name": message.model_name,
        "model_version": message.model_version,
    }))
}

fn current_assistant_references_markdown(references: &[Value]) -> Option<String> {
    if references.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    for reference in references {
        let index = reference
            .get("index")
            .and_then(Value::as_u64)
            .map(|value| value.to_string())
            .unwrap_or_else(|| String::from("?"));
        let title = reference
            .get("title")
            .or_else(|| reference.get("source"))
            .and_then(Value::as_str)
            .unwrap_or("Untitled source");
        let url = reference.get("url").and_then(Value::as_str).unwrap_or("");
        if url.is_empty() {
            lines.push(format!("[^{index}]: {title}"));
        } else {
            lines.push(format!("[^{index}]: [{title}]({url})"));
        }
    }

    Some(lines.join("\n"))
}

#[derive(Debug)]
struct TranslateBootstrapResult {
    translate_session: String,
    method: String,
}

#[derive(Debug, Clone)]
struct AppliedNewsFilter {
    stories: Vec<Value>,
    summary: NewsContentFilterSummary,
}

#[derive(Debug, Clone)]
struct ResolvedNewsFilter {
    active_presets: Vec<String>,
    custom_keywords: Vec<String>,
    active_keywords: Vec<String>,
    mode: NewsFilterMode,
    scope: NewsFilterScope,
}

#[derive(Debug, Deserialize)]
struct NewsFilterPresetFile {
    filters: Vec<NewsFilterPresetDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
struct NewsFilterPresetDefinition {
    id: String,
    label: String,
    keywords: NewsFilterPresetKeywords,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum NewsFilterPresetKeywords {
    Flat(Vec<String>),
    Localized(HashMap<String, Vec<String>>),
}

impl NewsFilterPresetDefinition {
    fn resolve_keywords(&self, language: &str) -> Vec<String> {
        match &self.keywords {
            NewsFilterPresetKeywords::Flat(keywords) => keywords.clone(),
            NewsFilterPresetKeywords::Localized(map) => map
                .get(language)
                .or_else(|| map.get("default"))
                .or_else(|| map.get("en"))
                .cloned()
                .unwrap_or_default(),
        }
    }
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
                debug!(
                    surface,
                    body_len = body.len(),
                    body_preview = %debug_body_preview(&body),
                    error = %error,
                    "failed to parse Kagi API response body"
                );
                KagiError::Parse(format!("failed to parse {surface} response: {error}"))
            })
        }
        status @ (StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Auth(format!(
                "invalid Kagi API token or access is not enabled for {surface}: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_client_error() => {
            let body = http::read_error_body(response, surface).await;
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
        status if status.is_server_error() => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Network(format!(
                "Kagi {surface} server error: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Network(format!(
                "unexpected Kagi {surface} response status: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
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
                debug!(
                    surface,
                    body_len = body.len(),
                    body_preview = %debug_body_preview(&body),
                    error = %error,
                    "failed to parse free Kagi response body"
                );
                KagiError::Parse(format!("failed to parse {surface} response: {error}"))
            })
        }
        status @ (StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Auth(format!(
                "authentication is not supported for public Kagi {surface} endpoints: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_client_error() => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Config(format!(
                "Kagi {surface} request rejected: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_server_error() => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Network(format!(
                "Kagi {surface} server error: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Network(format!(
                "unexpected Kagi {surface} response status: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
    }
}

async fn decode_translate_json<T>(
    response: reqwest::Response,
    surface: &str,
) -> Result<T, KagiError>
where
    T: DeserializeOwned,
{
    match response.status() {
        StatusCode::OK => {
            let body = response.text().await.map_err(|error| {
                KagiError::Network(format!(
                    "failed to read Kagi Translate {surface} response body: {error}"
                ))
            })?;
            if looks_like_html_document(&body) {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token for Kagi Translate".to_string(),
                ));
            }
            serde_json::from_str(&body).map_err(|error| {
                debug!(
                    surface,
                    body_len = body.len(),
                    body_preview = %debug_body_preview(&body),
                    error = %error,
                    "failed to parse Kagi Translate response body"
                );
                KagiError::Parse(format!(
                    "failed to parse Kagi Translate {surface} response: {error}"
                ))
            })
        }
        status @ (StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => {
            let body = http::read_error_body(response, surface).await;
            if let Some(message) = translate_subscription_error_message(&body) {
                return Err(KagiError::Auth(message));
            }
            Err(KagiError::Auth(format!(
                "invalid or expired Kagi session token for Kagi Translate {surface}: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_client_error() => {
            let body = http::read_error_body(response, surface).await;
            if let Some(message) = translate_subscription_error_message(&body) {
                return Err(KagiError::Auth(message));
            }
            Err(KagiError::Config(format!(
                "Kagi Translate {surface} request rejected: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_server_error() => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Network(format!(
                "Kagi Translate {surface} server error: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status => {
            let body = http::read_error_body(response, surface).await;
            Err(KagiError::Network(format!(
                "unexpected Kagi Translate {surface} response status: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
    }
}

fn translate_subscription_error_message(body: &str) -> Option<String> {
    let normalized = body.to_ascii_lowercase();
    let mentions_subscription = [
        "active subscription",
        "subscription",
        "subscriber",
        "not subscribed",
        "payment required",
        "entitlement",
        "free access",
    ]
    .iter()
    .any(|marker| normalized.contains(marker));

    mentions_subscription.then(|| {
        "Kagi Translate requires an active Kagi subscription; free access is paused".to_string()
    })
}

fn build_client() -> Result<Client, KagiError> {
    http::client_30s()
}

#[cfg(test)]
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
/// Generic wrapper for Kagi API responses containing a data payload.
pub struct KagiEnvelope<T> {
    pub meta: ApiMeta,
    pub data: T,
}

#[cfg(test)]
mod tests {
    use super::{
        ApiErrorBody, KagiEnvelope, NewsFilterRequest, TRANSLATE_BOOTSTRAP_MISSING_COOKIE_ERROR,
        TranslateSuggestionContext, apply_news_content_filters, build_ask_page_prompt,
        build_translate_option_state, build_translate_payload, build_translate_suggestions_payload,
        build_translate_word_insights_payload, capture_optional_translate_section,
        effective_translate_source_language, execute_news_filter_presets, extract_set_cookie_value,
        fake_header_map, finalize_translate_text_response, format_client_error_suffix,
        normalize_ask_page_question, normalize_ask_page_url, normalize_assistant_query,
        normalize_assistant_thread_id, normalize_assistant_thread_ref, normalize_aux_quality,
        normalize_custom_bang_trigger, normalize_lens_name, normalize_redirect_rule,
        normalize_subscriber_summary_input, normalize_subscriber_summary_length,
        normalize_subscriber_summary_type, parse_assistant_thread_cursor,
        parse_assistant_thread_delete_stream, parse_assistant_thread_list_stream,
        parse_assistant_thread_open_stream, parse_content_disposition_filename,
        parse_subscriber_summarize_stream, parse_translate_detect_value,
        resolve_custom_assistant_ref, resolve_custom_bang_ref, resolve_lens_ref,
        resolve_news_category, resolve_redirect_ref, resolve_translate_bootstrap,
        should_retry_lens_mutation_lookup, should_retry_translate_bootstrap,
        take_next_assistant_sse_frame, text_contains_news_filter_keyword,
        translate_subscription_error_message, validate_translate_request,
    };
    use crate::api::{
        execute_assistant_prompt, execute_assistant_thread_delete, execute_assistant_thread_export,
        execute_assistant_thread_get, execute_assistant_thread_list,
        execute_custom_assistant_create, execute_custom_assistant_delete,
        execute_custom_assistant_get, execute_custom_assistant_list,
        execute_custom_assistant_update, execute_custom_bang_create, execute_custom_bang_delete,
        execute_custom_bang_get, execute_custom_bang_update, execute_lens_create,
        execute_lens_delete, execute_lens_set_enabled, execute_lens_update,
        execute_redirect_create, execute_redirect_delete, execute_redirect_list,
        execute_redirect_set_enabled, execute_redirect_update,
    };
    use crate::auth::{SESSION_TOKEN_ENV, load_credential_inventory, normalize_session_token};
    use crate::cli::{NewsFilterMode, NewsFilterScope};
    use crate::error::KagiError;
    use crate::test_support::lock_env;
    use crate::types::{AskPageRequest, SubscriberSummarizeRequest};
    use crate::types::{
        AssistantProfileCreateRequest, AssistantProfileSummary, AssistantProfileUpdateRequest,
        AssistantPromptRequest, CustomBangCreateRequest, CustomBangSummary,
        CustomBangUpdateRequest, FastGptAnswer, LensCreateRequest, LensSummary, LensUpdateRequest,
        NewsBatchCategory, NewsCategoryMetadata, RedirectRuleCreateRequest, RedirectRuleSummary,
        RedirectRuleUpdateRequest, Reference, Summarization, TranslateCommandRequest,
        TranslateDetectedLanguage, TranslateTextResponse,
    };
    use reqwest::StatusCode;
    use serde_json::{Value, json};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tempfile::TempDir;

    struct ScopedEnvVar {
        key: &'static str,
        previous: Option<String>,
    }

    impl ScopedEnvVar {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe { std::env::set_var(key, value) }

            Self { key, previous }
        }
    }

    impl Drop for ScopedEnvVar {
        fn drop(&mut self) {
            match self.previous.as_deref() {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    fn set_env_var(key: &'static str, value: &str) -> ScopedEnvVar {
        ScopedEnvVar::set(key, value)
    }

    fn sample_translate_request() -> TranslateCommandRequest {
        TranslateCommandRequest {
            text: "Bonjour".to_string(),
            from: "auto".to_string(),
            to: "en".to_string(),
            quality: None,
            model: None,
            prediction: None,
            predicted_language: None,
            formality: None,
            speaker_gender: None,
            addressee_gender: None,
            language_complexity: None,
            translation_style: None,
            context: None,
            dictionary_language: None,
            time_format: None,
            use_definition_context: None,
            enable_language_features: None,
            preserve_formatting: None,
            context_memory: None,
            fetch_alternatives: true,
            fetch_word_insights: true,
            fetch_suggestions: true,
            fetch_alignments: true,
        }
    }

    fn sample_detected_language() -> TranslateDetectedLanguage {
        TranslateDetectedLanguage {
            iso: "fr".to_string(),
            label: "French".to_string(),
            is_uncertain: false,
            is_mixed: false,
            alternatives: vec![],
        }
    }

    fn live_translate_session_token() -> Option<String> {
        std::env::var("KAGI_SESSION_TOKEN")
            .ok()
            .and_then(|value| normalize_session_token(&value).ok())
    }

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
    fn formats_html_error_body_as_text_suffix() {
        let suffix = format_client_error_suffix(
            "<html><body><h1>Rate limited</h1><p>Retry later</p></body></html>",
        );

        assert_eq!(suffix, "; Rate limited Retry later");
    }

    #[test]
    fn truncates_long_error_body_suffixes() {
        let suffix = format_client_error_suffix(&"x".repeat(600));

        assert_eq!(suffix.len(), 505);
        assert!(suffix.ends_with("..."));
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
    fn parses_subscriber_summarize_final_stream() {
        let raw = "update:{\"type\":\"update\",\"output_text\":\"\",\"output_data\":{\"status\":\"done\",\"title\":\"Example Domain\"},\"tokens\":0}\0\nfinal:{\"type\":\"final\",\"output_text\":\"summary output\",\"output_data\":null,\"tokens\":0,\"error\":false}\0\n";

        let parsed = parse_subscriber_summarize_stream(raw).expect("stream parses");
        assert_eq!(parsed.data.state, "done");
        assert_eq!(parsed.data.output, "summary output");
        assert_eq!(parsed.data.markdown, "summary output");
        assert!(parsed.data.documents.is_empty());
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
    fn lists_news_filter_presets_with_language_fallback() {
        let response = execute_news_filter_presets("xx").expect("preset list should load");
        let politics = response
            .presets
            .iter()
            .find(|preset| preset.id == "politics")
            .expect("politics preset should exist");

        assert_eq!(response.language, "xx");
        assert_eq!(politics.label, "Politics");
        assert!(politics.keywords.contains(&"trump".to_string()));
    }

    #[test]
    fn news_filter_keyword_matching_respects_word_boundaries() {
        assert!(text_contains_news_filter_keyword(
            "trump and iran trade threats",
            "trump"
        ));
        assert!(!text_contains_news_filter_keyword(
            "the strumpet headline",
            "trump"
        ));
        assert!(text_contains_news_filter_keyword(
            "u.s. adults remain concerned",
            "u.s."
        ));
    }

    #[test]
    fn hide_mode_omits_matching_news_stories() {
        let stories = vec![
            json!({
                "title": "Trump and Iran trade threats",
                "category": "Middle East",
                "short_summary": "Escalation in the strait."
            }),
            json!({
                "title": "Satellite launch succeeds",
                "category": "Science",
                "short_summary": "A quiet day for spaceflight."
            }),
        ];
        let filtered = apply_news_content_filters(
            stories,
            &NewsFilterRequest {
                preset_ids: vec![],
                keywords: vec!["trump".to_string()],
                mode: NewsFilterMode::Hide,
                scope: NewsFilterScope::All,
            },
            "en",
        )
        .expect("hide mode should succeed");

        assert_eq!(filtered.stories.len(), 1);
        assert_eq!(filtered.summary.mode, "hide");
        assert_eq!(filtered.summary.scope, "all");
        assert_eq!(filtered.summary.filtered_count, 1);
        assert_eq!(filtered.summary.active_keywords, vec!["trump".to_string()]);
        assert_eq!(filtered.stories[0]["title"], "Satellite launch succeeds");
    }

    #[test]
    fn blur_mode_tags_matching_news_stories() {
        let stories = vec![
            json!({
                "title": "Election coverage intensifies",
                "category": "Politics",
                "short_summary": "Candidates enter the final stretch."
            }),
            json!({
                "title": "Mars rover finds new rock sample",
                "category": "Science",
                "short_summary": "Planetary geology keeps moving."
            }),
        ];
        let filtered = apply_news_content_filters(
            stories,
            &NewsFilterRequest {
                preset_ids: vec![],
                keywords: vec!["election".to_string()],
                mode: NewsFilterMode::Blur,
                scope: NewsFilterScope::All,
            },
            "en",
        )
        .expect("blur mode should succeed");

        assert_eq!(filtered.stories.len(), 2);
        assert_eq!(filtered.summary.mode, "blur");
        assert_eq!(filtered.summary.filtered_count, 1);
        assert_eq!(
            filtered.stories[0]["_content_filter"]["mode"],
            Value::String("blur".to_string())
        );
        assert_eq!(
            filtered.stories[0]["_content_filter"]["matched_keywords"],
            json!(["election"])
        );
        assert!(filtered.stories[1].get("_content_filter").is_none());
    }

    #[test]
    fn rejects_unknown_news_filter_preset() {
        let error = apply_news_content_filters(
            vec![json!({
                "title": "Example",
                "category": "World",
                "short_summary": "Summary"
            })],
            &NewsFilterRequest {
                preset_ids: vec!["not-a-real-preset".to_string()],
                keywords: vec![],
                mode: NewsFilterMode::Hide,
                scope: NewsFilterScope::All,
            },
            "en",
        )
        .expect_err("unknown presets should fail");

        assert!(
            error
                .to_string()
                .contains("unknown news filter preset 'not-a-real-preset'")
        );
    }

    #[test]
    fn parses_assistant_thread_cursor_from_cursor_payload() {
        assert_eq!(
            parse_assistant_thread_cursor(
                r#"{"ack":"2026-02-11T16:22:13Z","created_at":"2026-02-11T16:22:13Z","id":"cursor-123"}"#
            ),
            Some(json!({
                "ack": "2026-02-11T16:22:13Z",
                "created_at": "2026-02-11T16:22:13Z",
                "id": "cursor-123"
            }))
        );
        assert_eq!(
            parse_assistant_thread_cursor("cursor-123"),
            Some(json!({ "id": "cursor-123" }))
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn assistant_thread_list_follows_cursor_pagination() {
        use httpmock::Method::GET;
        use httpmock::MockServer;

        let server = MockServer::start();
        let _folders = server.mock(|when, then| {
            when.method(GET)
                .path("/api/folders")
                .header("cookie", "kagi_session=test-session");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!([]));
        });
        let _first_page = server.mock(|when, then| {
            when.method(GET)
                .path("/api/conversations")
                .header("cookie", "kagi_session=test-session")
                .query_param("limit", "100")
                .query_param("all", "true")
                .query_param_missing("after_id");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "items": [{
                        "conversation": {
                            "uuid": "thread-1",
                            "title": "First Thread",
                            "created_at": "2026-02-11T16:22:13Z",
                            "updated_at": "2026-02-11T16:22:13Z",
                            "is_saved": false,
                            "is_shared": false,
                            "folder_uuid": null
                        },
                        "rank": 0,
                        "snippet": "First snippet"
                    }],
                    "has_more": true
                }));
        });
        let _second_page = server.mock(|when, then| {
            when.method(GET)
                .path("/api/conversations")
                .header("cookie", "kagi_session=test-session")
                .query_param("limit", "100")
                .query_param("all", "true")
                .query_param("after_id", "thread-1");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "items": [{
                        "conversation": {
                            "uuid": "thread-2",
                            "title": "Second Thread",
                            "created_at": "2026-02-12T16:22:13Z",
                            "updated_at": "2026-02-12T16:22:13Z",
                            "is_saved": false,
                            "is_shared": false,
                            "folder_uuid": null
                        },
                        "rank": 1,
                        "snippet": "Second snippet"
                    }],
                    "has_more": false
                }));
        });

        let _env_guard = lock_env();
        let _base_url_env = set_env_var("KAGI_ASSISTANT_BASE_URL", &server.base_url());
        let response = execute_assistant_thread_list("test-session")
            .await
            .expect("thread list should succeed");

        assert_eq!(response.threads.len(), 2);
        assert_eq!(response.threads[0].id, "thread-1");
        assert_eq!(response.threads[1].id, "thread-2");
        assert_eq!(response.pagination.count, 2);
        assert_eq!(response.pagination.total_counts.get("all"), Some(&2));
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

        let path_escape_error = normalize_assistant_thread_id(Some("../../api/folders"))
            .expect_err("path-like thread id should fail");
        assert!(
            path_escape_error
                .to_string()
                .contains("may only contain ASCII letters")
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn assistant_thread_get_migration_regression() {
        use httpmock::Method::GET;
        use httpmock::MockServer;

        assert_eq!(
            normalize_assistant_thread_id(Some(
                "  https://assistant.kagi.com/chat/thread-1/branch-1  "
            ))
            .expect("chat URL should normalize"),
            Some(String::from("thread-1"))
        );
        let thread_ref =
            normalize_assistant_thread_ref("  https://assistant.kagi.com/chat/thread-1/branch-1  ")
                .expect("chat URL should preserve branch");
        assert_eq!(thread_ref.thread_id, "thread-1");
        assert_eq!(thread_ref.branch_id.as_deref(), Some("branch-1"));

        let stream_error = parse_assistant_thread_open_stream(concat!(
            "hi:{\"v\":\"test\",\"trace\":\"trace-open\"}\0\n",
            "tags.json:[]\0\n",
            "error.json:{\"error\":{\"code\":\"not_found\",\"message\":\"Conversation not found\"}}\0\n"
        ))
        .expect_err("error.json should surface the upstream error");
        assert_eq!(
            stream_error.to_string(),
            "configuration error: Kagi Assistant thread open returned an error: Conversation not found"
        );

        let server = MockServer::start();
        let _folders = server.mock(|when, then| {
            when.method(GET)
                .path("/api/folders")
                .header("cookie", "kagi_session=test-session");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!([]));
        });
        let _thread = server.mock(|when, then| {
            when.method(GET)
                .path("/api/conversations/thread-1/init")
                .header("cookie", "kagi_session=test-session")
                .header("accept", "application/json")
                .query_param("branch_uuid", "branch-1");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "conversation": {
                        "uuid": "thread-1",
                        "title": "Migrated Chat",
                        "created_at": "2026-07-06T20:19:29Z",
                        "updated_at": "2026-07-06T20:20:29Z",
                        "is_saved": true,
                        "is_shared": false,
                        "folder_uuid": "folder-1"
                    },
                    "active_branch": {
                        "uuid": "branch-1",
                        "conversation_uuid": "thread-1",
                        "is_default": true,
                        "message_count": 4,
                        "updated_at": "2026-07-06T20:20:29Z"
                    },
                    "branches": [],
                    "messages": {
                        "items": [
                            {
                                "uuid": "msg-user",
                                "role": "user",
                                "content": "Hello",
                                "created_at": "2026-07-06T20:19:29Z"
                            },
                            {
                                "uuid": "msg-assistant",
                                "role": "assistant",
                                "content": "Hello from the migrated Assistant",
                                "html_content": "<p>Hello from the migrated Assistant</p>",
                                "created_at": "2026-07-06T20:20:29Z",
                                "references": [{
                                    "index": 1,
                                    "title": "Kagi",
                                    "url": "https://kagi.com"
                                }],
                                "model_name": "test-model"
                            }
                        ],
                        "has_more": true
                    }
                }));
        });
        let _older_messages = server.mock(|when, then| {
            when.method(GET)
                .path("/api/branches/branch-1/messages")
                .header("cookie", "kagi_session=test-session")
                .header("accept", "application/json")
                .query_param("limit", "100")
                .query_param("after_id", "msg-user");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "branch": {
                        "uuid": "branch-1",
                        "conversation_uuid": "thread-1",
                        "is_default": true,
                        "message_count": 4,
                        "updated_at": "2026-07-06T20:20:29Z"
                    },
                    "messages": {
                        "items": [
                            {
                                "uuid": "msg-earlier-assistant",
                                "role": "assistant",
                                "content": "Earlier answer",
                                "html_content": "<p>Earlier answer</p>",
                                "created_at": "2026-07-06T20:18:29Z",
                                "references": []
                            },
                            {
                                "uuid": "msg-earlier-user",
                                "role": "user",
                                "content": "Earlier question",
                                "created_at": "2026-07-06T20:17:29Z"
                            }
                        ],
                        "has_more": false
                    }
                }));
        });

        let _env_guard = lock_env();
        let _base_url_env = set_env_var("KAGI_ASSISTANT_BASE_URL", &server.base_url());
        let response = execute_assistant_thread_get(
            "https://assistant.kagi.com/chat/thread-1/branch-1",
            "test-session",
        )
        .await
        .expect("current Assistant thread should load");

        assert_eq!(response.thread.id, "thread-1");
        assert_eq!(response.thread.branch_id, "branch-1");
        assert_eq!(response.thread.folder_ids, vec![String::from("folder-1")]);
        assert_eq!(response.messages.len(), 2);
        assert_eq!(response.messages[0].prompt, "Earlier question");
        assert_eq!(
            response.messages[0].markdown.as_deref(),
            Some("Earlier answer")
        );
        assert_eq!(response.messages[1].prompt, "Hello");
        assert_eq!(
            response.messages[1].markdown.as_deref(),
            Some("Hello from the migrated Assistant")
        );
        assert_eq!(
            response.messages[1].references_markdown.as_deref(),
            Some("[^1]: [Kagi](https://kagi.com)")
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn assistant_thread_delete_rejects_html_redirect_success() {
        use httpmock::Method::DELETE;
        use httpmock::MockServer;

        let server = MockServer::start();
        let _delete = server.mock(|when, then| {
            when.method(DELETE)
                .path("/api/conversations/thread-1")
                .header("cookie", "kagi_session=test-session")
                .header("accept", "application/json");
            then.status(200)
                .header("content-type", "text/html")
                .body("<!doctype html><html><title>Sign in</title></html>");
        });

        let _env_guard = lock_env();
        let _base_url_env = set_env_var("KAGI_ASSISTANT_BASE_URL", &server.base_url());
        let error = execute_assistant_thread_delete("thread-1", "test-session")
            .await
            .expect_err("HTML success body should not be accepted as a delete");

        assert!(
            error
                .to_string()
                .contains("invalid or expired Kagi session token")
        );
    }

    #[test]
    fn assistant_sse_frames_preserve_split_utf8_and_crlf_delimiters() {
        let encoded = "data: {\"text\":\"hello 🙂\"}\r\n\r\n".as_bytes();
        let emoji_start = encoded
            .windows("🙂".len())
            .position(|window| window == "🙂".as_bytes())
            .expect("emoji should be present");
        let mut pending = encoded[..emoji_start + 1].to_vec();

        assert!(
            take_next_assistant_sse_frame(&mut pending)
                .expect("partial frame should be valid")
                .is_none()
        );

        pending.extend_from_slice(&encoded[emoji_start + 1..]);
        let frame = take_next_assistant_sse_frame(&mut pending)
            .expect("complete frame should be valid")
            .expect("complete frame should be returned");
        assert_eq!(frame, "data: {\"text\":\"hello 🙂\"}");
        assert!(pending.is_empty());
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn rejects_missing_assistant_attachment_before_creating_conversation() {
        use httpmock::Method::POST;
        use httpmock::MockServer;

        let server = MockServer::start();
        let conversation = server.mock(|when, then| {
            when.method(POST).path("/api/conversations");
            then.status(500);
        });
        let missing = PathBuf::from("/tmp/definitely-missing-kagi-assistant-attachment.txt");
        let _env_guard = lock_env();
        let _base_url_env = set_env_var("KAGI_ASSISTANT_BASE_URL", &server.base_url());
        let error = execute_assistant_prompt(
            &AssistantPromptRequest {
                query: "hello".to_string(),
                thread_id: None,
                attachments: vec![missing.clone()],
                profile_id: None,
                model: None,
                lens_id: None,
                internet_access: None,
                personalizations: None,
            },
            "test-session",
        )
        .await
        .expect_err("missing attachment should fail");

        assert!(
            error
                .to_string()
                .contains("failed to read assistant attachment")
        );
        assert!(error.to_string().contains(&missing.display().to_string()));
        conversation.assert_calls(0);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn assistant_prompt_uses_multipart_when_attachments_are_present() {
        use httpmock::Method::POST;
        use httpmock::MockServer;

        let server = MockServer::start();
        let _conversation = server.mock(|when, then| {
            when.method(POST).path("/api/conversations");
            then.status(200).json_body(json!({
                "conversation": {
                    "uuid": "thread-1",
                    "title": "Upload test",
                    "created_at": "2026-04-24T00:00:00Z",
                    "updated_at": "2026-04-24T00:00:00Z",
                    "is_saved": false,
                    "is_shared": false
                },
                "default_branch": {"uuid": "branch-1", "is_default": true}
            }));
        });
        let _upload = server.mock(|when, then| {
            when.method(POST)
                .path("/api/upload")
                .body_includes("attached-note");
            then.status(200).json_body(json!({"uuid": "attachment-1"}));
        });
        let _message = server.mock(|when, then| {
            when.method(POST)
                .path("/api/branches/branch-1/messages")
                .body_includes("attachment-1")
                .body_includes("Reply with exactly: attached-note");
            then.status(200).json_body(json!({
                "conversation": {
                    "uuid": "thread-1",
                    "title": "Upload test",
                    "created_at": "2026-04-24T00:00:00Z",
                    "updated_at": "2026-04-24T00:00:00Z",
                    "is_saved": false,
                    "is_shared": false
                },
                "branch": {"uuid": "branch-1", "is_default": true},
                "user_message": {
                    "uuid": "user-1",
                    "role": "user",
                    "content": "Reply with exactly: attached-note",
                    "created_at": "2026-04-24T00:00:00Z"
                }
            }));
        });
        let _stream = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/branches/branch-1/stream");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(concat!(
                    "data: {\"text\":\"attached-note\",\"html_content\":\"<p>attached-note</p>\",",
                    "\"assistant_message_uuid\":\"msg-1\",\"is_final\":true}\n\n",
                    "data: [DONE]\n\n"
                ));
        });
        let tempdir = TempDir::new().expect("tempdir");
        let attachment_path = tempdir.path().join("note.txt");
        fs::write(&attachment_path, "attached-note").expect("attachment should write");

        let _env_guard = lock_env();
        let _base_url_env = set_env_var("KAGI_ASSISTANT_BASE_URL", &server.base_url());
        let response = execute_assistant_prompt(
            &AssistantPromptRequest {
                query: "Reply with exactly: attached-note".to_string(),
                thread_id: None,
                attachments: vec![attachment_path],
                profile_id: None,
                model: Some("gpt-5-mini".to_string()),
                lens_id: None,
                internet_access: Some(false),
                personalizations: Some(false),
            },
            "test-session",
        )
        .await
        .expect("assistant prompt should succeed");

        assert_eq!(response.thread.id, "thread-1");
        assert_eq!(response.message.markdown.as_deref(), Some("attached-note"));
        assert_eq!(
            response
                .message
                .profile
                .as_ref()
                .and_then(|profile| { profile.get("model_name").and_then(Value::as_str) }),
            Some("gpt-5-mini")
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn assistant_prompt_accepts_delayed_stream_response() {
        use httpmock::Method::POST;
        use httpmock::MockServer;

        let server = MockServer::start();
        let _conversation = server.mock(|when, then| {
            when.method(POST).path("/api/conversations");
            then.status(200).json_body(json!({
                "conversation": {
                    "uuid": "thread-delayed",
                    "title": "Delayed test",
                    "created_at": "2026-05-01T00:00:00Z",
                    "updated_at": "2026-05-01T00:00:00Z",
                    "is_saved": false,
                    "is_shared": false
                },
                "default_branch": {"uuid": "branch-delayed", "is_default": true}
            }));
        });
        let _message = server.mock(|when, then| {
            when.method(POST)
                .path("/api/branches/branch-delayed/messages");
            then.status(200).json_body(json!({
                "conversation": {
                    "uuid": "thread-delayed",
                    "title": "Delayed test",
                    "created_at": "2026-05-01T00:00:00Z",
                    "updated_at": "2026-05-01T00:00:00Z",
                    "is_saved": false,
                    "is_shared": false
                },
                "branch": {"uuid": "branch-delayed", "is_default": true},
                "user_message": {
                    "uuid": "user-delayed",
                    "role": "user",
                    "content": "Hello",
                    "created_at": "2026-05-01T00:00:00Z"
                }
            }));
        });
        let _stream = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/api/branches/branch-delayed/stream");
            then.status(200)
                .header("content-type", "text/event-stream")
                .delay(Duration::from_millis(200))
                .body(concat!(
                    "data: {\"text\":\"delayed-ok\",",
                    "\"assistant_message_uuid\":\"msg-delayed\",\"is_final\":true}\n\n",
                    "data: [DONE]\n\n"
                ));
        });
        let _env_guard = lock_env();
        let _base_url_env = set_env_var("KAGI_ASSISTANT_BASE_URL", &server.base_url());
        let response = execute_assistant_prompt(
            &AssistantPromptRequest {
                query: "Hello".to_string(),
                thread_id: None,
                attachments: Vec::new(),
                profile_id: None,
                model: None,
                lens_id: None,
                internet_access: None,
                personalizations: None,
            },
            "test-session",
        )
        .await
        .expect("delayed assistant prompt should succeed");

        assert_eq!(response.thread.id, "thread-delayed");
        assert_eq!(response.message.markdown.as_deref(), Some("delayed-ok"));
    }

    #[test]
    fn normalizes_custom_bang_trigger_and_redirect_rule() {
        assert_eq!(
            normalize_custom_bang_trigger(" !gh ").expect("trigger should normalize"),
            "gh"
        );
        assert_eq!(
            normalize_redirect_rule("^https://a|https://b").expect("rule should normalize"),
            "^https://a|https://b"
        );
        assert!(normalize_custom_bang_trigger("   ").is_err());
        assert!(normalize_redirect_rule("https://a").is_err());
    }

    #[test]
    fn resolves_new_resource_refs_by_expected_keys() {
        let assistants = vec![
            AssistantProfileSummary {
                id: "built-in".to_string(),
                name: "Code".to_string(),
                invoke_profile: "code".to_string(),
                model: "Quick".to_string(),
                bang_trigger: None,
                internet_access: true,
                built_in: true,
                edit_url: None,
            },
            AssistantProfileSummary {
                id: "custom-1".to_string(),
                name: "Writer".to_string(),
                invoke_profile: "custom-1".to_string(),
                model: "GPT 5 Mini".to_string(),
                bang_trigger: Some("!write".to_string()),
                internet_access: false,
                built_in: false,
                edit_url: Some("/settings/custom_assistant?id=custom-1".to_string()),
            },
        ];
        let lenses = vec![LensSummary {
            id: "22524".to_string(),
            name: "Reddit".to_string(),
            description: None,
            enabled: true,
            position: Some(0),
            edit_url: "/settings/update_lens?id=22524".to_string(),
            toggle_field: "active_index".to_string(),
            toggle_value: "0".to_string(),
        }];
        let bangs = vec![CustomBangSummary {
            id: "1".to_string(),
            name: "Google".to_string(),
            trigger: "!g".to_string(),
            shortcut_menu: true,
            edit_url: "/settings/custom_bangs_form?bang_id=1".to_string(),
        }];
        let redirects = vec![RedirectRuleSummary {
            id: "16641".to_string(),
            rule: "^https://www.reddit.com|https://old.reddit.com".to_string(),
            enabled: true,
            edit_url: "/settings/redirects_form?rule_id=16641".to_string(),
        }];

        assert_eq!(
            resolve_custom_assistant_ref(&assistants, "code", false)
                .expect("built-in assistant should resolve")
                .id,
            "built-in"
        );
        assert!(resolve_custom_assistant_ref(&assistants, "Code", true).is_err());
        assert_eq!(
            resolve_lens_ref(&lenses, "Reddit")
                .expect("lens should resolve")
                .id,
            "22524"
        );
        assert_eq!(
            resolve_custom_bang_ref(&bangs, "g")
                .expect("bang should resolve")
                .id,
            "1"
        );
        assert_eq!(
            resolve_redirect_ref(&redirects, "^https://www.reddit.com|https://old.reddit.com")
                .expect("redirect should resolve")
                .id,
            "16641"
        );
    }

    #[test]
    fn parses_assistant_thread_open_stream() {
        let raw = concat!(
            "hi:{\"v\":\"202603171911.stage.707e740\",\"trace\":\"trace-open\"}\0\n",
            "folders.json:[]\0\n",
            "thread.json:{\"id\":\"thread-1\",\"title\":\"Greeting\",\"ack\":\"2026-03-16T06:19:07Z\",\"created_at\":\"2026-03-16T06:19:07Z\",\"expires_at\":\"2026-03-16T07:19:07Z\",\"saved\":false,\"shared\":false,\"branch_id\":\"00000000-0000-4000-0000-000000000000\",\"folder_ids\":[]}\0\n",
            "messages.json:[{\"id\":\"msg-1\",\"thread_id\":\"thread-1\",\"created_at\":\"2026-03-16T06:19:07Z\",\"branch_list\":[],\"state\":\"done\",\"prompt\":\"Hello\",\"reply\":\"<p>Hi</p>\",\"md\":\"Hi\",\"metadata\":\"\",\"documents\":[],\"trace_id\":\"trace-msg\"}]\0\n"
        );

        let parsed = parse_assistant_thread_open_stream(raw).expect("thread open parses");
        assert_eq!(parsed.meta.trace.as_deref(), Some("trace-open"));
        assert_eq!(parsed.thread.id, "thread-1");
        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.messages[0].trace_id.as_deref(), Some("trace-msg"));
    }

    #[test]
    fn parses_assistant_thread_open_stream_with_thread_html_fallback() {
        let raw = concat!(
            "hi:{\"v\":\"202603171911.stage.707e740\",\"trace\":\"trace-open-html\"}\0\n",
            "tags.json:[]\0\n",
            "thread.html:<li class=\"thread\"\n",
            "    data-code=\"bb8254ea-4b02-4353-be53-b1bd8632e55e\"\n",
            "    data-saved=\"false\"\n",
            "    data-public=\"false\"\n",
            "    data-tags=\"[]\"\n",
            "    data-folders='[\"folder-1\", \"folder-2\"]'\n",
            "    data-snippet=\"none\"\n",
            "    >\n",
            "  <i title=\"Temporary\">temp</i>\n",
            "  <a href=\"/assistant/thread/bb8254ea-4b02-4353-be53-b1bd8632e55e\">\n",
            "    <span class=\"title\">Testing Conversation</span>\n",
            "  </a>\n",
            "</li>\0\n",
            "messages.json:[{\"id\":\"msg-1\",\"thread_id\":\"bb8254ea-4b02-4353-be53-b1bd8632e55e\",\"created_at\":\"2026-03-16T06:19:07Z\",\"branch_list\":[],\"state\":\"done\",\"prompt\":\"Hello\",\"reply\":\"<p>Hi</p>\",\"md\":\"Hi\",\"metadata\":\"\",\"documents\":[],\"trace_id\":\"trace-msg\"}]\0\n"
        );

        let parsed = parse_assistant_thread_open_stream(raw).expect("thread open parses from html");

        assert_eq!(parsed.meta.trace.as_deref(), Some("trace-open-html"));
        assert_eq!(parsed.thread.id, "bb8254ea-4b02-4353-be53-b1bd8632e55e");
        assert_eq!(parsed.thread.title, "Testing Conversation");
        assert!(!parsed.thread.saved);
        assert!(!parsed.thread.shared);
        assert!(parsed.thread.ack.is_empty());
        assert!(parsed.thread.created_at.is_empty());
        assert!(parsed.thread.expires_at.is_empty());
        assert!(parsed.thread.branch_id.is_empty());
        assert_eq!(parsed.thread.folder_ids, vec!["folder-1", "folder-2"]);
        assert_eq!(parsed.messages.len(), 1);
    }

    #[test]
    fn reports_received_tags_when_thread_open_stream_has_no_thread_frame() {
        let raw = concat!(
            "hi:{\"v\":\"202603171911.stage.707e740\",\"trace\":\"trace-open-missing\"}\0\n",
            "tags.json:[]\0\n",
            "messages.json:[{\"id\":\"msg-1\",\"thread_id\":\"thread-1\",\"created_at\":\"2026-03-16T06:19:07Z\",\"branch_list\":[],\"state\":\"done\",\"prompt\":\"Hello\",\"reply\":\"<p>Hi</p>\",\"md\":\"Hi\",\"metadata\":\"\",\"documents\":[],\"trace_id\":\"trace-msg\"}]\0\n"
        );

        let error = parse_assistant_thread_open_stream(raw)
            .expect_err("missing thread frame should fail to parse");

        assert_eq!(
            error.to_string(),
            "parse error: assistant thread open response did not include a thread.json or thread.html frame (received tags: hi, tags.json, messages.json)"
        );
    }

    #[test]
    fn parses_assistant_thread_list_stream() {
        let raw = concat!(
            "hi:{\"v\":\"202603171911.stage.707e740\",\"trace\":\"trace-list\"}\0\n",
            "folders.json:[]\0\n",
            "thread_list.html:{\"html\":\"<div class=\\\"hide-if-no-threads\\\"><ul class=\\\"thread-list\\\"><li class=\\\"thread\\\" data-code=\\\"thread-1\\\" data-saved=\\\"true\\\" data-public=\\\"false\\\" data-folders='[&quot;folder-1&quot;]' data-snippet=\\\"First snippet\\\"><a href=\\\"/assistant/thread-1\\\"><div class=\\\"title\\\">First Thread</div><div class=\\\"excerpt\\\">First snippet</div></a></li></ul></div>\",\"next_cursor\":null,\"has_more\":false,\"count\":1,\"total_counts\":{\"all\":1}}\0\n"
        );

        let parsed = parse_assistant_thread_list_stream(raw).expect("thread list parses");
        assert_eq!(parsed.meta.trace.as_deref(), Some("trace-list"));
        assert_eq!(parsed.threads.len(), 1);
        assert_eq!(parsed.threads[0].id, "thread-1");
        assert_eq!(parsed.pagination.count, 1);
        assert_eq!(parsed.pagination.total_counts.get("all"), Some(&1));
    }

    #[test]
    fn parses_assistant_thread_list_stream_with_wrapped_html_and_object_cursor() {
        let raw = concat!(
            "hi:{\"v\":\"202603171911.stage.707e740\",\"trace\":\"trace-list-object\"}\0\n",
            "folders.json:[]\0\n",
            "thread_list.html:{\"html\":{\"html\":\"<div class=\\\"hide-if-no-threads\\\"><ul class=\\\"thread-list\\\"><li class=\\\"thread\\\" data-code=\\\"thread-2\\\" data-saved=\\\"false\\\" data-public=\\\"true\\\" data-folders='[]' data-snippet=\\\"Second snippet\\\"><a href=\\\"/assistant/thread-2\\\"><div class=\\\"title\\\">Second Thread</div><div class=\\\"excerpt\\\">Second snippet</div></a></li></ul></div>\"},\"next_cursor\":{\"offset\":100,\"has_more\":true},\"has_more\":true,\"count\":100,\"total_counts\":{\"all\":250}}\0\n"
        );

        let parsed = parse_assistant_thread_list_stream(raw).expect("thread list parses");
        assert_eq!(parsed.meta.trace.as_deref(), Some("trace-list-object"));
        assert_eq!(parsed.threads.len(), 1);
        assert_eq!(parsed.threads[0].id, "thread-2");
        let next_cursor = parsed
            .pagination
            .next_cursor
            .as_deref()
            .expect("wrapped cursor should be preserved");
        let next_cursor_json: Value =
            serde_json::from_str(next_cursor).expect("cursor should stay valid JSON");
        assert_eq!(next_cursor_json["offset"], 100);
        assert_eq!(next_cursor_json["has_more"], true);
        assert!(parsed.pagination.has_more);
        assert_eq!(parsed.pagination.count, 100);
        assert_eq!(parsed.pagination.total_counts.get("all"), Some(&250));
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

    async fn live_assistant_base_model(token: &str) -> String {
        let catalog = super::execute_assistant_model_catalog(token)
            .await
            .expect("assistant model catalog should load");
        catalog.default
    }

    #[tokio::test]
    #[ignore]
    async fn live_assistant_thread_roundtrip() {
        let Some(token) = live_session_token() else {
            eprintln!("skipping live assistant test because {SESSION_TOKEN_ENV} is not set");
            return;
        };

        let model = live_assistant_base_model(&token).await;
        let request = AssistantPromptRequest {
            query: format!("Reply with exactly: assistant-v2-smoke-{}", live_nonce()),
            thread_id: None,
            attachments: Vec::new(),
            profile_id: None,
            model: Some(model.clone()),
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
            Some(model.as_str())
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

    #[tokio::test]
    #[ignore = "requires live KAGI_SESSION_TOKEN and mutates custom assistants"]
    async fn live_custom_assistant_crud_roundtrip() {
        let Some(token) = live_session_token() else {
            eprintln!("skipping live assistant custom test because {SESSION_TOKEN_ENV} is not set");
            return;
        };

        let nonce = live_nonce();
        let name = format!("codex-assistant-{nonce}");
        let updated_name = format!("{name}-updated");
        let bang = format!("ca{nonce}");
        let model = live_assistant_base_model(&token).await;

        let created = execute_custom_assistant_create(
            &AssistantProfileCreateRequest {
                name: name.clone(),
                bang_trigger: Some(bang.clone()),
                internet_access: Some(false),
                selected_lens: Some("0".to_string()),
                personalizations: Some(false),
                base_model: Some(model.clone()),
                custom_instructions: Some("Reply in exactly one sentence.".to_string()),
            },
            &token,
        )
        .await
        .expect("custom assistant create should succeed");

        let created_id = created
            .profile_id
            .clone()
            .expect("created assistant should have id");
        assert_eq!(created.name, name);
        assert_eq!(created.bang_trigger.as_deref(), Some(bang.as_str()));
        assert!(!created.internet_access);

        let listed = execute_custom_assistant_list(&token)
            .await
            .expect("custom assistant list should succeed");
        assert!(listed.iter().any(|assistant| assistant.id == created_id));

        let fetched = execute_custom_assistant_get(&created_id, &token)
            .await
            .expect("custom assistant get should succeed");
        assert_eq!(fetched.base_model, model.as_str());

        let prompt = execute_assistant_prompt(
            &AssistantPromptRequest {
                query: "Reply with exactly: custom-assistant-smoke".to_string(),
                thread_id: None,
                attachments: Vec::new(),
                profile_id: Some(created_id.clone()),
                model: None,
                lens_id: None,
                internet_access: None,
                personalizations: None,
            },
            &token,
        )
        .await
        .expect("assistant prompt with saved assistant should succeed");
        assert_eq!(prompt.message.state, "done");

        let updated = execute_custom_assistant_update(
            &AssistantProfileUpdateRequest {
                target: created_id.clone(),
                name: Some(updated_name.clone()),
                bang_trigger: None,
                internet_access: Some(true),
                selected_lens: Some("0".to_string()),
                personalizations: Some(true),
                base_model: Some(model),
                custom_instructions: Some("Use bullet points when useful.".to_string()),
            },
            &token,
        )
        .await
        .expect("custom assistant update should succeed");

        assert_eq!(updated.name, updated_name);
        assert!(updated.internet_access);
        assert_eq!(updated.selected_lens, "0");
        assert!(updated.personalizations);

        let deleted = execute_custom_assistant_delete(&created_id, &token)
            .await
            .expect("custom assistant delete should succeed");
        assert_eq!(deleted.id, created_id);
    }

    #[tokio::test]
    #[ignore = "requires live KAGI_SESSION_TOKEN and mutates lenses"]
    async fn live_lens_crud_roundtrip() {
        let Some(token) = live_session_token() else {
            eprintln!("skipping live lens test because {SESSION_TOKEN_ENV} is not set");
            return;
        };

        let nonce = live_nonce();
        let suffix = (nonce % 1_000_000_000).to_string();
        let name = format!("cl-{suffix}");
        let updated_name = format!("clu-{suffix}");

        let created = execute_lens_create(
            &LensCreateRequest {
                name: name.clone(),
                included_sites: Some("example.invalid".to_string()),
                included_keywords: Some("codex".to_string()),
                description: Some("Codex test lens".to_string()),
                search_region: Some("no_region".to_string()),
                before_time: None,
                after_time: None,
                excluded_sites: Some("blocked.example.invalid".to_string()),
                excluded_keywords: Some("ignoreme".to_string()),
                shortcut_keyword: Some(format!("cl{nonce}")),
                autocomplete_keywords: Some(false),
                template: Some("0".to_string()),
                file_type: Some("pdf".to_string()),
                share_with_team: Some(false),
                share_copy_code: Some(false),
            },
            &token,
        )
        .await
        .expect("lens create should succeed");

        let lens_id = created.id.clone().expect("created lens should have id");
        assert_eq!(created.name, name);
        assert_eq!(created.included_sites, "example.invalid");

        let toggled_off = execute_lens_set_enabled(&lens_id, false, &token)
            .await
            .expect("lens disable should succeed");
        assert!(!toggled_off.enabled);

        let toggled_on = execute_lens_set_enabled(&lens_id, true, &token)
            .await
            .expect("lens enable should succeed");
        assert!(toggled_on.enabled);

        let updated = execute_lens_update(
            &LensUpdateRequest {
                target: lens_id.clone(),
                name: Some(updated_name.clone()),
                included_sites: Some("example.invalid, docs.example.invalid".to_string()),
                included_keywords: Some("codex, rust".to_string()),
                description: Some("Updated Codex lens".to_string()),
                search_region: Some("us".to_string()),
                before_time: None,
                after_time: None,
                excluded_sites: Some("blocked.example.invalid".to_string()),
                excluded_keywords: Some("ignoreme".to_string()),
                shortcut_keyword: Some(format!("clu{nonce}")),
                autocomplete_keywords: Some(true),
                template: Some("1".to_string()),
                file_type: Some("md".to_string()),
                share_with_team: Some(false),
                share_copy_code: Some(false),
            },
            &token,
        )
        .await
        .expect("lens update should succeed");

        assert_eq!(updated.name, updated_name);
        assert_eq!(updated.search_region, "us");
        assert!(updated.autocomplete_keywords);
        assert_eq!(updated.template, "1");

        let deleted = execute_lens_delete(&lens_id, &token)
            .await
            .expect("lens delete should succeed");
        assert_eq!(deleted.id, lens_id);
    }

    #[tokio::test]
    #[ignore = "requires live KAGI_SESSION_TOKEN and mutates custom bangs"]
    async fn live_custom_bang_crud_roundtrip() {
        let Some(token) = live_session_token() else {
            eprintln!("skipping live custom bang test because {SESSION_TOKEN_ENV} is not set");
            return;
        };

        let nonce = live_nonce();
        let name = format!("Codex Bang {nonce}");
        let trigger = format!("cb{nonce}");
        let updated_trigger = format!("cbu{nonce}");

        let created = execute_custom_bang_create(
            &CustomBangCreateRequest {
                name: name.clone(),
                trigger: trigger.clone(),
                template: Some("https://example.invalid/search?q=%s".to_string()),
                snap_domain: Some("example.invalid".to_string()),
                regex_pattern: None,
                shortcut_menu: Some(false),
                fmt_open_snap_domain: Some(false),
                fmt_open_base_path: Some(true),
                fmt_url_encode_placeholder: Some(true),
                fmt_url_encode_space_to_plus: Some(false),
            },
            &token,
        )
        .await
        .expect("custom bang create should succeed");

        let bang_id = created
            .bang_id
            .clone()
            .expect("created bang should have id");
        assert_eq!(created.name, name);
        assert_eq!(created.trigger, trigger);

        let fetched = execute_custom_bang_get(&bang_id, &token)
            .await
            .expect("custom bang get should succeed");
        assert_eq!(fetched.snap_domain, "example.invalid");

        let updated = execute_custom_bang_update(
            &CustomBangUpdateRequest {
                target: bang_id.clone(),
                name: Some(format!("{name} Updated")),
                trigger: Some(updated_trigger.clone()),
                template: Some("https://example.invalid/find?q=%s".to_string()),
                snap_domain: Some("example.invalid".to_string()),
                regex_pattern: Some("^(.+)$".to_string()),
                shortcut_menu: Some(true),
                fmt_open_snap_domain: Some(false),
                fmt_open_base_path: Some(true),
                fmt_url_encode_placeholder: Some(true),
                fmt_url_encode_space_to_plus: Some(true),
            },
            &token,
        )
        .await
        .expect("custom bang update should succeed");

        assert_eq!(updated.trigger, updated_trigger);
        assert!(updated.shortcut_menu);
        assert!(updated.fmt_url_encode_space_to_plus);

        let deleted = execute_custom_bang_delete(&bang_id, &token)
            .await
            .expect("custom bang delete should succeed");
        assert_eq!(deleted.id, bang_id);
    }

    #[tokio::test]
    #[ignore = "requires live KAGI_SESSION_TOKEN and mutates redirects"]
    async fn live_redirect_crud_roundtrip() {
        let Some(token) = live_session_token() else {
            eprintln!("skipping live redirect test because {SESSION_TOKEN_ENV} is not set");
            return;
        };

        let nonce = live_nonce();
        let rule = format!(
            "^https://probe-{nonce}.example.invalid|https://target-{nonce}.example.invalid"
        );
        let updated_rule = format!(
            "^https://probe-{nonce}.example.invalid|https://updated-{nonce}.example.invalid"
        );

        let created =
            execute_redirect_create(&RedirectRuleCreateRequest { rule: rule.clone() }, &token)
                .await
                .expect("redirect create should succeed");

        let rule_id = created
            .rule_id
            .clone()
            .expect("created redirect should have id");
        assert_eq!(created.rule, rule);

        let listed = execute_redirect_list(&token)
            .await
            .expect("redirect list should succeed");
        assert!(listed.iter().any(|redirect| redirect.id == rule_id));

        let toggled_off = execute_redirect_set_enabled(&rule_id, false, &token)
            .await
            .expect("redirect disable should succeed");
        assert!(!toggled_off.enabled);

        let toggled_on = execute_redirect_set_enabled(&rule_id, true, &token)
            .await
            .expect("redirect enable should succeed");
        assert!(toggled_on.enabled);

        let updated = execute_redirect_update(
            &RedirectRuleUpdateRequest {
                target: rule_id.clone(),
                rule: updated_rule.clone(),
            },
            &token,
        )
        .await
        .expect("redirect update should succeed");

        assert_eq!(updated.rule, updated_rule);

        let deleted = execute_redirect_delete(&rule_id, &token)
            .await
            .expect("redirect delete should succeed");
        assert_eq!(deleted.id, rule_id);
    }

    #[test]
    fn normalizes_ask_page_url() {
        let normalized = normalize_ask_page_url("https://rust-lang.org").expect("url parses");
        assert_eq!(normalized, "https://rust-lang.org/");
    }

    #[test]
    fn rejects_invalid_ask_page_url() {
        let error = normalize_ask_page_url("rust-lang.org").expect_err("url should fail");
        assert!(error.to_string().contains("invalid ask-page URL"));
    }

    #[test]
    fn rejects_non_http_ask_page_url() {
        let error =
            normalize_ask_page_url("file:///tmp/page.html").expect_err("scheme should fail");
        assert!(error.to_string().contains("http or https"));
    }

    #[test]
    fn rejects_empty_ask_page_question() {
        let error = normalize_ask_page_question("   ").expect_err("question should fail");
        assert!(error.to_string().contains("question cannot be empty"));
    }

    #[test]
    fn builds_ask_page_prompt() {
        let prompt = build_ask_page_prompt("https://rust-lang.org/", "What is this page about?");
        assert_eq!(prompt, "https://rust-lang.org/\nWhat is this page about?");
    }

    #[tokio::test]
    #[ignore = "requires live KAGI_SESSION_TOKEN"]
    async fn live_ask_page_rust_homepage() {
        let Some(token) = live_session_token() else {
            eprintln!("skipping live ask-page test because {SESSION_TOKEN_ENV} is not set");
            return;
        };

        let response = super::execute_ask_page(
            &AskPageRequest {
                url: "https://rust-lang.org/".to_string(),
                question: "What is this page about?".to_string(),
            },
            &token,
        )
        .await
        .expect("live ask-page should succeed");

        assert_eq!(response.source.url, "https://rust-lang.org/");
        assert!(!response.thread.id.is_empty());
        let answer = response
            .message
            .markdown
            .unwrap_or_default()
            .to_ascii_lowercase();
        assert!(answer.contains("rust"));
    }

    #[test]
    fn parses_translate_detect_from_object_or_array() {
        let object = json!({
            "iso": "fr",
            "label": "French",
            "isUncertain": false,
            "isMixed": false
        });
        let array = json!([
            {
                "iso": "fr",
                "label": "French",
                "isUncertain": false,
                "isMixed": false
            }
        ]);

        let parsed_object = parse_translate_detect_value(object).expect("object should parse");
        let parsed_array = parse_translate_detect_value(array).expect("array should parse");

        assert_eq!(parsed_object.iso, "fr");
        assert_eq!(parsed_array.label, "French");
    }

    #[test]
    fn rejects_empty_translate_detect_array() {
        let error = parse_translate_detect_value(Value::Array(vec![]))
            .expect_err("empty array should fail");
        assert!(error.to_string().contains("empty array"));
    }

    #[test]
    fn rejects_translate_target_auto_value() {
        let mut request = sample_translate_request();
        request.to = "auto".to_string();

        let error = validate_translate_request(&request).expect_err("auto target should fail");
        assert!(error.to_string().contains("explicit target language code"));
    }

    #[test]
    fn rejects_empty_translate_text() {
        let mut request = sample_translate_request();
        request.text = "   ".to_string();

        let error = validate_translate_request(&request).expect_err("empty text should fail");
        assert!(error.to_string().contains("translate text cannot be empty"));
    }

    #[test]
    fn rejects_empty_translate_source_language() {
        let mut request = sample_translate_request();
        request.from = "   ".to_string();

        let error = validate_translate_request(&request).expect_err("empty source should fail");
        assert!(
            error
                .to_string()
                .contains("translate --from cannot be empty")
        );
    }

    #[test]
    fn rejects_empty_translate_target_language() {
        let mut request = sample_translate_request();
        request.to = "   ".to_string();

        let error = validate_translate_request(&request).expect_err("empty target should fail");
        assert!(error.to_string().contains("translate --to cannot be empty"));
    }

    #[test]
    fn extracts_translate_session_from_set_cookie_headers() {
        let headers = fake_header_map(&[
            "translate_language=en; Max-Age=31536000; Path=/; HttpOnly; Secure; SameSite=Lax",
            "translate_session=abc.def.ghi; Path=/; Expires=Wed, 18 Mar 2026 23:41:41 GMT; HttpOnly; Secure; SameSite=Lax",
        ]);

        let cookie = extract_set_cookie_value(&headers, "translate_session");

        assert_eq!(cookie.as_deref(), Some("abc.def.ghi"));
    }

    #[test]
    fn returns_none_when_set_cookie_name_is_missing() {
        let headers = fake_header_map(&[
            "translate_language=en; Max-Age=31536000; Path=/; HttpOnly; Secure; SameSite=Lax",
        ]);

        assert_eq!(
            extract_set_cookie_value(&headers, "translate_session"),
            None
        );
    }

    #[test]
    fn resolves_translate_bootstrap_from_success_cookie() {
        let headers = fake_header_map(&[
            "translate_session=abc.def.ghi; Path=/; HttpOnly; Secure; SameSite=Lax",
        ]);

        let bootstrap =
            resolve_translate_bootstrap(StatusCode::OK, &headers).expect("bootstrap resolves");

        assert_eq!(bootstrap.translate_session, "abc.def.ghi");
        assert_eq!(bootstrap.method, "reqwest(set-cookie bootstrap)");
    }

    #[test]
    fn rejects_translate_bootstrap_success_without_cookie() {
        let headers = fake_header_map(&[]);

        let error = resolve_translate_bootstrap(StatusCode::OK, &headers)
            .expect_err("missing cookie should fail");

        assert!(
            error
                .to_string()
                .contains("did not mint a translate_session cookie")
        );
    }

    #[test]
    fn maps_translate_bootstrap_auth_statuses() {
        let headers = fake_header_map(&[]);

        for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
            let error =
                resolve_translate_bootstrap(status, &headers).expect_err("auth status should fail");
            assert!(error.to_string().contains("inactive Kagi subscription"));
        }
    }

    #[test]
    fn maps_translate_subscription_error_bodies() {
        let message = translate_subscription_error_message(
            r#"{"error":"Kagi Translate requires an active subscription"}"#,
        )
        .expect("subscription body should map to a clear message");

        assert!(message.contains("requires an active Kagi subscription"));
        assert!(message.contains("free access is paused"));
    }

    #[test]
    fn retries_translate_bootstrap_when_cookie_is_missing() {
        let error = KagiError::Auth(TRANSLATE_BOOTSTRAP_MISSING_COOKIE_ERROR.to_string());
        assert!(should_retry_translate_bootstrap(&error));
    }

    #[test]
    fn does_not_retry_translate_bootstrap_for_invalid_session_auth() {
        let error =
            KagiError::Auth("invalid or expired Kagi session token for Kagi Translate".to_string());
        assert!(!should_retry_translate_bootstrap(&error));
    }

    #[test]
    fn retries_lens_mutation_lookup_when_created_lens_is_not_visible_yet() {
        let error = KagiError::Config("no lens matched '30147'".to_string());
        assert!(should_retry_lens_mutation_lookup(&error));
    }

    #[test]
    fn retries_lens_mutation_lookup_for_transient_settings_decode_errors() {
        let error = KagiError::Network(
            "failed to read lens settings page response body: error decoding response body"
                .to_string(),
        );
        assert!(should_retry_lens_mutation_lookup(&error));
    }

    #[test]
    fn does_not_retry_lens_mutation_lookup_for_auth_errors() {
        let error = KagiError::Auth("invalid or expired Kagi session token".to_string());
        assert!(!should_retry_lens_mutation_lookup(&error));
    }

    #[test]
    fn accepts_lens_names_at_kagi_persisted_length_limit() {
        let name = "a".repeat(super::KAGI_LENS_NAME_MAX_CHARS);
        assert_eq!(normalize_lens_name(&name).unwrap(), name);
    }

    #[test]
    fn rejects_lens_names_that_kagi_would_truncate() {
        let name = "a".repeat(super::KAGI_LENS_NAME_MAX_CHARS + 1);
        let error = normalize_lens_name(&name).expect_err("overlong lens name should fail");
        assert!(error.to_string().contains("Kagi truncates longer names"));
    }

    #[test]
    fn rejects_empty_lens_names() {
        let error = normalize_lens_name("   ").expect_err("empty lens name should fail");
        assert!(error.to_string().contains("lens name cannot be empty"));
    }

    #[test]
    fn uses_detected_source_language_when_translate_from_is_auto() {
        let source = effective_translate_source_language("auto", &sample_detected_language());
        assert_eq!(source, "fr");
    }

    #[test]
    fn preserves_explicit_translate_source_language() {
        let source = effective_translate_source_language("es", &sample_detected_language());
        assert_eq!(source, "es");
    }

    #[test]
    fn falls_back_to_requested_source_when_detected_iso_is_empty() {
        let detected_language = TranslateDetectedLanguage {
            iso: String::new(),
            label: "Unknown".to_string(),
            is_uncertain: true,
            is_mixed: false,
            alternatives: vec![],
        };

        let source = effective_translate_source_language("auto", &detected_language);

        assert_eq!(source, "auto");
    }

    #[test]
    fn backfills_translate_language_metadata() {
        let translation = TranslateTextResponse {
            translation: "Hello everyone".to_string(),
            source_language: None,
            target_language: None,
            detected_language: None,
            definition: None,
        };

        let finalized =
            finalize_translate_text_response(translation, &sample_detected_language(), "fr", "en");

        assert_eq!(finalized.source_language.as_deref(), Some("fr"));
        assert_eq!(finalized.target_language.as_deref(), Some("en"));
        assert_eq!(
            finalized
                .detected_language
                .as_ref()
                .map(|value| value.iso.as_str()),
            Some("fr")
        );
    }

    #[test]
    fn keeps_existing_translate_detected_language_when_present() {
        let translation = TranslateTextResponse {
            translation: "Hello everyone".to_string(),
            source_language: None,
            target_language: None,
            detected_language: Some(TranslateDetectedLanguage {
                iso: "es".to_string(),
                label: "Spanish".to_string(),
                is_uncertain: false,
                is_mixed: false,
                alternatives: vec![],
            }),
            definition: None,
        };

        let finalized =
            finalize_translate_text_response(translation, &sample_detected_language(), "fr", "en");

        assert_eq!(
            finalized
                .detected_language
                .as_ref()
                .map(|value| value.iso.as_str()),
            Some("es")
        );
    }

    #[test]
    fn omits_empty_translate_option_state() {
        assert!(build_translate_option_state(&sample_translate_request()).is_none());
    }

    #[test]
    fn builds_translate_payload_with_optional_fields() {
        let request = TranslateCommandRequest {
            text: "Bonjour".to_string(),
            from: "auto".to_string(),
            to: "en".to_string(),
            quality: Some("best".to_string()),
            model: Some("kagi".to_string()),
            prediction: Some("Hello".to_string()),
            predicted_language: Some("fr".to_string()),
            formality: Some("formal".to_string()),
            speaker_gender: Some("female".to_string()),
            addressee_gender: Some("male".to_string()),
            language_complexity: Some("simple".to_string()),
            translation_style: Some("natural".to_string()),
            context: Some("Office email".to_string()),
            dictionary_language: Some("en".to_string()),
            time_format: Some("24h".to_string()),
            use_definition_context: Some(true),
            enable_language_features: Some(true),
            preserve_formatting: Some(true),
            context_memory: Some(vec![json!({"kind": "glossary"})]),
            fetch_alternatives: true,
            fetch_word_insights: true,
            fetch_suggestions: true,
            fetch_alignments: true,
        };

        let payload = build_translate_payload(&request, "translate-session", "fr");
        let object = payload.as_object().expect("payload should be object");

        assert_eq!(object.get("from"), Some(&Value::String("fr".to_string())));
        assert_eq!(
            object.get("translation_style"),
            Some(&Value::String("natural".to_string()))
        );
        assert_eq!(
            object.get("context_memory"),
            Some(&Value::Array(vec![json!({"kind": "glossary"})]))
        );
        assert_eq!(
            object.get("session_token"),
            Some(&Value::String("translate-session".to_string()))
        );
    }

    #[test]
    fn localizes_translate_suggestions_payload_to_target_language() {
        let payload = build_translate_suggestions_payload(
            TranslateSuggestionContext {
                source_text: "Bonjour tout le monde",
                target_text: "みなさん、こんにちは",
                source_language: "fr",
                target_language: "ja",
                translation_options: None,
            },
            "translate-session",
        )
        .expect("payload should build");

        assert_eq!(
            payload.get("language"),
            Some(&Value::String("ja".to_string()))
        );
    }

    #[test]
    fn localizes_translate_word_insights_payload_to_target_language() {
        let payload = build_translate_word_insights_payload(
            "Bonjour tout le monde",
            "みなさん、こんにちは",
            "ja",
            "translate-session",
            None,
        )
        .expect("payload should build");

        assert_eq!(
            payload.get("target_explanation_language"),
            Some(&Value::String("ja".to_string()))
        );
    }

    #[test]
    fn normalizes_aux_quality_values() {
        assert_eq!(normalize_aux_quality(None), None);
        assert_eq!(normalize_aux_quality(Some("best")).as_deref(), Some("best"));
        assert_eq!(
            normalize_aux_quality(Some("deep_contextual")).as_deref(),
            Some("best")
        );
        assert_eq!(
            normalize_aux_quality(Some("standard")).as_deref(),
            Some("standard")
        );
    }

    #[tokio::test]
    async fn skips_disabled_translate_optional_sections_without_polling() {
        let polled = Arc::new(AtomicBool::new(false));
        let future_polled = Arc::clone(&polled);

        let (value, warning) =
            capture_optional_translate_section("word_insights", false, async move {
                future_polled.store(true, Ordering::SeqCst);
                Ok::<_, crate::error::KagiError>("value")
            })
            .await;

        assert!(value.is_none());
        assert!(warning.is_none());
        assert!(!polled.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn captures_translate_optional_section_failures_as_warnings() {
        let (value, warning) = capture_optional_translate_section("word_insights", true, async {
            Err::<Value, _>(crate::error::KagiError::Network(
                "temporary upstream failure".to_string(),
            ))
        })
        .await;

        assert!(value.is_none());
        let warning = warning.expect("warning should be returned");
        assert_eq!(warning.section, "word_insights");
        assert!(warning.message.contains("temporary upstream failure"));
    }

    #[tokio::test]
    #[ignore = "requires live KAGI_SESSION_TOKEN"]
    async fn live_translate_populates_language_metadata_and_sections() {
        let token = live_translate_session_token().expect("set KAGI_SESSION_TOKEN for live tests");
        let request = TranslateCommandRequest {
            text: "Bonjour tout le monde".to_string(),
            ..sample_translate_request()
        };

        let response = super::execute_translate(&request, &token)
            .await
            .expect("live translate should succeed");

        assert_eq!(response.detected_language.iso, "fr");
        assert_eq!(response.translation.source_language.as_deref(), Some("fr"));
        assert_eq!(response.translation.target_language.as_deref(), Some("en"));
        assert!(!response.translation.translation.trim().is_empty());

        for (section, present) in [
            ("alternatives", response.alternatives.is_some()),
            ("text_alignments", response.text_alignments.is_some()),
            (
                "translation_suggestions",
                response.translation_suggestions.is_some(),
            ),
            ("word_insights", response.word_insights.is_some()),
        ] {
            let warned = response
                .warnings
                .iter()
                .any(|warning| warning.section == section);
            assert!(
                present || warned,
                "expected {section} to be present or downgraded to a warning"
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires live KAGI_SESSION_TOKEN"]
    async fn live_translate_core_only_skips_auxiliary_sections() {
        let token = live_translate_session_token().expect("set KAGI_SESSION_TOKEN for live tests");
        let request = TranslateCommandRequest {
            text: "Bonjour tout le monde".to_string(),
            to: "ja".to_string(),
            fetch_alternatives: false,
            fetch_word_insights: false,
            fetch_suggestions: false,
            fetch_alignments: false,
            ..sample_translate_request()
        };

        let response = super::execute_translate(&request, &token)
            .await
            .expect("live translate should succeed");

        assert_eq!(response.translation.source_language.as_deref(), Some("fr"));
        assert_eq!(response.translation.target_language.as_deref(), Some("ja"));
        assert!(response.alternatives.is_none());
        assert!(response.text_alignments.is_none());
        assert!(response.translation_suggestions.is_none());
        assert!(response.word_insights.is_none());
        assert!(response.warnings.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires live KAGI_SESSION_TOKEN"]
    async fn live_translate_localizes_auxiliary_metadata_for_non_english_targets() {
        let token = live_translate_session_token().expect("set KAGI_SESSION_TOKEN for live tests");
        let request = TranslateCommandRequest {
            text: "Bonjour tout le monde".to_string(),
            to: "ja".to_string(),
            ..sample_translate_request()
        };

        let response = super::execute_translate(&request, &token)
            .await
            .expect("live translate should succeed");

        if let Some(suggestions) = response.translation_suggestions.as_ref() {
            assert!(
                suggestions
                    .suggestions
                    .iter()
                    .any(|entry| !entry.label.is_ascii()),
                "expected at least one localized suggestion label"
            );
        } else {
            assert!(
                response
                    .warnings
                    .iter()
                    .any(|warning| warning.section == "translation_suggestions")
            );
        }

        if let Some(insights) = response.word_insights.as_ref() {
            assert!(
                insights
                    .insights
                    .iter()
                    .any(|entry| !entry.r#type.is_ascii()),
                "expected at least one localized insight type"
            );
        } else {
            assert!(
                response
                    .warnings
                    .iter()
                    .any(|warning| warning.section == "word_insights")
            );
        }
    }
}
