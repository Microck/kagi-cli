use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use reqwest::{Client, StatusCode, Url, header};
use scraper::Html;
use serde::Deserialize;
#[cfg(test)]
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use serde_json::{Map, Value};
use tokio::time::sleep;

use crate::error::KagiError;
use crate::http;
use crate::parser::parse_assistant_thread_list;
#[cfg(test)]
use crate::types::ApiMeta;
use crate::types::{
    AlternativeTranslationsResponse, AskPageRequest, AskPageResponse, AskPageSource,
    AssistantMessage, AssistantMeta, AssistantPromptRequest, AssistantPromptResponse,
    AssistantThread, AssistantThreadDeleteResponse, AssistantThreadExportResponse,
    AssistantThreadListResponse, AssistantThreadOpenResponse, AssistantThreadPagination,
    EnrichResponse, FastGptRequest, FastGptResponse, NewsBatchCategories, NewsBatchCategory,
    NewsCategoriesResponse, NewsCategoryMetadata, NewsCategoryMetadataList, NewsChaos,
    NewsChaosResponse, NewsLatestBatch, NewsResolvedCategory, NewsStoriesPayload,
    NewsStoriesResponse, SmallWebFeed, SubscriberSummarization, SubscriberSummarizeMeta,
    SubscriberSummarizeRequest, SubscriberSummarizeResponse, SummarizeRequest, SummarizeResponse,
    TextAlignmentsResponse, TranslateBootstrapMetadata, TranslateCommandRequest,
    TranslateDetectedLanguage, TranslateOptionState, TranslateResponse, TranslateTextResponse,
    TranslateWarning, TranslationSuggestionsResponse, WordInsightsResponse,
};

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
const KAGI_TRANSLATE_DETECT_URL: &str = "https://translate.kagi.com/api/detect";
const KAGI_TRANSLATE_URL: &str = "https://translate.kagi.com/api/translate";
const KAGI_TRANSLATE_ALTERNATIVES_URL: &str =
    "https://translate.kagi.com/api/alternative-translations";
const KAGI_TRANSLATE_ALIGNMENTS_URL: &str = "https://translate.kagi.com/api/text-alignments";
const KAGI_TRANSLATE_SUGGESTIONS_URL: &str =
    "https://translate.kagi.com/api/translation-suggestions";
const KAGI_TRANSLATE_WORD_INSIGHTS_URL: &str = "https://translate.kagi.com/api/word-insights";
const ASSISTANT_ZERO_BRANCH_UUID: &str = "00000000-0000-4000-0000-000000000000";
const TRANSLATE_BOOTSTRAP_MAX_ATTEMPTS: usize = 3;
const TRANSLATE_BOOTSTRAP_MISSING_COOKIE_ERROR: &str =
    "translate bootstrap did not mint a translate_session cookie";

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

async fn bootstrap_translate_session(
    session_token: &str,
) -> Result<TranslateBootstrapResult, KagiError> {
    let client = build_client()?;
    let mut last_error = None;

    for attempt in 0..TRANSLATE_BOOTSTRAP_MAX_ATTEMPTS {
        let response = client
            .get("https://translate.kagi.com/")
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
        .post(KAGI_TRANSLATE_DETECT_URL)
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
        .post(KAGI_TRANSLATE_URL)
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
        .post(KAGI_TRANSLATE_ALTERNATIVES_URL)
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
        .post(KAGI_TRANSLATE_ALIGNMENTS_URL)
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
        .post(KAGI_TRANSLATE_SUGGESTIONS_URL)
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
        .post(KAGI_TRANSLATE_WORD_INSIGHTS_URL)
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
            "invalid or expired Kagi session token for Kagi Translate".to_string(),
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

#[derive(Debug)]
struct TranslateBootstrapResult {
    translate_session: String,
    method: String,
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
                KagiError::Parse(format!(
                    "failed to parse Kagi Translate {surface} response: {error}"
                ))
            })
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(KagiError::Auth(
            "invalid or expired Kagi session token for Kagi Translate".to_string(),
        )),
        status if status.is_client_error() => {
            let body = response.text().await.unwrap_or_else(|_| String::new());
            Err(KagiError::Config(format!(
                "Kagi Translate {surface} request rejected: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi Translate {surface} server error: HTTP {status}"
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi Translate {surface} response status: HTTP {status}"
        ))),
    }
}

fn build_client() -> Result<Client, KagiError> {
    http::client_30s()
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
        ApiErrorBody, KagiEnvelope, TRANSLATE_BOOTSTRAP_MISSING_COOKIE_ERROR,
        TranslateSuggestionContext, build_ask_page_prompt, build_translate_option_state,
        build_translate_payload, build_translate_suggestions_payload,
        build_translate_word_insights_payload, capture_optional_translate_section,
        effective_translate_source_language, extract_set_cookie_value, fake_header_map,
        finalize_translate_text_response, normalize_ask_page_question, normalize_ask_page_url,
        normalize_assistant_query, normalize_assistant_thread_id, normalize_aux_quality,
        normalize_subscriber_summary_input, normalize_subscriber_summary_length,
        normalize_subscriber_summary_type, parse_assistant_prompt_stream,
        parse_assistant_thread_delete_stream, parse_assistant_thread_list_stream,
        parse_assistant_thread_open_stream, parse_content_disposition_filename,
        parse_subscriber_summarize_stream, parse_translate_detect_value, resolve_news_category,
        resolve_translate_bootstrap, should_retry_translate_bootstrap, validate_translate_request,
    };
    use crate::api::{
        execute_assistant_prompt, execute_assistant_thread_delete, execute_assistant_thread_export,
        execute_assistant_thread_get, execute_assistant_thread_list,
    };
    use crate::auth::{SESSION_TOKEN_ENV, load_credential_inventory, normalize_session_token};
    use crate::error::KagiError;
    use crate::types::{AskPageRequest, SubscriberSummarizeRequest};
    use crate::types::{
        AssistantPromptRequest, FastGptAnswer, NewsBatchCategory, NewsCategoryMetadata, Reference,
        Summarization, TranslateCommandRequest, TranslateDetectedLanguage, TranslateTextResponse,
    };
    use reqwest::StatusCode;
    use serde_json::{Value, json};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{SystemTime, UNIX_EPOCH};

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
            assert!(
                error
                    .to_string()
                    .contains("invalid or expired Kagi session token")
            );
        }
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

        let suggestions = response
            .translation_suggestions
            .as_ref()
            .expect("suggestions should be present for ja target");
        let insights = response
            .word_insights
            .as_ref()
            .expect("word insights should be present for ja target");

        assert!(
            suggestions
                .suggestions
                .iter()
                .any(|entry| !entry.label.is_ascii()),
            "expected at least one localized suggestion label"
        );
        assert!(
            insights
                .insights
                .iter()
                .any(|entry| !entry.r#type.is_ascii()),
            "expected at least one localized insight type"
        );
    }
}
