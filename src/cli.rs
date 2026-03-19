use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    #[value(name = "powershell")]
    PowerShell,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum AssistantThreadExportFormat {
    Markdown,
    Json,
}

/// Output format options for search results
#[derive(Debug, Clone, ValueEnum)]
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
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Pretty => write!(f, "pretty"),
            OutputFormat::Compact => write!(f, "compact"),
            OutputFormat::Markdown => write!(f, "markdown"),
            OutputFormat::Csv => write!(f, "csv"),
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
pub struct Cli {
    /// Generate shell completion script and print to stdout
    #[arg(long, value_name = "SHELL", value_enum)]
    pub generate_completion: Option<CompletionShell>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Search Kagi and emit structured JSON
    ///
    /// Example: kagi search "rust programming" --format pretty
    ///
    /// Features:
    /// • Multiple output formats: json (default), pretty, compact, markdown, csv
    /// • Colorized pretty output (disable with --no-color)
    /// • Lens support for scoped searches
    Search(SearchArgs),
    /// Inspect and validate configured credentials
    Auth(AuthCommand),
    /// Summarize a URL or text with Kagi's public API or subscriber web Summarizer
    Summarize(SummarizeArgs),
    /// Read Kagi News from the live public JSON endpoints
    News(NewsArgs),
    /// Prompt Kagi Assistant and manage Assistant threads
    Assistant(AssistantArgs),
    /// Answer a query with Kagi's FastGPT API
    Fastgpt(FastGptArgs),
    /// Query Kagi's enrichment indexes
    Enrich(EnrichCommand),
    /// Fetch the Kagi Small Web feed
    Smallweb(SmallWebArgs),
    /// Execute multiple searches in parallel with rate limiting
    ///
    /// Example: kagi batch "rust" "python" "go" --concurrency 5 --rate-limit 120
    ///
    /// Features:
    /// • Parallel execution with configurable concurrency
    /// • Token bucket rate limiting to respect API limits
    /// • All output formats supported (json, pretty, compact, markdown, csv)
    /// • Lens support for scoped searches
    /// • Color output control with --no-color
    Batch(BatchSearchArgs),
}

#[derive(Debug, Args)]
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

    /// Scope search to a Kagi lens by numeric index (e.g., "0", "1", "2").
    ///
    /// Lens indices are user-specific. Find yours by:
    /// 1. Visit https://kagi.com/settings/lenses to see enabled lenses
    /// 2. Search in Kagi web UI with a lens active
    /// 3. Check the URL for the "l=" parameter value
    #[arg(long, value_name = "INDEX")]
    pub lens: Option<String>,
}

#[derive(Debug, Args)]
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

    /// Scope all searches to a Kagi lens by numeric index
    #[arg(long, value_name = "INDEX")]
    pub lens: Option<String>,
}

impl BatchSearchArgs {
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
pub struct AuthCommand {
    #[command(subcommand)]
    pub command: AuthSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AuthSubcommand {
    /// Show which credential types are configured and where they come from
    Status,
    /// Validate the selected credential path without printing secret values
    Check,
    /// Save API/session credentials to local config
    Set(AuthSetArgs),
}

#[derive(Debug, Args)]
pub struct AuthSetArgs {
    /// Kagi API token to save into .kagi.toml
    #[arg(long, value_name = "TOKEN")]
    pub api_token: Option<String>,

    /// Kagi session token or full Session Link URL to save into .kagi.toml
    #[arg(long, value_name = "TOKEN_OR_URL")]
    pub session_token: Option<String>,
}

#[derive(Debug, Args)]
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

#[derive(Debug, Args)]
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
}

#[derive(Debug, Args)]
#[command(arg_required_else_help = true, args_conflicts_with_subcommands = true)]
pub struct AssistantArgs {
    #[command(subcommand)]
    pub command: Option<AssistantSubcommand>,

    /// Prompt to send to Kagi Assistant
    #[arg(value_name = "QUERY")]
    pub query: Option<String>,

    /// Continue an existing assistant thread by id
    #[arg(long, value_name = "THREAD_ID")]
    pub thread_id: Option<String>,

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
pub enum AssistantSubcommand {
    /// Manage Assistant threads
    Thread(AssistantThreadArgs),
}

#[derive(Debug, Args)]
pub struct AssistantThreadArgs {
    #[command(subcommand)]
    pub command: AssistantThreadSubcommand,
}

#[derive(Debug, Subcommand)]
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
pub struct AssistantThreadIdArgs {
    /// Assistant thread id
    #[arg(value_name = "THREAD_ID")]
    pub thread_id: String,
}

#[derive(Debug, Args)]
pub struct AssistantThreadExportArgs {
    /// Assistant thread id
    #[arg(value_name = "THREAD_ID")]
    pub thread_id: String,

    /// Export format
    #[arg(long, value_name = "FORMAT", value_enum, default_value = "markdown")]
    pub format: AssistantThreadExportFormat,
}

#[derive(Debug, Args)]
pub struct EnrichCommand {
    #[command(subcommand)]
    pub command: EnrichSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum EnrichSubcommand {
    /// Query Kagi's Teclis web enrichment index
    Web(EnrichArgs),
    /// Query Kagi's TinyGem news enrichment index
    News(EnrichArgs),
}

#[derive(Debug, Args)]
pub struct EnrichArgs {
    /// Query to enrich
    #[arg(value_name = "QUERY")]
    pub query: String,
}

#[derive(Debug, Args)]
pub struct SmallWebArgs {
    /// Limit number of feed entries returned by the Small Web feed
    #[arg(long, value_name = "COUNT")]
    pub limit: Option<u32>,
}
