//! Command-line interface definitions for kagi-cli.
//!
//! Uses `clap` to define the full CLI structure including subcommands
//! (search, summarize, news, assistant, quick, etc.), global flags,
//! and per-subcommand options.

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
/// Supported shell types for tab-completion generation.
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    #[value(name = "powershell")]
    PowerShell,
}

#[derive(Debug, Clone, ValueEnum)]
/// Output format for assistant thread exports.
pub enum AssistantThreadExportFormat {
    Markdown,
    Json,
}

/// Output format options for search results
#[derive(Debug, Clone, ValueEnum)]
/// Output format for search results.
pub enum OutputFormat {
    /// JSON output (default) - structured data for scripts and APIs
    Json,
    /// Pretty formatted output with colors - human-readable terminal display
    Pretty,
    /// Compact JSON output - minified JSON for reduced size
    Compact,
    /// Markdown formatted output - headers and links for documentation
    Markdown,
    /// CSV formatted output - spreadsheet-compatible table format
    Csv,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json => write!(f, "json"),
            Self::Pretty => write!(f, "pretty"),
            Self::Compact => write!(f, "compact"),
            Self::Markdown => write!(f, "markdown"),
            Self::Csv => write!(f, "csv"),
        }
    }
}

#[derive(Debug, Clone, ValueEnum)]
/// Output format for quick answers.
pub enum QuickOutputFormat {
    /// JSON output (default) - structured data for scripts and APIs
    Json,
    /// Pretty formatted output with colors - human-readable terminal display
    Pretty,
    /// Compact JSON output - minified JSON for reduced size
    Compact,
    /// Markdown formatted output - optimized for documentation and notes
    Markdown,
}

impl std::fmt::Display for QuickOutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json => write!(f, "json"),
            Self::Pretty => write!(f, "pretty"),
            Self::Compact => write!(f, "compact"),
            Self::Markdown => write!(f, "markdown"),
        }
    }
}

#[derive(Debug, Clone, ValueEnum)]
/// Output format for assistant responses.
pub enum AssistantOutputFormat {
    Json,
    Pretty,
    Compact,
    Markdown,
}

impl std::fmt::Display for AssistantOutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json => write!(f, "json"),
            Self::Pretty => write!(f, "pretty"),
            Self::Compact => write!(f, "compact"),
            Self::Markdown => write!(f, "markdown"),
        }
    }
}

#[derive(Debug, Clone, ValueEnum)]
/// Sort order for search results.
pub enum SearchOrder {
    Default,
    Recency,
    Website,
    Trackers,
}

#[derive(Debug, Clone, ValueEnum)]
/// Time range filter for search results.
pub enum SearchTime {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, ValueEnum)]
/// Predefined lens templates for common search scopes.
pub enum LensTemplate {
    Default,
    News,
}

impl LensTemplate {
    /// Returns the form value string for this lens template.
    pub const fn as_form_value(&self) -> &'static str {
        match self {
            Self::Default => "0",
            Self::News => "1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
/// News filtering mode (top stories vs. all).
pub enum NewsFilterMode {
    Hide,
    Blur,
}

impl NewsFilterMode {
    /// Returns the string representation of this news filter mode.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Hide => "hide",
            Self::Blur => "blur",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
/// Scope for news filtering (global, regional, etc.).
pub enum NewsFilterScope {
    Title,
    Summary,
    All,
}

impl NewsFilterScope {
    /// Returns the string representation of this news filter scope.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Summary => "summary",
            Self::All => "all",
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "kagi",
    version,
    about = "Agent-native CLI for Kagi subscribers with shell completion and batch processing",
    long_about = "Search Kagi from the command line with JSON-first output for agents.

Features:
• Shell completion generation (bash, zsh, fish, powershell)
• Multiple output formats (json, pretty, compact, markdown, csv)
• Parallel batch searches with rate limiting
• Colorized terminal output (disable with --no-color)
• Full Kagi API coverage with session token support",
    propagate_version = true
)]
#[command(disable_help_subcommand = true)]
#[command(arg_required_else_help = true)]
/// Top-level CLI definition with global flags and subcommands.
pub struct Cli {
    /// Generate shell completion script and print to stdout
    #[arg(long, value_name = "SHELL", value_enum)]
    pub generate_completion: Option<CompletionShell>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
/// All available CLI subcommands.
pub enum Commands {
    /// Search Kagi and emit structured JSON
    ///
    /// Example: kagi search "rust programming" --format pretty
    ///
    /// Features:
    /// • Multiple output formats: json (default), pretty, compact, markdown, csv
    /// • Colorized pretty output (disable with --no-color)
    /// • Lens support for scoped searches
    /// • Region, time, date, order, verbatim, and personalization filters
    Search(SearchArgs),
    /// Launch the auth setup wizard or use credential management subcommands
    Auth(AuthCommand),
    /// Summarize a URL or text with Kagi's public API or subscriber web Summarizer
    Summarize(SummarizeArgs),
    /// Read Kagi News from the live public JSON endpoints
    News(NewsArgs),
    /// Prompt Kagi Assistant and manage Assistant threads
    Assistant(AssistantArgs),
    /// Generate a Kagi Quick Answer from live search results
    Quick(QuickArgs),
    /// Ask Kagi Assistant about a specific web page
    AskPage(AskPageArgs),
    /// Translate text through Kagi Translate using session-token auth
    Translate(Box<TranslateArgs>),
    /// Answer a query with Kagi's `FastGPT` API
    Fastgpt(FastGptArgs),
    /// Query Kagi's enrichment indexes
    Enrich(EnrichCommand),
    /// Fetch the Kagi Small Web feed
    Smallweb(SmallWebArgs),
    /// Manage Kagi search lenses
    Lens(LensCommand),
    /// Manage custom bangs
    Bang(BangCommand),
    /// Manage search redirect rules
    Redirect(RedirectCommand),
    /// Execute multiple searches in parallel with rate limiting
    ///
    /// Example: kagi batch "rust" "python" "go" --concurrency 5 --rate-limit 120
    ///
    /// Features:
    /// • Parallel execution with configurable concurrency
    /// • Token bucket rate limiting to respect API limits
    /// • All output formats supported (json, pretty, compact, markdown, csv)
    /// • Lens support for scoped searches
    /// • Shared region, time, date, order, verbatim, and personalization filters
    /// • Color output control with --no-color
    Batch(BatchSearchArgs),
}

#[derive(Debug, Args)]
/// Arguments for the `search` subcommand.
pub struct SearchArgs {
    /// Search query to send to Kagi
    #[arg(value_name = "QUERY", required = true)]
    pub query: String,

    /// Output format
    #[arg(long, value_name = "FORMAT", default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,

    /// Disable colored terminal output (only affects pretty format)
    #[arg(long)]
    pub no_color: bool,

    /// Prefix the search with a Snap shortcut (for example "reddit" becomes "@reddit QUERY")
    #[arg(long, value_name = "SNAP")]
    pub snap: Option<String>,

    /// Scope search to a Kagi lens by numeric index (e.g., "0", "1", "2").
    ///
    /// Lens indices are user-specific. Find yours by:
    /// 1. Visit <https://kagi.com/settings/lenses> to see enabled lenses
    /// 2. Search in Kagi web UI with a lens active
    /// 3. Check the URL for the "l=" parameter value
    #[arg(long, value_name = "INDEX")]
    pub lens: Option<String>,

    /// Restrict results to a Kagi region code such as "us", "gb", or "`no_region`"
    #[arg(long, value_name = "REGION")]
    pub region: Option<String>,

    /// Restrict results to a recent time window
    #[arg(long, value_name = "WINDOW", value_enum)]
    pub time: Option<SearchTime>,

    /// Restrict results to pages updated on or after this date
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub from_date: Option<String>,

    /// Restrict results to pages updated on or before this date
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub to_date: Option<String>,

    /// Reorder search results
    #[arg(long, value_name = "ORDER", value_enum)]
    pub order: Option<SearchOrder>,

    /// Enable verbatim search mode for this request
    #[arg(long)]
    pub verbatim: bool,

    /// Force personalized search on for this request
    #[arg(long, conflicts_with = "no_personalized")]
    pub personalized: bool,

    /// Force personalized search off for this request
    #[arg(long, conflicts_with = "personalized")]
    pub no_personalized: bool,
}

#[derive(Debug, Args)]
/// Arguments for the `batch` subcommand (parallel search).
pub struct BatchSearchArgs {
    /// List of search queries to execute in parallel
    #[arg(value_name = "QUERIES", required = true)]
    pub queries: Vec<String>,

    /// Maximum number of concurrent requests (default: 3)
    #[arg(long, value_name = "NUM", default_value_t = 3)]
    pub concurrency: usize,

    /// Maximum requests per minute (default: 60)
    #[arg(long, value_name = "RPM", default_value_t = 60)]
    pub rate_limit: u32,

    /// Output format for batch results
    #[arg(long, value_name = "FORMAT", default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,

    /// Disable colored terminal output
    #[arg(long)]
    pub no_color: bool,

    /// Prefix every search with a Snap shortcut (for example "reddit" becomes "@reddit QUERY")
    #[arg(long, value_name = "SNAP")]
    pub snap: Option<String>,

    /// Scope all searches to a Kagi lens by numeric index
    #[arg(long, value_name = "INDEX")]
    pub lens: Option<String>,

    /// Restrict results to a Kagi region code such as "us", "gb", or "`no_region`"
    #[arg(long, value_name = "REGION")]
    pub region: Option<String>,

    /// Restrict results to a recent time window
    #[arg(long, value_name = "WINDOW", value_enum)]
    pub time: Option<SearchTime>,

    /// Restrict results to pages updated on or after this date
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub from_date: Option<String>,

    /// Restrict results to pages updated on or before this date
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub to_date: Option<String>,

    /// Reorder search results
    #[arg(long, value_name = "ORDER", value_enum)]
    pub order: Option<SearchOrder>,

    /// Enable verbatim search mode for all batch requests
    #[arg(long)]
    pub verbatim: bool,

    /// Force personalized search on for all batch requests
    #[arg(long, conflicts_with = "no_personalized")]
    pub personalized: bool,

    /// Force personalized search off for all batch requests
    #[arg(long, conflicts_with = "personalized")]
    pub no_personalized: bool,
}

impl BatchSearchArgs {
    /// Validates batch search arguments.
    ///
    /// # Errors
    /// Returns an error if concurrency or rate-limit is zero.
    pub fn validate(&self) -> Result<(), String> {
        if self.concurrency == 0 {
            return Err("concurrency must be at least 1".to_string());
        }
        if self.rate_limit == 0 {
            return Err("rate-limit must be at least 1".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Args)]
/// Arguments for the `auth` command group.
pub struct AuthCommand {
    #[command(subcommand)]
    pub command: AuthSubcommand,
}

#[derive(Debug, Subcommand)]
/// Subcommands for authentication management.
pub enum AuthSubcommand {
    /// Show which credential types are configured and where they come from
    Status,
    /// Validate the selected credential path without printing secret values
    Check,
    /// Save API/session credentials to local config
    Set(AuthSetArgs),
}

#[derive(Debug, Args)]
/// Arguments for `auth set` (store a credential).
pub struct AuthSetArgs {
    /// Kagi API token to save into .kagi.toml
    #[arg(long, value_name = "TOKEN")]
    pub api_token: Option<String>,

    /// Kagi session token or full Session Link URL to save into .kagi.toml
    #[arg(long, value_name = "TOKEN_OR_URL")]
    pub session_token: Option<String>,
}

#[derive(Debug, Args)]
/// Arguments for the `summarize` subcommand.
pub struct SummarizeArgs {
    /// URL to summarize
    #[arg(long, value_name = "URL", conflicts_with = "text")]
    pub url: Option<String>,

    /// Text to summarize
    #[arg(long, value_name = "TEXT", conflicts_with = "url")]
    pub text: Option<String>,

    /// Use Kagi's subscriber web Summarizer via session-token auth instead of the paid public API.
    #[arg(long)]
    pub subscriber: bool,

    /// Subscriber web mode only: output length (headline, overview, digest, medium, long)
    #[arg(long, value_name = "LENGTH")]
    pub length: Option<String>,

    /// Public API mode only: summarization engine (cecil, agnes, daphne, muriel)
    #[arg(long, value_name = "ENGINE")]
    pub engine: Option<String>,

    /// Summarization mode/type. `--subscriber` accepts summary, keypoints, or eli5.
    #[arg(long, value_name = "TYPE")]
    pub summary_type: Option<String>,

    /// Target language code (for example EN, ES, JA)
    #[arg(long, value_name = "LANG")]
    pub target_language: Option<String>,

    /// Allow cached requests/responses
    #[arg(long)]
    pub cache: Option<bool>,
}

impl SummarizeArgs {
    /// Validates summarize arguments.
    ///
    /// # Errors
    /// Returns an error when neither `--url` nor `--text` is provided.
    pub fn validate(&self) -> Result<(), String> {
        if self.url.is_none() && self.text.is_none() {
            return Err("summarize requires exactly one of --url or --text".to_string());
        }

        Ok(())
    }
}

#[derive(Debug, Args)]
/// Arguments for the `fastgpt` subcommand.
pub struct FastGptArgs {
    /// Query to answer
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// Allow cached requests/responses
    #[arg(long)]
    pub cache: Option<bool>,

    /// Whether to perform web search. Kagi docs note values other than true are currently unsupported.
    #[arg(long)]
    pub web_search: Option<bool>,
}

#[derive(Debug, Args)]
/// Arguments for the `news` subcommand.
pub struct NewsArgs {
    /// News category slug (for example world, usa, tech, science)
    #[arg(long, value_name = "CATEGORY", default_value = "world")]
    pub category: String,

    /// Number of stories to return
    #[arg(long, value_name = "COUNT", default_value_t = 12)]
    pub limit: u32,

    /// News language code
    #[arg(long, value_name = "LANG", default_value = "default")]
    pub lang: String,

    /// List currently available categories instead of stories
    #[arg(long, conflicts_with = "chaos")]
    pub list_categories: bool,

    /// Return only the current Kagi News chaos index
    #[arg(long, conflicts_with = "list_categories")]
    pub chaos: bool,

    /// List built-in content-filter presets instead of stories
    #[arg(long, conflicts_with_all = ["list_categories", "chaos"])]
    pub list_filter_presets: bool,

    /// Built-in content-filter preset id to apply
    #[arg(long, value_name = "PRESET_ID")]
    pub filter_preset: Vec<String>,

    /// Custom keyword to filter out from the feed
    #[arg(long, value_name = "KEYWORD")]
    pub filter_keyword: Vec<String>,

    /// Filter behavior for matching stories
    #[arg(long, value_name = "MODE", value_enum, default_value = "hide")]
    pub filter_mode: NewsFilterMode,

    /// Story fields to inspect for keyword matches
    #[arg(long, value_name = "SCOPE", value_enum, default_value = "all")]
    pub filter_scope: NewsFilterScope,
}

impl NewsArgs {
    /// Validates news arguments.
    ///
    /// # Errors
    /// Returns an error if filter options conflict with list/chaos modes,
    /// or if filter mode/scope are used without filter inputs.
    pub fn validate(&self) -> Result<(), String> {
        let has_filter_inputs = self.has_filter_inputs();
        let has_non_default_filter_options =
            self.filter_mode != NewsFilterMode::Hide || self.filter_scope != NewsFilterScope::All;

        if (self.list_categories || self.chaos || self.list_filter_presets)
            && (has_filter_inputs || has_non_default_filter_options)
        {
            return Err(
                "news filters are not supported with --list-categories, --chaos, or --list-filter-presets"
                    .to_string(),
            );
        }

        if !has_filter_inputs && has_non_default_filter_options {
            return Err(
                "--filter-mode and --filter-scope require at least one --filter-preset or --filter-keyword"
                    .to_string(),
            );
        }

        Ok(())
    }

    /// Returns `true` if any filter inputs (presets or keywords) are specified.
    pub const fn has_filter_inputs(&self) -> bool {
        !self.filter_preset.is_empty() || !self.filter_keyword.is_empty()
    }
}

#[derive(Debug, Args)]
#[command(arg_required_else_help = true, args_conflicts_with_subcommands = true)]
/// Arguments for the `assistant` subcommand.
pub struct AssistantArgs {
    #[command(subcommand)]
    pub command: Option<AssistantSubcommand>,

    /// Prompt to send to Kagi Assistant
    #[arg(value_name = "QUERY")]
    pub query: Option<String>,

    /// Continue an existing assistant thread by id
    #[arg(long, value_name = "THREAD_ID")]
    pub thread_id: Option<String>,

    /// Use a saved assistant by name, id, or invoke profile slug
    #[arg(long, value_name = "ASSISTANT")]
    pub assistant: Option<String>,

    /// Output format for assistant prompt mode
    #[arg(long, value_name = "FORMAT", default_value_t = AssistantOutputFormat::Json)]
    pub format: AssistantOutputFormat,

    /// Disable colored terminal output (only affects pretty format)
    #[arg(long)]
    pub no_color: bool,

    /// Override the Assistant model slug for this prompt
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,

    /// Override the Assistant lens id for this prompt
    #[arg(long, value_name = "LENS_ID")]
    pub lens: Option<u64>,

    /// Force web access on for this prompt
    #[arg(long, conflicts_with = "no_web_access")]
    pub web_access: bool,

    /// Force web access off for this prompt
    #[arg(long, conflicts_with = "web_access")]
    pub no_web_access: bool,

    /// Force personalizations on for this prompt
    #[arg(long, conflicts_with = "no_personalized")]
    pub personalized: bool,

    /// Force personalizations off for this prompt
    #[arg(long, conflicts_with = "personalized")]
    pub no_personalized: bool,
}

#[derive(Debug, Subcommand)]
/// Subcommands for the assistant feature.
pub enum AssistantSubcommand {
    /// Manage Assistant threads
    Thread(AssistantThreadArgs),
    /// Manage custom assistants
    Custom(AssistantCustomArgs),
}

#[derive(Debug, Args)]
/// Arguments for `assistant thread` management.
pub struct AssistantThreadArgs {
    #[command(subcommand)]
    pub command: AssistantThreadSubcommand,
}

#[derive(Debug, Subcommand)]
/// Subcommands for assistant thread operations.
pub enum AssistantThreadSubcommand {
    /// List Assistant threads for the current account
    List,
    /// Fetch one Assistant thread with its messages
    Get(AssistantThreadIdArgs),
    /// Delete one Assistant thread
    Delete(AssistantThreadIdArgs),
    /// Export one Assistant thread
    Export(AssistantThreadExportArgs),
}

#[derive(Debug, Args)]
/// Arguments for thread-specific operations (by ID).
pub struct AssistantThreadIdArgs {
    /// Assistant thread id
    #[arg(value_name = "THREAD_ID")]
    pub thread_id: String,
}

#[derive(Debug, Args)]
/// Arguments for exporting an assistant thread.
pub struct AssistantThreadExportArgs {
    /// Assistant thread id
    #[arg(value_name = "THREAD_ID")]
    pub thread_id: String,

    /// Export format
    #[arg(long, value_name = "FORMAT", value_enum, default_value = "markdown")]
    pub format: AssistantThreadExportFormat,
}

#[derive(Debug, Args)]
/// Arguments for `assistant custom` (custom assistants).
pub struct AssistantCustomArgs {
    #[command(subcommand)]
    pub command: AssistantCustomSubcommand,
}

#[derive(Debug, Subcommand)]
/// Subcommands for custom assistant management.
pub enum AssistantCustomSubcommand {
    /// List custom and built-in assistants visible to the account
    List,
    /// Fetch one custom assistant definition by id or name
    Get(AssistantCustomTargetArgs),
    /// Create a custom assistant
    Create(AssistantCustomCreateArgs),
    /// Update a custom assistant by id or name
    Update(AssistantCustomUpdateArgs),
    /// Delete a custom assistant by id or name
    Delete(AssistantCustomTargetArgs),
}

#[derive(Debug, Args)]
/// Arguments targeting a specific custom assistant.
pub struct AssistantCustomTargetArgs {
    /// Custom assistant id or exact assistant name
    #[arg(value_name = "ID_OR_NAME")]
    pub target: String,
}

#[derive(Debug, Args)]
/// Arguments for creating a new custom assistant.
pub struct AssistantCustomCreateArgs {
    /// Assistant name
    #[arg(value_name = "NAME")]
    pub name: String,

    /// Optional bang trigger without the leading '!'
    #[arg(long, value_name = "TRIGGER")]
    pub bang_trigger: Option<String>,

    /// Enable internet access for this assistant
    #[arg(long, conflicts_with = "no_web_access")]
    pub web_access: bool,

    /// Disable internet access for this assistant
    #[arg(long, conflicts_with = "web_access")]
    pub no_web_access: bool,

    /// Lens id to scope assistant web access
    #[arg(long, value_name = "LENS_ID")]
    pub lens: Option<String>,

    /// Enable personalizations
    #[arg(long, conflicts_with = "no_personalized")]
    pub personalized: bool,

    /// Disable personalizations
    #[arg(long, conflicts_with = "personalized")]
    pub no_personalized: bool,

    /// Base model slug
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,

    /// Custom instructions text
    #[arg(long, value_name = "TEXT")]
    pub instructions: Option<String>,
}

#[derive(Debug, Args)]
/// Arguments for updating an existing custom assistant.
pub struct AssistantCustomUpdateArgs {
    /// Custom assistant id or exact assistant name
    #[arg(value_name = "ID_OR_NAME")]
    pub target: String,

    /// New assistant name
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,

    /// New bang trigger without the leading '!'
    #[arg(long, value_name = "TRIGGER")]
    pub bang_trigger: Option<String>,

    /// Enable internet access for this assistant
    #[arg(long, conflicts_with = "no_web_access")]
    pub web_access: bool,

    /// Disable internet access for this assistant
    #[arg(long, conflicts_with = "web_access")]
    pub no_web_access: bool,

    /// New lens id to scope assistant web access
    #[arg(long, value_name = "LENS_ID")]
    pub lens: Option<String>,

    /// Enable personalizations
    #[arg(long, conflicts_with = "no_personalized")]
    pub personalized: bool,

    /// Disable personalizations
    #[arg(long, conflicts_with = "personalized")]
    pub no_personalized: bool,

    /// New base model slug
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,

    /// New custom instructions text
    #[arg(long, value_name = "TEXT")]
    pub instructions: Option<String>,
}

#[derive(Debug, Args)]
/// Arguments for the `ask-page` subcommand.
pub struct AskPageArgs {
    /// Absolute page URL to discuss with Assistant
    #[arg(value_name = "URL")]
    pub url: String,

    /// Question to ask about the page
    #[arg(value_name = "QUESTION")]
    pub question: String,
}

#[derive(Debug, Args)]
/// Arguments for the `translate` subcommand.
pub struct TranslateArgs {
    /// Text to translate
    #[arg(value_name = "TEXT")]
    pub text: String,

    /// Source language code (default: auto)
    #[arg(long, value_name = "LANG", default_value = "auto")]
    pub from: String,

    /// Target language code (default: en)
    #[arg(long, value_name = "LANG", default_value = "en")]
    pub to: String,

    /// Translation quality preference
    #[arg(long, value_name = "QUALITY")]
    pub quality: Option<String>,

    /// Translation model override
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,

    /// Prediction text to bias the translation
    #[arg(long, value_name = "TEXT")]
    pub prediction: Option<String>,

    /// Predicted source language code
    #[arg(long, value_name = "LANG")]
    pub predicted_language: Option<String>,

    /// Formality setting
    #[arg(long, value_name = "LEVEL")]
    pub formality: Option<String>,

    /// Speaker gender hint
    #[arg(long, value_name = "GENDER")]
    pub speaker_gender: Option<String>,

    /// Addressee gender hint
    #[arg(long, value_name = "GENDER")]
    pub addressee_gender: Option<String>,

    /// Language complexity setting
    #[arg(long, value_name = "LEVEL")]
    pub language_complexity: Option<String>,

    /// Translation style setting
    #[arg(long, value_name = "STYLE")]
    pub translation_style: Option<String>,

    /// Extra translation context
    #[arg(long, value_name = "TEXT")]
    pub context: Option<String>,

    /// Dictionary language override
    #[arg(long, value_name = "LANG")]
    pub dictionary_language: Option<String>,

    /// Time formatting style
    #[arg(long, value_name = "FORMAT")]
    pub time_format: Option<String>,

    /// Toggle definition-aware translation behavior
    #[arg(long)]
    pub use_definition_context: Option<bool>,

    /// Toggle language-feature enrichment
    #[arg(long)]
    pub enable_language_features: Option<bool>,

    /// Preserve source formatting when possible
    #[arg(long)]
    pub preserve_formatting: Option<bool>,

    /// Raw JSON array passed through as `context_memory`
    #[arg(long, value_name = "JSON")]
    pub context_memory_json: Option<String>,

    /// Skip the alternative translations call
    #[arg(long)]
    pub no_alternatives: bool,

    /// Skip the word insights call
    #[arg(long)]
    pub no_word_insights: bool,

    /// Skip the translation suggestions call
    #[arg(long)]
    pub no_suggestions: bool,

    /// Skip the text alignments call
    #[arg(long)]
    pub no_alignments: bool,
}

#[derive(Debug, Args)]
/// Arguments for the `quick` subcommand (quick answers).
pub struct QuickArgs {
    /// Query to answer with Kagi Quick Answer
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// Output format
    #[arg(long, value_name = "FORMAT", default_value_t = QuickOutputFormat::Json)]
    pub format: QuickOutputFormat,

    /// Disable colored terminal output (only affects pretty format)
    #[arg(long)]
    pub no_color: bool,

    /// Scope quick answer to a Kagi lens by numeric index
    #[arg(long, value_name = "INDEX")]
    pub lens: Option<String>,
}

#[derive(Debug, Args)]
/// Arguments for the `enrich` command group.
pub struct EnrichCommand {
    #[command(subcommand)]
    pub command: EnrichSubcommand,
}

#[derive(Debug, Subcommand)]
/// Subcommands for web/news enrichment.
pub enum EnrichSubcommand {
    /// Query Kagi's Teclis web enrichment index
    Web(EnrichArgs),
    /// Query Kagi's `TinyGem` news enrichment index
    News(EnrichArgs),
}

#[derive(Debug, Args)]
/// Arguments for enrichment operations.
pub struct EnrichArgs {
    /// Query to enrich
    #[arg(value_name = "QUERY")]
    pub query: String,
}

#[derive(Debug, Args)]
/// Arguments for the `smallweb` feed subcommand.
pub struct SmallWebArgs {
    /// Limit number of feed entries returned by the Small Web feed
    #[arg(long, value_name = "COUNT")]
    pub limit: Option<u32>,
}

#[derive(Debug, Args)]
/// Arguments for the `lens` command group.
pub struct LensCommand {
    #[command(subcommand)]
    pub command: LensSubcommand,
}

#[derive(Debug, Subcommand)]
/// Subcommands for lens management.
pub enum LensSubcommand {
    /// List available lenses and whether they are enabled
    List,
    /// Fetch one lens definition by id or exact name
    Get(LensTargetArgs),
    /// Create a lens
    Create(LensCreateArgs),
    /// Update a lens by id or exact name
    Update(LensUpdateArgs),
    /// Delete a lens by id or exact name
    Delete(LensTargetArgs),
    /// Enable a lens by id or exact name
    Enable(LensTargetArgs),
    /// Disable a lens by id or exact name
    Disable(LensTargetArgs),
}

#[derive(Debug, Args)]
/// Arguments targeting a specific lens.
pub struct LensTargetArgs {
    /// Lens id or exact lens name
    #[arg(value_name = "ID_OR_NAME")]
    pub target: String,
}

#[derive(Debug, Args)]
/// Arguments for creating a new lens.
pub struct LensCreateArgs {
    /// Lens display name
    #[arg(value_name = "NAME")]
    pub name: String,

    #[arg(long, value_name = "CSV")]
    pub included_sites: Option<String>,

    #[arg(long, value_name = "CSV")]
    pub included_keywords: Option<String>,

    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,

    #[arg(long, value_name = "REGION")]
    pub region: Option<String>,

    #[arg(long, value_name = "YYYY-MM-DD")]
    pub before_date: Option<String>,

    #[arg(long, value_name = "YYYY-MM-DD")]
    pub after_date: Option<String>,

    #[arg(long, value_name = "CSV")]
    pub excluded_sites: Option<String>,

    #[arg(long, value_name = "CSV")]
    pub excluded_keywords: Option<String>,

    #[arg(long, value_name = "KEYWORD")]
    pub shortcut: Option<String>,

    #[arg(long, conflicts_with = "no_autocomplete_keywords")]
    pub autocomplete_keywords: bool,

    #[arg(long, conflicts_with = "autocomplete_keywords")]
    pub no_autocomplete_keywords: bool,

    #[arg(long, value_name = "TEMPLATE", value_enum)]
    pub template: Option<LensTemplate>,

    #[arg(long, value_name = "EXT")]
    pub file_type: Option<String>,

    #[arg(long, conflicts_with = "no_share_with_team")]
    pub share_with_team: bool,

    #[arg(long, conflicts_with = "share_with_team")]
    pub no_share_with_team: bool,

    #[arg(long, conflicts_with = "no_share_copy_code")]
    pub share_copy_code: bool,

    #[arg(long, conflicts_with = "share_copy_code")]
    pub no_share_copy_code: bool,
}

#[derive(Debug, Args)]
/// Arguments for updating an existing lens.
pub struct LensUpdateArgs {
    /// Lens id or exact lens name
    #[arg(value_name = "ID_OR_NAME")]
    pub target: String,

    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,

    #[arg(long, value_name = "CSV")]
    pub included_sites: Option<String>,

    #[arg(long, value_name = "CSV")]
    pub included_keywords: Option<String>,

    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,

    #[arg(long, value_name = "REGION")]
    pub region: Option<String>,

    #[arg(long, value_name = "YYYY-MM-DD")]
    pub before_date: Option<String>,

    #[arg(long, value_name = "YYYY-MM-DD")]
    pub after_date: Option<String>,

    #[arg(long, value_name = "CSV")]
    pub excluded_sites: Option<String>,

    #[arg(long, value_name = "CSV")]
    pub excluded_keywords: Option<String>,

    #[arg(long, value_name = "KEYWORD")]
    pub shortcut: Option<String>,

    #[arg(long, conflicts_with = "no_autocomplete_keywords")]
    pub autocomplete_keywords: bool,

    #[arg(long, conflicts_with = "autocomplete_keywords")]
    pub no_autocomplete_keywords: bool,

    #[arg(long, value_name = "TEMPLATE", value_enum)]
    pub template: Option<LensTemplate>,

    #[arg(long, value_name = "EXT")]
    pub file_type: Option<String>,

    #[arg(long, conflicts_with = "no_share_with_team")]
    pub share_with_team: bool,

    #[arg(long, conflicts_with = "share_with_team")]
    pub no_share_with_team: bool,

    #[arg(long, conflicts_with = "no_share_copy_code")]
    pub share_copy_code: bool,

    #[arg(long, conflicts_with = "share_copy_code")]
    pub no_share_copy_code: bool,
}

#[derive(Debug, Args)]
/// Arguments for the `bang` command group.
pub struct BangCommand {
    #[command(subcommand)]
    pub command: BangSubcommand,
}

#[derive(Debug, Subcommand)]
/// Subcommands for bang shortcuts.
pub enum BangSubcommand {
    /// Manage custom bangs
    Custom(CustomBangCommand),
}

#[derive(Debug, Args)]
/// Arguments for the `bang custom` command group.
pub struct CustomBangCommand {
    #[command(subcommand)]
    pub command: CustomBangSubcommand,
}

#[derive(Debug, Subcommand)]
/// Subcommands for custom bang management.
pub enum CustomBangSubcommand {
    /// List custom bangs
    List,
    /// Fetch one custom bang by id, exact name, or trigger
    Get(CustomBangTargetArgs),
    /// Create a custom bang
    Create(CustomBangCreateArgs),
    /// Update a custom bang by id, exact name, or trigger
    Update(CustomBangUpdateArgs),
    /// Delete a custom bang by id, exact name, or trigger
    Delete(CustomBangTargetArgs),
}

#[derive(Debug, Args)]
/// Arguments targeting a specific custom bang.
pub struct CustomBangTargetArgs {
    /// Bang id, exact name, or trigger (with or without leading '!')
    #[arg(value_name = "ID_OR_NAME")]
    pub target: String,
}

#[derive(Debug, Args)]
/// Arguments for creating a new custom bang.
pub struct CustomBangCreateArgs {
    /// Bang display name
    #[arg(value_name = "NAME")]
    pub name: String,

    /// Bang trigger without the leading '!'
    #[arg(long, value_name = "TRIGGER")]
    pub trigger: String,

    #[arg(long, value_name = "URL")]
    pub template: Option<String>,

    #[arg(long, value_name = "DOMAIN")]
    pub snap_domain: Option<String>,

    #[arg(long, value_name = "REGEX")]
    pub regex_pattern: Option<String>,

    #[arg(long, conflicts_with = "no_shortcut_menu")]
    pub shortcut_menu: bool,

    #[arg(long, conflicts_with = "shortcut_menu")]
    pub no_shortcut_menu: bool,

    #[arg(long, conflicts_with = "no_open_snap_domain")]
    pub open_snap_domain: bool,

    #[arg(long, conflicts_with = "open_snap_domain")]
    pub no_open_snap_domain: bool,

    #[arg(long, conflicts_with = "no_open_base_path")]
    pub open_base_path: bool,

    #[arg(long, conflicts_with = "open_base_path")]
    pub no_open_base_path: bool,

    #[arg(long, conflicts_with = "no_encode_placeholder")]
    pub encode_placeholder: bool,

    #[arg(long, conflicts_with = "encode_placeholder")]
    pub no_encode_placeholder: bool,

    #[arg(long, conflicts_with = "no_plus_for_space")]
    pub plus_for_space: bool,

    #[arg(long, conflicts_with = "plus_for_space")]
    pub no_plus_for_space: bool,
}

#[derive(Debug, Args)]
/// Arguments for updating an existing custom bang.
pub struct CustomBangUpdateArgs {
    /// Bang id, exact name, or trigger (with or without leading '!')
    #[arg(value_name = "ID_OR_NAME")]
    pub target: String,

    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,

    #[arg(long, value_name = "TRIGGER")]
    pub trigger: Option<String>,

    #[arg(long, value_name = "URL")]
    pub template: Option<String>,

    #[arg(long, value_name = "DOMAIN")]
    pub snap_domain: Option<String>,

    #[arg(long, value_name = "REGEX")]
    pub regex_pattern: Option<String>,

    #[arg(long, conflicts_with = "no_shortcut_menu")]
    pub shortcut_menu: bool,

    #[arg(long, conflicts_with = "shortcut_menu")]
    pub no_shortcut_menu: bool,

    #[arg(long, conflicts_with = "no_open_snap_domain")]
    pub open_snap_domain: bool,

    #[arg(long, conflicts_with = "open_snap_domain")]
    pub no_open_snap_domain: bool,

    #[arg(long, conflicts_with = "no_open_base_path")]
    pub open_base_path: bool,

    #[arg(long, conflicts_with = "open_base_path")]
    pub no_open_base_path: bool,

    #[arg(long, conflicts_with = "no_encode_placeholder")]
    pub encode_placeholder: bool,

    #[arg(long, conflicts_with = "encode_placeholder")]
    pub no_encode_placeholder: bool,

    #[arg(long, conflicts_with = "no_plus_for_space")]
    pub plus_for_space: bool,

    #[arg(long, conflicts_with = "plus_for_space")]
    pub no_plus_for_space: bool,
}

#[derive(Debug, Args)]
/// Arguments for the `redirect` command group.
pub struct RedirectCommand {
    #[command(subcommand)]
    pub command: RedirectSubcommand,
}

#[derive(Debug, Subcommand)]
/// Subcommands for redirect rule management.
pub enum RedirectSubcommand {
    /// List redirect rules
    List,
    /// Fetch one redirect rule by id or exact rule text
    Get(RedirectTargetArgs),
    /// Create a redirect rule
    Create(RedirectCreateArgs),
    /// Update a redirect rule by id or exact rule text
    Update(RedirectUpdateArgs),
    /// Delete a redirect rule by id or exact rule text
    Delete(RedirectTargetArgs),
    /// Enable a redirect rule by id or exact rule text
    Enable(RedirectTargetArgs),
    /// Disable a redirect rule by id or exact rule text
    Disable(RedirectTargetArgs),
}

#[derive(Debug, Args)]
/// Arguments targeting a specific redirect rule.
pub struct RedirectTargetArgs {
    /// Redirect id or exact rule text
    #[arg(value_name = "ID_OR_RULE")]
    pub target: String,
}

#[derive(Debug, Args)]
/// Arguments for creating a new redirect rule.
pub struct RedirectCreateArgs {
    /// Full regex|replacement rule
    #[arg(value_name = "RULE")]
    pub rule: String,
}

#[derive(Debug, Args)]
/// Arguments for updating an existing redirect rule.
pub struct RedirectUpdateArgs {
    /// Redirect id or exact rule text
    #[arg(value_name = "ID_OR_RULE")]
    pub target: String,

    /// Full replacement regex|replacement rule
    #[arg(value_name = "RULE")]
    pub rule: String,
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands, NewsArgs, NewsFilterMode, NewsFilterScope, SummarizeArgs};
    use clap::Parser;

    fn sample_news_args() -> NewsArgs {
        NewsArgs {
            category: "world".to_string(),
            limit: 12,
            lang: "default".to_string(),
            list_categories: false,
            chaos: false,
            list_filter_presets: false,
            filter_preset: vec![],
            filter_keyword: vec![],
            filter_mode: NewsFilterMode::Hide,
            filter_scope: NewsFilterScope::All,
        }
    }

    #[test]
    fn rejects_non_default_filter_options_without_inputs() {
        let mut args = sample_news_args();
        args.filter_mode = NewsFilterMode::Blur;

        let error = args
            .validate()
            .expect_err("non-default filter mode should require filter inputs");
        assert!(error.contains("--filter-mode and --filter-scope require"));
    }

    #[test]
    fn rejects_filters_with_listing_modes() {
        let mut args = sample_news_args();
        args.list_filter_presets = true;
        args.filter_keyword = vec!["trump".to_string()];

        let error = args
            .validate()
            .expect_err("filter inputs should conflict with preset listing");
        assert!(error.contains("news filters are not supported"));
    }

    #[test]
    fn accepts_filter_inputs_with_default_mode_and_scope() {
        let mut args = sample_news_args();
        args.filter_preset = vec!["politics".to_string()];

        assert!(args.validate().is_ok());
        assert!(args.has_filter_inputs());
    }

    #[test]
    fn rejects_summarize_args_without_url_or_text() {
        let args = SummarizeArgs {
            url: None,
            text: None,
            subscriber: false,
            length: None,
            engine: None,
            summary_type: None,
            target_language: None,
            cache: None,
        };

        let error = args
            .validate()
            .expect_err("summarize should require url or text input");
        assert!(error.contains("exactly one of --url or --text"));
    }

    #[test]
    fn parses_lens_enable_command() {
        let cli = Cli::try_parse_from(["kagi", "lens", "enable", "Reddit"])
            .expect("lens command should parse");

        match cli.command.expect("command") {
            Commands::Lens(command) => match command.command {
                super::LensSubcommand::Enable(target) => assert_eq!(target.target, "Reddit"),
                other => panic!("unexpected lens subcommand: {other:?}"),
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_assistant_custom_update_command() {
        let cli = Cli::try_parse_from([
            "kagi",
            "assistant",
            "custom",
            "update",
            "Writer",
            "--model",
            "gpt-5-mini",
            "--no-web-access",
        ])
        .expect("assistant custom update should parse");

        match cli.command.expect("command") {
            Commands::Assistant(args) => match args.command.expect("subcommand") {
                super::AssistantSubcommand::Custom(custom) => match custom.command {
                    super::AssistantCustomSubcommand::Update(update) => {
                        assert_eq!(update.target, "Writer");
                        assert_eq!(update.model.as_deref(), Some("gpt-5-mini"));
                        assert!(update.no_web_access);
                    }
                    other => panic!("unexpected assistant custom subcommand: {other:?}"),
                },
                other => panic!("unexpected assistant subcommand: {other:?}"),
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn rejects_conflicting_redirect_flags() {
        let error = Cli::try_parse_from([
            "kagi",
            "bang",
            "custom",
            "create",
            "Example",
            "--trigger",
            "ex",
            "--shortcut-menu",
            "--no-shortcut-menu",
        ])
        .expect_err("conflicting flags should fail");

        let rendered = error.to_string();
        assert!(rendered.contains("--shortcut-menu"));
    }
}
