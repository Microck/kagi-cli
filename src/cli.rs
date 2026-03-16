use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "kagi",
    version,
    about = "Agent-native CLI for Kagi subscribers",
    long_about = "Search Kagi from the command line with JSON-first output for agents."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Search Kagi and emit structured JSON
    Search(SearchArgs),
    /// Inspect and validate configured credentials
    Auth(AuthCommand),
    /// Summarize a URL or text with Kagi's public API or subscriber web Summarizer
    Summarize(SummarizeArgs),
    /// Read Kagi News from the live public JSON endpoints
    News(NewsArgs),
    /// Prompt Kagi Assistant with subscriber session-token auth
    Assistant(AssistantArgs),
    /// Answer a query with Kagi's FastGPT API
    Fastgpt(FastGptArgs),
    /// Query Kagi's enrichment indexes
    Enrich(EnrichCommand),
    /// Fetch the Kagi Small Web feed
    Smallweb(SmallWebArgs),
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Search query to send to Kagi
    #[arg(value_name = "QUERY", required = true)]
    pub query: String,

    /// Render results in a human-readable terminal format instead of JSON.
    #[arg(long)]
    pub pretty: bool,

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
pub struct AssistantArgs {
    /// Prompt to send to Kagi Assistant
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// Continue an existing assistant thread by id
    #[arg(long, value_name = "THREAD_ID")]
    pub thread_id: Option<String>,
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
