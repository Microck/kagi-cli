mod api;
mod auth;
mod cli;
mod error;
mod parser;
mod search;
mod types;

use clap::{CommandFactory, Parser};
use clap_complete::{generate, shells};

use crate::api::{
    execute_ask_page, execute_assistant_prompt, execute_assistant_thread_delete,
    execute_assistant_thread_export, execute_assistant_thread_get, execute_assistant_thread_list,
    execute_enrich_news, execute_enrich_web, execute_fastgpt, execute_news,
    execute_news_categories, execute_news_chaos, execute_smallweb, execute_subscriber_summarize,
    execute_summarize,
};
use crate::auth::{
    Credential, CredentialKind, SearchCredentials, format_status, load_credential_inventory,
    save_credentials,
};
use crate::cli::{
    AssistantSubcommand, AssistantThreadExportFormat, AssistantThreadSubcommand, AuthSetArgs,
    AuthSubcommand, Cli, Commands, CompletionShell, EnrichSubcommand,
};
use crate::error::KagiError;
use crate::types::{
    AskPageRequest, AssistantPromptRequest, FastGptRequest, SearchResponse,
    SubscriberSummarizeRequest, SummarizeRequest,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), KagiError> {
    let cli = Cli::parse();

    if cli.generate_completion.is_some() && cli.command.is_some() {
        return Err(KagiError::Config(
            "--generate-completion cannot be used with a command".to_string(),
        ));
    }

    if let Some(shell) = cli.generate_completion {
        print_completion(shell);
        return Ok(());
    }

    match cli
        .command
        .ok_or_else(|| KagiError::Config("missing command".to_string()))?
    {
        Commands::Search(args) => {
            let request = search::SearchRequest::new(args.query);
            let request = if let Some(lens) = args.lens {
                request.with_lens(lens)
            } else {
                request
            };
            let format_str = match args.format {
                cli::OutputFormat::Json => "json",
                cli::OutputFormat::Pretty => "pretty",
                cli::OutputFormat::Compact => "compact",
                cli::OutputFormat::Markdown => "markdown",
                cli::OutputFormat::Csv => "csv",
            };
            run_search(request, format_str.to_string(), !args.no_color).await
        }
        Commands::Auth(auth) => match auth.command {
            AuthSubcommand::Status => run_auth_status(),
            AuthSubcommand::Check => run_auth_check().await,
            AuthSubcommand::Set(args) => run_auth_set(args),
        },
        Commands::Summarize(args) => {
            if args.subscriber {
                if args.engine.is_some() {
                    return Err(KagiError::Config(
                        "--engine is only supported for the paid public summarizer API".to_string(),
                    ));
                }
                if args.cache.is_some() {
                    return Err(KagiError::Config(
                        "--cache is only supported for the paid public summarizer API".to_string(),
                    ));
                }

                let request = SubscriberSummarizeRequest {
                    url: args.url,
                    text: args.text,
                    summary_type: args.summary_type,
                    target_language: args.target_language,
                    length: args.length,
                };
                let token = resolve_session_token()?;
                let response = execute_subscriber_summarize(&request, &token).await?;
                print_json(&response)
            } else {
                if args.length.is_some() {
                    return Err(KagiError::Config(
                        "--length requires --subscriber".to_string(),
                    ));
                }

                let request = SummarizeRequest {
                    url: args.url,
                    text: args.text,
                    engine: args.engine,
                    summary_type: args.summary_type,
                    target_language: args.target_language,
                    cache: args.cache,
                };
                let token = resolve_api_token()?;
                let response = execute_summarize(&request, &token).await?;
                print_json(&response)
            }
        }
        Commands::News(args) => {
            if args.list_categories {
                let response = execute_news_categories(&args.lang).await?;
                print_json(&response)
            } else if args.chaos {
                let response = execute_news_chaos(&args.lang).await?;
                print_json(&response)
            } else {
                let response = execute_news(&args.category, args.limit, &args.lang).await?;
                print_json(&response)
            }
        }
        Commands::Assistant(args) => {
            let token = resolve_session_token()?;
            if let Some(AssistantSubcommand::Thread(thread_args)) = args.command {
                match thread_args.command {
                    AssistantThreadSubcommand::List => {
                        let response = execute_assistant_thread_list(&token).await?;
                        print_json(&response)
                    }
                    AssistantThreadSubcommand::Get(thread) => {
                        let response =
                            execute_assistant_thread_get(&thread.thread_id, &token).await?;
                        print_json(&response)
                    }
                    AssistantThreadSubcommand::Delete(thread) => {
                        let response =
                            execute_assistant_thread_delete(&thread.thread_id, &token).await?;
                        print_json(&response)
                    }
                    AssistantThreadSubcommand::Export(export) => match export.format {
                        AssistantThreadExportFormat::Markdown => {
                            let response =
                                execute_assistant_thread_export(&export.thread_id, &token).await?;
                            println!("{}", response.markdown);
                            Ok(())
                        }
                        AssistantThreadExportFormat::Json => {
                            let response =
                                execute_assistant_thread_get(&export.thread_id, &token).await?;
                            print_json(&response)
                        }
                    },
                }
            } else {
                let query = args.query.ok_or_else(|| {
                    KagiError::Config(
                        "assistant prompt mode requires a QUERY unless a thread subcommand is used"
                            .to_string(),
                    )
                })?;
                let request = AssistantPromptRequest {
                    query,
                    thread_id: args.thread_id,
                    model: args.model,
                    lens_id: args.lens,
                    internet_access: match (args.web_access, args.no_web_access) {
                        (true, false) => Some(true),
                        (false, true) => Some(false),
                        _ => None,
                    },
                    personalizations: match (args.personalized, args.no_personalized) {
                        (true, false) => Some(true),
                        (false, true) => Some(false),
                        _ => None,
                    },
                };
                let response = execute_assistant_prompt(&request, &token).await?;
                print_json(&response)
            }
        }
        Commands::AskPage(args) => {
            let token = resolve_session_token()?;
            let request = AskPageRequest {
                url: args.url,
                question: args.question,
            };
            let response = execute_ask_page(&request, &token).await?;
            print_json(&response)
        }
        Commands::Fastgpt(args) => {
            let request = FastGptRequest {
                query: args.query,
                cache: args.cache,
                web_search: args.web_search,
            };
            let token = resolve_api_token()?;
            let response = execute_fastgpt(&request, &token).await?;
            print_json(&response)
        }
        Commands::Enrich(enrich) => {
            let token = resolve_api_token()?;
            let response = match enrich.command {
                EnrichSubcommand::Web(args) => execute_enrich_web(&args.query, &token).await?,
                EnrichSubcommand::News(args) => execute_enrich_news(&args.query, &token).await?,
            };
            print_json(&response)
        }
        Commands::Smallweb(args) => {
            let response = execute_smallweb(args.limit).await?;
            print_json(&response)
        }
        Commands::Batch(args) => {
            // Validate batch arguments
            args.validate().map_err(KagiError::Config)?;

            let format_str = match args.format {
                cli::OutputFormat::Json => "json",
                cli::OutputFormat::Pretty => "pretty",
                cli::OutputFormat::Compact => "compact",
                cli::OutputFormat::Markdown => "markdown",
                cli::OutputFormat::Csv => "csv",
            };
            run_batch_search(
                args.queries,
                args.concurrency,
                args.rate_limit,
                format_str.to_string(),
                !args.no_color,
                args.lens,
            )
            .await
        }
    }
}

fn print_completion(shell: CompletionShell) {
    let mut cmd = Cli::command();

    match shell {
        CompletionShell::Bash => generate(shells::Bash, &mut cmd, "kagi", &mut std::io::stdout()),
        CompletionShell::Zsh => generate(shells::Zsh, &mut cmd, "kagi", &mut std::io::stdout()),
        CompletionShell::Fish => generate(shells::Fish, &mut cmd, "kagi", &mut std::io::stdout()),
        CompletionShell::PowerShell => {
            generate(shells::PowerShell, &mut cmd, "kagi", &mut std::io::stdout())
        }
    }
}

fn run_auth_status() -> Result<(), KagiError> {
    let inventory = load_credential_inventory()?;
    println!("{}", format_status(&inventory));
    Ok(())
}

fn run_auth_set(args: AuthSetArgs) -> Result<(), KagiError> {
    let inventory = save_credentials(args.api_token.as_deref(), args.session_token.as_deref())?;
    println!("saved credentials to {}", inventory.config_path.display());
    println!("{}", format_status(&inventory));
    Ok(())
}

async fn run_auth_check() -> Result<(), KagiError> {
    let inventory = load_credential_inventory()?;
    let credentials = inventory.resolve_for_search(false)?;

    let request = search::SearchRequest::new("rust lang");
    let selected_kind = credentials.primary.kind;
    let selected_source = credentials.primary.source;
    execute_primary_search_request(&request, &credentials.primary).await?;

    println!(
        "auth check passed: {} ({})",
        selected_kind.as_str(),
        selected_source.as_str()
    );
    Ok(())
}

async fn execute_search_request(
    request: &search::SearchRequest,
    credentials: SearchCredentials,
) -> Result<SearchResponse, KagiError> {
    match execute_primary_search_request(request, &credentials.primary).await {
        Ok(response) => Ok(response),
        Err(api_error)
            if credentials.primary.kind == CredentialKind::ApiToken
                && should_fallback_to_session(&api_error) =>
        {
            let fallback = credentials.fallback_session.ok_or(api_error)?;
            search::execute_search(request, &fallback.value).await
        }
        Err(api_error) => Err(api_error),
    }
}

async fn execute_primary_search_request(
    request: &search::SearchRequest,
    credential: &Credential,
) -> Result<SearchResponse, KagiError> {
    match credential.kind {
        CredentialKind::ApiToken => search::execute_api_search(request, &credential.value).await,
        CredentialKind::SessionToken => search::execute_search(request, &credential.value).await,
    }
}

fn should_fallback_to_session(error: &KagiError) -> bool {
    matches!(error, KagiError::Auth(_))
}

fn resolve_api_token() -> Result<String, KagiError> {
    let inventory = load_credential_inventory()?;
    inventory
        .api_token
        .map(|credential| credential.value)
        .ok_or_else(|| {
            KagiError::Config(
                "this command requires KAGI_API_TOKEN (env or .kagi.toml [auth.api_token])"
                    .to_string(),
            )
        })
}

fn resolve_session_token() -> Result<String, KagiError> {
    let inventory = load_credential_inventory()?;
    inventory
        .session_token
        .map(|credential| credential.value)
        .ok_or_else(|| {
            KagiError::Config(
                "this command requires KAGI_SESSION_TOKEN (env or .kagi.toml [auth.session_token])"
                    .to_string(),
            )
        })
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), KagiError> {
    let output = serde_json::to_string_pretty(value)
        .map_err(|error| KagiError::Parse(format!("failed to serialize JSON output: {error}")))?;
    println!("{output}");
    Ok(())
}

async fn run_search(
    request: search::SearchRequest,
    format: String,
    use_color: bool,
) -> Result<(), KagiError> {
    let inventory = load_credential_inventory()?;
    let credentials = inventory.resolve_for_search(request.lens.is_some())?;

    let response = execute_search_request(&request, credentials).await?;
    let output = match format.as_str() {
        "pretty" => format_pretty_response(&response, use_color),
        "compact" => serde_json::to_string(&response).map_err(|error| {
            KagiError::Parse(format!("failed to serialize search response: {error}"))
        })?,
        "markdown" => format_markdown_response(&response),
        "csv" => format_csv_response(&response),
        _ => serde_json::to_string_pretty(&response).map_err(|error| {
            KagiError::Parse(format!("failed to serialize search response: {error}"))
        })?,
    };

    println!("{output}");
    Ok(())
}

fn format_pretty_response(response: &SearchResponse, use_color: bool) -> String {
    if response.data.is_empty() {
        return "No results found.".to_string();
    }

    response
        .data
        .iter()
        .enumerate()
        .map(|(index, result)| {
            let title_color = if use_color { "\x1b[1;34m" } else { "" };
            let url_color = if use_color { "\x1b[36m" } else { "" };
            let reset_color = if use_color { "\x1b[0m" } else { "" };

            let mut section = format!(
                "{}{}. {}{}\n   {}{}",
                title_color,
                index + 1,
                result.title,
                url_color,
                result.url,
                reset_color
            );
            if !result.snippet.trim().is_empty() {
                section.push_str(&format!("\n\n   {}", result.snippet.trim()));
            }
            section
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_markdown_response(response: &SearchResponse) -> String {
    if response.data.is_empty() {
        return "# No results found.".to_string();
    }

    response
        .data
        .iter()
        .enumerate()
        .map(|(index, result)| {
            let mut section = format!("## {}. [{}]({})\n\n", index + 1, result.title, result.url);
            if !result.snippet.trim().is_empty() {
                section.push_str(&format!("{}\n\n", result.snippet.trim()));
            }
            section
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_csv_field(field: &str) -> String {
    if field.contains('"') || field.contains(',') || field.contains('\n') || field.contains('\r') {
        let escaped = field.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        field.to_string()
    }
}

fn format_csv_response(response: &SearchResponse) -> String {
    if response.data.is_empty() {
        return "title,url,snippet".to_string();
    }

    let mut output = String::from("title,url,snippet\n");

    for result in &response.data {
        let title = escape_csv_field(&result.title);
        let url = escape_csv_field(&result.url);
        let snippet = escape_csv_field(&result.snippet);
        output.push_str(&format!("{},{},{}\n", title, url, snippet));
    }

    output
}

/// Simple rate limiter using token bucket algorithm
struct RateLimiter {
    capacity: u32,
    tokens: Arc<tokio::sync::Mutex<u32>>,
    last_refill: Arc<tokio::sync::Mutex<Instant>>,
    refill_rate: u32, // tokens per minute
}

impl RateLimiter {
    fn new(capacity: u32, refill_rate: u32) -> Self {
        Self {
            capacity,
            tokens: Arc::new(tokio::sync::Mutex::new(capacity)),
            last_refill: Arc::new(tokio::sync::Mutex::new(Instant::now())),
            refill_rate,
        }
    }

    async fn acquire(&self) -> Result<(), KagiError> {
        if self.refill_rate == 0 {
            return Err(KagiError::Config(
                "rate-limit must be at least 1".to_string(),
            ));
        }

        loop {
            let mut tokens = self.tokens.lock().await;
            let mut last_refill = self.last_refill.lock().await;

            let now = Instant::now();
            let elapsed = now.duration_since(*last_refill).as_secs_f64();
            let refill_interval = 60.0 / self.refill_rate as f64;
            let refill_tokens = (elapsed / refill_interval).floor() as u32;

            if refill_tokens > 0 {
                *tokens = (*tokens + refill_tokens).min(self.capacity);
                *last_refill += Duration::from_secs_f64(refill_tokens as f64 * refill_interval);
            }

            if *tokens > 0 {
                *tokens -= 1;
                return Ok(());
            }

            let elapsed_since_refill = Instant::now().duration_since(*last_refill).as_secs_f64();
            let seconds_to_wait = (refill_interval - elapsed_since_refill).max(0.001);

            drop(last_refill);
            drop(tokens);

            tokio::time::sleep(Duration::from_secs_f64(seconds_to_wait)).await;
        }
    }
}

async fn run_batch_search(
    queries: Vec<String>,
    concurrency: usize,
    rate_limit: u32,
    format: String,
    use_color: bool,
    lens: Option<String>,
) -> Result<(), KagiError> {
    let inventory = load_credential_inventory()?;
    let credentials = inventory.resolve_for_search(lens.is_some())?;

    let rate_limiter = Arc::new(RateLimiter::new(rate_limit, rate_limit));
    let semaphore = Arc::new(Semaphore::new(concurrency));

    let mut handles = vec![];

    for query in queries {
        let rate_limiter_clone = Arc::clone(&rate_limiter);
        let semaphore_clone = Arc::clone(&semaphore);
        let credentials_clone = credentials.clone();
        let lens_clone = lens.clone();
        let format_clone = format.clone();
        let query_clone = query.clone();

        let handle: tokio::task::JoinHandle<Result<(String, String), KagiError>> =
            tokio::spawn(async move {
                let _permit = semaphore_clone.acquire().await;
                rate_limiter_clone.acquire().await?;

                let request = search::SearchRequest::new(query_clone);
                let request = if let Some(lens) = lens_clone {
                    request.with_lens(lens)
                } else {
                    request
                };

                let response = execute_search_request(&request, credentials_clone).await?;

                let output = match format_clone.as_str() {
                    "pretty" => format_pretty_response(&response, use_color),
                    "compact" => serde_json::to_string(&response).map_err(|error| {
                        KagiError::Parse(format!("failed to serialize search response: {error}"))
                    })?,
                    "markdown" => format_markdown_response(&response),
                    "csv" => format_csv_response(&response),
                    _ => serde_json::to_string_pretty(&response).map_err(|error| {
                        KagiError::Parse(format!("failed to serialize search response: {error}"))
                    })?,
                };

                Ok((query, output))
            });

        handles.push(handle);
    }

    let mut results = vec![];
    let mut had_errors = false;

    for handle in handles {
        match handle.await {
            Ok(Ok((query, output))) => results.push((query, output)),
            Ok(Err(e)) => {
                eprintln!("Error processing query: {e}");
                had_errors = true;
            }
            Err(e) => {
                eprintln!("Task failed: {e}");
                had_errors = true;
            }
        }
    }

    if had_errors && (format == "json" || format == "compact") {
        // For machine-readable formats, exit with error code if any queries failed
        return Err(KagiError::Batch(
            "One or more batch queries failed".to_string(),
        ));
    }

    // Output results in order
    if format == "json" || format == "compact" {
        // For machine-readable formats, create a proper JSON envelope
        let queries: Vec<String> = results.iter().map(|(query, _)| query.clone()).collect();
        let mut results_json = serde_json::json!({
            "queries": queries,
            "results": []
        });

        let results_array = results_json["results"].as_array_mut().unwrap();

        for (query, output) in results {
            // Parse the individual JSON output and add to array
            let parsed: serde_json::Value = serde_json::from_str(&output).map_err(|e| {
                KagiError::Parse(format!(
                    "failed to parse batch result for '{}': {}",
                    query, e
                ))
            })?;
            results_array.push(parsed);
        }

        if format == "compact" {
            println!("{}", serde_json::to_string(&results_json)?);
        } else {
            println!("{}", serde_json::to_string_pretty(&results_json)?);
        }
    } else {
        // For human-readable formats, output with headers
        for (query, output) in results {
            println!("=== Results for: {} ===", query);
            println!("{}", output);
            println!();
        }
    }

    if had_errors {
        Err(KagiError::Batch(
            "One or more batch queries failed".to_string(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RateLimiter, format_csv_response, format_markdown_response, format_pretty_response,
        should_fallback_to_session,
    };
    use crate::error::KagiError;
    use crate::types::{SearchResponse, SearchResult};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn formats_pretty_output_for_results() {
        let response = SearchResponse {
            data: vec![
                SearchResult {
                    t: 0,
                    rank: None,
                    title: "Rust Programming Language".to_string(),
                    url: "https://www.rust-lang.org".to_string(),
                    snippet:
                        "A language empowering everyone to build reliable and efficient software."
                            .to_string(),
                    published: None,
                },
                SearchResult {
                    t: 0,
                    rank: None,
                    title: "The Rust Book".to_string(),
                    url: "https://doc.rust-lang.org/book/".to_string(),
                    snippet: "Learn Rust with the official book.".to_string(),
                    published: None,
                },
            ],
        };

        let output = format_pretty_response(&response, false);

        assert_eq!(
            output,
            "1. Rust Programming Language\n   https://www.rust-lang.org\n\n   A language empowering everyone to build reliable and efficient software.\n\n2. The Rust Book\n   https://doc.rust-lang.org/book/\n\n   Learn Rust with the official book."
        );
    }

    #[test]
    fn formats_pretty_output_for_empty_results() {
        let response = SearchResponse { data: vec![] };
        let output = format_pretty_response(&response, false);

        assert_eq!(output, "No results found.");
    }

    #[test]
    fn omits_blank_snippets_in_pretty_output() {
        let response = SearchResponse {
            data: vec![SearchResult {
                t: 0,
                rank: None,
                title: "Example".to_string(),
                url: "https://example.com".to_string(),
                snippet: "   ".to_string(),
                published: None,
            }],
        };

        let output = format_pretty_response(&response, false);

        assert_eq!(output, "1. Example\n   https://example.com");
    }

    #[test]
    fn formats_pretty_output_with_color() {
        let response = SearchResponse {
            data: vec![SearchResult {
                t: 0,
                rank: None,
                title: "Example".to_string(),
                url: "https://example.com".to_string(),
                snippet: "Test snippet".to_string(),
                published: None,
            }],
        };

        let output = format_pretty_response(&response, true);

        assert!(output.contains("\x1b[1;34m"));
        assert!(output.contains("\x1b[36m"));
        assert!(output.contains("\x1b[0m"));
    }

    #[tokio::test]
    async fn test_rate_limiter_basic_functionality() {
        let rate_limiter = RateLimiter::new(10, 60);

        // Should be able to acquire tokens up to capacity
        for _ in 0..10 {
            let result = rate_limiter.acquire().await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_refill() {
        let rate_limiter = RateLimiter::new(2, 60); // 2 tokens, 60 per minute

        // Acquire both tokens
        rate_limiter.acquire().await.unwrap();
        rate_limiter.acquire().await.unwrap();

        // Third acquisition should wait (but we can't easily test the wait in a test)
        // This just verifies it doesn't panic
        let result = rate_limiter.acquire().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_throttles_under_contention() {
        let rate_limiter = Arc::new(RateLimiter::new(1, 1200)); // 1 token capacity, 20 tokens/sec
        let start = Instant::now();

        let mut handles = Vec::new();
        for _ in 0..3 {
            let limiter = Arc::clone(&rate_limiter);
            handles.push(tokio::spawn(async move {
                limiter.acquire().await.unwrap();
                Instant::now()
            }));
        }

        let mut latest = start;
        for handle in handles {
            let acquired_at = handle.await.unwrap();
            if acquired_at > latest {
                latest = acquired_at;
            }
        }

        let elapsed = latest.duration_since(start);
        assert!(
            elapsed >= Duration::from_millis(95),
            "expected throttling to delay final acquisition by at least ~100ms, got {:?}",
            elapsed
        );
    }

    #[test]
    fn formats_markdown_output() {
        let response = SearchResponse {
            data: vec![SearchResult {
                t: 0,
                rank: None,
                title: "Rust Programming Language".to_string(),
                url: "https://www.rust-lang.org".to_string(),
                snippet: "A language empowering everyone to build reliable and efficient software."
                    .to_string(),
                published: None,
            }],
        };

        let output = format_markdown_response(&response);

        assert_eq!(
            output,
            "## 1. [Rust Programming Language](https://www.rust-lang.org)\n\nA language empowering everyone to build reliable and efficient software.\n\n"
        );
    }

    #[test]
    fn formats_csv_output() {
        let response = SearchResponse {
            data: vec![SearchResult {
                t: 0,
                rank: None,
                title: "Rust Programming Language".to_string(),
                url: "https://www.rust-lang.org".to_string(),
                snippet: "A language empowering everyone to build reliable and efficient software."
                    .to_string(),
                published: None,
            }],
        };

        let output = format_csv_response(&response);

        assert_eq!(
            output,
            "title,url,snippet\nRust Programming Language,https://www.rust-lang.org,A language empowering everyone to build reliable and efficient software.\n"
        );
    }

    #[test]
    fn formats_csv_output_with_escaping() {
        let response = SearchResponse {
            data: vec![SearchResult {
                t: 0,
                rank: None,
                title: "Rust, \"The Language\"".to_string(),
                url: "https://example.com/a,b".to_string(),
                snippet: "line 1\nline 2".to_string(),
                published: None,
            }],
        };

        let output = format_csv_response(&response);

        assert_eq!(
            output,
            "title,url,snippet\n\"Rust, \"\"The Language\"\"\",\"https://example.com/a,b\",\"line 1\nline 2\"\n"
        );
    }

    #[test]
    fn falls_back_for_any_search_api_auth_error() {
        assert!(should_fallback_to_session(&KagiError::Auth(
            "Kagi Search API request rejected: HTTP 400 Bad Request; Insufficient credit"
                .to_string(),
        )));
        assert!(should_fallback_to_session(&KagiError::Auth(
            "Kagi Search API request rejected: HTTP 403 Forbidden".to_string(),
        )));
        assert!(!should_fallback_to_session(&KagiError::Config(
            "missing credentials".to_string(),
        )));
        assert!(!should_fallback_to_session(&KagiError::Network(
            "request to Kagi timed out".to_string(),
        )));
    }
}
