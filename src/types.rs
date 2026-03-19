use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResult {
    pub t: u8,
    #[serde(default)]
    pub rank: Option<u32>,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub snippet: String,
    #[serde(default)]
    pub published: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResponse {
    pub data: Vec<SearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiMeta {
    pub id: String,
    pub node: String,
    pub ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SummarizeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Summarization {
    pub output: String,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SummarizeResponse {
    pub meta: ApiMeta,
    pub data: Summarization,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriberSummarizeMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriberSummarizeRequest {
    pub url: Option<String>,
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriberSummarization {
    pub id: String,
    pub thread_id: String,
    pub created_at: String,
    pub state: String,
    pub prompt: String,
    pub output: String,
    pub markdown: String,
    pub metadata_html: String,
    #[serde(default)]
    pub documents: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriberSummarizeResponse {
    pub meta: SubscriberSummarizeMeta,
    pub data: SubscriberSummarization,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsLatestBatch {
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "dateSlug")]
    pub date_slug: String,
    pub id: String,
    #[serde(rename = "languageCode")]
    pub language_code: String,
    #[serde(rename = "processingTime")]
    pub processing_time: u64,
    #[serde(rename = "totalArticles")]
    pub total_articles: u64,
    #[serde(rename = "totalCategories")]
    pub total_categories: u64,
    #[serde(rename = "totalClusters")]
    pub total_clusters: u64,
    #[serde(rename = "totalReadCount")]
    pub total_read_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsCategoryMetadata {
    #[serde(rename = "categoryId")]
    pub category_id: String,
    #[serde(rename = "categoryType")]
    pub category_type: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "isCore")]
    pub is_core: bool,
    #[serde(rename = "sourceLanguage")]
    pub source_language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsCategoryMetadataList {
    pub categories: Vec<NewsCategoryMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsBatchCategory {
    pub id: String,
    #[serde(rename = "categoryId")]
    pub category_id: String,
    #[serde(rename = "categoryName")]
    pub category_name: String,
    #[serde(rename = "sourceLanguage")]
    pub source_language: String,
    pub timestamp: u64,
    #[serde(rename = "readCount")]
    pub read_count: u64,
    #[serde(rename = "clusterCount")]
    pub cluster_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsBatchCategories {
    #[serde(rename = "batchId")]
    pub batch_id: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "hasOnThisDay")]
    pub has_on_this_day: bool,
    pub categories: Vec<NewsBatchCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsResolvedCategory {
    pub id: String,
    pub category_id: String,
    pub category_name: String,
    pub source_language: String,
    pub timestamp: u64,
    pub read_count: u64,
    pub cluster_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<NewsCategoryMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsCategoriesResponse {
    pub latest_batch: NewsLatestBatch,
    pub categories: Vec<NewsResolvedCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewsStoriesPayload {
    #[serde(rename = "batchId")]
    pub batch_id: String,
    #[serde(rename = "categoryId")]
    pub category_id: String,
    #[serde(rename = "categoryName")]
    pub category_name: String,
    pub timestamp: u64,
    pub stories: Vec<Value>,
    #[serde(rename = "totalStories")]
    pub total_stories: String,
    #[serde(default)]
    pub domains: Vec<Value>,
    #[serde(rename = "readCount")]
    pub read_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewsStoriesResponse {
    pub latest_batch: NewsLatestBatch,
    pub category: NewsResolvedCategory,
    pub stories: Vec<Value>,
    pub total_stories: String,
    #[serde(default)]
    pub domains: Vec<Value>,
    pub read_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsChaos {
    #[serde(rename = "chaosIndex")]
    pub chaos_index: u64,
    #[serde(rename = "chaosDescription")]
    pub chaos_description: String,
    #[serde(rename = "chaosLastUpdated")]
    pub chaos_last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsChaosResponse {
    pub latest_batch: NewsLatestBatch,
    pub chaos: NewsChaos,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantPromptRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lens_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internet_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personalizations: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AskPageRequest {
    pub url: String,
    pub question: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantThread {
    pub id: String,
    pub title: String,
    pub ack: String,
    pub created_at: String,
    pub expires_at: String,
    pub saved: bool,
    pub shared: bool,
    pub branch_id: String,
    #[serde(default)]
    pub tag_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantMessage {
    pub id: String,
    pub thread_id: String,
    pub created_at: String,
    #[serde(default)]
    pub branch_list: Vec<String>,
    pub state: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references_html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references_markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_html: Option<String>,
    #[serde(default)]
    pub documents: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantPromptResponse {
    pub meta: AssistantMeta,
    pub thread: AssistantThread,
    pub message: AssistantMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AskPageSource {
    pub url: String,
    pub question: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AskPageResponse {
    pub meta: AssistantMeta,
    pub source: AskPageSource,
    pub thread: AssistantThread,
    pub message: AssistantMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantThreadSummary {
    pub id: String,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub saved: bool,
    pub shared: bool,
    #[serde(default)]
    pub tag_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantThreadPagination {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub count: u64,
    #[serde(default)]
    pub total_counts: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantThreadListResponse {
    pub meta: AssistantMeta,
    #[serde(default)]
    pub tags: Vec<Value>,
    pub threads: Vec<AssistantThreadSummary>,
    pub pagination: AssistantThreadPagination,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantThreadOpenResponse {
    pub meta: AssistantMeta,
    #[serde(default)]
    pub tags: Vec<Value>,
    pub thread: AssistantThread,
    #[serde(default)]
    pub messages: Vec<AssistantMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantThreadDeleteResponse {
    pub deleted_thread_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantThreadExportResponse {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FastGptRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reference {
    pub title: String,
    pub snippet: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FastGptAnswer {
    pub output: String,
    pub tokens: u64,
    #[serde(default)]
    pub references: Vec<Reference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FastGptResponse {
    pub meta: ApiMeta,
    pub data: FastGptAnswer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnrichResponse {
    pub meta: ApiMeta,
    pub data: Vec<SearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SmallWebFeed {
    pub xml: String,
}
