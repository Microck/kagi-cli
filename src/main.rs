mod api;
mod auth;
mod cli;
mod error;
mod parser;
mod search;
mod types;

use clap::Parser;

use crate::api::{
    execute_assistant_prompt, execute_enrich_news, execute_enrich_web, execute_fastgpt,
    execute_news, execute_news_categories, execute_news_chaos, execute_smallweb,
    execute_subscriber_summarize, execute_summarize,
};
use crate::auth::{
    Credential, CredentialKind, SearchCredentials, format_status, load_credential_inventory,
    save_credentials,
};
use crate::cli::{AuthSetArgs, AuthSubcommand, Cli, Commands, EnrichSubcommand};
use crate::error::KagiError;
use crate::types::{
    AssistantPromptRequest, FastGptRequest, SearchResponse, SubscriberSummarizeRequest,
    SummarizeRequest,
};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), KagiError> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Search(args) => {
            let request = search::SearchRequest::new(args.query);
            let request = if let Some(lens) = args.lens {
                request.with_lens(lens)
            } else {
                request
            };
            run_search(request, args.pretty).await
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
            let request = AssistantPromptRequest {
                query: args.query,
                thread_id: args.thread_id,
            };
            let response = execute_assistant_prompt(&request, &token).await?;
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
    }
}

async fn run_search(request: search::SearchRequest, pretty: bool) -> Result<(), KagiError> {
    let inventory = load_credential_inventory()?;
    let credentials = inventory.resolve_for_search(request.lens.is_some())?;

    let response = execute_search_request(&request, credentials).await?;
    let output = if pretty {
        format_pretty_response(&response)
    } else {
        serde_json::to_string_pretty(&response).map_err(|error| {
            KagiError::Parse(format!("failed to serialize search response: {error}"))
        })?
    };

    println!("{output}");
    Ok(())
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

fn format_pretty_response(response: &SearchResponse) -> String {
    if response.data.is_empty() {
        return "No results found.".to_string();
    }

    response
        .data
        .iter()
        .enumerate()
        .map(|(index, result)| {
            let mut section = format!("{}. {}\n   {}", index + 1, result.title, result.url);
            if !result.snippet.trim().is_empty() {
                section.push_str(&format!("\n\n   {}", result.snippet.trim()));
            }
            section
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::{format_pretty_response, should_fallback_to_session};
    use crate::error::KagiError;
    use crate::types::{SearchResponse, SearchResult};

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

        let output = format_pretty_response(&response);

        assert_eq!(
            output,
            "1. Rust Programming Language\n   https://www.rust-lang.org\n\n   A language empowering everyone to build reliable and efficient software.\n\n2. The Rust Book\n   https://doc.rust-lang.org/book/\n\n   Learn Rust with the official book."
        );
    }

    #[test]
    fn formats_pretty_output_for_empty_results() {
        let response = SearchResponse { data: vec![] };
        let output = format_pretty_response(&response);

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

        let output = format_pretty_response(&response);

        assert_eq!(output, "1. Example\n   https://example.com");
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
