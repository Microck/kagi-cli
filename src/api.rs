use std::collections::HashMap;
use std::future::Future;
use std::process::Stdio;

use reqwest::{Client, StatusCode, header};
use serde::Deserialize;
#[cfg(test)]
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use serde_json::{Map, Value};
use tokio::process::Command;

use crate::error::KagiError;
#[cfg(test)]
use crate::types::ApiMeta;
use crate::types::{
    AlternativeTranslationsResponse, AssistantMessage, AssistantMeta, AssistantPromptRequest,
    AssistantPromptResponse, AssistantThread, EnrichResponse, FastGptRequest, FastGptResponse,
    NewsBatchCategories, NewsBatchCategory, NewsCategoriesResponse, NewsCategoryMetadata,
    NewsCategoryMetadataList, NewsChaos, NewsChaosResponse, NewsLatestBatch, NewsResolvedCategory,
    NewsStoriesPayload, NewsStoriesResponse, SmallWebFeed, SubscriberSummarization,
    SubscriberSummarizeMeta, SubscriberSummarizeRequest, SubscriberSummarizeResponse,
    SummarizeRequest, SummarizeResponse, TextAlignmentsResponse, TranslateBootstrapMetadata,
    TranslateCommandRequest, TranslateDetectedLanguage, TranslateOptionState, TranslateResponse,
    TranslateTextResponse, TranslateWarning, TranslationSuggestionsResponse, WordInsightsResponse,
};

const USER_AGENT: &str = "kagi-cli/0.1.0 (+https://github.com/)";
const KAGI_SUMMARIZE_URL: &str = "https://kagi.com/api/v0/summarize";
const KAGI_SUBSCRIBER_SUMMARIZE_URL: &str = "https://kagi.com/mother/summary_labs";
const KAGI_NEWS_LATEST_URL: &str = "https://news.kagi.com/api/batches/latest";
const KAGI_NEWS_CATEGORIES_METADATA_URL: &str = "https://news.kagi.com/api/categories/metadata";
const KAGI_NEWS_BATCH_CATEGORIES_URL: &str = "https://news.kagi.com/api/batches";
const KAGI_ASSISTANT_PROMPT_URL: &str = "https://kagi.com/assistant/prompt";
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
    if token.trim().is_empty() {
        return Err(KagiError::Auth(
            "missing Kagi session token (expected KAGI_SESSION_TOKEN)".to_string(),
        ));
    }

    let query = request.query.trim();
    if query.is_empty() {
        return Err(KagiError::Config(
            "assistant query cannot be empty".to_string(),
        ));
    }

    let thread_id = request
        .thread_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if request.thread_id.is_some() && thread_id.is_none() {
        return Err(KagiError::Config(
            "assistant --thread-id cannot be empty".to_string(),
        ));
    }

    let client = build_client()?;
    let response = client
        .post(KAGI_ASSISTANT_PROMPT_URL)
        .header(header::COOKIE, format!("kagi_session={token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/vnd.kagi.stream")
        .json(&json!({
            "focus": {
                "thread_id": thread_id,
                "branch_id": ASSISTANT_ZERO_BRANCH_UUID,
                "prompt": query,
                "message_id": Value::Null,
            }
        }))
        .send()
        .await
        .map_err(map_transport_error)?;

    match response.status() {
        StatusCode::OK => {
            let body = response.text().await.map_err(|error| {
                KagiError::Network(format!("failed to read assistant response body: {error}"))
            })?;
            if looks_like_html_document(&body) {
                return Err(KagiError::Auth(
                    "invalid or expired Kagi session token".to_string(),
                ));
            }
            parse_assistant_prompt_stream(&body)
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(KagiError::Auth(
            "invalid or expired Kagi session token".to_string(),
        )),
        status if status.is_client_error() => {
            let body = response.text().await.unwrap_or_else(|_| String::new());
            Err(KagiError::Config(format!(
                "Kagi Assistant request rejected: HTTP {status}{}",
                format_client_error_suffix(&body)
            )))
        }
        status if status.is_server_error() => Err(KagiError::Network(format!(
            "Kagi Assistant server error: HTTP {status}"
        ))),
        status => Err(KagiError::Network(format!(
            "unexpected Kagi Assistant response status: HTTP {status}"
        ))),
    }
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
    let translation_options = build_translate_option_state(request);
    let translated_text = translation.translation.clone();
    let target_language = request.to.clone();
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
                request.text.trim(),
                &translated_text,
                &effective_source_language,
                &target_language,
                translation_options.as_ref(),
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
    const BOOTSTRAP_SCRIPT: &str = r#"import json
import os
import sys

def finish(ok, **kwargs):
    payload = {"ok": ok}
    payload.update(kwargs)
    print(json.dumps(payload))
    sys.exit(0 if ok else 1)

token = os.environ.get("KAGI_SESSION_TOKEN_RAW", "").strip()
if not token:
    finish(False, kind="config", message="missing normalized KAGI session token")

try:
    from curl_cffi import requests
except Exception as exc:
    finish(
        False,
        kind="config",
        message="kagi translate requires python3 with the curl_cffi package installed",
        detail=str(exc),
    )

try:
    session = requests.Session(impersonate="chrome136")
    session.cookies.set("kagi_session", token, domain=".kagi.com", path="/", secure=True)
    response = session.get("https://translate.kagi.com/", timeout=30, allow_redirects=True)
except Exception as exc:
    finish(False, kind="network", message="translate bootstrap request failed", detail=str(exc))

if response.status_code >= 400:
    finish(
        False,
        kind="network",
        message=f"translate bootstrap returned HTTP {response.status_code}",
    )

translate_session = ""
for cookie in session.cookies.jar:
    if cookie.name == "translate_session" and cookie.value:
        translate_session = cookie.value
        break

if not translate_session:
    finish(
        False,
        kind="auth",
        message="translate bootstrap did not mint a translate_session cookie",
    )

finish(
    True,
    translate_session=translate_session,
    method="python3+curl_cffi(chrome136)",
)
"#;

    let output = Command::new("python3")
        .arg("-c")
        .arg(BOOTSTRAP_SCRIPT)
        .env("KAGI_SESSION_TOKEN_RAW", session_token)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => KagiError::Config(
                "kagi translate requires `python3` with the `curl_cffi` package installed"
                    .to_string(),
            ),
            _ => KagiError::Config(format!(
                "failed to start translate bootstrap helper: {error}"
            )),
        })?;

    let stdout = String::from_utf8(output.stdout).map_err(|error| {
        KagiError::Parse(format!(
            "translate bootstrap helper returned non-UTF-8 output: {error}"
        ))
    })?;
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let trimmed_stdout = stdout.trim();

    if trimmed_stdout.is_empty() {
        return Err(KagiError::Config(if stderr.is_empty() {
            "translate bootstrap helper returned no output".to_string()
        } else {
            format!("translate bootstrap helper failed: {stderr}")
        }));
    }

    let parsed: TranslateBootstrapScriptOutput =
        serde_json::from_str(trimmed_stdout).map_err(|error| {
            KagiError::Parse(format!(
                "failed to parse translate bootstrap helper output: {error}"
            ))
        })?;

    if parsed.ok {
        let translate_session = parsed.translate_session.ok_or_else(|| {
            KagiError::Parse(
                "translate bootstrap helper did not return translate_session".to_string(),
            )
        })?;
        let method = parsed
            .method
            .unwrap_or_else(|| "python3+curl_cffi".to_string());

        return Ok(TranslateBootstrapResult {
            translate_session,
            method,
        });
    }

    let mut message = parsed
        .message
        .unwrap_or_else(|| "translate bootstrap failed".to_string());
    if !stderr.is_empty() {
        message.push_str(&format!("; stderr: {stderr}"));
    }

    Err(map_bootstrap_kind_to_error(
        parsed.kind.as_deref().unwrap_or("config"),
        &message,
    ))
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

async fn execute_translate_suggestions(
    client: &Client,
    cookie_header: &str,
    translate_session: &str,
    source_text: &str,
    target_text: &str,
    source_language: &str,
    target_language: &str,
    translation_options: Option<&TranslateOptionState>,
) -> Result<TranslationSuggestionsResponse, KagiError> {
    let mut payload = Map::new();
    payload.insert(
        "originalText".to_string(),
        Value::String(source_text.to_string()),
    );
    payload.insert(
        "translatedText".to_string(),
        Value::String(target_text.to_string()),
    );
    payload.insert(
        "sourceLanguage".to_string(),
        Value::String(source_language.to_string()),
    );
    payload.insert(
        "targetLanguage".to_string(),
        Value::String(target_language.to_string()),
    );
    payload.insert("language".to_string(), Value::String("en".to_string()));
    payload.insert(
        "session_token".to_string(),
        Value::String(translate_session.to_string()),
    );

    if let Some(options) = translation_options {
        payload.insert(
            "translationOptions".to_string(),
            serde_json::to_value(options).map_err(|error| {
                KagiError::Parse(format!(
                    "failed to serialize translate suggestion options: {error}"
                ))
            })?,
        );
    }

    let response = client
        .post(KAGI_TRANSLATE_SUGGESTIONS_URL)
        .header(header::COOKIE, cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&Value::Object(payload))
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
    translation_options: Option<&TranslateOptionState>,
) -> Result<WordInsightsResponse, KagiError> {
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
        Value::String("en".to_string()),
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

    let response = client
        .post(KAGI_TRANSLATE_WORD_INSIGHTS_URL)
        .header(header::COOKIE, cookie_header)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&Value::Object(payload))
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
                thread = Some(AssistantThread {
                    id: payload.id,
                    title: payload.title,
                    ack: payload.ack,
                    created_at: payload.created_at,
                    expires_at: payload.expires_at,
                    saved: payload.saved,
                    shared: payload.shared,
                    branch_id: payload.branch_id,
                    tag_ids: payload.tag_ids,
                });
            }
            "new_message.json" => {
                let payload: AssistantMessagePayload =
                    serde_json::from_str(payload).map_err(|error| {
                        KagiError::Parse(format!(
                            "failed to parse assistant message frame: {error}"
                        ))
                    })?;
                message = Some(AssistantMessage {
                    id: payload.id,
                    thread_id: payload.thread_id,
                    created_at: payload.created_at,
                    state: payload.state,
                    prompt: payload.prompt,
                    reply_html: payload.reply,
                    markdown: payload.md,
                    metadata_html: payload.metadata,
                    documents: payload.documents,
                    profile: payload.profile,
                });
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

fn map_bootstrap_kind_to_error(kind: &str, message: &str) -> KagiError {
    match kind {
        "auth" => KagiError::Auth(message.to_string()),
        "network" => KagiError::Network(message.to_string()),
        "parse" => KagiError::Parse(message.to_string()),
        _ => KagiError::Config(message.to_string()),
    }
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

#[derive(Debug, Deserialize)]
struct AssistantMessagePayload {
    id: String,
    thread_id: String,
    created_at: String,
    state: String,
    prompt: String,
    #[serde(default)]
    reply: Option<String>,
    #[serde(default)]
    md: Option<String>,
    #[serde(default)]
    metadata: Option<String>,
    #[serde(default)]
    documents: Vec<Value>,
    #[serde(default)]
    profile: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct TranslateBootstrapScriptOutput {
    ok: bool,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    translate_session: Option<String>,
    #[serde(default)]
    method: Option<String>,
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
        ApiErrorBody, KagiEnvelope, build_translate_payload, normalize_aux_quality,
        normalize_subscriber_summary_input, normalize_subscriber_summary_length,
        normalize_subscriber_summary_type, parse_assistant_prompt_stream,
        parse_subscriber_summarize_stream, parse_translate_detect_value, resolve_news_category,
    };
    use crate::types::SubscriberSummarizeRequest;
    use crate::types::{
        FastGptAnswer, NewsBatchCategory, NewsCategoryMetadata, Reference, Summarization,
        TranslateCommandRequest,
    };
    use serde_json::{Value, json};

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
            "new_message.json:{\"id\":\"msg-1\",\"thread_id\":\"thread-1\",\"created_at\":\"2026-03-16T06:19:07Z\",\"state\":\"done\",\"prompt\":\"Hello\",\"reply\":\"<p>Hi</p>\",\"md\":\"Hi\",\"metadata\":\"<li>meta</li>\",\"documents\":[]}\0\n"
        );

        let parsed = parse_assistant_prompt_stream(raw).expect("assistant stream parses");
        assert_eq!(parsed.meta.trace.as_deref(), Some("trace-123"));
        assert_eq!(parsed.thread.id, "thread-1");
        assert_eq!(parsed.message.markdown.as_deref(), Some("Hi"));
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
    fn normalizes_aux_quality_values() {
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
}
