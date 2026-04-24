//! API request and response types for Kagi services.
//!
//! This module defines the data structures used across all Kagi API endpoints:
//! search, summarization, news, assistant, lenses, and translation.
//!
//! Types are grouped by feature:
//! - **Search**: [`SearchResult`], [`SearchResponse`]
//! - **Summarization**: [`SummarizeRequest`], [`SummarizeResponse`], [`SubscriberSummarizeRequest`], [`SubscriberSummarizeResponse`]
//! - **News**: [`NewsLatestBatch`], [`NewsCategoriesResponse`], [`NewsStoriesResponse`], [`NewsChaosResponse`]
//! - **Assistant**: [`AssistantPromptRequest`], [`AssistantPromptResponse`], [`AssistantThread`], [`AssistantMessage`]
//! - **Lenses**: [`LensSummary`], [`LensDetails`]
//! - **Translation**: [`TranslateRequest`], [`TranslateResponse`]

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A single search result from the Kagi search API.
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
/// Wrapper for a list of search results.
pub struct SearchResponse {
    pub data: Vec<SearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Metadata returned with most Kagi API responses (request ID, node, latency).
pub struct ApiMeta {
    pub id: String,
    pub node: String,
    pub ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for the public API summarization endpoint.
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
/// The summarization output from a public API request.
pub struct Summarization {
    pub output: String,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response from the public API summarization endpoint.
pub struct SummarizeResponse {
    pub meta: ApiMeta,
    pub data: Summarization,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Metadata for the subscriber-mode summarization endpoint.
pub struct SubscriberSummarizeMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for the subscriber-mode summarization endpoint.
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
/// The full summarization result from the subscriber endpoint.
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
/// Response from the subscriber-mode summarization endpoint.
pub struct SubscriberSummarizeResponse {
    pub meta: SubscriberSummarizeMeta,
    pub data: SubscriberSummarization,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Metadata about the latest processed news batch.
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
/// Metadata for a news category (ID, display name, language).
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
/// A list of news category metadata entries.
pub struct NewsCategoryMetadataList {
    pub categories: Vec<NewsCategoryMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A category within a news batch (with read/cluster counts).
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
/// All categories for a specific news batch.
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
/// A news category with resolved metadata.
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
/// Response containing the latest batch and its categories.
pub struct NewsCategoriesResponse {
    pub latest_batch: NewsLatestBatch,
    pub categories: Vec<NewsResolvedCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A single category payload within news stories.
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response containing news stories for a category.
pub struct NewsStoriesResponse {
    pub latest_batch: NewsLatestBatch,
    pub category: NewsResolvedCategory,
    pub stories: Vec<Value>,
    pub total_stories: String,
    #[serde(default)]
    pub domains: Vec<Value>,
    pub read_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_filter: Option<NewsContentFilterSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// The Kagi News "chaos index" value and description.
pub struct NewsChaos {
    #[serde(rename = "chaosIndex")]
    pub chaos_index: u64,
    #[serde(rename = "chaosDescription")]
    pub chaos_description: String,
    #[serde(rename = "chaosLastUpdated")]
    pub chaos_last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response containing the chaos index.
pub struct NewsChaosResponse {
    pub latest_batch: NewsLatestBatch,
    pub chaos: NewsChaos,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A single filter preset entry (id, label, keywords).
pub struct NewsFilterPresetListEntry {
    pub id: String,
    pub label: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response listing available news filter presets.
pub struct NewsFilterPresetListResponse {
    pub language: String,
    pub presets: Vec<NewsFilterPresetListEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Summary of active content filter settings for news.
pub struct NewsContentFilterSummary {
    pub mode: String,
    pub scope: String,
    pub active_presets: Vec<String>,
    pub custom_keywords: Vec<String>,
    pub active_keywords: Vec<String>,
    pub filtered_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Content filter summary for a single news story.
pub struct NewsStoryContentFilterSummary {
    pub mode: String,
    pub matched_keywords: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Metadata returned with assistant API responses.
pub struct AssistantMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for the assistant prompt endpoint.
pub struct AssistantPromptRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
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
/// Request body for the "ask about a page" endpoint.
pub struct AskPageRequest {
    pub url: String,
    pub question: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A conversation thread in the Kagi Assistant.
pub struct AssistantThread {
    pub id: String,
    pub title: String,
    pub ack: String,
    pub created_at: String,
    #[serde(default)]
    pub expires_at: String,
    pub saved: bool,
    pub shared: bool,
    pub branch_id: String,
    #[serde(default)]
    pub tag_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A single message within an assistant thread.
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
/// Response from the assistant prompt endpoint.
pub struct AssistantPromptResponse {
    pub meta: AssistantMeta,
    pub thread: AssistantThread,
    pub message: AssistantMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Source information for an "ask about a page" query.
pub struct AskPageSource {
    pub url: String,
    pub question: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response from the "ask about a page" endpoint.
pub struct AskPageResponse {
    pub meta: AssistantMeta,
    pub source: AskPageSource,
    pub thread: AssistantThread,
    pub message: AssistantMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A summary view of an assistant thread (for listing).
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
/// Pagination info for thread list responses.
pub struct AssistantThreadPagination {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub count: u64,
    #[serde(default)]
    pub total_counts: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response listing assistant threads.
pub struct AssistantThreadListResponse {
    pub meta: AssistantMeta,
    #[serde(default)]
    pub tags: Vec<Value>,
    pub threads: Vec<AssistantThreadSummary>,
    pub pagination: AssistantThreadPagination,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response when opening a specific assistant thread.
pub struct AssistantThreadOpenResponse {
    pub meta: AssistantMeta,
    #[serde(default)]
    pub tags: Vec<Value>,
    pub thread: AssistantThread,
    #[serde(default)]
    pub messages: Vec<AssistantMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response confirming thread deletion.
pub struct AssistantThreadDeleteResponse {
    pub deleted_thread_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response containing an exported thread as markdown.
pub struct AssistantThreadExportResponse {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Summary of an assistant profile.
pub struct AssistantProfileSummary {
    pub id: String,
    pub name: String,
    pub invoke_profile: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bang_trigger: Option<String>,
    pub internet_access: bool,
    pub built_in: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Detailed view of an assistant profile configuration.
pub struct AssistantProfileDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bang_trigger: Option<String>,
    pub internet_access: bool,
    pub selected_lens: String,
    pub personalizations: bool,
    pub base_model: String,
    pub custom_instructions: String,
    pub delete_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Request body for creating a new assistant profile.
pub struct AssistantProfileCreateRequest {
    pub name: String,
    pub bang_trigger: Option<String>,
    pub internet_access: Option<bool>,
    pub selected_lens: Option<String>,
    pub personalizations: Option<bool>,
    pub base_model: Option<String>,
    pub custom_instructions: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Request body for updating an existing assistant profile.
pub struct AssistantProfileUpdateRequest {
    pub target: String,
    pub name: Option<String>,
    pub bang_trigger: Option<String>,
    pub internet_access: Option<bool>,
    pub selected_lens: Option<String>,
    pub personalizations: Option<bool>,
    pub base_model: Option<String>,
    pub custom_instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Summary of a Kagi search lens.
pub struct LensSummary {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    pub edit_url: String,
    #[serde(skip)]
    pub toggle_field: String,
    #[serde(skip)]
    pub toggle_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Detailed configuration of a Kagi search lens.
pub struct LensDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub included_sites: String,
    pub included_keywords: String,
    pub description: String,
    pub search_region: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_time: Option<String>,
    pub excluded_sites: String,
    pub excluded_keywords: String,
    pub shortcut_keyword: String,
    pub autocomplete_keywords: bool,
    pub template: String,
    pub file_type: String,
    pub share_with_team: bool,
    pub share_copy_code: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Request body for creating a new Kagi lens.
pub struct LensCreateRequest {
    pub name: String,
    pub included_sites: Option<String>,
    pub included_keywords: Option<String>,
    pub description: Option<String>,
    pub search_region: Option<String>,
    pub before_time: Option<String>,
    pub after_time: Option<String>,
    pub excluded_sites: Option<String>,
    pub excluded_keywords: Option<String>,
    pub shortcut_keyword: Option<String>,
    pub autocomplete_keywords: Option<bool>,
    pub template: Option<String>,
    pub file_type: Option<String>,
    pub share_with_team: Option<bool>,
    pub share_copy_code: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Request body for updating an existing Kagi lens.
pub struct LensUpdateRequest {
    pub target: String,
    pub name: Option<String>,
    pub included_sites: Option<String>,
    pub included_keywords: Option<String>,
    pub description: Option<String>,
    pub search_region: Option<String>,
    pub before_time: Option<String>,
    pub after_time: Option<String>,
    pub excluded_sites: Option<String>,
    pub excluded_keywords: Option<String>,
    pub shortcut_keyword: Option<String>,
    pub autocomplete_keywords: Option<bool>,
    pub template: Option<String>,
    pub file_type: Option<String>,
    pub share_with_team: Option<bool>,
    pub share_copy_code: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Summary view of a custom bang shortcut.
pub struct CustomBangSummary {
    pub id: String,
    pub name: String,
    pub trigger: String,
    pub shortcut_menu: bool,
    pub edit_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Detailed view of a custom bang shortcut.
pub struct CustomBangDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bang_id: Option<String>,
    pub name: String,
    pub trigger: String,
    pub template: String,
    pub snap_domain: String,
    pub regex_pattern: String,
    pub shortcut_menu: bool,
    pub fmt_open_snap_domain: bool,
    pub fmt_open_base_path: bool,
    pub fmt_url_encode_placeholder: bool,
    pub fmt_url_encode_space_to_plus: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Request body for creating a new custom bang.
pub struct CustomBangCreateRequest {
    pub name: String,
    pub trigger: String,
    pub template: Option<String>,
    pub snap_domain: Option<String>,
    pub regex_pattern: Option<String>,
    pub shortcut_menu: Option<bool>,
    pub fmt_open_snap_domain: Option<bool>,
    pub fmt_open_base_path: Option<bool>,
    pub fmt_url_encode_placeholder: Option<bool>,
    pub fmt_url_encode_space_to_plus: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Request body for updating an existing custom bang.
pub struct CustomBangUpdateRequest {
    pub target: String,
    pub name: Option<String>,
    pub trigger: Option<String>,
    pub template: Option<String>,
    pub snap_domain: Option<String>,
    pub regex_pattern: Option<String>,
    pub shortcut_menu: Option<bool>,
    pub fmt_open_snap_domain: Option<bool>,
    pub fmt_open_base_path: Option<bool>,
    pub fmt_url_encode_placeholder: Option<bool>,
    pub fmt_url_encode_space_to_plus: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Summary view of a URL redirect rule.
pub struct RedirectRuleSummary {
    pub id: String,
    pub rule: String,
    pub enabled: bool,
    pub edit_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Detailed view of a URL redirect rule.
pub struct RedirectRuleDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    pub rule: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Request body for creating a new redirect rule.
pub struct RedirectRuleCreateRequest {
    pub rule: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Request body for updating an existing redirect rule.
pub struct RedirectRuleUpdateRequest {
    pub target: String,
    pub rule: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response confirming resource deletion.
pub struct DeletedResourceResponse {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response confirming a resource state toggle.
pub struct ToggleResourceResponse {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for the FastGPT quick-answer endpoint.
pub struct FastGptRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A web reference cited in a FastGPT answer.
pub struct Reference {
    pub title: String,
    pub snippet: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A single FastGPT answer with references.
pub struct FastGptAnswer {
    pub output: String,
    pub tokens: u64,
    #[serde(default)]
    pub references: Vec<Reference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response from the FastGPT endpoint.
pub struct FastGptResponse {
    pub meta: ApiMeta,
    pub data: FastGptAnswer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response from the web/news enrichment endpoint.
pub struct EnrichResponse {
    pub meta: ApiMeta,
    pub data: Vec<SearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A single entry from the Small Web feed.
pub struct SmallWebFeed {
    pub xml: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Metadata for a quick-answer response.
pub struct QuickMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A message in a quick-answer conversation.
pub struct QuickMessage {
    pub id: String,
    pub thread_id: String,
    pub created_at: String,
    pub state: String,
    pub prompt: String,
    pub html: String,
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A single reference item in a quick answer.
pub struct QuickReferenceItem {
    pub index: usize,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contribution_pct: Option<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Collection of reference items for a quick answer.
pub struct QuickReferenceCollection {
    #[serde(default)]
    pub markdown: String,
    #[serde(default)]
    pub items: Vec<QuickReferenceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response from the quick-answer endpoint.
pub struct QuickResponse {
    pub meta: QuickMeta,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens: Option<String>,
    pub message: QuickMessage,
    pub references: QuickReferenceCollection,
    #[serde(default)]
    pub followup_questions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Request body for the translate endpoint.
pub struct TranslateCommandRequest {
    pub text: String,
    pub from: String,
    pub to: String,
    pub quality: Option<String>,
    pub model: Option<String>,
    pub prediction: Option<String>,
    pub predicted_language: Option<String>,
    pub formality: Option<String>,
    pub speaker_gender: Option<String>,
    pub addressee_gender: Option<String>,
    pub language_complexity: Option<String>,
    pub translation_style: Option<String>,
    pub context: Option<String>,
    pub dictionary_language: Option<String>,
    pub time_format: Option<String>,
    pub use_definition_context: Option<bool>,
    pub enable_language_features: Option<bool>,
    pub preserve_formatting: Option<bool>,
    pub context_memory: Option<Vec<Value>>,
    pub fetch_alternatives: bool,
    pub fetch_word_insights: bool,
    pub fetch_suggestions: bool,
    pub fetch_alignments: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// State of a translation option (enabled/disabled).
pub struct TranslateOptionState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_gender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addressee_gender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_complexity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Bootstrap metadata for translation initialization.
pub struct TranslateBootstrapMetadata {
    pub method: String,
    pub authenticated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A warning returned during translation.
pub struct TranslateWarning {
    pub section: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// An alternative detected language candidate.
pub struct TranslateDetectedLanguageAlternative {
    pub iso: String,
    pub label: String,
    #[serde(default, rename = "isUncertain")]
    pub is_uncertain: bool,
    #[serde(default, rename = "isMixed")]
    pub is_mixed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Detected source language with confidence info.
pub struct TranslateDetectedLanguage {
    pub iso: String,
    pub label: String,
    #[serde(default, rename = "isUncertain")]
    pub is_uncertain: bool,
    #[serde(default, rename = "isMixed")]
    pub is_mixed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<TranslateDetectedLanguageAlternative>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response containing the translated text.
pub struct TranslateTextResponse {
    pub translation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_language: Option<TranslateDetectedLanguage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A single alternative translation variant.
pub struct AlternativeTranslationElement {
    pub translation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response containing alternative translations.
pub struct AlternativeTranslationsResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_description: Option<String>,
    #[serde(default)]
    pub elements: Vec<AlternativeTranslationElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response containing word-level alignment data.
pub struct TextAlignmentsResponse {
    #[serde(default)]
    pub source_blocks: Vec<Value>,
    #[serde(default)]
    pub target_blocks: Vec<Value>,
    #[serde(default)]
    pub source_roles: Vec<Value>,
    #[serde(default)]
    pub target_roles: Vec<Value>,
    #[serde(default)]
    pub alignments: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A translation suggestion for a word or phrase.
pub struct TranslationSuggestion {
    pub id: String,
    pub label: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub steps: Vec<Value>,
    #[serde(default, rename = "exclusiveWith")]
    pub exclusive_with: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Response containing translation suggestions.
pub struct TranslationSuggestionsResponse {
    #[serde(default)]
    pub suggestions: Vec<TranslationSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// A variation of a word insight entry.
pub struct WordInsightVariation {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Detailed insight about a translated word.
pub struct WordInsight {
    pub id: String,
    pub original_text: String,
    pub r#type: String,
    #[serde(default)]
    pub variations: Vec<WordInsightVariation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Response containing word insights.
pub struct WordInsightsResponse {
    #[serde(default)]
    pub insights: Vec<WordInsight>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marked_translation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Full response from the translate endpoint.
pub struct TranslateResponse {
    pub bootstrap: TranslateBootstrapMetadata,
    pub detected_language: TranslateDetectedLanguage,
    pub translation: TranslateTextResponse,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alternatives: Option<AlternativeTranslationsResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_alignments: Option<TextAlignmentsResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation_suggestions: Option<TranslationSuggestionsResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word_insights: Option<WordInsightsResponse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<TranslateWarning>,
}
