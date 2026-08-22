mod agent;
mod api;
mod auth;
mod auth_wizard;
mod cli;
mod error;
mod http;
mod local;
mod mcp_install;
mod parser;
mod quick;
mod search;
#[cfg(test)]
#[path = "test-support.rs"]
mod test_support;
mod types;
mod usage;

use clap::{CommandFactory, Parser};
use clap_complete::{generate, shells};

use crate::api::{
    NewsFilterRequest, execute_ask_page, execute_assistant_model_catalog, execute_assistant_prompt,
    execute_assistant_prompt_stream, execute_assistant_thread_delete,
    execute_assistant_thread_export, execute_assistant_thread_get, execute_assistant_thread_list,
    execute_custom_assistant_create, execute_custom_assistant_delete, execute_custom_assistant_get,
    execute_custom_assistant_list, execute_custom_assistant_update, execute_custom_bang_create,
    execute_custom_bang_delete, execute_custom_bang_get, execute_custom_bang_list,
    execute_custom_bang_update, execute_enrich_news, execute_enrich_web, execute_extract,
    execute_extract_response, execute_fastgpt, execute_lens_create, execute_lens_delete,
    execute_lens_get, execute_lens_list, execute_lens_set_enabled, execute_lens_update,
    execute_news, execute_news_categories, execute_news_chaos, execute_news_filter_presets,
    execute_redirect_create, execute_redirect_delete, execute_redirect_get, execute_redirect_list,
    execute_redirect_set_enabled, execute_redirect_update, execute_smallweb,
    execute_subscriber_summarize, execute_summarize, execute_translate,
};
use crate::auth::{
    Credential, CredentialKind, SearchAuthRequirement, SearchCredentials, format_status,
    load_credential_inventory_for_profile, save_credentials_for_profile,
};
use crate::auth_wizard::{run_auth_wizard, supports_interactive_auth, validate_credential};
use crate::cli::{
    AssistantCustomSubcommand, AssistantOutputFormat, AssistantReplArgs, AssistantStreamOutput,
    AssistantSubcommand, AssistantThreadExportFormat, AssistantThreadSubcommand, AuthSetArgs,
    AuthSubcommand, BangSubcommand, Cli, Commands, CompletionCommand, CompletionInstallArgs,
    CompletionShell, CompletionSubcommand, CustomBangSubcommand, EnrichSubcommand,
    ErrorOutputFormat, ExtractOutputFormat, HistorySubcommand, McpArgs, NewsFilterMode,
    NewsFilterScope, NotifyArgs, OutputFormat, QuickOutputFormat, SearchArgs, SearchOrder,
    SearchTime, SitePrefMode, SitePrefSubcommand, SkillsCommand, SkillsSubcommand, TranslateArgs,
    UsageOutputFormat, WatchArgs,
};
use crate::error::KagiError;
use crate::quick::{execute_quick, format_quick_markdown, format_quick_pretty};
use crate::types::{
    AskPageRequest, AssistantProfileCreateRequest, AssistantProfileUpdateRequest,
    AssistantPromptRequest, CustomBangCreateRequest, CustomBangUpdateRequest, FastGptRequest,
    LensCreateRequest, LensUpdateRequest, NewsSearchResponse, QuickResponse,
    RedirectRuleCreateRequest, RedirectRuleUpdateRequest, SearchResponse,
    SubscriberSummarizeRequest, SummarizeRequest, TranslateCommandRequest,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::future::Future;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;
use tracing::error;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone)]
struct SearchRequestOptions {
    snap: Option<String>,
    lens: Option<String>,
    region: Option<String>,
    time: Option<SearchTime>,
    from_date: Option<String>,
    to_date: Option<String>,
    limit: Option<usize>,
    order: Option<SearchOrder>,
    verbatim: bool,
    personalized: bool,
    no_personalized: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    init_tracing();
    let error_format = selected_error_output_format();
    if let Err(error) = run().await {
        print_kagi_error(&error, error_format);
        std::process::exit(1);
    }
}

fn selected_error_output_format() -> ErrorOutputFormat {
    error_output_format_from_args(env::args_os())
        .or_else(|| {
            env::var("KAGI_ERROR_FORMAT")
                .ok()
                .and_then(|value| parse_error_output_format(&value))
        })
        .unwrap_or(ErrorOutputFormat::Text)
}

fn error_output_format_from_args<I>(args: I) -> Option<ErrorOutputFormat>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter().skip(1);
    while let Some(arg) = args.next() {
        let arg = arg.to_string_lossy();
        if arg == "--" {
            break;
        }
        if let Some(value) = arg.strip_prefix("--error-format=") {
            return parse_error_output_format(value);
        }
        if arg == "--error-format" {
            return args
                .next()
                .and_then(|value| parse_error_output_format(&value.to_string_lossy()));
        }
    }

    None
}

fn parse_error_output_format(value: &str) -> Option<ErrorOutputFormat> {
    match value.trim().to_ascii_lowercase().as_str() {
        "json" => Some(ErrorOutputFormat::Json),
        "text" => Some(ErrorOutputFormat::Text),
        _ => None,
    }
}

fn print_kagi_error(error: &KagiError, format: ErrorOutputFormat) {
    match format {
        ErrorOutputFormat::Text => eprintln!("{error}"),
        ErrorOutputFormat::Json => match serde_json::to_string(&error_envelope(error)) {
            Ok(output) => eprintln!("{output}"),
            Err(_) => eprintln!("{error}"),
        },
    }
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    code: &'static str,
    category: &'static str,
    retryable: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_auth: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_status: Option<u16>,
    suggested_commands: Vec<&'static str>,
    docs_url: &'static str,
}

fn error_envelope(error: &KagiError) -> ErrorEnvelope {
    let (code, category, retryable, detail) = match error {
        KagiError::Network(message) => ("network_error", "network", true, message.as_str()),
        KagiError::Auth(message) => ("authentication_error", "auth", false, message.as_str()),
        KagiError::Parse(message) => ("parse_error", "parse", false, message.as_str()),
        KagiError::Config(message) if message.starts_with("assistant contract") => {
            ("contract_error", "contract", false, message.as_str())
        }
        KagiError::Config(message) if message.contains("missing credentials") => (
            "missing_credentials",
            "configuration",
            false,
            message.as_str(),
        ),
        KagiError::Config(message) => (
            "configuration_error",
            "configuration",
            false,
            message.as_str(),
        ),
        KagiError::Batch(message) => ("batch_error", "batch", false, message.as_str()),
    };
    let required_auth = required_auth_for_message(detail);

    ErrorEnvelope {
        code,
        category,
        retryable,
        message: error.to_string(),
        required_auth,
        http_status: extract_http_status(detail),
        suggested_commands: suggested_commands_for_error(category, required_auth),
        docs_url: "https://kagi.micr.dev/reference/error-reference",
    }
}

fn required_auth_for_message(message: &str) -> Option<&'static str> {
    if message.contains("missing credentials") {
        Some("KAGI_API_KEY or KAGI_SESSION_TOKEN")
    } else if message.contains("KAGI_API_KEY") {
        Some("KAGI_API_KEY")
    } else if message.contains("KAGI_API_TOKEN") {
        Some("KAGI_API_TOKEN")
    } else if message.contains("KAGI_SESSION_TOKEN") {
        Some("KAGI_SESSION_TOKEN")
    } else {
        None
    }
}

fn suggested_commands_for_error(
    category: &str,
    required_auth: Option<&'static str>,
) -> Vec<&'static str> {
    match required_auth {
        Some("KAGI_API_KEY") => vec![
            "kagi auth status",
            "kagi auth set --api-key <key>",
            "kagi auth check",
        ],
        Some("KAGI_API_TOKEN") => vec![
            "kagi auth status",
            "kagi auth set --api-token <token>",
            "kagi auth check",
        ],
        Some("KAGI_SESSION_TOKEN") => vec![
            "kagi auth status",
            "kagi auth set --session-token <token>",
            "kagi auth check",
        ],
        Some("KAGI_API_KEY or KAGI_SESSION_TOKEN") => vec![
            "kagi auth status",
            "kagi auth set --api-key <key>",
            "kagi auth set --session-token <token>",
        ],
        _ if category == "auth" => vec!["kagi auth status", "kagi auth check"],
        _ => Vec::new(),
    }
}

fn extract_http_status(message: &str) -> Option<u16> {
    message.split("HTTP ").nth(1).and_then(|suffix| {
        suffix
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>()
            .parse::<u16>()
            .ok()
    })
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .without_time()
        .with_writer(std::io::stderr)
        .try_init();
}

async fn run() -> Result<(), KagiError> {
    if is_bare_auth_invocation() {
        if supports_interactive_auth() {
            return run_auth_wizard().await;
        }

        return Err(KagiError::Config(
            "`kagi auth` needs an interactive terminal. Use `kagi auth set`, `kagi auth status`, or `kagi auth check` in non-interactive environments"
                .to_string(),
        ));
    }

    let cli = Cli::parse();

    if cli.generate_completion.is_some() && cli.command.is_some() {
        return Err(KagiError::Config(
            "completion was not generated because --generate-completion cannot be used with a command. Run `kagi --generate-completion <shell>` by itself".to_string(),
        ));
    }

    if let Some(shell) = cli.generate_completion {
        print_completion(shell);
        return Ok(());
    }
    let profile = cli.profile;

    match cli.command.ok_or_else(|| {
        KagiError::Config(
            "missing command. Run `kagi --help` to see available commands".to_string(),
        )
    })? {
        Commands::Search(args) => {
            args.validate().map_err(KagiError::Config)?;

            if args.news {
                args.validate_news_search().map_err(KagiError::Config)?;
                let token = resolve_session_token(profile.as_deref())?;
                let request = build_news_search_request(&args);
                let response = cached_json(
                    args.local_cache,
                    args.cache_ttl.unwrap_or(900),
                    "news-search",
                    &request,
                    || async { search::execute_news_search(&request, &token).await },
                )
                .await?;
                return print_news_search(&response, &args.format, !args.no_color);
            }

            let mut options = SearchRequestOptions {
                snap: args.snap,
                lens: args.lens,
                region: args.region,
                time: args.time,
                from_date: args.from_date,
                to_date: args.to_date,
                limit: args.limit,
                order: args.order,
                verbatim: args.verbatim,
                personalized: args.personalized,
                no_personalized: args.no_personalized,
            };
            options.lens = resolve_search_lens_option(options.lens, profile.as_deref()).await?;
            let request = build_search_request(args.query, &options);
            let format_str = args.format.to_string();
            if let Some(follow_count) = args.follow {
                run_search_follow(request, follow_count, args.limit, profile.as_deref()).await
            } else {
                run_search(
                    request,
                    format_str,
                    !args.no_color,
                    args.template,
                    args.local_cache,
                    args.cache_ttl.unwrap_or(900),
                    args.limit,
                    profile.as_deref(),
                )
                .await
            }
        }
        Commands::Auth(auth) => match auth.command {
            AuthSubcommand::Status => run_auth_status(profile.as_deref()),
            AuthSubcommand::Check => run_auth_check(profile.as_deref()).await,
            AuthSubcommand::Set(args) => run_auth_set(args, profile.as_deref()),
        },
        Commands::Usage(args) => {
            let token = resolve_session_token(profile.as_deref())?;
            let report = usage::execute_usage(&token).await?;
            match args.format {
                UsageOutputFormat::Json => print_json(&report),
                UsageOutputFormat::Compact => print_compact_json(&report),
                UsageOutputFormat::Pretty => {
                    println!("{}", usage::format_pretty(&report));
                    Ok(())
                }
            }
        }
        Commands::Agent => {
            let content = agent::skill_content(agent::KAGI_SKILL).ok_or_else(|| {
                KagiError::Config("embedded kagi skill is unavailable".to_string())
            })?;
            println!("{content}");
            Ok(())
        }
        Commands::Skills(args) => run_skills(args),
        Commands::Completion(args) => run_completion(args),
        Commands::Summarize(args) => {
            args.validate().map_err(KagiError::Config)?;

            if args.filter {
                return run_summarize_filter(args, profile.as_deref()).await;
            }

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
                let token = resolve_session_token(profile.as_deref())?;
                let response = cached_json(
                    args.local_cache,
                    args.cache_ttl.unwrap_or(3600),
                    "subscriber-summarize",
                    &request,
                    || async { execute_subscriber_summarize(&request, &token).await },
                )
                .await?;
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
                let token = resolve_api_token(profile.as_deref())?;
                let response = cached_json(
                    args.local_cache,
                    args.cache_ttl.unwrap_or(3600),
                    "summarize",
                    &request,
                    || async { execute_summarize(&request, &token).await },
                )
                .await?;
                print_json(&response)
            }
        }
        Commands::Extract(args) => {
            args.validate().map_err(KagiError::Config)?;

            if args.filter {
                return run_extract_filter(profile.as_deref()).await;
            }

            let url = args.url.as_deref().ok_or_else(|| {
                KagiError::Config(
                    "extract requires a URL, or use --filter to read URLs from stdin".to_string(),
                )
            })?;
            match args.format {
                ExtractOutputFormat::Markdown => {
                    let markdown =
                        execute_extract_with_available_auth(url, profile.as_deref()).await?;
                    println!("{markdown}");
                    Ok(())
                }
                ExtractOutputFormat::Json => {
                    let response =
                        execute_extract_response_with_available_auth(url, profile.as_deref())
                            .await?;
                    print_json(&response)
                }
                ExtractOutputFormat::Compact => {
                    let response =
                        execute_extract_response_with_available_auth(url, profile.as_deref())
                            .await?;
                    print_compact_json(&response)
                }
            }
        }
        Commands::News(args) => {
            args.validate().map_err(KagiError::Config)?;

            if args.list_categories {
                let response = execute_news_categories(&args.lang).await?;
                print_json(&response)
            } else if args.chaos {
                let response = execute_news_chaos(&args.lang).await?;
                print_json(&response)
            } else if args.list_filter_presets {
                let response = execute_news_filter_presets(&args.lang)?;
                print_json(&response)
            } else {
                let filter_request = args.has_filter_inputs().then(|| NewsFilterRequest {
                    preset_ids: args.filter_preset.clone(),
                    keywords: args.filter_keyword.clone(),
                    mode: args.filter_mode,
                    scope: args.filter_scope,
                });
                let response = execute_news(
                    &args.category,
                    args.limit,
                    &args.lang,
                    filter_request.as_ref(),
                )
                .await?;
                print_json(&response)
            }
        }
        Commands::Assistant(args) => {
            let token = resolve_session_token(profile.as_deref())?;
            if let Some(subcommand) = args.command {
                match subcommand {
                    AssistantSubcommand::Thread(thread_args) => match thread_args.command {
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
                                    execute_assistant_thread_export(&export.thread_id, &token)
                                        .await?;
                                println!("{}", response.markdown);
                                Ok(())
                            }
                            AssistantThreadExportFormat::Json => {
                                let response =
                                    execute_assistant_thread_get(&export.thread_id, &token).await?;
                                print_json(&response)
                            }
                        },
                    },
                    AssistantSubcommand::Models => {
                        let response = execute_assistant_model_catalog(&token).await?;
                        print_json(&response)
                    }
                    AssistantSubcommand::Repl(repl_args) => {
                        run_assistant_repl(repl_args, &token).await
                    }
                    AssistantSubcommand::Custom(custom_args) => match custom_args.command {
                        AssistantCustomSubcommand::List => {
                            let response = execute_custom_assistant_list(&token).await?;
                            print_json(&response)
                        }
                        AssistantCustomSubcommand::Get(target) => {
                            let response =
                                execute_custom_assistant_get(&target.target, &token).await?;
                            print_json(&response)
                        }
                        AssistantCustomSubcommand::Create(create) => {
                            let response = execute_custom_assistant_create(
                                &AssistantProfileCreateRequest {
                                    name: create.name,
                                    bang_trigger: normalize_optional_string(create.bang_trigger),
                                    internet_access: bool_flag_choice(
                                        create.web_access,
                                        create.no_web_access,
                                    ),
                                    selected_lens: normalize_optional_string(create.lens),
                                    personalizations: bool_flag_choice(
                                        create.personalized,
                                        create.no_personalized,
                                    ),
                                    base_model: normalize_optional_string(create.model),
                                    custom_instructions: create.instructions,
                                },
                                &token,
                            )
                            .await?;
                            print_json(&response)
                        }
                        AssistantCustomSubcommand::Update(update) => {
                            let response = execute_custom_assistant_update(
                                &AssistantProfileUpdateRequest {
                                    target: update.target,
                                    name: normalize_optional_string(update.name),
                                    bang_trigger: normalize_optional_string(update.bang_trigger),
                                    internet_access: bool_flag_choice(
                                        update.web_access,
                                        update.no_web_access,
                                    ),
                                    selected_lens: normalize_optional_string(update.lens),
                                    personalizations: bool_flag_choice(
                                        update.personalized,
                                        update.no_personalized,
                                    ),
                                    base_model: normalize_optional_string(update.model),
                                    custom_instructions: update.instructions,
                                },
                                &token,
                            )
                            .await?;
                            print_json(&response)
                        }
                        AssistantCustomSubcommand::Delete(target) => {
                            let response =
                                execute_custom_assistant_delete(&target.target, &token).await?;
                            print_json(&response)
                        }
                    },
                }
            } else {
                let contract = load_assistant_contract(
                    args.contract.as_deref(),
                    args.contract_file.as_deref(),
                )?;
                if contract.is_some() {
                    validate_assistant_contract_output_format(args.format.clone())?;
                }
                let mut query = read_assistant_prompt_query(args.query)?;
                if let Some(contract) = contract.as_ref() {
                    query = contract_prompt_query(&query, contract);
                }
                let request = AssistantPromptRequest {
                    query,
                    thread_id: args.thread_id,
                    attachments: args.attach,
                    profile_id: normalize_optional_string(args.assistant),
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
                if let Some(contract) = contract.as_ref() {
                    let response = execute_assistant_prompt_for_args(
                        &request,
                        args.once,
                        args.stream.then_some(args.stream_output),
                        &token,
                    )
                    .await?;
                    let value = match validate_assistant_contract_response(contract, &response) {
                        Ok(value) => value,
                        Err(first_error) => {
                            let mut repair_request = request.clone();
                            repair_request.thread_id = Some(response.thread.id.clone());
                            repair_request.query = contract_repair_query(
                                contract,
                                assistant_message_content(&response),
                                &first_error,
                            );
                            let repaired = execute_assistant_prompt_for_args(
                                &repair_request,
                                args.once,
                                None,
                                &token,
                            )
                            .await?;
                            validate_assistant_contract_response(contract, &repaired).map_err(
                                |second_error| {
                                    KagiError::Config(format!(
                                        "assistant contract '{}' was not satisfied after one repair attempt: {second_error}",
                                        contract.name
                                    ))
                                },
                            )?
                        }
                    };
                    print_assistant_contract_value(&value, args.format)
                } else if args.once {
                    let response = execute_once_assistant_prompt(
                        &request,
                        args.stream.then_some(args.stream_output),
                        &token,
                    )
                    .await?;
                    if args.stream {
                        Ok(())
                    } else {
                        print_assistant_response(&response, args.format, !args.no_color)
                    }
                } else if args.stream {
                    execute_streaming_assistant_prompt(&request, &token, args.stream_output)
                        .await?;
                    Ok(())
                } else {
                    let response = execute_assistant_prompt(&request, &token).await?;
                    print_assistant_response(&response, args.format, !args.no_color)
                }
            }
        }
        Commands::AskPage(args) => {
            let token = resolve_session_token(profile.as_deref())?;
            let request = AskPageRequest {
                url: args.url,
                question: args.question,
            };
            let response = execute_ask_page(&request, &token).await?;
            print_json(&response)
        }
        Commands::Quick(args) => {
            let token = resolve_session_token(profile.as_deref())?;
            let request = search::SearchRequest::new(args.query.trim().to_string());
            let request = if let Some(lens) = args.lens {
                request.with_lens(lens)
            } else {
                request
            };
            let format_str = args.format.to_string();
            let response = cached_json(
                args.local_cache,
                args.cache_ttl.unwrap_or(900),
                "quick",
                &request,
                || async { execute_quick(&request, &token).await },
            )
            .await?;
            print_quick_response(&response, &format_str, !args.no_color)
        }
        Commands::Translate(args) => {
            let token = resolve_session_token(profile.as_deref())?;
            let request = build_translate_request(*args)?;
            let response = execute_translate(&request, &token).await?;
            print_json(&response)
        }
        Commands::Fastgpt(args) => {
            let request = FastGptRequest {
                query: args.query,
                cache: args.cache,
                web_search: args.web_search,
            };
            let token = resolve_api_token(profile.as_deref())?;
            let response = cached_json(
                args.local_cache,
                args.cache_ttl.unwrap_or(3600),
                "fastgpt",
                &request,
                || async { execute_fastgpt(&request, &token).await },
            )
            .await?;
            print_json(&response)
        }
        Commands::Enrich(enrich) => {
            let token = resolve_api_token(profile.as_deref())?;
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
        Commands::Watch(args) => run_watch(args, profile.as_deref()).await,
        Commands::Mcp(args) => {
            if let Some(command) = args.command.clone() {
                let setup_args = match command {
                    cli::McpSubcommand::Install(setup_args)
                    | cli::McpSubcommand::Setup(setup_args)
                    | cli::McpSubcommand::Auth(setup_args) => setup_args,
                };
                return mcp_install::run_mcp_setup(setup_args);
            }

            run_mcp(args, profile.as_deref()).await
        }
        Commands::Notify(args) => run_notify(args, profile.as_deref()).await,
        Commands::History(command) => run_history(command.command),
        Commands::SitePref(command) => run_site_pref(command.command),
        Commands::Lens(command) => {
            let token = resolve_session_token(profile.as_deref())?;
            match command.command {
                cli::LensSubcommand::List => {
                    let response = execute_lens_list(&token).await?;
                    print_json(&response)
                }
                cli::LensSubcommand::Get(target) => {
                    let response = execute_lens_get(&target.target, &token).await?;
                    print_json(&response)
                }
                cli::LensSubcommand::Create(create) => {
                    let response = execute_lens_create(
                        &LensCreateRequest {
                            name: create.name,
                            included_sites: normalize_optional_string(create.included_sites),
                            included_keywords: normalize_optional_string(create.included_keywords),
                            description: create.description,
                            search_region: normalize_optional_string(create.region),
                            before_time: normalize_optional_string(create.before_date),
                            after_time: normalize_optional_string(create.after_date),
                            excluded_sites: normalize_optional_string(create.excluded_sites),
                            excluded_keywords: normalize_optional_string(create.excluded_keywords),
                            shortcut_keyword: normalize_optional_string(create.shortcut),
                            autocomplete_keywords: bool_flag_choice(
                                create.autocomplete_keywords,
                                create.no_autocomplete_keywords,
                            ),
                            template: create
                                .template
                                .map(|value| value.as_form_value().to_string()),
                            file_type: normalize_optional_string(create.file_type),
                            share_with_team: bool_flag_choice(
                                create.share_with_team,
                                create.no_share_with_team,
                            ),
                            share_copy_code: bool_flag_choice(
                                create.share_copy_code,
                                create.no_share_copy_code,
                            ),
                        },
                        &token,
                    )
                    .await?;
                    print_json(&response)
                }
                cli::LensSubcommand::Update(update) => {
                    let response = execute_lens_update(
                        &LensUpdateRequest {
                            target: update.target,
                            name: normalize_optional_string(update.name),
                            included_sites: normalize_optional_string(update.included_sites),
                            included_keywords: normalize_optional_string(update.included_keywords),
                            description: update.description,
                            search_region: normalize_optional_string(update.region),
                            before_time: normalize_optional_string(update.before_date),
                            after_time: normalize_optional_string(update.after_date),
                            excluded_sites: normalize_optional_string(update.excluded_sites),
                            excluded_keywords: normalize_optional_string(update.excluded_keywords),
                            shortcut_keyword: normalize_optional_string(update.shortcut),
                            autocomplete_keywords: bool_flag_choice(
                                update.autocomplete_keywords,
                                update.no_autocomplete_keywords,
                            ),
                            template: update
                                .template
                                .map(|value| value.as_form_value().to_string()),
                            file_type: normalize_optional_string(update.file_type),
                            share_with_team: bool_flag_choice(
                                update.share_with_team,
                                update.no_share_with_team,
                            ),
                            share_copy_code: bool_flag_choice(
                                update.share_copy_code,
                                update.no_share_copy_code,
                            ),
                        },
                        &token,
                    )
                    .await?;
                    print_json(&response)
                }
                cli::LensSubcommand::Delete(target) => {
                    let response = execute_lens_delete(&target.target, &token).await?;
                    print_json(&response)
                }
                cli::LensSubcommand::Enable(target) => {
                    let response = execute_lens_set_enabled(&target.target, true, &token).await?;
                    print_json(&response)
                }
                cli::LensSubcommand::Disable(target) => {
                    let response = execute_lens_set_enabled(&target.target, false, &token).await?;
                    print_json(&response)
                }
            }
        }
        Commands::Bang(command) => {
            let token = resolve_session_token(profile.as_deref())?;
            match command.command {
                BangSubcommand::Custom(custom) => match custom.command {
                    CustomBangSubcommand::List => {
                        let response = execute_custom_bang_list(&token).await?;
                        print_json(&response)
                    }
                    CustomBangSubcommand::Get(target) => {
                        let response = execute_custom_bang_get(&target.target, &token).await?;
                        print_json(&response)
                    }
                    CustomBangSubcommand::Create(create) => {
                        let response = execute_custom_bang_create(
                            &CustomBangCreateRequest {
                                name: create.name,
                                trigger: create.trigger,
                                template: normalize_optional_string(create.template),
                                snap_domain: normalize_optional_string(create.snap_domain),
                                regex_pattern: create.regex_pattern,
                                shortcut_menu: bool_flag_choice(
                                    create.shortcut_menu,
                                    create.no_shortcut_menu,
                                ),
                                fmt_open_snap_domain: bool_flag_choice(
                                    create.open_snap_domain,
                                    create.no_open_snap_domain,
                                ),
                                fmt_open_base_path: bool_flag_choice(
                                    create.open_base_path,
                                    create.no_open_base_path,
                                ),
                                fmt_url_encode_placeholder: bool_flag_choice(
                                    create.encode_placeholder,
                                    create.no_encode_placeholder,
                                ),
                                fmt_url_encode_space_to_plus: bool_flag_choice(
                                    create.plus_for_space,
                                    create.no_plus_for_space,
                                ),
                            },
                            &token,
                        )
                        .await?;
                        print_json(&response)
                    }
                    CustomBangSubcommand::Update(update) => {
                        let response = execute_custom_bang_update(
                            &CustomBangUpdateRequest {
                                target: update.target,
                                name: normalize_optional_string(update.name),
                                trigger: normalize_optional_string(update.trigger),
                                template: normalize_optional_string(update.template),
                                snap_domain: normalize_optional_string(update.snap_domain),
                                regex_pattern: update.regex_pattern,
                                shortcut_menu: bool_flag_choice(
                                    update.shortcut_menu,
                                    update.no_shortcut_menu,
                                ),
                                fmt_open_snap_domain: bool_flag_choice(
                                    update.open_snap_domain,
                                    update.no_open_snap_domain,
                                ),
                                fmt_open_base_path: bool_flag_choice(
                                    update.open_base_path,
                                    update.no_open_base_path,
                                ),
                                fmt_url_encode_placeholder: bool_flag_choice(
                                    update.encode_placeholder,
                                    update.no_encode_placeholder,
                                ),
                                fmt_url_encode_space_to_plus: bool_flag_choice(
                                    update.plus_for_space,
                                    update.no_plus_for_space,
                                ),
                            },
                            &token,
                        )
                        .await?;
                        print_json(&response)
                    }
                    CustomBangSubcommand::Delete(target) => {
                        let response = execute_custom_bang_delete(&target.target, &token).await?;
                        print_json(&response)
                    }
                },
            }
        }
        Commands::Redirect(command) => {
            let token = resolve_session_token(profile.as_deref())?;
            match command.command {
                cli::RedirectSubcommand::List => {
                    let response = execute_redirect_list(&token).await?;
                    print_json(&response)
                }
                cli::RedirectSubcommand::Get(target) => {
                    let response = execute_redirect_get(&target.target, &token).await?;
                    print_json(&response)
                }
                cli::RedirectSubcommand::Create(create) => {
                    let response = execute_redirect_create(
                        &RedirectRuleCreateRequest { rule: create.rule },
                        &token,
                    )
                    .await?;
                    print_json(&response)
                }
                cli::RedirectSubcommand::Update(update) => {
                    let response = execute_redirect_update(
                        &RedirectRuleUpdateRequest {
                            target: update.target,
                            rule: update.rule,
                        },
                        &token,
                    )
                    .await?;
                    print_json(&response)
                }
                cli::RedirectSubcommand::Delete(target) => {
                    let response = execute_redirect_delete(&target.target, &token).await?;
                    print_json(&response)
                }
                cli::RedirectSubcommand::Enable(target) => {
                    let response =
                        execute_redirect_set_enabled(&target.target, true, &token).await?;
                    print_json(&response)
                }
                cli::RedirectSubcommand::Disable(target) => {
                    let response =
                        execute_redirect_set_enabled(&target.target, false, &token).await?;
                    print_json(&response)
                }
            }
        }
        Commands::Batch(mut args) => {
            if args.queries.is_empty() {
                args.queries = read_stdin_lines()?;
            }
            args.validate().map_err(KagiError::Config)?;

            let format_str = args.format.to_string();
            run_batch_search(BatchSearchConfig {
                queries: args.queries,
                concurrency: args.concurrency,
                rate_limit: args.rate_limit,
                format: format_str,
                use_color: !args.no_color,
                options: SearchRequestOptions {
                    snap: args.snap,
                    lens: args.lens,
                    region: args.region,
                    time: args.time,
                    from_date: args.from_date,
                    to_date: args.to_date,
                    limit: args.limit,
                    order: args.order,
                    verbatim: args.verbatim,
                    personalized: args.personalized,
                    no_personalized: args.no_personalized,
                },
                template: args.template,
                limit: args.limit,
                profile: profile.as_deref(),
            })
            .await
        }
    }
}

fn run_skills(args: SkillsCommand) -> Result<(), KagiError> {
    match args.command.unwrap_or(SkillsSubcommand::List) {
        SkillsSubcommand::List => {
            for skill in agent::skills() {
                println!("  {:<20} {}", skill.name, skill.description);
            }
            Ok(())
        }
        SkillsSubcommand::Get(args) => {
            let content = if args.full {
                agent::skill_full_content(&args.name)
            } else {
                agent::skill_content(&args.name)
            }
            .ok_or_else(|| {
                KagiError::Config(format!(
                    "unknown skill `{}`; run `kagi skills list` to see available skills",
                    args.name
                ))
            })?;
            println!("{content}");
            Ok(())
        }
        SkillsSubcommand::Path(args) => {
            let locator = agent::skill_locator(args.name.as_deref()).ok_or_else(|| {
                KagiError::Config(format!(
                    "unknown skill `{}`; run `kagi skills list` to see available skills",
                    args.name.as_deref().unwrap_or_default()
                ))
            })?;
            println!("{locator}");
            Ok(())
        }
    }
}

fn is_bare_auth_invocation() -> bool {
    let args: Vec<String> = env::args().collect();
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    is_bare_auth_invocation_from(&arg_refs)
}

fn is_bare_auth_invocation_from(args: &[&str]) -> bool {
    args.len() == 2 && args[1] == "auth"
}

fn print_completion(shell: CompletionShell) {
    let output = completion_script(shell);
    print!("{output}");
}

fn run_completion(args: CompletionCommand) -> Result<(), KagiError> {
    match args.command {
        CompletionSubcommand::Generate(args) => {
            print_completion(args.shell);
            Ok(())
        }
        CompletionSubcommand::Install(args) => install_completion(args),
    }
}

fn completion_script(shell: CompletionShell) -> String {
    let mut cmd = Cli::command();
    let mut buffer = Vec::new();

    match shell {
        CompletionShell::Bash => generate(shells::Bash, &mut cmd, "kagi", &mut buffer),
        CompletionShell::Zsh => generate(shells::Zsh, &mut cmd, "kagi", &mut buffer),
        CompletionShell::Fish => generate(shells::Fish, &mut cmd, "kagi", &mut buffer),
        CompletionShell::PowerShell => {
            generate(shells::PowerShell, &mut cmd, "kagi", &mut buffer);
        }
    }

    String::from_utf8(buffer).expect("clap completion scripts are valid UTF-8")
}

fn install_completion(args: CompletionInstallArgs) -> Result<(), KagiError> {
    let shell = args.shell.or_else(detect_completion_shell).ok_or_else(|| {
        KagiError::Config(
            "could not detect shell; rerun with `kagi completion install --shell <bash|zsh|fish|powershell>`"
                .to_string(),
        )
    })?;
    let target_dir = args.dir.unwrap_or_else(|| default_completion_dir(&shell));
    let target_path = target_dir.join(completion_filename(&shell));

    fs::create_dir_all(&target_dir).map_err(|error| {
        KagiError::Config(format!(
            "failed to create completion directory {}: {error}",
            target_dir.display()
        ))
    })?;
    fs::write(&target_path, completion_script(shell.clone())).map_err(|error| {
        KagiError::Config(format!(
            "failed to write completion file {}: {error}",
            target_path.display()
        ))
    })?;

    maybe_update_zshrc(&shell, &target_dir)?;
    maybe_update_powershell_profile(&shell, &target_path)?;

    println!(
        "installed {shell_name} completions to {}",
        target_path.display(),
        shell_name = completion_shell_name(&shell)
    );
    Ok(())
}

fn detect_completion_shell() -> Option<CompletionShell> {
    let shell = env::var("SHELL")
        .ok()
        .or_else(|| env::var("ComSpec").ok())
        .or_else(|| env::var("PSModulePath").ok())?;
    let name = Path::new(&shell)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(shell.as_str())
        .to_ascii_lowercase();

    match name.as_str() {
        "bash" => Some(CompletionShell::Bash),
        "zsh" => Some(CompletionShell::Zsh),
        "fish" => Some(CompletionShell::Fish),
        "pwsh" | "powershell" | "powershell.exe" | "pwsh.exe" => Some(CompletionShell::PowerShell),
        _ => None,
    }
}

fn default_completion_dir(shell: &CompletionShell) -> PathBuf {
    match shell {
        CompletionShell::Bash => xdg_data_home().join("bash-completion").join("completions"),
        CompletionShell::Zsh => home_dir().join(".zsh").join("completions"),
        CompletionShell::Fish => xdg_config_home().join("fish").join("completions"),
        CompletionShell::PowerShell => xdg_config_home().join("powershell"),
    }
}

fn completion_filename(shell: &CompletionShell) -> &'static str {
    match shell {
        CompletionShell::Bash => "kagi",
        CompletionShell::Zsh => "_kagi",
        CompletionShell::Fish => "kagi.fish",
        CompletionShell::PowerShell => "kagi-completions.ps1",
    }
}

fn completion_shell_name(shell: &CompletionShell) -> &'static str {
    match shell {
        CompletionShell::Bash => "bash",
        CompletionShell::Zsh => "zsh",
        CompletionShell::Fish => "fish",
        CompletionShell::PowerShell => "powershell",
    }
}

fn maybe_update_zshrc(shell: &CompletionShell, target_dir: &Path) -> Result<(), KagiError> {
    if !matches!(shell, CompletionShell::Zsh) {
        return Ok(());
    }

    let zshrc = home_dir().join(".zshrc");
    let line = format!("fpath=({} $fpath)", target_dir.display());
    append_line_if_missing(&zshrc, &line)?;
    append_line_if_missing(&zshrc, "autoload -Uz compinit && compinit")?;
    Ok(())
}

fn maybe_update_powershell_profile(
    shell: &CompletionShell,
    target_path: &Path,
) -> Result<(), KagiError> {
    if !matches!(shell, CompletionShell::PowerShell) {
        return Ok(());
    }

    let profile = xdg_config_home()
        .join("powershell")
        .join("Microsoft.PowerShell_profile.ps1");
    let line = format!(". '{}'", target_path.display());
    append_line_if_missing(&profile, &line)
}

fn append_line_if_missing(path: &Path, line: &str) -> Result<(), KagiError> {
    let existing = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(KagiError::Config(format!(
                "failed to read {}: {error}",
                path.display()
            )));
        }
    };

    if existing.lines().any(|existing_line| existing_line == line) {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            KagiError::Config(format!("failed to create {}: {error}", parent.display()))
        })?;
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(line);
    updated.push('\n');
    fs::write(path, updated)
        .map_err(|error| KagiError::Config(format!("failed to update {}: {error}", path.display())))
}

fn xdg_data_home() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local").join("share"))
}

fn xdg_config_home() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn run_auth_status(profile: Option<&str>) -> Result<(), KagiError> {
    let inventory = load_credential_inventory_for_profile(profile)?;
    println!("{}", format_status(&inventory));
    Ok(())
}

fn run_auth_set(args: AuthSetArgs, profile: Option<&str>) -> Result<(), KagiError> {
    let inventory = save_credentials_for_profile(
        profile,
        args.api_key.as_deref(),
        args.api_token.as_deref(),
        args.session_token.as_deref(),
    )?;
    println!("saved credentials to {}", inventory.config_path.display());
    println!("{}", format_status(&inventory));
    Ok(())
}

async fn run_auth_check(profile: Option<&str>) -> Result<(), KagiError> {
    let inventory = load_credential_inventory_for_profile(profile)?;
    let credential = inventory.preferred_for_status().cloned().ok_or_else(|| {
        KagiError::Config(
            "missing credentials: auth check could not verify an account. Set KAGI_API_KEY, KAGI_API_TOKEN, or KAGI_SESSION_TOKEN, or run `kagi auth set` with the credential you want to save"
                .to_string(),
        )
    })?;

    let selected_kind = credential.kind;
    let selected_source = credential.source;
    validate_credential(&credential).await?;

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
            if credentials.primary.kind == CredentialKind::ApiKey
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
        CredentialKind::ApiKey => search::execute_api_search(request, &credential.value).await,
        CredentialKind::ApiToken => Err(KagiError::Config(
            "base search was not sent because API-first mode requires KAGI_API_KEY. Use a Kagi API key, switch preferred_auth to `session`, or use KAGI_API_TOKEN only with /api/v0 commands"
                .to_string(),
        )),
        CredentialKind::SessionToken => search::execute_search(request, &credential.value).await,
    }
}

const fn should_fallback_to_session(error: &KagiError) -> bool {
    matches!(error, KagiError::Auth(_))
}

fn resolve_api_token(profile: Option<&str>) -> Result<String, KagiError> {
    let inventory = load_credential_inventory_for_profile(profile)?;
    inventory
        .api_token
        .map(|credential| credential.value)
        .ok_or_else(|| {
            KagiError::Config(
                "this command requires KAGI_API_TOKEN. Set it in the environment or run `kagi auth set --api-token <token>`"
                    .to_string(),
            )
        })
}

fn resolve_session_token(profile: Option<&str>) -> Result<String, KagiError> {
    let inventory = load_credential_inventory_for_profile(profile)?;
    inventory
        .session_token
        .map(|credential| credential.value)
        .ok_or_else(|| {
            KagiError::Config(
                "this command requires KAGI_SESSION_TOKEN. Set it in the environment or run `kagi auth set --session-token <token>`"
                    .to_string(),
            )
        })
}

fn resolve_api_key(profile: Option<&str>) -> Result<String, KagiError> {
    let inventory = load_credential_inventory_for_profile(profile)?;
    inventory.api_key.map(|credential| credential.value).ok_or_else(|| {
        KagiError::Config(
            "extract requires KAGI_API_KEY. Set it in the environment or run `kagi auth set --api-key <key>`"
                .to_string(),
        )
    })
}

async fn execute_extract_with_available_auth(
    url: &str,
    profile: Option<&str>,
) -> Result<String, KagiError> {
    let api_key = resolve_api_key(profile)?;
    execute_extract(url, &api_key).await
}

async fn execute_extract_response_with_available_auth(
    url: &str,
    profile: Option<&str>,
) -> Result<crate::types::ExtractResponse, KagiError> {
    let api_key = resolve_api_key(profile)?;
    execute_extract_response(url, &api_key).await
}

async fn run_extract_filter(profile: Option<&str>) -> Result<(), KagiError> {
    let urls = read_stdin_lines()?;
    if urls.is_empty() {
        return Err(KagiError::Config(
            "extract --filter requires at least one stdin URL".to_string(),
        ));
    }

    let api_key = resolve_api_key(profile)?;
    let mut failure_count = 0usize;

    for url in urls {
        match execute_extract_response(&url, &api_key).await {
            Ok(response) => print_compact_json(&serde_json::json!({
                "url": url,
                "ok": true,
                "response": response,
            }))?,
            Err(error) => {
                failure_count += 1;
                print_compact_json(&serde_json::json!({
                    "url": url,
                    "ok": false,
                    "error": error_envelope(&error),
                }))?;
            }
        }
    }

    if failure_count > 0 {
        return Err(KagiError::Batch(format!(
            "extract --filter completed with {failure_count} failed item(s)"
        )));
    }

    Ok(())
}

fn build_translate_request(args: TranslateArgs) -> Result<TranslateCommandRequest, KagiError> {
    let text = match args.text {
        Some(text) => text,
        None => read_stdin_to_string()?.trim().to_string(),
    };
    if text.trim().is_empty() {
        return Err(KagiError::Config(
            "translate requires TEXT or non-empty stdin".to_string(),
        ));
    }

    Ok(TranslateCommandRequest {
        text: text.trim().to_string(),
        from: args.from.trim().to_string(),
        to: args.to.trim().to_string(),
        quality: normalize_optional_string(args.quality),
        model: normalize_optional_string(args.model),
        prediction: normalize_optional_string(args.prediction),
        predicted_language: normalize_optional_string(args.predicted_language),
        formality: normalize_optional_string(args.formality),
        speaker_gender: normalize_optional_string(args.speaker_gender),
        addressee_gender: normalize_optional_string(args.addressee_gender),
        language_complexity: normalize_optional_string(args.language_complexity),
        translation_style: normalize_optional_string(args.translation_style),
        context: normalize_optional_string(args.context),
        dictionary_language: normalize_optional_string(args.dictionary_language),
        time_format: normalize_optional_string(args.time_format),
        use_definition_context: args.use_definition_context,
        enable_language_features: args.enable_language_features,
        preserve_formatting: args.preserve_formatting,
        context_memory: parse_context_memory_json(args.context_memory_json.as_deref())?,
        fetch_alternatives: !args.no_alternatives,
        fetch_word_insights: !args.no_word_insights,
        fetch_suggestions: !args.no_suggestions,
        fetch_alignments: !args.no_alignments,
    })
}

fn read_assistant_prompt_query(query: Option<String>) -> Result<String, KagiError> {
    let query = match query {
        Some(query) => query,
        None => read_stdin_to_string()?.trim().to_string(),
    };
    if query.trim().is_empty() {
        return Err(KagiError::Config(
            "assistant prompt mode requires a QUERY or non-empty stdin unless a thread subcommand is used"
                .to_string(),
        ));
    }

    Ok(query.trim().to_string())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

const fn bool_flag_choice(enabled: bool, disabled: bool) -> Option<bool> {
    match (enabled, disabled) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

fn parse_context_memory_json(raw: Option<&str>) -> Result<Option<Vec<Value>>, KagiError> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let parsed: Value = serde_json::from_str(raw).map_err(|error| {
        KagiError::Config(format!(
            "--context-memory-json must be valid JSON; parse failed: {error}"
        ))
    })?;

    match parsed {
        Value::Array(values) => Ok(Some(values)),
        _ => Err(KagiError::Config(
            "--context-memory-json must be a JSON array".to_string(),
        )),
    }
}

fn build_search_request(query: String, options: &SearchRequestOptions) -> search::SearchRequest {
    let mut query = query.trim().to_string();
    if let Some(snap) = options
        .snap
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let snap = snap.trim_start_matches('@').trim();
        if !snap.is_empty() {
            query = format!("@{snap} {query}");
        }
    }

    let mut request = search::SearchRequest::new(query);

    if let Some(lens) = options.lens.clone() {
        request = request.with_lens(lens);
    }
    if let Some(region) = options.region.clone() {
        request = request.with_region(region);
    }
    if let Some(time) = options.time.clone() {
        request = request.with_time_filter(match time {
            SearchTime::Day => "1",
            SearchTime::Week => "2",
            SearchTime::Month => "3",
            SearchTime::Year => "4",
        });
    }
    if let Some(from_date) = options.from_date.clone() {
        request = request.with_from_date(from_date);
    }
    if let Some(to_date) = options.to_date.clone() {
        request = request.with_to_date(to_date);
    }
    if let Some(limit) = options.limit {
        request = request.with_limit(limit);
    }
    if let Some(order) = options.order.clone() {
        request = match order {
            SearchOrder::Default => request,
            SearchOrder::Recency => request.with_order("2"),
            SearchOrder::Website => request.with_order("3"),
            SearchOrder::Trackers => request.with_order("4"),
        };
    }
    if options.verbatim {
        request = request.with_verbatim(true);
    }
    if options.personalized {
        request = request.with_personalized(true);
    } else if options.no_personalized {
        request = request.with_personalized(false);
    }

    request
}

fn search_auth_requirement(request: &search::SearchRequest) -> SearchAuthRequirement {
    if request.lens.is_some() {
        SearchAuthRequirement::Lens
    } else if request.requires_session_auth() {
        SearchAuthRequirement::Filtered
    } else {
        SearchAuthRequirement::Base
    }
}

async fn resolve_search_lens_option(
    lens: Option<String>,
    profile: Option<&str>,
) -> Result<Option<String>, KagiError> {
    let Some(lens) = lens else {
        return Ok(None);
    };
    let lens = lens.trim().to_string();
    if lens.is_empty() || lens.parse::<u32>().is_ok() {
        return Ok(Some(lens));
    }

    let token = resolve_session_token(profile)?;
    let lenses = execute_lens_list(&token).await?;
    let matches = lenses
        .iter()
        .filter(|candidate| candidate.name == lens)
        .collect::<Vec<_>>();

    let [selected] = matches.as_slice() else {
        if matches.is_empty() {
            return Err(KagiError::Config(format!(
                "lens named '{lens}' was not found. Lens names are matched exactly; run `kagi lens list` to inspect available lenses"
            )));
        }

        return Err(KagiError::Config(format!(
            "lens name '{lens}' is ambiguous because multiple lenses have that exact name. Use the numeric lens index instead"
        )));
    };

    if !selected.enabled {
        return Err(KagiError::Config(format!(
            "lens named '{lens}' is disabled. Enable it with `kagi lens enable {}` or choose an enabled lens",
            selected.id
        )));
    }

    selected
        .position
        .map(|position| Some(position.to_string()))
        .ok_or_else(|| {
            KagiError::Config(format!(
                "lens named '{lens}' is enabled but has no active search position. Disable and re-enable it, then retry"
            ))
        })
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), KagiError> {
    let output = serde_json::to_string_pretty(value)
        .map_err(|error| KagiError::Parse(format!("failed to serialize JSON output: {error}")))?;
    println!("{output}");
    Ok(())
}

async fn execute_once_assistant_prompt(
    request: &AssistantPromptRequest,
    stream_output: Option<AssistantStreamOutput>,
    token: &str,
) -> Result<crate::types::AssistantPromptResponse, KagiError> {
    let model = request
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| KagiError::Config("--once requires --model".to_string()))?;
    if request
        .profile_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(KagiError::Config(
            "--once cannot be combined with --assistant".to_string(),
        ));
    }

    let created = execute_custom_assistant_create(
        &AssistantProfileCreateRequest {
            name: temporary_assistant_name(),
            bang_trigger: None,
            internet_access: request.internet_access,
            selected_lens: request.lens_id.map(|lens_id| lens_id.to_string()),
            personalizations: request.personalizations,
            base_model: Some(model.to_string()),
            custom_instructions: None,
        },
        token,
    )
    .await?;

    let delete_target = created.profile_id.clone().unwrap_or(created.name.clone());
    let mut prompt_request = request.clone();
    prompt_request.profile_id = Some(delete_target.clone());
    prompt_request.model = None;

    let prompt_result = if let Some(stream_output) = stream_output {
        execute_streaming_assistant_prompt(&prompt_request, token, stream_output).await
    } else {
        execute_assistant_prompt(&prompt_request, token).await
    };

    let delete_result = execute_custom_assistant_delete(&delete_target, token).await;
    match (prompt_result, delete_result) {
        (Ok(response), Ok(_)) => Ok(response),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn temporary_assistant_name() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("kagi-cli-once-{millis}-{}", std::process::id())
}

async fn execute_assistant_prompt_for_args(
    request: &AssistantPromptRequest,
    once: bool,
    stream_output: Option<AssistantStreamOutput>,
    token: &str,
) -> Result<crate::types::AssistantPromptResponse, KagiError> {
    if once {
        execute_once_assistant_prompt(request, stream_output, token).await
    } else if let Some(stream_output) = stream_output {
        execute_streaming_assistant_prompt(request, token, stream_output).await
    } else {
        execute_assistant_prompt(request, token).await
    }
}

async fn execute_streaming_assistant_prompt(
    request: &AssistantPromptRequest,
    token: &str,
    stream_output: AssistantStreamOutput,
) -> Result<crate::types::AssistantPromptResponse, KagiError> {
    let response = match stream_output {
        AssistantStreamOutput::Text => {
            let mut saw_text = false;
            let response = execute_assistant_prompt_stream(request, token, |event| {
                if !event.md_delta.is_empty() {
                    saw_text = true;
                    print!("{}", event.md_delta);
                    io::stdout().flush().map_err(|error| {
                        KagiError::Network(format!("failed to flush assistant stream: {error}"))
                    })?;
                }
                Ok(())
            })
            .await?;
            if saw_text {
                println!();
            }
            response
        }
        AssistantStreamOutput::Json => {
            execute_assistant_prompt_stream(request, token, print_compact_json).await?
        }
    };

    Ok(response)
}

fn print_compact_json<T: serde::Serialize>(value: &T) -> Result<(), KagiError> {
    let output = serde_json::to_string(value)
        .map_err(|error| KagiError::Parse(format!("failed to serialize JSON output: {error}")))?;
    println!("{output}");
    Ok(())
}

fn print_toon<T: serde::Serialize>(value: &T) -> Result<(), KagiError> {
    let value = serde_json::to_value(value)
        .map_err(|error| KagiError::Parse(format!("failed to serialize TOON output: {error}")))?;
    println!("{}", toon::encode(&value, None));
    Ok(())
}

#[derive(Debug, Clone)]
struct AssistantContractSpec {
    name: String,
    fields: Vec<AssistantContractField>,
}

#[derive(Debug, Clone)]
struct AssistantContractField {
    name: String,
    kind: AssistantContractFieldKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssistantContractFieldKind {
    String,
    Number,
    Boolean,
    Object,
    Array,
    StringArray,
}

fn load_assistant_contract(
    builtin: Option<&str>,
    file: Option<&Path>,
) -> Result<Option<AssistantContractSpec>, KagiError> {
    match (builtin, file) {
        (Some(name), None) => builtin_assistant_contract(name).map(Some),
        (None, Some(path)) => load_assistant_contract_file(path).map(Some),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err(KagiError::Config(
            "assistant contract mode accepts either --contract or --contract-file, not both"
                .to_string(),
        )),
    }
}

fn builtin_assistant_contract(name: &str) -> Result<AssistantContractSpec, KagiError> {
    let normalized = name.trim().to_ascii_lowercase();
    let fields = match normalized.as_str() {
        "decision" => vec![
            contract_field("decision", AssistantContractFieldKind::String),
            contract_field("rationale", AssistantContractFieldKind::String),
            contract_field("next_actions", AssistantContractFieldKind::StringArray),
        ],
        "checklist" => vec![contract_field(
            "items",
            AssistantContractFieldKind::StringArray,
        )],
        "plan" => vec![contract_field(
            "steps",
            AssistantContractFieldKind::StringArray,
        )],
        _ => {
            return Err(KagiError::Config(format!(
                "unknown assistant contract '{name}'. Supported contracts: decision, checklist, plan"
            )));
        }
    };

    Ok(AssistantContractSpec {
        name: normalized,
        fields,
    })
}

fn load_assistant_contract_file(path: &Path) -> Result<AssistantContractSpec, KagiError> {
    let raw = fs::read_to_string(path).map_err(|error| {
        KagiError::Config(format!(
            "failed to read assistant contract file '{}': {error}",
            path.display()
        ))
    })?;
    let value: Value = serde_json::from_str(&raw).map_err(|error| {
        KagiError::Config(format!(
            "assistant contract file '{}' must be valid JSON: {error}",
            path.display()
        ))
    })?;
    if value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind != "object")
    {
        return Err(KagiError::Config(
            "assistant contract file only supports top-level type \"object\"".to_string(),
        ));
    }

    let required = value
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            KagiError::Config("assistant contract file must include a required array".to_string())
        })?;
    if required.is_empty() {
        return Err(KagiError::Config(
            "assistant contract file required array cannot be empty".to_string(),
        ));
    }

    let mut fields = Vec::new();
    for item in required {
        let name = item
            .as_str()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                KagiError::Config(
                    "assistant contract file required entries must be non-empty strings"
                        .to_string(),
                )
            })?;
        fields.push(contract_field(
            name,
            contract_file_field_kind(&value, name)?,
        ));
    }

    let name = value
        .get("title")
        .and_then(Value::as_str)
        .or_else(|| value.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "contract-file".to_string());

    Ok(AssistantContractSpec { name, fields })
}

fn contract_file_field_kind(
    contract: &Value,
    field_name: &str,
) -> Result<AssistantContractFieldKind, KagiError> {
    let Some(kind) = contract
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get(field_name))
        .and_then(|property| property.get("type"))
        .and_then(Value::as_str)
    else {
        return Ok(AssistantContractFieldKind::String);
    };

    match kind {
        "string" => Ok(AssistantContractFieldKind::String),
        "number" | "integer" => Ok(AssistantContractFieldKind::Number),
        "boolean" => Ok(AssistantContractFieldKind::Boolean),
        "object" => Ok(AssistantContractFieldKind::Object),
        "array" => Ok(AssistantContractFieldKind::Array),
        _ => Err(KagiError::Config(format!(
            "assistant contract file property '{field_name}' uses unsupported type '{kind}'"
        ))),
    }
}

fn contract_field(name: &str, kind: AssistantContractFieldKind) -> AssistantContractField {
    AssistantContractField {
        name: name.to_string(),
        kind,
    }
}

fn validate_assistant_contract_output_format(
    format: AssistantOutputFormat,
) -> Result<(), KagiError> {
    if matches!(
        format,
        AssistantOutputFormat::Json | AssistantOutputFormat::Compact
    ) {
        return Ok(());
    }

    Err(KagiError::Config(
        "assistant contract mode only supports --format json or --format compact because the validated output is JSON".to_string(),
    ))
}

fn contract_prompt_query(query: &str, contract: &AssistantContractSpec) -> String {
    format!(
        "{query}\n\n{}",
        assistant_contract_instruction(contract, "Return only a valid JSON object")
    )
}

fn contract_repair_query(
    contract: &AssistantContractSpec,
    previous_reply: &str,
    validation_error: &str,
) -> String {
    format!(
        "Your previous answer did not satisfy the assistant contract '{}': {validation_error}\n\nPrevious answer:\n{previous_reply}\n\n{}",
        contract.name,
        assistant_contract_instruction(
            contract,
            "Repair the answer and return only the valid JSON object"
        )
    )
}

fn assistant_contract_instruction(contract: &AssistantContractSpec, lead: &str) -> String {
    let fields = contract
        .fields
        .iter()
        .map(|field| format!("- {}: {}", field.name, field.kind.description()))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Assistant contract: {lead}. Do not include markdown fences, prose outside JSON, comments, or trailing commas.\nRequired top-level fields:\n{fields}"
    )
}

fn validate_assistant_contract_response(
    contract: &AssistantContractSpec,
    response: &crate::types::AssistantPromptResponse,
) -> Result<Value, String> {
    let content = assistant_message_content(response);
    let value = serde_json::from_str::<Value>(content)
        .map_err(|error| format!("assistant reply was not valid JSON: {error}"))?;
    validate_assistant_contract_value(contract, &value)?;
    Ok(value)
}

fn validate_assistant_contract_value(
    contract: &AssistantContractSpec,
    value: &Value,
) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "assistant reply must be a JSON object".to_string())?;

    for field in &contract.fields {
        let field_value = object
            .get(&field.name)
            .ok_or_else(|| format!("missing required key '{}'", field.name))?;
        field.kind.validate(&field.name, field_value)?;
    }

    Ok(())
}

fn print_assistant_contract_value(
    value: &Value,
    format: AssistantOutputFormat,
) -> Result<(), KagiError> {
    match format {
        AssistantOutputFormat::Compact => print_compact_json(value),
        AssistantOutputFormat::Json => print_json(value),
        _ => validate_assistant_contract_output_format(format),
    }
}

impl AssistantContractFieldKind {
    const fn description(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Object => "object",
            Self::Array => "array",
            Self::StringArray => "array of strings",
        }
    }

    fn validate(self, field_name: &str, value: &Value) -> Result<(), String> {
        let valid = match self {
            Self::String => value.as_str().is_some_and(|text| !text.trim().is_empty()),
            Self::Number => value.is_number(),
            Self::Boolean => value.is_boolean(),
            Self::Object => value.is_object(),
            Self::Array => value.is_array(),
            Self::StringArray => value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string)),
        };

        if valid {
            Ok(())
        } else {
            Err(format!("key '{field_name}' must be {}", self.description()))
        }
    }
}

async fn cached_json<T, K, Fut, F>(
    enabled: bool,
    ttl_seconds: u64,
    namespace: &str,
    key_source: &K,
    fetch: F,
) -> Result<T, KagiError>
where
    T: Serialize + DeserializeOwned,
    K: Serialize,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, KagiError>>,
{
    if !enabled {
        return fetch().await;
    }

    let key_json = serde_json::to_string(key_source)?;
    let key = local::cache_key(&[namespace, &key_json]);
    if let Some(value) = local::cache_get(&key)? {
        return serde_json::from_value(value).map_err(KagiError::from);
    }

    let fetched = fetch().await?;
    let value = serde_json::to_value(&fetched)?;
    local::cache_put(&key, ttl_seconds, &value)?;
    Ok(fetched)
}

fn record_history(
    command: &str,
    query: Option<&str>,
    result_count: Option<usize>,
) -> Result<(), KagiError> {
    local::append_history(&local::HistoryEntry {
        timestamp: local::now_unix_seconds()?,
        command: command.to_string(),
        query: query.map(str::to_string),
        result_count,
    })
}

fn read_stdin_to_string() -> Result<String, KagiError> {
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| KagiError::Config(format!("failed to read stdin: {error}")))?;
    if input.is_empty() {
        return Ok(String::new());
    }

    let mut rest = String::new();
    io::stdin()
        .read_to_string(&mut rest)
        .map_err(|error| KagiError::Config(format!("failed to read stdin: {error}")))?;
    input.push_str(&rest);
    Ok(input)
}

fn read_stdin_lines() -> Result<Vec<String>, KagiError> {
    let stdin = io::stdin();
    stdin
        .lock()
        .lines()
        .map(|line| {
            line.map(|value| value.trim().to_string())
                .map_err(|error| KagiError::Config(format!("failed to read stdin: {error}")))
        })
        .filter_map(|line| match line {
            Ok(value) if value.is_empty() => None,
            other => Some(other),
        })
        .collect()
}

fn print_quick_response(
    response: &QuickResponse,
    format: &str,
    use_color: bool,
) -> Result<(), KagiError> {
    match format {
        "pretty" => {
            println!("{}", format_quick_pretty(response, use_color));
            Ok(())
        }
        "toon" => print_toon(response),
        "compact" => print_compact_json(response),
        "markdown" => {
            println!("{}", format_quick_markdown(response));
            Ok(())
        }
        _ => print_json(response),
    }
}

fn print_assistant_response(
    response: &crate::types::AssistantPromptResponse,
    format: AssistantOutputFormat,
    use_color: bool,
) -> Result<(), KagiError> {
    match format {
        AssistantOutputFormat::Pretty => {
            println!("{}", format_assistant_pretty(response, use_color));
            Ok(())
        }
        AssistantOutputFormat::Toon => print_toon(response),
        AssistantOutputFormat::Compact => print_compact_json(response),
        AssistantOutputFormat::Markdown => {
            println!("{}", format_assistant_markdown(response));
            Ok(())
        }
        AssistantOutputFormat::Json => print_json(response),
    }
}

fn assistant_message_content(response: &crate::types::AssistantPromptResponse) -> &str {
    response
        .message
        .markdown
        .as_deref()
        .or(response.message.reply_html.as_deref())
        .unwrap_or("")
        .trim()
}

fn assistant_references_markdown(response: &crate::types::AssistantPromptResponse) -> &str {
    response
        .message
        .references_markdown
        .as_deref()
        .unwrap_or("")
        .trim()
}

fn format_assistant_pretty(
    response: &crate::types::AssistantPromptResponse,
    use_color: bool,
) -> String {
    let title_color = if use_color { "\x1b[1;34m" } else { "" };
    let muted_color = if use_color { "\x1b[36m" } else { "" };
    let reset_color = if use_color { "\x1b[0m" } else { "" };
    let mut sections = vec![format!(
        "{title_color}Thread{reset_color}: {}\n{muted_color}Message{reset_color}: {}\n\n{}",
        response.thread.id,
        response.message.id,
        assistant_message_content(response)
    )];
    let references = assistant_references_markdown(response);

    if !references.is_empty() {
        sections.push(format!(
            "{title_color}References{reset_color}\n\n{references}"
        ));
    }

    sections.join("\n\n")
}

fn format_assistant_markdown(response: &crate::types::AssistantPromptResponse) -> String {
    let mut sections = vec![assistant_message_content(response).to_string()];
    let references = assistant_references_markdown(response);

    if !references.is_empty() {
        sections.push(references.to_string());
    }

    sections
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[allow(clippy::too_many_arguments)]
async fn run_search(
    request: search::SearchRequest,
    format: String,
    use_color: bool,
    template: Option<String>,
    local_cache: bool,
    cache_ttl: u64,
    limit: Option<usize>,
    profile: Option<&str>,
) -> Result<(), KagiError> {
    let inventory = load_credential_inventory_for_profile(profile)?;
    let credentials = inventory.resolve_for_search(search_auth_requirement(&request))?;

    let response = cached_json(local_cache, cache_ttl, "search", &request, || async {
        execute_search_request(&request, credentials).await
    })
    .await?;
    record_history("search", Some(&request.query), Some(response.data.len()))?;
    let mut response = apply_local_site_preferences(response)?;
    if let Some(n) = limit {
        response.data.truncate(n);
    }

    let output = if let Some(template) = template.as_deref() {
        format_template_response(&response, template)
    } else {
        match format.as_str() {
            "pretty" => format_pretty_response(&response, use_color),
            "toon" => {
                return print_toon(&response);
            }
            "compact" => serde_json::to_string(&response).map_err(|error| {
                KagiError::Parse(format!("failed to serialize search response: {error}"))
            })?,
            "markdown" => format_markdown_response(&response),
            "csv" => format_csv_response(&response),
            _ => serde_json::to_string_pretty(&response).map_err(|error| {
                KagiError::Parse(format!("failed to serialize search response: {error}"))
            })?,
        }
    };

    println!("{output}");
    Ok(())
}

fn format_template_response(response: &SearchResponse, template: &str) -> String {
    response
        .data
        .iter()
        .enumerate()
        .map(|(index, result)| {
            template
                .replace("{{rank}}", &(index + 1).to_string())
                .replace("{{title}}", &result.title)
                .replace("{{url}}", &result.url)
                .replace("{{snippet}}", &result.snippet)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_local_site_preferences(mut response: SearchResponse) -> Result<SearchResponse, KagiError> {
    let preferences = local::load_site_preferences()?;
    if preferences.domains.is_empty() {
        return Ok(response);
    }

    response.data.retain(|result| {
        result_domain(&result.url).and_then(|domain| preferences.domains.get(&domain).copied())
            != Some(local::SitePreferenceMode::Block)
    });
    response.data.sort_by_key(|result| {
        site_preference_sort_rank(
            result_domain(&result.url)
                .and_then(|domain| preferences.domains.get(&domain).copied())
                .unwrap_or(local::SitePreferenceMode::Normal),
        )
    });
    Ok(response)
}

const fn site_preference_sort_rank(mode: local::SitePreferenceMode) -> u8 {
    match mode {
        local::SitePreferenceMode::Pin => 0,
        local::SitePreferenceMode::Higher => 1,
        local::SitePreferenceMode::Normal => 2,
        local::SitePreferenceMode::Lower => 3,
        local::SitePreferenceMode::Block => 4,
    }
}

fn result_domain(url: &str) -> Option<String> {
    let without_scheme = url
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    without_scheme
        .split('/')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
}

fn format_pretty_response(response: &SearchResponse, use_color: bool) -> String {
    if response.data.is_empty() {
        let mut output = "No results found.".to_string();
        append_pretty_related_searches(&mut output, response, use_color);
        return output;
    }

    let mut output = response
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
        .join("\n\n");
    append_pretty_related_searches(&mut output, response, use_color);
    output
}

fn format_markdown_response(response: &SearchResponse) -> String {
    if response.data.is_empty() {
        let mut output = "# No results found.".to_string();
        append_markdown_related_searches(&mut output, response);
        return output;
    }

    let mut output = response
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
        .join("\n");
    append_markdown_related_searches(&mut output, response);
    output
}

fn append_pretty_related_searches(output: &mut String, response: &SearchResponse, use_color: bool) {
    let related = related_search_labels(response);
    if related.is_empty() {
        return;
    }

    let title_color = if use_color { "\x1b[1;34m" } else { "" };
    let reset_color = if use_color { "\x1b[0m" } else { "" };
    output.push_str(&format!(
        "\n\n{title_color}Related searches{reset_color}\n   {}",
        related.join("\n   ")
    ));
}

fn append_markdown_related_searches(output: &mut String, response: &SearchResponse) {
    let related = related_search_labels(response);
    if related.is_empty() {
        return;
    }

    output.push_str("\n\n## Related searches\n\n");
    for label in related {
        output.push_str(&format!("- {label}\n"));
    }
}

fn related_search_labels(response: &SearchResponse) -> Vec<String> {
    response
        .related_searches
        .iter()
        .filter_map(related_search_label)
        .collect()
}

fn related_search_label(value: &Value) -> Option<String> {
    if let Some(text) = value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return Some(text.to_string());
    }

    let object = value.as_object()?;
    ["query", "text", "title", "label", "display"]
        .iter()
        .find_map(|key| object.get(*key)?.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn escape_csv_field(field: &str) -> String {
    if field.contains('"') || field.contains(',') || field.contains('\n') || field.contains('\r') {
        let escaped = field.replace('"', "\"\"");
        format!("\"{escaped}\"")
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
        output.push_str(&format!("{title},{url},{snippet}\n"));
    }

    output
}

fn build_news_search_request(args: &SearchArgs) -> search::NewsSearchRequest {
    let freshness = args.time.as_ref().and_then(|time| match time {
        SearchTime::Day => Some(search::NewsFreshness::Day),
        SearchTime::Week => Some(search::NewsFreshness::Week),
        SearchTime::Month => Some(search::NewsFreshness::Month),
        SearchTime::Year => None,
    });
    let order = args.order.as_ref().and_then(|order| match order {
        SearchOrder::Default => Some(search::NewsSearchOrder::Default),
        SearchOrder::Recency => Some(search::NewsSearchOrder::Recency),
        SearchOrder::Website => Some(search::NewsSearchOrder::Website),
        SearchOrder::Trackers => None,
    });
    search::NewsSearchRequest {
        query: args.query.trim().to_string(),
        region: args.region.clone(),
        freshness,
        order,
        dir_desc: false,
        limit: args.limit,
    }
}

fn print_news_search(
    response: &NewsSearchResponse,
    format: &OutputFormat,
    use_color: bool,
) -> Result<(), KagiError> {
    match format {
        OutputFormat::Json => print_json(response),
        OutputFormat::Toon => print_toon(response),
        OutputFormat::Compact => print_compact_json(response),
        OutputFormat::Pretty => {
            println!("{}", format_pretty_news_response(response, use_color));
            Ok(())
        }
        OutputFormat::Markdown => {
            println!("{}", format_markdown_news_response(response));
            Ok(())
        }
        OutputFormat::Csv => {
            println!("{}", format_csv_news_response(response));
            Ok(())
        }
    }
}

fn format_pretty_news_response(response: &NewsSearchResponse, use_color: bool) -> String {
    if response.clusters.is_empty() {
        return "No news results found.".to_string();
    }
    let bold = if use_color { "\x1b[1;34m" } else { "" };
    let dim = if use_color { "\x1b[2m" } else { "" };
    let url_color = if use_color { "\x1b[36m" } else { "" };
    let reset = if use_color { "\x1b[0m" } else { "" };

    let mut blocks = Vec::with_capacity(response.clusters.len());
    for (cluster_index, cluster) in response.clusters.iter().enumerate() {
        let mut lines = Vec::with_capacity(cluster.items.len() + 1);
        lines.push(format!(
            "{dim}── Cluster {}{reset}",
            cluster_index + 1,
            dim = dim,
            reset = reset,
        ));
        for item in &cluster.items {
            let header = match (item.source.as_deref(), item.time_relative.as_deref()) {
                (Some(source), Some(time)) => format!("{source} · {time}"),
                (Some(source), None) => source.to_string(),
                (None, Some(time)) => time.to_string(),
                (None, None) => String::new(),
            };
            let paywall = if item.paywall { " [paywall]" } else { "" };
            if header.is_empty() {
                lines.push(format!(
                    "{bold}{}{reset}{paywall}\n  {url_color}{}{reset}",
                    item.title, item.url
                ));
            } else {
                lines.push(format!(
                    "{dim}{header}{reset}{paywall}\n  {bold}{}{reset}\n  {url_color}{}{reset}",
                    item.title, item.url
                ));
            }
            if let Some(snippet) = item.snippet.as_deref() {
                lines.push(format!("  {snippet}"));
            }
        }
        blocks.push(lines.join("\n"));
    }
    blocks.join("\n\n")
}

fn format_markdown_news_response(response: &NewsSearchResponse) -> String {
    if response.clusters.is_empty() {
        return "# No news results found.".to_string();
    }
    let mut sections = Vec::with_capacity(response.clusters.len());
    for (cluster_index, cluster) in response.clusters.iter().enumerate() {
        let mut section = format!("## Cluster {}\n\n", cluster_index + 1);
        for item in &cluster.items {
            let suffix = match (item.source.as_deref(), item.time_relative.as_deref()) {
                (Some(source), Some(time)) => format!(" - {source}, {time}"),
                (Some(source), None) => format!(" - {source}"),
                (None, Some(time)) => format!(" - {time}"),
                (None, None) => String::new(),
            };
            let paywall = if item.paywall { " *(paywall)*" } else { "" };
            section.push_str(&format!(
                "- [{}]({}){suffix}{paywall}\n",
                item.title, item.url,
            ));
            if let Some(snippet) = item.snippet.as_deref() {
                section.push_str(&format!("  {snippet}\n"));
            }
        }
        sections.push(section);
    }
    sections.join("\n")
}

fn format_csv_news_response(response: &NewsSearchResponse) -> String {
    let header = "cluster,source,time_relative,title,url,paywall,snippet";
    if response.clusters.is_empty() {
        return header.to_string();
    }
    let mut output = String::from(header);
    output.push('\n');
    for (cluster_index, cluster) in response.clusters.iter().enumerate() {
        for item in &cluster.items {
            let cluster_index = (cluster_index + 1).to_string();
            let source = escape_csv_field(item.source.as_deref().unwrap_or(""));
            let time = escape_csv_field(item.time_relative.as_deref().unwrap_or(""));
            let title = escape_csv_field(&item.title);
            let url = escape_csv_field(&item.url);
            let paywall = if item.paywall { "true" } else { "false" };
            let snippet = escape_csv_field(item.snippet.as_deref().unwrap_or(""));
            output.push_str(&format!(
                "{cluster_index},{source},{time},{title},{url},{paywall},{snippet}\n"
            ));
        }
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
            let refill_interval = 60.0 / f64::from(self.refill_rate);
            let refill_tokens = (elapsed / refill_interval).floor() as u32;

            if refill_tokens > 0 {
                *tokens = (*tokens + refill_tokens).min(self.capacity);
                *last_refill += Duration::from_secs_f64(f64::from(refill_tokens) * refill_interval);
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

struct BatchSearchConfig<'a> {
    queries: Vec<String>,
    concurrency: usize,
    rate_limit: u32,
    format: String,
    use_color: bool,
    options: SearchRequestOptions,
    template: Option<String>,
    limit: Option<usize>,
    profile: Option<&'a str>,
}

async fn run_batch_search(config: BatchSearchConfig<'_>) -> Result<(), KagiError> {
    let BatchSearchConfig {
        queries,
        concurrency,
        rate_limit,
        format,
        use_color,
        options,
        template,
        limit,
        profile,
    } = config;

    let inventory = load_credential_inventory_for_profile(profile)?;
    let auth_probe_request = build_search_request("auth probe".to_string(), &options);
    let credentials = inventory.resolve_for_search(search_auth_requirement(&auth_probe_request))?;

    let rate_limiter = Arc::new(RateLimiter::new(rate_limit, rate_limit));
    let semaphore = Arc::new(Semaphore::new(concurrency));

    let mut handles = vec![];

    for query in queries {
        let rate_limiter_clone = Arc::clone(&rate_limiter);
        let semaphore_clone = Arc::clone(&semaphore);
        let credentials_clone = credentials.clone();
        let options_clone = options.clone();
        let query_for_task = query.clone();
        let query_for_logging = query.clone();

        let handle: tokio::task::JoinHandle<(String, Result<SearchResponse, KagiError>)> =
            tokio::spawn(async move {
                let _permit = semaphore_clone.acquire().await;
                let result = async {
                    rate_limiter_clone.acquire().await?;

                    let request = build_search_request(query_for_task, &options_clone);

                    execute_search_request(&request, credentials_clone).await
                }
                .await;

                (query, result)
            });

        handles.push((query_for_logging, handle));
    }

    let mut results = vec![];
    let mut failures = vec![];

    for (query, handle) in handles {
        match handle.await {
            Ok((completed_query, Ok(mut output))) => {
                if let Some(n) = limit {
                    output.data.truncate(n);
                }
                results.push((completed_query, output));
            }
            Ok((completed_query, Err(e))) => {
                error!(query = %completed_query, error = %e, "batch query failed");
                failures.push(format!("{completed_query}: {e}"));
            }
            Err(e) => {
                error!(query = %query, error = %e, "batch worker task failed");
                failures.push(format!("{query}: worker task failed: {e}"));
            }
        }
    }

    if !failures.is_empty() && (format == "json" || format == "compact" || format == "toon") {
        // For machine-readable formats, exit with error code if any queries failed
        return Err(KagiError::Batch(format_batch_failure_message(
            results.len(),
            &failures,
        )));
    }

    let success_count = results.len();

    // Output results in order
    if format == "json" || format == "compact" || format == "toon" {
        // For machine-readable formats, create a proper JSON envelope
        let queries: Vec<String> = results.iter().map(|(query, _)| query.clone()).collect();
        let results_payload = results
            .into_iter()
            .map(|(_, response)| serde_json::to_value(response))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                KagiError::Parse(format!(
                    "failed to serialize batch search response: {error}"
                ))
            })?;
        let results_json = serde_json::json!({
            "queries": queries,
            "results": results_payload
        });

        if format == "compact" {
            println!("{}", serde_json::to_string(&results_json)?);
        } else if format == "toon" {
            println!("{}", toon::encode(&results_json, None));
        } else {
            println!("{}", serde_json::to_string_pretty(&results_json)?);
        }
    } else {
        // For human-readable formats, output with headers
        for (query, response) in results {
            let output = if let Some(template) = template.as_deref() {
                format_template_response(&response, template)
            } else {
                match format.as_str() {
                    "pretty" => format_pretty_response(&response, use_color),
                    "markdown" => format_markdown_response(&response),
                    "csv" => format_csv_response(&response),
                    _ => serde_json::to_string_pretty(&response).map_err(|error| {
                        KagiError::Parse(format!("failed to serialize search response: {error}"))
                    })?,
                }
            };
            println!("=== Results for: {query} ===");
            println!("{output}");
            println!();
        }
    }

    if !failures.is_empty() {
        Err(KagiError::Batch(format_batch_failure_message(
            success_count,
            &failures,
        )))
    } else {
        Ok(())
    }
}

fn format_batch_failure_message(success_count: usize, failures: &[String]) -> String {
    let failure_count = failures.len();
    let query_word = if failure_count == 1 {
        "query"
    } else {
        "queries"
    };
    format!(
        "{failure_count} batch {query_word} failed ({success_count} succeeded): {}",
        failures.join("; ")
    )
}

async fn run_search_follow(
    request: search::SearchRequest,
    follow_count: usize,
    limit: Option<usize>,
    profile: Option<&str>,
) -> Result<(), KagiError> {
    let inventory = load_credential_inventory_for_profile(profile)?;
    let credentials = inventory.resolve_for_search(search_auth_requirement(&request))?;
    let mut response =
        apply_local_site_preferences(execute_search_request(&request, credentials).await?)?;
    if let Some(n) = limit {
        response.data.truncate(n);
    }
    let token = resolve_session_token(profile)?;
    let mut summaries = Vec::new();

    for result in response.data.iter().take(follow_count) {
        let summarize_request = SubscriberSummarizeRequest {
            url: Some(result.url.clone()),
            text: None,
            summary_type: None,
            target_language: None,
            length: None,
        };
        let summary = execute_subscriber_summarize(&summarize_request, &token).await?;
        summaries.push(serde_json::json!({
            "title": result.title,
            "url": result.url,
            "summary": summary,
        }));
    }

    record_history(
        "search-follow",
        Some(&request.query),
        Some(response.data.len()),
    )?;
    print_json(&serde_json::json!({
        "query": request.query,
        "search": response,
        "summaries": summaries,
    }))
}

async fn run_summarize_filter(
    args: cli::SummarizeArgs,
    profile: Option<&str>,
) -> Result<(), KagiError> {
    let lines = read_stdin_lines()?;
    if lines.is_empty() {
        return Err(KagiError::Config(
            "summarize --filter requires at least one stdin line".to_string(),
        ));
    }

    let mut results = Vec::new();
    if args.subscriber {
        let token = resolve_session_token(profile)?;
        for item in lines {
            let request = summarize_item_request_subscriber(&item, &args);
            let response = execute_subscriber_summarize(&request, &token).await?;
            results.push(serde_json::json!({ "input": item, "response": response }));
        }
    } else {
        let token = resolve_api_token(profile)?;
        for item in lines {
            let request = summarize_item_request_public(&item, &args);
            let response = execute_summarize(&request, &token).await?;
            results.push(serde_json::json!({ "input": item, "response": response }));
        }
    }

    print_json(&serde_json::json!({ "results": results }))
}

fn summarize_item_request_subscriber(
    item: &str,
    args: &cli::SummarizeArgs,
) -> SubscriberSummarizeRequest {
    let is_url = item.starts_with("http://") || item.starts_with("https://");
    SubscriberSummarizeRequest {
        url: is_url.then(|| item.to_string()),
        text: (!is_url).then(|| item.to_string()),
        summary_type: args.summary_type.clone(),
        target_language: args.target_language.clone(),
        length: args.length.clone(),
    }
}

fn summarize_item_request_public(item: &str, args: &cli::SummarizeArgs) -> SummarizeRequest {
    let is_url = item.starts_with("http://") || item.starts_with("https://");
    SummarizeRequest {
        url: is_url.then(|| item.to_string()),
        text: (!is_url).then(|| item.to_string()),
        engine: args.engine.clone(),
        summary_type: args.summary_type.clone(),
        target_language: args.target_language.clone(),
        cache: args.cache,
    }
}

async fn run_watch(args: WatchArgs, profile: Option<&str>) -> Result<(), KagiError> {
    if args.interval == 0 {
        return Err(KagiError::Config(
            "watch --interval must be at least 1 second".to_string(),
        ));
    }

    let format = args.format.to_string();
    let mut previous_urls = BTreeSet::new();
    let mut iteration = 0_u32;

    loop {
        iteration += 1;
        let request = search::SearchRequest::new(args.query.trim().to_string());
        let inventory = load_credential_inventory_for_profile(profile)?;
        let credentials = inventory.resolve_for_search(SearchAuthRequirement::Base)?;
        let response =
            apply_local_site_preferences(execute_search_request(&request, credentials).await?)?;
        let current_urls = response
            .data
            .iter()
            .map(|result| result.url.clone())
            .collect::<BTreeSet<_>>();
        let added = current_urls
            .difference(&previous_urls)
            .cloned()
            .collect::<Vec<_>>();
        let removed = previous_urls
            .difference(&current_urls)
            .cloned()
            .collect::<Vec<_>>();
        let event = serde_json::json!({
            "iteration": iteration,
            "query": args.query,
            "changed": iteration == 1 || !added.is_empty() || !removed.is_empty(),
            "added": added,
            "removed": removed,
            "result_count": response.data.len(),
        });

        match format.as_str() {
            "compact" => print_compact_json(&event)?,
            "toon" => print_toon(&event)?,
            "pretty" => println!(
                "watch #{iteration}: {} added, {} removed",
                event["added"].as_array().map_or(0, Vec::len),
                event["removed"].as_array().map_or(0, Vec::len)
            ),
            _ => print_json(&event)?,
        }

        record_history("watch", Some(&args.query), Some(response.data.len()))?;
        previous_urls = current_urls;
        if args.count > 0 && iteration >= args.count {
            break;
        }
        tokio::time::sleep(Duration::from_secs(args.interval)).await;
    }
    Ok(())
}

async fn run_notify(args: NotifyArgs, profile: Option<&str>) -> Result<(), KagiError> {
    let payload = if let Some(query) = args.query.as_ref() {
        let request = search::SearchRequest::new(query.trim().to_string());
        let inventory = load_credential_inventory_for_profile(profile)?;
        let credentials = inventory.resolve_for_search(SearchAuthRequirement::Base)?;
        let response = execute_search_request(&request, credentials).await?;
        if args.change_only {
            let key = local::cache_key(&["notify", query]);
            let current = serde_json::to_value(&response)?;
            if local::cache_get(&key)? == Some(current.clone()) {
                return Ok(());
            }
            local::cache_put(&key, u64::MAX / 2, &current)?;
        }
        serde_json::json!({ "kind": "search", "query": query, "response": response })
    } else {
        let category = args.news_category.unwrap_or_else(|| "world".to_string());
        let response = execute_news(&category, 12, "default", None).await?;
        serde_json::json!({ "kind": "news", "category": category, "response": response })
    };

    let client = http::client_20s()?;
    let response = client
        .post(&args.webhook_url)
        .json(&payload)
        .send()
        .await
        .map_err(http::map_transport_error)?;
    if !response.status().is_success() {
        return Err(KagiError::Network(format!(
            "webhook rejected notification: HTTP {}",
            response.status()
        )));
    }
    print_json(&serde_json::json!({ "sent": true }))
}

fn run_history(command: HistorySubcommand) -> Result<(), KagiError> {
    match command {
        HistorySubcommand::List(args) => print_json(&local::read_history(args.limit)?),
        HistorySubcommand::Stats => print_json(&local::history_stats()?),
    }
}

fn run_site_pref(command: SitePrefSubcommand) -> Result<(), KagiError> {
    match command {
        SitePrefSubcommand::List => print_json(&local::load_site_preferences()?),
        SitePrefSubcommand::Set(args) => {
            let mut preferences = local::load_site_preferences()?;
            let domain = local::normalize_domain(&args.domain)?;
            preferences
                .domains
                .insert(domain.clone(), site_pref_mode(args.mode));
            local::save_site_preferences(&preferences)?;
            print_json(
                &serde_json::json!({ "domain": domain, "mode": site_pref_mode(args.mode).as_str() }),
            )
        }
        SitePrefSubcommand::Remove(args) => {
            let mut preferences = local::load_site_preferences()?;
            let domain = local::normalize_domain(&args.domain)?;
            preferences.domains.remove(&domain);
            local::save_site_preferences(&preferences)?;
            print_json(&serde_json::json!({ "domain": domain, "removed": true }))
        }
    }
}

const fn site_pref_mode(mode: SitePrefMode) -> local::SitePreferenceMode {
    match mode {
        SitePrefMode::Block => local::SitePreferenceMode::Block,
        SitePrefMode::Lower => local::SitePreferenceMode::Lower,
        SitePrefMode::Normal => local::SitePreferenceMode::Normal,
        SitePrefMode::Higher => local::SitePreferenceMode::Higher,
        SitePrefMode::Pin => local::SitePreferenceMode::Pin,
    }
}

async fn run_assistant_repl(args: AssistantReplArgs, token: &str) -> Result<(), KagiError> {
    let mut thread_id = args.thread_id;
    let mut transcript = Vec::new();
    let stdin = io::stdin();

    eprintln!("kagi assistant repl. Type /exit to quit, /thread to print current thread.");
    loop {
        eprint!("kagi> ");
        io::stderr().flush().ok();
        let mut line = String::new();
        stdin
            .read_line(&mut line)
            .map_err(|error| KagiError::Config(format!("failed to read stdin: {error}")))?;
        let prompt = line.trim();
        if prompt.is_empty() {
            continue;
        }
        if prompt == "/exit" || prompt == "/quit" {
            break;
        }
        if prompt == "/thread" {
            println!("{}", thread_id.as_deref().unwrap_or("<new>"));
            continue;
        }
        if let Some(model) = prompt.strip_prefix("/model ").map(str::trim) {
            eprintln!("model switching is per prompt in this REPL; restart with --model {model}");
            continue;
        }

        let request = AssistantPromptRequest {
            query: prompt.to_string(),
            thread_id: thread_id.clone(),
            attachments: vec![],
            profile_id: normalize_optional_string(args.assistant.clone()),
            model: args.model.clone(),
            lens_id: None,
            internet_access: None,
            personalizations: None,
        };
        let response = execute_assistant_prompt(&request, token).await?;
        thread_id = Some(response.thread.id.clone());
        print_assistant_response(&response, args.format.clone(), !args.no_color)?;
        transcript.push(serde_json::json!({ "prompt": prompt, "response": response }));
    }

    if let Some(path) = args.export {
        let raw = serde_json::to_string_pretty(&transcript)?;
        fs::write(&path, raw).map_err(|error| {
            KagiError::Config(format!(
                "failed to write transcript {}: {error}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct McpServerConfig {
    default_output: Option<OutputFormat>,
    enable_mutating_tools: bool,
    tool_definitions: Vec<Value>,
}

const MCP_PROTOCOL_VERSION: &str = "2026-07-28";
const MCP_PROTOCOL_VERSION_META_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const MCP_CLIENT_INFO_META_KEY: &str = "io.modelcontextprotocol/clientInfo";
const MCP_CLIENT_CAPABILITIES_META_KEY: &str = "io.modelcontextprotocol/clientCapabilities";
const MCP_CACHE_TTL_MS: u64 = 3_600_000;

#[derive(Debug)]
struct McpProtocolError {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl McpProtocolError {
    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }
}

impl McpServerConfig {
    fn new(default_output: Option<OutputFormat>, enable_mutating_tools: bool) -> Self {
        let tool_definitions = build_mcp_tool_definitions(enable_mutating_tools);
        Self {
            default_output,
            enable_mutating_tools,
            tool_definitions,
        }
    }

    fn default_output_or(&self, fallback: OutputFormat) -> OutputFormat {
        self.default_output.clone().unwrap_or(fallback)
    }
}

async fn run_mcp(args: McpArgs, profile: Option<&str>) -> Result<(), KagiError> {
    let _json_lines = args.json_lines;
    let config = McpServerConfig::new(args.default_output, args.enable_mutating_tools);
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line =
            line.map_err(|error| KagiError::Config(format!("failed to read stdin: {error}")))?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                let response = json_rpc_error(Value::Null, -32700, format!("Parse error: {error}"));
                println!("{}", serde_json::to_string(&response)?);
                continue;
            }
        };
        // Notifications have no `id` field; per JSON-RPC 2.0 they must not be answered.
        let Some(id) = request.get("id").cloned() else {
            continue;
        };

        let Some(method) = request.get("method").and_then(Value::as_str) else {
            let response = json_rpc_error(id, -32600, "Invalid Request: method is required".into());
            println!("{}", serde_json::to_string(&response)?);
            continue;
        };

        let response = match validate_mcp_request(&request) {
            Ok(()) => match method {
                "server/discover" => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": mcp_discover_result(),
                }),
                "tools/list" => match mcp_tools_list_result(&request, &config) {
                    Ok(result) => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result,
                    }),
                    Err(error) => {
                        json_rpc_error_with_data(id, error.code, error.message, error.data)
                    }
                },
                "tools/call" => match validate_mcp_tool_call(&request, &config) {
                    Ok(()) => match run_mcp_tool_call(&request, profile, &config).await {
                        Ok(result) => serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": result,
                        }),
                        Err(error) => serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": mcp_tool_result(error.to_string(), true, false),
                        }),
                    },
                    Err(error) => {
                        json_rpc_error_with_data(id, error.code, error.message, error.data)
                    }
                },
                _ => json_rpc_error(id, -32601, format!("Method not found: {method}")),
            },
            Err(error) => json_rpc_error_with_data(id, error.code, error.message, error.data),
        };
        println!("{}", serde_json::to_string(&response)?);
    }
    Ok(())
}

fn json_rpc_error(id: Value, code: i64, message: String) -> Value {
    json_rpc_error_with_data(id, code, message, None)
}

fn json_rpc_error_with_data(id: Value, code: i64, message: String, data: Option<Value>) -> Value {
    let mut error = serde_json::json!({
        "code": code,
        "message": message,
    });
    if let Some(data) = data {
        error["data"] = data;
    }

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": error,
    })
}

fn validate_mcp_request(request: &Value) -> Result<(), McpProtocolError> {
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(McpProtocolError {
            code: -32600,
            message: "Invalid Request: jsonrpc must be \"2.0\"".to_string(),
            data: None,
        });
    }

    let params = request
        .get("params")
        .and_then(Value::as_object)
        .ok_or_else(|| McpProtocolError::invalid_params("MCP request params must be an object"))?;
    let meta = params
        .get("_meta")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            McpProtocolError::invalid_params(
                "MCP request params._meta must include protocolVersion and clientCapabilities",
            )
        })?;
    let requested_version = meta
        .get(MCP_PROTOCOL_VERSION_META_KEY)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            McpProtocolError::invalid_params(format!(
                "MCP request params._meta.{MCP_PROTOCOL_VERSION_META_KEY} is required"
            ))
        })?;
    if requested_version != MCP_PROTOCOL_VERSION {
        return Err(McpProtocolError {
            code: -32022,
            message: "Unsupported protocol version".to_string(),
            data: Some(serde_json::json!({
                "supported": [MCP_PROTOCOL_VERSION],
                "requested": requested_version,
            })),
        });
    }

    if !meta
        .get(MCP_CLIENT_CAPABILITIES_META_KEY)
        .is_some_and(Value::is_object)
    {
        return Err(McpProtocolError::invalid_params(format!(
            "MCP request params._meta.{MCP_CLIENT_CAPABILITIES_META_KEY} is required and must be an object"
        )));
    }

    if let Some(client_info) = meta.get(MCP_CLIENT_INFO_META_KEY) {
        let valid_client_info = client_info.as_object().is_some_and(|client_info| {
            client_info.get("name").and_then(Value::as_str).is_some()
                && client_info.get("version").and_then(Value::as_str).is_some()
        });
        if !valid_client_info {
            return Err(McpProtocolError::invalid_params(format!(
                "MCP request params._meta.{MCP_CLIENT_INFO_META_KEY} must include name and version"
            )));
        }
    }

    Ok(())
}

fn mcp_discover_result() -> Value {
    serde_json::json!({
        "resultType": "complete",
        "supportedVersions": [MCP_PROTOCOL_VERSION],
        "capabilities": {
            "tools": {}
        },
        "_meta": {
            "io.modelcontextprotocol/serverInfo": {
                "name": "kagi-cli",
                "version": env!("CARGO_PKG_VERSION")
            }
        },
        "instructions": "Search Kagi web and news, extract and summarize pages, inspect Assistant threads, and inspect Kagi account or local CLI state. Prompting Assistant or changing account and local state requires --enable-mutating-tools.",
        "ttlMs": MCP_CACHE_TTL_MS,
        "cacheScope": "public"
    })
}

fn mcp_tools_list_result(
    request: &Value,
    config: &McpServerConfig,
) -> Result<Value, McpProtocolError> {
    let params = request
        .get("params")
        .and_then(Value::as_object)
        .expect("validated MCP request params should be an object");
    if params.get("cursor").is_some_and(|cursor| !cursor.is_null()) {
        return Err(McpProtocolError::invalid_params(
            "MCP tools/list does not issue pagination cursors because the complete tool catalog fits in one response",
        ));
    }

    Ok(serde_json::json!({
        "resultType": "complete",
        "tools": config.tool_definitions.clone(),
        "ttlMs": MCP_CACHE_TTL_MS,
        "cacheScope": "public"
    }))
}

fn validate_mcp_tool_call(
    request: &Value,
    config: &McpServerConfig,
) -> Result<(), McpProtocolError> {
    let params = request
        .get("params")
        .and_then(Value::as_object)
        .expect("validated MCP request params should be an object");
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| {
            McpProtocolError::invalid_params("MCP tools/call requires a non-empty name")
        })?;
    if !config
        .tool_definitions
        .iter()
        .any(|tool| tool["name"].as_str() == Some(name))
    {
        return Err(McpProtocolError {
            code: -32602,
            message: format!("Unknown tool: {name}"),
            data: None,
        });
    }

    if let Some(arguments) = params.get("arguments")
        && !arguments.is_object()
    {
        return Err(McpProtocolError::invalid_params(
            "MCP tools/call arguments must be an object",
        ));
    }

    Ok(())
}

fn mcp_tool_result(text: String, is_error: bool, include_structured_content: bool) -> Value {
    let mut result = serde_json::json!({
        "resultType": "complete",
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    });
    if include_structured_content && !is_error {
        let text = result["content"][0]["text"]
            .as_str()
            .expect("MCP text content should be a string");
        if let Ok(structured_content) = serde_json::from_str::<Value>(text) {
            result["structuredContent"] = structured_content;
        }
    }
    result
}

fn mcp_tool_has_structured_content(
    name: &str,
    arguments: &Value,
    config: &McpServerConfig,
) -> Result<bool, KagiError> {
    let default = config.default_output_or(OutputFormat::Json);
    // Only these tools honor a per-call `format` field. The remaining tools
    // either use the server default, always return text, or have a format
    // mapping handled explicitly below.
    match name {
        "kagi_auth_status" | "kagi_auth_check" => Ok(false),
        "kagi_extract" => Ok(matches!(
            mcp_extract_output_format(arguments, config)?,
            OutputFormat::Json | OutputFormat::Compact
        )),
        "kagi_quick" => Ok(matches!(
            mcp_quick_format(arguments, &default)?,
            QuickOutputFormat::Json | QuickOutputFormat::Compact
        )),
        "kagi_assistant" => Ok(matches!(
            mcp_assistant_format(arguments, &default)?,
            AssistantOutputFormat::Json | AssistantOutputFormat::Compact
        )),
        "kagi_assistant_thread_export" => {
            match mcp_string_or(arguments, "format", "markdown").as_str() {
                "json" => Ok(true),
                "markdown" => Ok(false),
                other => Err(KagiError::Config(format!(
                    "unsupported thread export format `{other}`. Use markdown or json"
                ))),
            }
        }
        "kagi_search" => {
            let is_json = is_structured_output_format(&mcp_output_format(arguments, &default)?);
            let uses_template = arguments.get("template").and_then(Value::as_str).is_some()
                && arguments.get("news").and_then(Value::as_bool) != Some(true);
            Ok(is_json && !uses_template)
        }
        "kagi_batch_search"
        | "kagi_summarize"
        | "kagi_news"
        | "kagi_news_categories"
        | "kagi_news_chaos"
        | "kagi_news_filter_presets"
        | "kagi_news_search"
        | "kagi_ask_page"
        | "kagi_translate"
        | "kagi_fastgpt"
        | "kagi_enrich_web"
        | "kagi_enrich_news"
        | "kagi_smallweb"
        | "kagi_cli" => Ok(is_structured_output_format(&mcp_output_format(
            arguments, &default,
        )?)),
        _ => Ok(is_structured_output_format(&default)),
    }
}

fn is_structured_output_format(format: &OutputFormat) -> bool {
    matches!(format, OutputFormat::Json | OutputFormat::Compact)
}

fn build_mcp_tool_definitions(enable_mutating_tools: bool) -> Vec<Value> {
    let mut tools = vec![
        tool_schema(
            "kagi_search",
            "Search Kagi web or news results",
            search_schema(),
        ),
        tool_schema(
            "kagi_batch_search",
            "Run multiple Kagi searches with shared filters",
            batch_search_schema(),
        ),
        tool_schema(
            "kagi_summarize",
            "Summarize a URL or text through public API or subscriber summarizer",
            summarize_schema(),
        ),
        tool_schema(
            "kagi_extract",
            "Extract a page's full content through Kagi Extract",
            extract_schema(),
        ),
        tool_schema("kagi_quick", "Get a Kagi Quick Answer", quick_schema()),
        tool_schema(
            "kagi_news",
            "Fetch Kagi News stories for a category",
            news_schema(),
        ),
        tool_schema(
            "kagi_news_categories",
            "List Kagi News categories",
            lang_only_schema(),
        ),
        tool_schema(
            "kagi_news_chaos",
            "Fetch the Kagi News chaos index",
            lang_only_schema(),
        ),
        tool_schema(
            "kagi_news_filter_presets",
            "List built-in Kagi News content-filter presets",
            lang_only_schema(),
        ),
        tool_schema(
            "kagi_news_search",
            "Search the News tab of kagi.com",
            news_search_schema(),
        ),
        tool_schema(
            "kagi_assistant_models",
            "List Assistant base-model slugs",
            empty_schema(),
        ),
        tool_schema(
            "kagi_assistant_thread_list",
            "List Assistant threads",
            empty_schema(),
        ),
        tool_schema(
            "kagi_assistant_thread_get",
            "Fetch an Assistant thread by id",
            target_schema("thread_id", "Assistant thread id"),
        ),
        tool_schema(
            "kagi_assistant_thread_export",
            "Export an Assistant thread as markdown or JSON",
            thread_export_schema(),
        ),
        tool_schema(
            "kagi_assistant_custom_list",
            "List custom and built-in assistants",
            empty_schema(),
        ),
        tool_schema(
            "kagi_assistant_custom_get",
            "Fetch a custom assistant by id or name",
            target_schema("target", "Custom assistant id or exact name"),
        ),
        tool_schema(
            "kagi_translate",
            "Translate text through Kagi Translate",
            translate_schema(),
        ),
        tool_schema(
            "kagi_fastgpt",
            "Answer a query with Kagi FastGPT",
            fastgpt_schema(),
        ),
        tool_schema(
            "kagi_enrich_web",
            "Query Kagi's Teclis web enrichment index",
            query_schema("Query to enrich"),
        ),
        tool_schema(
            "kagi_enrich_news",
            "Query Kagi's TinyGem news enrichment index",
            query_schema("Query to enrich"),
        ),
        tool_schema(
            "kagi_smallweb",
            "Fetch the Kagi Small Web feed",
            smallweb_schema(),
        ),
        tool_schema("kagi_lens_list", "List Kagi search lenses", empty_schema()),
        tool_schema(
            "kagi_lens_get",
            "Fetch one Kagi search lens by id or name",
            target_schema("target", "Lens id or exact name"),
        ),
        tool_schema("kagi_custom_bang_list", "List custom bangs", empty_schema()),
        tool_schema(
            "kagi_custom_bang_get",
            "Fetch one custom bang by id, name, or trigger",
            target_schema("target", "Bang id, exact name, or trigger"),
        ),
        tool_schema("kagi_redirect_list", "List redirect rules", empty_schema()),
        tool_schema(
            "kagi_redirect_get",
            "Fetch one redirect rule by id or exact rule text",
            target_schema("target", "Redirect id or exact rule text"),
        ),
        tool_schema(
            "kagi_auth_status",
            "Show configured credential status",
            empty_schema(),
        ),
        tool_schema(
            "kagi_auth_check",
            "Validate selected credentials",
            empty_schema(),
        ),
        tool_schema(
            "kagi_history_list",
            "List local command history",
            history_list_schema(),
        ),
        tool_schema(
            "kagi_history_stats",
            "Show local command history stats",
            empty_schema(),
        ),
        tool_schema(
            "kagi_site_pref_list",
            "List local search site preferences",
            empty_schema(),
        ),
    ];

    if enable_mutating_tools {
        tools.extend([
            tool_schema(
                "kagi_assistant",
                "Prompt Kagi Assistant",
                assistant_schema(),
            ),
            tool_schema(
                "kagi_ask_page",
                "Ask Assistant about a page",
                ask_page_schema(),
            ),
            tool_schema(
                "kagi_assistant_thread_delete",
                "Delete an Assistant thread",
                target_schema("thread_id", "Assistant thread id"),
            ),
            tool_schema(
                "kagi_assistant_custom_create",
                "Create a custom assistant",
                assistant_custom_create_schema(),
            ),
            tool_schema(
                "kagi_assistant_custom_update",
                "Update a custom assistant",
                assistant_custom_update_schema(),
            ),
            tool_schema(
                "kagi_assistant_custom_delete",
                "Delete a custom assistant",
                target_schema("target", "Custom assistant id or exact name"),
            ),
            tool_schema(
                "kagi_lens_create",
                "Create a Kagi search lens",
                lens_create_schema(),
            ),
            tool_schema(
                "kagi_lens_update",
                "Update a Kagi search lens",
                lens_update_schema(),
            ),
            tool_schema(
                "kagi_lens_delete",
                "Delete a Kagi search lens",
                target_schema("target", "Lens id or exact name"),
            ),
            tool_schema(
                "kagi_lens_enable",
                "Enable a Kagi search lens",
                target_schema("target", "Lens id or exact name"),
            ),
            tool_schema(
                "kagi_lens_disable",
                "Disable a Kagi search lens",
                target_schema("target", "Lens id or exact name"),
            ),
            tool_schema(
                "kagi_custom_bang_create",
                "Create a custom bang",
                custom_bang_create_schema(),
            ),
            tool_schema(
                "kagi_custom_bang_update",
                "Update a custom bang",
                custom_bang_update_schema(),
            ),
            tool_schema(
                "kagi_custom_bang_delete",
                "Delete a custom bang",
                target_schema("target", "Bang id, exact name, or trigger"),
            ),
            tool_schema(
                "kagi_redirect_create",
                "Create a redirect rule",
                redirect_create_schema(),
            ),
            tool_schema(
                "kagi_redirect_update",
                "Update a redirect rule",
                redirect_update_schema(),
            ),
            tool_schema(
                "kagi_redirect_delete",
                "Delete a redirect rule",
                target_schema("target", "Redirect id or exact rule text"),
            ),
            tool_schema(
                "kagi_redirect_enable",
                "Enable a redirect rule",
                target_schema("target", "Redirect id or exact rule text"),
            ),
            tool_schema(
                "kagi_redirect_disable",
                "Disable a redirect rule",
                target_schema("target", "Redirect id or exact rule text"),
            ),
            tool_schema(
                "kagi_site_pref_set",
                "Set a local search site preference",
                site_pref_set_schema(),
            ),
            tool_schema(
                "kagi_site_pref_remove",
                "Remove a local search site preference",
                target_schema("domain", "Domain to remove"),
            ),
            tool_schema(
                "kagi_cli",
                "Run an arbitrary kagi CLI command as an escape hatch for exact CLI parity",
                cli_passthrough_schema(),
            ),
        ]);
    }

    tools.sort_by(|left, right| {
        left["name"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["name"].as_str().unwrap_or_default())
    });
    tools
}

fn tool_schema(name: &str, description: &str, input_schema: Value) -> Value {
    serde_json::json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "annotations": mcp_tool_annotations(name),
    })
}

fn mcp_tool_annotations(name: &str) -> Value {
    let read_only = !mcp_mutating_tool_name(name);
    let mut annotations = serde_json::json!({
        "readOnlyHint": read_only,
        "openWorldHint": mcp_open_world_tool_name(name),
    });
    if !read_only {
        if mcp_destructive_tool_name(name) {
            annotations["destructiveHint"] = serde_json::json!(true);
        }
        annotations["idempotentHint"] = serde_json::json!(mcp_idempotent_tool_name(name));
    }
    annotations
}

fn mcp_mutating_tool_name(name: &str) -> bool {
    matches!(
        name,
        "kagi_assistant"
            | "kagi_ask_page"
            | "kagi_assistant_thread_delete"
            | "kagi_assistant_custom_create"
            | "kagi_assistant_custom_update"
            | "kagi_assistant_custom_delete"
            | "kagi_lens_create"
            | "kagi_lens_update"
            | "kagi_lens_delete"
            | "kagi_lens_enable"
            | "kagi_lens_disable"
            | "kagi_custom_bang_create"
            | "kagi_custom_bang_update"
            | "kagi_custom_bang_delete"
            | "kagi_redirect_create"
            | "kagi_redirect_update"
            | "kagi_redirect_delete"
            | "kagi_redirect_enable"
            | "kagi_redirect_disable"
            | "kagi_site_pref_set"
            | "kagi_site_pref_remove"
            | "kagi_cli"
    )
}

fn mcp_destructive_tool_name(name: &str) -> bool {
    matches!(
        name,
        "kagi_assistant_thread_delete"
            | "kagi_assistant_custom_delete"
            | "kagi_lens_delete"
            | "kagi_custom_bang_delete"
            | "kagi_redirect_delete"
            | "kagi_site_pref_remove"
            | "kagi_cli"
    )
}

fn mcp_idempotent_tool_name(name: &str) -> bool {
    matches!(
        name,
        "kagi_assistant_thread_delete"
            | "kagi_assistant_custom_update"
            | "kagi_assistant_custom_delete"
            | "kagi_lens_update"
            | "kagi_lens_delete"
            | "kagi_lens_enable"
            | "kagi_lens_disable"
            | "kagi_custom_bang_update"
            | "kagi_custom_bang_delete"
            | "kagi_redirect_update"
            | "kagi_redirect_delete"
            | "kagi_redirect_enable"
            | "kagi_redirect_disable"
            | "kagi_site_pref_set"
            | "kagi_site_pref_remove"
    )
}

fn mcp_open_world_tool_name(name: &str) -> bool {
    matches!(
        name,
        "kagi_search"
            | "kagi_batch_search"
            | "kagi_summarize"
            | "kagi_extract"
            | "kagi_quick"
            | "kagi_news"
            | "kagi_news_categories"
            | "kagi_news_chaos"
            | "kagi_news_filter_presets"
            | "kagi_news_search"
            | "kagi_assistant"
            | "kagi_ask_page"
            | "kagi_translate"
            | "kagi_fastgpt"
            | "kagi_enrich_web"
            | "kagi_enrich_news"
            | "kagi_smallweb"
            | "kagi_cli"
    )
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn empty_schema() -> Value {
    object_schema(serde_json::json!({}), &[])
}

fn query_schema(description: &str) -> Value {
    object_schema(
        serde_json::json!({
            "query": {"type": "string", "description": description},
            "format": output_format_schema(),
        }),
        &["query"],
    )
}

fn target_schema(name: &str, description: &str) -> Value {
    object_schema(
        serde_json::json!({name: {"type": "string", "description": description}}),
        &[name],
    )
}

fn output_format_schema() -> Value {
    serde_json::json!({
        "type": "string",
        "description": "Output format; defaults to the MCP server --default-output",
        "enum": ["json", "toon", "pretty", "compact", "markdown", "csv"]
    })
}

fn search_schema() -> Value {
    object_schema(
        serde_json::json!({
            "query": {"type": "string"},
            "format": output_format_schema(),
            "snap": {"type": "string"},
            "lens": {"type": "string"},
            "region": {"type": "string"},
            "time": {"type": "string", "enum": ["day", "week", "month", "year"]},
            "from_date": {"type": "string"},
            "to_date": {"type": "string"},
            "order": {"type": "string", "enum": ["default", "recency", "website", "trackers"]},
            "verbatim": {"type": "boolean"},
            "personalized": {"type": "boolean"},
            "no_personalized": {"type": "boolean"},
            "template": {"type": "string"},
            "local_cache": {"type": "boolean"},
            "cache_ttl": {"type": "integer", "minimum": 1},
            "follow": {"type": "integer", "minimum": 1},
            "limit": {"type": "integer", "minimum": 1},
            "news": {"type": "boolean"}
        }),
        &["query"],
    )
}

fn batch_search_schema() -> Value {
    object_schema(
        serde_json::json!({
            "queries": {"type": "array", "items": {"type": "string"}, "minItems": 1},
            "concurrency": {"type": "integer", "minimum": 1, "default": 3},
            "rate_limit": {"type": "integer", "minimum": 1, "default": 60},
            "format": output_format_schema(),
            "snap": {"type": "string"},
            "lens": {"type": "string"},
            "region": {"type": "string"},
            "time": {"type": "string", "enum": ["day", "week", "month", "year"]},
            "from_date": {"type": "string"},
            "to_date": {"type": "string"},
            "order": {"type": "string", "enum": ["default", "recency", "website", "trackers"]},
            "verbatim": {"type": "boolean"},
            "personalized": {"type": "boolean"},
            "no_personalized": {"type": "boolean"},
            "template": {"type": "string"},
            "limit": {"type": "integer", "minimum": 1}
        }),
        &["queries"],
    )
}

fn summarize_schema() -> Value {
    let mut schema = object_schema(
        serde_json::json!({
            "url": {"type": "string"},
            "text": {"type": "string"},
            "subscriber": {"type": "boolean"},
            "length": {"type": "string"},
            "engine": {"type": "string"},
            "summary_type": {"type": "string"},
            "target_language": {"type": "string"},
            "cache": {"type": "boolean"},
            "local_cache": {"type": "boolean"},
            "cache_ttl": {"type": "integer", "minimum": 1},
            "format": output_format_schema()
        }),
        &[],
    );
    schema["anyOf"] = serde_json::json!([{ "required": ["url"] }, { "required": ["text"] }]);
    schema
}

fn extract_schema() -> Value {
    object_schema(
        serde_json::json!({
            "url": {"type": "string"},
            "format": {"type": "string", "enum": ["markdown", "json", "compact", "toon"]}
        }),
        &["url"],
    )
}

fn quick_schema() -> Value {
    object_schema(
        serde_json::json!({
            "query": {"type": "string"},
            "format": {"type": "string", "enum": ["json", "toon", "pretty", "compact", "markdown"]},
            "lens": {"type": "string"},
            "local_cache": {"type": "boolean"},
            "cache_ttl": {"type": "integer", "minimum": 1}
        }),
        &["query"],
    )
}

fn news_schema() -> Value {
    object_schema(
        serde_json::json!({
            "category": {"type": "string", "default": "world"},
            "lang": {"type": "string", "default": "default"},
            "limit": {"type": "integer", "minimum": 1, "default": 12},
            "filter_preset": {"type": "array", "items": {"type": "string"}},
            "filter_keyword": {"type": "array", "items": {"type": "string"}},
            "filter_mode": {"type": "string", "enum": ["hide", "blur"], "default": "hide"},
            "filter_scope": {"type": "string", "enum": ["title", "summary", "all"], "default": "all"},
            "format": output_format_schema()
        }),
        &[],
    )
}

fn lang_only_schema() -> Value {
    object_schema(
        serde_json::json!({
            "lang": {"type": "string", "default": "default"},
            "format": output_format_schema()
        }),
        &[],
    )
}

fn news_search_schema() -> Value {
    object_schema(
        serde_json::json!({
            "query": {"type": "string"},
            "region": {"type": "string"},
            "freshness": {"type": "string", "enum": ["day", "week", "month"]},
            "order": {"type": "string", "enum": ["default", "recency", "website"]},
            "dir_desc": {"type": "boolean"},
            "limit": {"type": "integer", "minimum": 1},
            "format": output_format_schema()
        }),
        &["query"],
    )
}

fn assistant_schema() -> Value {
    object_schema(
        serde_json::json!({
            "query": {"type": "string"},
            "thread_id": {"type": "string"},
            "attach": {"type": "array", "items": {"type": "string"}},
            "assistant": {"type": "string"},
            "format": {"type": "string", "enum": ["json", "toon", "pretty", "compact", "markdown"]},
            "contract": {"type": "string", "enum": ["decision", "checklist", "plan"]},
            "contract_file": {"type": "string"},
            "model": {"type": "string"},
            "once": {"type": "boolean"},
            "lens": {"type": "integer"},
            "web_access": {"type": "boolean"},
            "no_web_access": {"type": "boolean"},
            "personalized": {"type": "boolean"},
            "no_personalized": {"type": "boolean"}
        }),
        &["query"],
    )
}

fn thread_export_schema() -> Value {
    object_schema(
        serde_json::json!({
            "thread_id": {"type": "string"},
            "format": {"type": "string", "enum": ["markdown", "json"], "default": "markdown"}
        }),
        &["thread_id"],
    )
}

fn ask_page_schema() -> Value {
    object_schema(
        serde_json::json!({
            "url": {"type": "string"},
            "question": {"type": "string"},
            "format": output_format_schema()
        }),
        &["url", "question"],
    )
}

fn translate_schema() -> Value {
    object_schema(
        serde_json::json!({
            "text": {"type": "string"},
            "from": {"type": "string", "default": "auto"},
            "to": {"type": "string", "default": "en"},
            "quality": {"type": "string"},
            "model": {"type": "string"},
            "prediction": {"type": "string"},
            "predicted_language": {"type": "string"},
            "formality": {"type": "string"},
            "speaker_gender": {"type": "string"},
            "addressee_gender": {"type": "string"},
            "language_complexity": {"type": "string"},
            "translation_style": {"type": "string"},
            "context": {"type": "string"},
            "dictionary_language": {"type": "string"},
            "time_format": {"type": "string"},
            "use_definition_context": {"type": "boolean"},
            "enable_language_features": {"type": "boolean"},
            "preserve_formatting": {"type": "boolean"},
            "context_memory": {"type": "array"},
            "fetch_alternatives": {"type": "boolean", "default": true},
            "fetch_word_insights": {"type": "boolean", "default": true},
            "fetch_suggestions": {"type": "boolean", "default": true},
            "fetch_alignments": {"type": "boolean", "default": true},
            "format": output_format_schema()
        }),
        &["text"],
    )
}

fn fastgpt_schema() -> Value {
    object_schema(
        serde_json::json!({
            "query": {"type": "string"},
            "cache": {"type": "boolean"},
            "web_search": {"type": "boolean"},
            "local_cache": {"type": "boolean"},
            "cache_ttl": {"type": "integer", "minimum": 1},
            "format": output_format_schema()
        }),
        &["query"],
    )
}

fn smallweb_schema() -> Value {
    object_schema(
        serde_json::json!({
            "limit": {"type": "integer", "minimum": 1},
            "format": output_format_schema()
        }),
        &[],
    )
}

fn assistant_custom_create_schema() -> Value {
    object_schema(
        serde_json::json!({
            "name": {"type": "string"},
            "bang_trigger": {"type": "string"},
            "web_access": {"type": "boolean"},
            "no_web_access": {"type": "boolean"},
            "lens": {"type": "string"},
            "personalized": {"type": "boolean"},
            "no_personalized": {"type": "boolean"},
            "model": {"type": "string"},
            "instructions": {"type": "string"}
        }),
        &["name"],
    )
}

fn assistant_custom_update_schema() -> Value {
    let mut schema = assistant_custom_create_schema();
    schema["properties"]["target"] = serde_json::json!({"type": "string"});
    schema["required"] = serde_json::json!(["target"]);
    schema
}

fn lens_create_schema() -> Value {
    object_schema(
        serde_json::json!({
            "name": {"type": "string"},
            "included_sites": {"type": "string"},
            "included_keywords": {"type": "string"},
            "description": {"type": "string"},
            "region": {"type": "string"},
            "before_date": {"type": "string"},
            "after_date": {"type": "string"},
            "excluded_sites": {"type": "string"},
            "excluded_keywords": {"type": "string"},
            "shortcut": {"type": "string"},
            "autocomplete_keywords": {"type": "boolean"},
            "no_autocomplete_keywords": {"type": "boolean"},
            "template": {"type": "string", "enum": ["default", "news"]},
            "file_type": {"type": "string"},
            "share_with_team": {"type": "boolean"},
            "no_share_with_team": {"type": "boolean"},
            "share_copy_code": {"type": "boolean"},
            "no_share_copy_code": {"type": "boolean"}
        }),
        &["name"],
    )
}

fn lens_update_schema() -> Value {
    let mut schema = lens_create_schema();
    schema["properties"]["target"] = serde_json::json!({"type": "string"});
    schema["required"] = serde_json::json!(["target"]);
    schema
}

fn custom_bang_create_schema() -> Value {
    object_schema(
        serde_json::json!({
            "name": {"type": "string"},
            "trigger": {"type": "string"},
            "template": {"type": "string"},
            "snap_domain": {"type": "string"},
            "regex_pattern": {"type": "string"},
            "shortcut_menu": {"type": "boolean"},
            "no_shortcut_menu": {"type": "boolean"},
            "open_snap_domain": {"type": "boolean"},
            "no_open_snap_domain": {"type": "boolean"},
            "open_base_path": {"type": "boolean"},
            "no_open_base_path": {"type": "boolean"},
            "encode_placeholder": {"type": "boolean"},
            "no_encode_placeholder": {"type": "boolean"},
            "plus_for_space": {"type": "boolean"},
            "no_plus_for_space": {"type": "boolean"}
        }),
        &["name", "trigger"],
    )
}

fn custom_bang_update_schema() -> Value {
    let mut schema = custom_bang_create_schema();
    schema["properties"]["target"] = serde_json::json!({"type": "string"});
    schema["required"] = serde_json::json!(["target"]);
    schema
}

fn redirect_create_schema() -> Value {
    object_schema(serde_json::json!({"rule": {"type": "string"}}), &["rule"])
}

fn redirect_update_schema() -> Value {
    object_schema(
        serde_json::json!({
            "target": {"type": "string"},
            "rule": {"type": "string"}
        }),
        &["target", "rule"],
    )
}

fn site_pref_set_schema() -> Value {
    object_schema(
        serde_json::json!({
            "domain": {"type": "string"},
            "mode": {"type": "string", "enum": ["block", "lower", "normal", "higher", "pin"]}
        }),
        &["domain", "mode"],
    )
}

fn history_list_schema() -> Value {
    object_schema(
        serde_json::json!({"limit": {"type": "integer", "minimum": 1, "default": 25}}),
        &[],
    )
}

fn cli_passthrough_schema() -> Value {
    object_schema(
        serde_json::json!({
            "args": {
                "type": "array",
                "items": {"type": "string"},
                "minItems": 1,
                "description": "Arguments passed to the kagi binary, excluding the binary name"
            },
            "stdin": {
                "type": "string",
                "description": "Optional stdin to pass to the command"
            },
            "format": output_format_schema()
        }),
        &["args"],
    )
}

async fn run_mcp_tool_call(
    request: &Value,
    profile: Option<&str>,
    config: &McpServerConfig,
) -> Result<Value, KagiError> {
    let params = request
        .get("params")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let include_structured_content = mcp_tool_has_structured_content(name, &arguments, config)?;
    let text = match name {
        "kagi_search" => mcp_search(&arguments, profile, config).await?,
        "kagi_batch_search" => mcp_batch_search(&arguments, profile, config).await?,
        "kagi_summarize" => mcp_summarize(&arguments, profile, config).await?,
        "kagi_extract" => mcp_extract(&arguments, profile, config).await?,
        "kagi_quick" => mcp_quick(&arguments, profile, config).await?,
        "kagi_news" => mcp_news(&arguments, config).await?,
        "kagi_news_categories" => mcp_output(
            &execute_news_categories(mcp_string_or(&arguments, "lang", "default").as_str()).await?,
            mcp_output_format(&arguments, &config.default_output_or(OutputFormat::Json))?,
        )?,
        "kagi_news_chaos" => mcp_output(
            &execute_news_chaos(mcp_string_or(&arguments, "lang", "default").as_str()).await?,
            mcp_output_format(&arguments, &config.default_output_or(OutputFormat::Json))?,
        )?,
        "kagi_news_filter_presets" => mcp_output(
            &execute_news_filter_presets(mcp_string_or(&arguments, "lang", "default").as_str())?,
            mcp_output_format(&arguments, &config.default_output_or(OutputFormat::Json))?,
        )?,
        "kagi_news_search" => mcp_news_search(&arguments, profile, config).await?,
        "kagi_assistant_models" => {
            let token = resolve_session_token(profile)?;
            mcp_output(
                &execute_assistant_model_catalog(&token).await?,
                config.default_output_or(OutputFormat::Json),
            )?
        }
        "kagi_assistant_thread_list" => {
            let token = resolve_session_token(profile)?;
            mcp_output(
                &execute_assistant_thread_list(&token).await?,
                config.default_output_or(OutputFormat::Json),
            )?
        }
        "kagi_assistant_thread_get" => {
            let token = resolve_session_token(profile)?;
            mcp_output(
                &execute_assistant_thread_get(
                    &mcp_required_string(&arguments, "thread_id")?,
                    &token,
                )
                .await?,
                config.default_output_or(OutputFormat::Json),
            )?
        }
        "kagi_assistant_thread_export" => {
            mcp_assistant_thread_export(&arguments, profile, config).await?
        }
        "kagi_assistant_custom_list" => {
            let token = resolve_session_token(profile)?;
            mcp_output(
                &execute_custom_assistant_list(&token).await?,
                config.default_output_or(OutputFormat::Json),
            )?
        }
        "kagi_assistant_custom_get" => {
            let token = resolve_session_token(profile)?;
            mcp_output(
                &execute_custom_assistant_get(&mcp_required_string(&arguments, "target")?, &token)
                    .await?,
                config.default_output_or(OutputFormat::Json),
            )?
        }
        "kagi_translate" => mcp_translate(&arguments, profile, config).await?,
        "kagi_fastgpt" => mcp_fastgpt(&arguments, profile, config).await?,
        "kagi_enrich_web" => {
            let token = resolve_api_token(profile)?;
            mcp_output(
                &execute_enrich_web(&mcp_required_string(&arguments, "query")?, &token).await?,
                mcp_output_format(&arguments, &config.default_output_or(OutputFormat::Json))?,
            )?
        }
        "kagi_enrich_news" => {
            let token = resolve_api_token(profile)?;
            mcp_output(
                &execute_enrich_news(&mcp_required_string(&arguments, "query")?, &token).await?,
                mcp_output_format(&arguments, &config.default_output_or(OutputFormat::Json))?,
            )?
        }
        "kagi_smallweb" => mcp_smallweb(&arguments, config).await?,
        "kagi_lens_list" => {
            let token = resolve_session_token(profile)?;
            mcp_output(
                &execute_lens_list(&token).await?,
                config.default_output_or(OutputFormat::Json),
            )?
        }
        "kagi_lens_get" => {
            let token = resolve_session_token(profile)?;
            mcp_output(
                &execute_lens_get(&mcp_required_string(&arguments, "target")?, &token).await?,
                config.default_output_or(OutputFormat::Json),
            )?
        }
        "kagi_custom_bang_list" => {
            let token = resolve_session_token(profile)?;
            mcp_output(
                &execute_custom_bang_list(&token).await?,
                config.default_output_or(OutputFormat::Json),
            )?
        }
        "kagi_custom_bang_get" => {
            let token = resolve_session_token(profile)?;
            mcp_output(
                &execute_custom_bang_get(&mcp_required_string(&arguments, "target")?, &token)
                    .await?,
                config.default_output_or(OutputFormat::Json),
            )?
        }
        "kagi_redirect_list" => {
            let token = resolve_session_token(profile)?;
            mcp_output(
                &execute_redirect_list(&token).await?,
                config.default_output_or(OutputFormat::Json),
            )?
        }
        "kagi_redirect_get" => {
            let token = resolve_session_token(profile)?;
            mcp_output(
                &execute_redirect_get(&mcp_required_string(&arguments, "target")?, &token).await?,
                config.default_output_or(OutputFormat::Json),
            )?
        }
        "kagi_auth_status" => {
            let inventory = load_credential_inventory_for_profile(profile)?;
            format_status(&inventory)
        }
        "kagi_auth_check" => mcp_auth_check(profile).await?,
        "kagi_history_list" => mcp_output(
            &local::read_history(mcp_usize_or(&arguments, "limit", 25)?)?,
            config.default_output_or(OutputFormat::Json),
        )?,
        "kagi_history_stats" => mcp_output(
            &local::history_stats()?,
            config.default_output_or(OutputFormat::Json),
        )?,
        "kagi_site_pref_list" => mcp_output(
            &local::load_site_preferences()?,
            config.default_output_or(OutputFormat::Json),
        )?,
        "kagi_assistant"
        | "kagi_ask_page"
        | "kagi_assistant_thread_delete"
        | "kagi_assistant_custom_create"
        | "kagi_assistant_custom_update"
        | "kagi_assistant_custom_delete"
        | "kagi_lens_create"
        | "kagi_lens_update"
        | "kagi_lens_delete"
        | "kagi_lens_enable"
        | "kagi_lens_disable"
        | "kagi_custom_bang_create"
        | "kagi_custom_bang_update"
        | "kagi_custom_bang_delete"
        | "kagi_redirect_create"
        | "kagi_redirect_update"
        | "kagi_redirect_delete"
        | "kagi_redirect_enable"
        | "kagi_redirect_disable"
        | "kagi_site_pref_set"
        | "kagi_site_pref_remove"
        | "kagi_cli" => mcp_mutating_tool_call(name, &arguments, profile, config).await?,
        _ => {
            return Err(KagiError::Config(format!(
                "unsupported MCP tool `{name}`. Call tools/list to inspect available tools"
            )));
        }
    };
    Ok(mcp_tool_result(text, false, include_structured_content))
}

async fn mcp_mutating_tool_call(
    name: &str,
    arguments: &Value,
    profile: Option<&str>,
    config: &McpServerConfig,
) -> Result<String, KagiError> {
    if !config.enable_mutating_tools {
        return Err(KagiError::Config(format!(
            "MCP tool `{name}` mutates Kagi or local CLI state. Restart `kagi mcp` with --enable-mutating-tools to expose it"
        )));
    }

    if name == "kagi_assistant" {
        return mcp_assistant(arguments, profile, config).await;
    }

    if name == "kagi_cli" {
        return mcp_cli_passthrough(arguments, config);
    }

    let token = resolve_session_token(profile)?;
    match name {
        "kagi_ask_page" => {
            let response = execute_ask_page(
                &AskPageRequest {
                    url: mcp_required_string(arguments, "url")?,
                    question: mcp_required_string(arguments, "question")?,
                },
                &token,
            )
            .await?;
            mcp_output(
                &response,
                mcp_output_format(arguments, &config.default_output_or(OutputFormat::Json))?,
            )
        }
        "kagi_assistant_thread_delete" => mcp_output(
            &execute_assistant_thread_delete(&mcp_required_string(arguments, "thread_id")?, &token)
                .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_assistant_custom_create" => mcp_output(
            &execute_custom_assistant_create(
                &mcp_assistant_custom_create_request(arguments)?,
                &token,
            )
            .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_assistant_custom_update" => mcp_output(
            &execute_custom_assistant_update(
                &mcp_assistant_custom_update_request(arguments)?,
                &token,
            )
            .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_assistant_custom_delete" => mcp_output(
            &execute_custom_assistant_delete(&mcp_required_string(arguments, "target")?, &token)
                .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_lens_create" => mcp_output(
            &execute_lens_create(&mcp_lens_create_request(arguments)?, &token).await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_lens_update" => mcp_output(
            &execute_lens_update(&mcp_lens_update_request(arguments)?, &token).await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_lens_delete" => mcp_output(
            &execute_lens_delete(&mcp_required_string(arguments, "target")?, &token).await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_lens_enable" => mcp_output(
            &execute_lens_set_enabled(&mcp_required_string(arguments, "target")?, true, &token)
                .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_lens_disable" => mcp_output(
            &execute_lens_set_enabled(&mcp_required_string(arguments, "target")?, false, &token)
                .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_custom_bang_create" => mcp_output(
            &execute_custom_bang_create(&mcp_custom_bang_create_request(arguments)?, &token)
                .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_custom_bang_update" => mcp_output(
            &execute_custom_bang_update(&mcp_custom_bang_update_request(arguments)?, &token)
                .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_custom_bang_delete" => mcp_output(
            &execute_custom_bang_delete(&mcp_required_string(arguments, "target")?, &token).await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_redirect_create" => mcp_output(
            &execute_redirect_create(
                &RedirectRuleCreateRequest {
                    rule: mcp_required_string(arguments, "rule")?,
                },
                &token,
            )
            .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_redirect_update" => mcp_output(
            &execute_redirect_update(
                &RedirectRuleUpdateRequest {
                    target: mcp_required_string(arguments, "target")?,
                    rule: mcp_required_string(arguments, "rule")?,
                },
                &token,
            )
            .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_redirect_delete" => mcp_output(
            &execute_redirect_delete(&mcp_required_string(arguments, "target")?, &token).await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_redirect_enable" => mcp_output(
            &execute_redirect_set_enabled(&mcp_required_string(arguments, "target")?, true, &token)
                .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_redirect_disable" => mcp_output(
            &execute_redirect_set_enabled(
                &mcp_required_string(arguments, "target")?,
                false,
                &token,
            )
            .await?,
            config.default_output_or(OutputFormat::Json),
        ),
        "kagi_site_pref_set" => {
            let mut preferences = local::load_site_preferences()?;
            let domain = local::normalize_domain(&mcp_required_string(arguments, "domain")?)?;
            let mode = mcp_site_pref_mode(arguments)?;
            preferences.domains.insert(domain.clone(), mode);
            local::save_site_preferences(&preferences)?;
            mcp_output(
                &serde_json::json!({"domain": domain, "mode": mode.as_str()}),
                config.default_output_or(OutputFormat::Json),
            )
        }
        "kagi_site_pref_remove" => {
            let mut preferences = local::load_site_preferences()?;
            let domain = local::normalize_domain(&mcp_required_string(arguments, "domain")?)?;
            preferences.domains.remove(&domain);
            local::save_site_preferences(&preferences)?;
            mcp_output(
                &serde_json::json!({"domain": domain, "removed": true}),
                config.default_output_or(OutputFormat::Json),
            )
        }
        _ => Err(KagiError::Config(format!(
            "unsupported mutating MCP tool `{name}`"
        ))),
    }
}

fn mcp_cli_passthrough(arguments: &Value, config: &McpServerConfig) -> Result<String, KagiError> {
    let args = mcp_string_array(arguments, "args")?;
    if args.first().is_some_and(|arg| arg == "mcp") {
        return Err(KagiError::Config(
            "kagi_cli cannot launch nested `kagi mcp` servers".to_string(),
        ));
    }

    let executable = env::current_exe().map_err(|error| {
        KagiError::Config(format!("failed to locate current kagi executable: {error}"))
    })?;
    let mut command = ProcessCommand::new(executable);
    command.args(&args);

    if mcp_string(arguments, "stdin").is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| KagiError::Config(format!("failed to run kagi command: {error}")))?;

    if let Some(stdin) = mcp_string(arguments, "stdin") {
        let mut child_stdin = child.stdin.take().ok_or_else(|| {
            KagiError::Config("failed to open stdin for kagi command".to_string())
        })?;
        child_stdin.write_all(stdin.as_bytes()).map_err(|error| {
            KagiError::Config(format!("failed to write stdin to kagi command: {error}"))
        })?;
    }

    let output = child.wait_with_output().map_err(|error| {
        KagiError::Config(format!("failed to collect kagi command output: {error}"))
    })?;
    let payload = serde_json::json!({
        "status": output.status.code(),
        "success": output.status.success(),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
    });
    mcp_output(
        &payload,
        mcp_output_format(arguments, &config.default_output_or(OutputFormat::Json))?,
    )
}

async fn mcp_search(
    arguments: &Value,
    profile: Option<&str>,
    config: &McpServerConfig,
) -> Result<String, KagiError> {
    let format = mcp_output_format(arguments, &config.default_output_or(OutputFormat::Json))?;
    if mcp_bool(arguments, "news") {
        let token = resolve_session_token(profile)?;
        let request = mcp_news_search_request(arguments)?;
        let response = search::execute_news_search(&request, &token).await?;
        return format_news_search_response(&response, &format, false);
    }

    let mut options = mcp_search_options(arguments)?;
    options.lens = resolve_search_lens_option(options.lens, profile).await?;
    let request = build_search_request(mcp_required_string(arguments, "query")?, &options);

    if let Some(follow_count) = mcp_usize(arguments, "follow")? {
        let value = mcp_search_follow_value(request, follow_count, options.limit, profile).await?;
        return mcp_output(&value, format);
    }

    let inventory = load_credential_inventory_for_profile(profile)?;
    let credentials = inventory.resolve_for_search(search_auth_requirement(&request))?;
    let response = cached_json(
        mcp_bool(arguments, "local_cache"),
        mcp_u64_or(arguments, "cache_ttl", 900)?,
        "search",
        &request,
        || async { execute_search_request(&request, credentials).await },
    )
    .await?;
    record_history("search", Some(&request.query), Some(response.data.len()))?;
    let mut response = apply_local_site_preferences(response)?;
    if let Some(limit) = options.limit {
        response.data.truncate(limit);
    }
    format_search_response(
        &response,
        &format,
        arguments.get("template").and_then(Value::as_str),
        false,
    )
}

async fn mcp_batch_search(
    arguments: &Value,
    profile: Option<&str>,
    config: &McpServerConfig,
) -> Result<String, KagiError> {
    let queries = mcp_string_array(arguments, "queries")?;
    if queries.is_empty() {
        return Err(KagiError::Config(
            "kagi_batch_search requires at least one query".to_string(),
        ));
    }

    let format = mcp_output_format(arguments, &config.default_output_or(OutputFormat::Json))?;
    let mut options = mcp_search_options(arguments)?;
    options.lens = resolve_search_lens_option(options.lens, profile).await?;
    let inventory = load_credential_inventory_for_profile(profile)?;
    let auth_probe_request = build_search_request("auth probe".to_string(), &options);
    let credentials = inventory.resolve_for_search(search_auth_requirement(&auth_probe_request))?;
    let rate_limit = mcp_u32_or(arguments, "rate_limit", 60)?;
    let concurrency = mcp_usize_or(arguments, "concurrency", 3)?;
    if concurrency == 0 {
        return Err(KagiError::Config(
            "kagi_batch_search concurrency must be at least 1".to_string(),
        ));
    }
    let rate_limiter = Arc::new(RateLimiter::new(rate_limit, rate_limit));
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::new();

    for query in queries {
        let rate_limiter = Arc::clone(&rate_limiter);
        let semaphore = Arc::clone(&semaphore);
        let credentials = credentials.clone();
        let options = options.clone();
        let query_for_task = query.clone();
        let handle: tokio::task::JoinHandle<(String, Result<SearchResponse, KagiError>)> =
            tokio::spawn(async move {
                let _permit = semaphore.acquire().await;
                let result = async {
                    rate_limiter.acquire().await?;
                    let request = build_search_request(query_for_task, &options);
                    execute_search_request(&request, credentials).await
                }
                .await;
                (query, result)
            });
        handles.push(handle);
    }

    let mut query_order = Vec::new();
    let mut results = Vec::new();
    let mut failures = Vec::new();
    for handle in handles {
        match handle.await {
            Ok((query, Ok(mut response))) => {
                if let Some(limit) = options.limit {
                    response.data.truncate(limit);
                }
                query_order.push(query);
                results.push(serde_json::to_value(response)?);
            }
            Ok((query, Err(error))) => failures.push(format!("{query}: {error}")),
            Err(error) => failures.push(format!("worker task failed: {error}")),
        }
    }

    if !failures.is_empty()
        && matches!(
            format,
            OutputFormat::Json | OutputFormat::Compact | OutputFormat::Toon
        )
    {
        return Err(KagiError::Batch(format_batch_failure_message(
            results.len(),
            &failures,
        )));
    }

    let payload = serde_json::json!({
        "queries": query_order,
        "results": results,
        "failures": failures,
    });
    mcp_output(&payload, format)
}

async fn mcp_search_follow_value(
    request: search::SearchRequest,
    follow_count: usize,
    limit: Option<usize>,
    profile: Option<&str>,
) -> Result<Value, KagiError> {
    let inventory = load_credential_inventory_for_profile(profile)?;
    let credentials = inventory.resolve_for_search(search_auth_requirement(&request))?;
    let mut response =
        apply_local_site_preferences(execute_search_request(&request, credentials).await?)?;
    if let Some(limit) = limit {
        response.data.truncate(limit);
    }
    let token = resolve_session_token(profile)?;
    let mut summaries = Vec::new();
    for result in response.data.iter().take(follow_count) {
        let request = SubscriberSummarizeRequest {
            url: Some(result.url.clone()),
            text: None,
            summary_type: None,
            target_language: None,
            length: None,
        };
        summaries.push(serde_json::json!({
            "title": result.title,
            "url": result.url,
            "summary": execute_subscriber_summarize(&request, &token).await?,
        }));
    }
    record_history(
        "search-follow",
        Some(&request.query),
        Some(response.data.len()),
    )?;
    Ok(serde_json::json!({
        "query": request.query,
        "search": response,
        "summaries": summaries,
    }))
}

async fn mcp_summarize(
    arguments: &Value,
    profile: Option<&str>,
    config: &McpServerConfig,
) -> Result<String, KagiError> {
    let format = mcp_output_format(arguments, &config.default_output_or(OutputFormat::Json))?;
    if mcp_bool(arguments, "subscriber") {
        if arguments.get("engine").is_some() {
            return Err(KagiError::Config(
                "engine is only supported for the paid public summarizer API".to_string(),
            ));
        }
        if arguments.get("cache").is_some() {
            return Err(KagiError::Config(
                "cache is only supported for the paid public summarizer API".to_string(),
            ));
        }
        let request = SubscriberSummarizeRequest {
            url: mcp_string(arguments, "url"),
            text: mcp_string(arguments, "text"),
            summary_type: mcp_string(arguments, "summary_type"),
            target_language: mcp_string(arguments, "target_language"),
            length: mcp_string(arguments, "length"),
        };
        mcp_validate_url_or_text(
            request.url.as_deref(),
            request.text.as_deref(),
            "kagi_summarize",
        )?;
        let token = resolve_session_token(profile)?;
        let response = cached_json(
            mcp_bool(arguments, "local_cache"),
            mcp_u64_or(arguments, "cache_ttl", 3600)?,
            "subscriber-summarize",
            &request,
            || async { execute_subscriber_summarize(&request, &token).await },
        )
        .await?;
        return mcp_output(&response, format);
    }

    if arguments.get("length").is_some() {
        return Err(KagiError::Config(
            "length requires subscriber=true".to_string(),
        ));
    }
    let request = SummarizeRequest {
        url: mcp_string(arguments, "url"),
        text: mcp_string(arguments, "text"),
        engine: mcp_string(arguments, "engine"),
        summary_type: mcp_string(arguments, "summary_type"),
        target_language: mcp_string(arguments, "target_language"),
        cache: mcp_bool_option(arguments, "cache"),
    };
    mcp_validate_url_or_text(
        request.url.as_deref(),
        request.text.as_deref(),
        "kagi_summarize",
    )?;
    let token = resolve_api_token(profile)?;
    let response = cached_json(
        mcp_bool(arguments, "local_cache"),
        mcp_u64_or(arguments, "cache_ttl", 3600)?,
        "summarize",
        &request,
        || async { execute_summarize(&request, &token).await },
    )
    .await?;
    mcp_output(&response, format)
}

async fn mcp_extract(
    arguments: &Value,
    profile: Option<&str>,
    config: &McpServerConfig,
) -> Result<String, KagiError> {
    let url = mcp_required_string(arguments, "url")?;
    match mcp_extract_output_format(arguments, config)? {
        OutputFormat::Markdown => execute_extract_with_available_auth(&url, profile).await,
        OutputFormat::Compact => {
            let response = execute_extract_response_with_available_auth(&url, profile).await?;
            serde_json::to_string(&response).map_err(KagiError::from)
        }
        OutputFormat::Toon => {
            let response = execute_extract_response_with_available_auth(&url, profile).await?;
            mcp_output(&response, OutputFormat::Toon)
        }
        OutputFormat::Json => {
            let response = execute_extract_response_with_available_auth(&url, profile).await?;
            mcp_output(&response, OutputFormat::Json)
        }
        OutputFormat::Pretty | OutputFormat::Csv => {
            unreachable!("MCP extract format normalization should not return pretty or csv")
        }
    }
}

fn mcp_extract_output_format(
    arguments: &Value,
    config: &McpServerConfig,
) -> Result<OutputFormat, KagiError> {
    let requested = arguments
        .get("format")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    match requested.as_deref() {
        Some("markdown") => Ok(OutputFormat::Markdown),
        Some("compact") => Ok(OutputFormat::Compact),
        Some("toon") => Ok(OutputFormat::Toon),
        Some("json") => Ok(OutputFormat::Json),
        None => match config.default_output.clone() {
            None | Some(OutputFormat::Markdown | OutputFormat::Pretty) => {
                Ok(OutputFormat::Markdown)
            }
            Some(OutputFormat::Compact) => Ok(OutputFormat::Compact),
            Some(OutputFormat::Toon) => Ok(OutputFormat::Toon),
            Some(OutputFormat::Json | OutputFormat::Csv) => Ok(OutputFormat::Json),
        },
        Some(other) => Err(KagiError::Config(format!(
            "unsupported extract format `{other}`. Use markdown, json, compact, or toon"
        ))),
    }
}

async fn mcp_quick(
    arguments: &Value,
    profile: Option<&str>,
    config: &McpServerConfig,
) -> Result<String, KagiError> {
    let token = resolve_session_token(profile)?;
    let mut request = search::SearchRequest::new(mcp_required_string(arguments, "query")?);
    if let Some(lens) = mcp_string(arguments, "lens") {
        request = request.with_lens(lens);
    }
    let response = cached_json(
        mcp_bool(arguments, "local_cache"),
        mcp_u64_or(arguments, "cache_ttl", 900)?,
        "quick",
        &request,
        || async { execute_quick(&request, &token).await },
    )
    .await?;
    format_quick_response(
        &response,
        mcp_quick_format(arguments, &config.default_output_or(OutputFormat::Json))?,
        false,
    )
}

async fn mcp_news(arguments: &Value, config: &McpServerConfig) -> Result<String, KagiError> {
    let presets = mcp_string_array_or_empty(arguments, "filter_preset")?;
    let keywords = mcp_string_array_or_empty(arguments, "filter_keyword")?;
    let filter_mode = mcp_news_filter_mode(arguments)?.unwrap_or(NewsFilterMode::Hide);
    let filter_scope = mcp_news_filter_scope(arguments)?.unwrap_or(NewsFilterScope::All);
    let filter_request =
        (!presets.is_empty() || !keywords.is_empty()).then_some(NewsFilterRequest {
            preset_ids: presets,
            keywords,
            mode: filter_mode,
            scope: filter_scope,
        });
    let response = execute_news(
        &mcp_string_or(arguments, "category", "world"),
        mcp_u32_or(arguments, "limit", 12)?,
        &mcp_string_or(arguments, "lang", "default"),
        filter_request.as_ref(),
    )
    .await?;
    mcp_output(
        &response,
        mcp_output_format(arguments, &config.default_output_or(OutputFormat::Json))?,
    )
}

async fn mcp_news_search(
    arguments: &Value,
    profile: Option<&str>,
    config: &McpServerConfig,
) -> Result<String, KagiError> {
    let token = resolve_session_token(profile)?;
    let request = mcp_news_search_request(arguments)?;
    let response = search::execute_news_search(&request, &token).await?;
    format_news_search_response(
        &response,
        &mcp_output_format(arguments, &config.default_output_or(OutputFormat::Json))?,
        false,
    )
}

async fn mcp_assistant(
    arguments: &Value,
    profile: Option<&str>,
    config: &McpServerConfig,
) -> Result<String, KagiError> {
    let token = resolve_session_token(profile)?;
    let mut query = mcp_required_string(arguments, "query")?;
    let contract_file = mcp_string(arguments, "contract_file").map(PathBuf::from);
    let contract = load_assistant_contract(
        mcp_string(arguments, "contract").as_deref(),
        contract_file.as_deref(),
    )?;
    let format = mcp_assistant_format(arguments, &config.default_output_or(OutputFormat::Json))?;
    if contract.is_some() {
        validate_assistant_contract_output_format(format.clone())?;
    }
    if let Some(contract) = contract.as_ref() {
        query = contract_prompt_query(&query, contract);
    }

    let request = AssistantPromptRequest {
        query,
        thread_id: mcp_string(arguments, "thread_id"),
        attachments: mcp_path_array(arguments, "attach")?,
        profile_id: mcp_string(arguments, "assistant"),
        model: mcp_string(arguments, "model"),
        lens_id: mcp_u64(arguments, "lens")?,
        internet_access: mcp_bool_choice(arguments, "web_access", "no_web_access"),
        personalizations: mcp_bool_choice(arguments, "personalized", "no_personalized"),
    };
    let response = if mcp_bool(arguments, "once") {
        execute_once_assistant_prompt(&request, None, &token).await?
    } else {
        execute_assistant_prompt(&request, &token).await?
    };

    if let Some(contract) = contract.as_ref() {
        let value = validate_assistant_contract_response(contract, &response).map_err(|error| {
            KagiError::Config(format!(
                "assistant contract '{}' was not satisfied: {error}",
                contract.name
            ))
        })?;
        return mcp_output(&value, output_format_from_assistant(format));
    }

    format_assistant_response(&response, format, false)
}

async fn mcp_assistant_thread_export(
    arguments: &Value,
    profile: Option<&str>,
    _config: &McpServerConfig,
) -> Result<String, KagiError> {
    let token = resolve_session_token(profile)?;
    let thread_id = mcp_required_string(arguments, "thread_id")?;
    match mcp_string_or(arguments, "format", "markdown").as_str() {
        "markdown" => Ok(execute_assistant_thread_export(&thread_id, &token)
            .await?
            .markdown),
        "json" => mcp_output(
            &execute_assistant_thread_get(&thread_id, &token).await?,
            OutputFormat::Json,
        ),
        other => Err(KagiError::Config(format!(
            "unsupported thread export format `{other}`. Use markdown or json"
        ))),
    }
}

async fn mcp_translate(
    arguments: &Value,
    profile: Option<&str>,
    config: &McpServerConfig,
) -> Result<String, KagiError> {
    let token = resolve_session_token(profile)?;
    let request = TranslateCommandRequest {
        text: mcp_required_string(arguments, "text")?,
        from: mcp_string_or(arguments, "from", "auto"),
        to: mcp_string_or(arguments, "to", "en"),
        quality: mcp_string(arguments, "quality"),
        model: mcp_string(arguments, "model"),
        prediction: mcp_string(arguments, "prediction"),
        predicted_language: mcp_string(arguments, "predicted_language"),
        formality: mcp_string(arguments, "formality"),
        speaker_gender: mcp_string(arguments, "speaker_gender"),
        addressee_gender: mcp_string(arguments, "addressee_gender"),
        language_complexity: mcp_string(arguments, "language_complexity"),
        translation_style: mcp_string(arguments, "translation_style"),
        context: mcp_string(arguments, "context"),
        dictionary_language: mcp_string(arguments, "dictionary_language"),
        time_format: mcp_string(arguments, "time_format"),
        use_definition_context: mcp_bool_option(arguments, "use_definition_context"),
        enable_language_features: mcp_bool_option(arguments, "enable_language_features"),
        preserve_formatting: mcp_bool_option(arguments, "preserve_formatting"),
        context_memory: arguments
            .get("context_memory")
            .and_then(Value::as_array)
            .cloned(),
        fetch_alternatives: mcp_bool_or(arguments, "fetch_alternatives", true),
        fetch_word_insights: mcp_bool_or(arguments, "fetch_word_insights", true),
        fetch_suggestions: mcp_bool_or(arguments, "fetch_suggestions", true),
        fetch_alignments: mcp_bool_or(arguments, "fetch_alignments", true),
    };
    let response = execute_translate(&request, &token).await?;
    mcp_output(
        &response,
        mcp_output_format(arguments, &config.default_output_or(OutputFormat::Json))?,
    )
}

async fn mcp_fastgpt(
    arguments: &Value,
    profile: Option<&str>,
    config: &McpServerConfig,
) -> Result<String, KagiError> {
    let request = FastGptRequest {
        query: mcp_required_string(arguments, "query")?,
        cache: mcp_bool_option(arguments, "cache"),
        web_search: mcp_bool_option(arguments, "web_search"),
    };
    let token = resolve_api_token(profile)?;
    let response = cached_json(
        mcp_bool(arguments, "local_cache"),
        mcp_u64_or(arguments, "cache_ttl", 3600)?,
        "fastgpt",
        &request,
        || async { execute_fastgpt(&request, &token).await },
    )
    .await?;
    mcp_output(
        &response,
        mcp_output_format(arguments, &config.default_output_or(OutputFormat::Json))?,
    )
}

async fn mcp_smallweb(arguments: &Value, config: &McpServerConfig) -> Result<String, KagiError> {
    mcp_output(
        &execute_smallweb(mcp_u32(arguments, "limit")?).await?,
        mcp_output_format(arguments, &config.default_output_or(OutputFormat::Json))?,
    )
}

async fn mcp_auth_check(profile: Option<&str>) -> Result<String, KagiError> {
    let inventory = load_credential_inventory_for_profile(profile)?;
    let credential = inventory.preferred_for_status().cloned().ok_or_else(|| {
        KagiError::Config(
            "missing credentials: auth check could not verify an account. Set KAGI_API_KEY, KAGI_API_TOKEN, or KAGI_SESSION_TOKEN, or run `kagi auth set` with the credential you want to save"
                .to_string(),
        )
    })?;
    let selected_kind = credential.kind;
    let selected_source = credential.source;
    validate_credential(&credential).await?;
    Ok(format!(
        "auth check passed: {} ({})",
        selected_kind.as_str(),
        selected_source.as_str()
    ))
}

fn mcp_assistant_custom_create_request(
    arguments: &Value,
) -> Result<AssistantProfileCreateRequest, KagiError> {
    Ok(AssistantProfileCreateRequest {
        name: mcp_required_string(arguments, "name")?,
        bang_trigger: mcp_string(arguments, "bang_trigger"),
        internet_access: mcp_bool_choice(arguments, "web_access", "no_web_access"),
        selected_lens: mcp_string(arguments, "lens"),
        personalizations: mcp_bool_choice(arguments, "personalized", "no_personalized"),
        base_model: mcp_string(arguments, "model"),
        custom_instructions: mcp_string(arguments, "instructions"),
    })
}

fn mcp_assistant_custom_update_request(
    arguments: &Value,
) -> Result<AssistantProfileUpdateRequest, KagiError> {
    Ok(AssistantProfileUpdateRequest {
        target: mcp_required_string(arguments, "target")?,
        name: mcp_string(arguments, "name"),
        bang_trigger: mcp_string(arguments, "bang_trigger"),
        internet_access: mcp_bool_choice(arguments, "web_access", "no_web_access"),
        selected_lens: mcp_string(arguments, "lens"),
        personalizations: mcp_bool_choice(arguments, "personalized", "no_personalized"),
        base_model: mcp_string(arguments, "model"),
        custom_instructions: mcp_string(arguments, "instructions"),
    })
}

fn mcp_lens_create_request(arguments: &Value) -> Result<LensCreateRequest, KagiError> {
    Ok(LensCreateRequest {
        name: mcp_required_string(arguments, "name")?,
        included_sites: mcp_string(arguments, "included_sites"),
        included_keywords: mcp_string(arguments, "included_keywords"),
        description: mcp_string(arguments, "description"),
        search_region: mcp_string(arguments, "region"),
        before_time: mcp_string(arguments, "before_date"),
        after_time: mcp_string(arguments, "after_date"),
        excluded_sites: mcp_string(arguments, "excluded_sites"),
        excluded_keywords: mcp_string(arguments, "excluded_keywords"),
        shortcut_keyword: mcp_string(arguments, "shortcut"),
        autocomplete_keywords: mcp_bool_choice(
            arguments,
            "autocomplete_keywords",
            "no_autocomplete_keywords",
        ),
        template: mcp_lens_template(arguments)?,
        file_type: mcp_string(arguments, "file_type"),
        share_with_team: mcp_bool_choice(arguments, "share_with_team", "no_share_with_team"),
        share_copy_code: mcp_bool_choice(arguments, "share_copy_code", "no_share_copy_code"),
    })
}

fn mcp_lens_update_request(arguments: &Value) -> Result<LensUpdateRequest, KagiError> {
    Ok(LensUpdateRequest {
        target: mcp_required_string(arguments, "target")?,
        name: mcp_string(arguments, "name"),
        included_sites: mcp_string(arguments, "included_sites"),
        included_keywords: mcp_string(arguments, "included_keywords"),
        description: mcp_string(arguments, "description"),
        search_region: mcp_string(arguments, "region"),
        before_time: mcp_string(arguments, "before_date"),
        after_time: mcp_string(arguments, "after_date"),
        excluded_sites: mcp_string(arguments, "excluded_sites"),
        excluded_keywords: mcp_string(arguments, "excluded_keywords"),
        shortcut_keyword: mcp_string(arguments, "shortcut"),
        autocomplete_keywords: mcp_bool_choice(
            arguments,
            "autocomplete_keywords",
            "no_autocomplete_keywords",
        ),
        template: mcp_lens_template(arguments)?,
        file_type: mcp_string(arguments, "file_type"),
        share_with_team: mcp_bool_choice(arguments, "share_with_team", "no_share_with_team"),
        share_copy_code: mcp_bool_choice(arguments, "share_copy_code", "no_share_copy_code"),
    })
}

fn mcp_custom_bang_create_request(arguments: &Value) -> Result<CustomBangCreateRequest, KagiError> {
    Ok(CustomBangCreateRequest {
        name: mcp_required_string(arguments, "name")?,
        trigger: mcp_required_string(arguments, "trigger")?,
        template: mcp_string(arguments, "template"),
        snap_domain: mcp_string(arguments, "snap_domain"),
        regex_pattern: mcp_string(arguments, "regex_pattern"),
        shortcut_menu: mcp_bool_choice(arguments, "shortcut_menu", "no_shortcut_menu"),
        fmt_open_snap_domain: mcp_bool_choice(arguments, "open_snap_domain", "no_open_snap_domain"),
        fmt_open_base_path: mcp_bool_choice(arguments, "open_base_path", "no_open_base_path"),
        fmt_url_encode_placeholder: mcp_bool_choice(
            arguments,
            "encode_placeholder",
            "no_encode_placeholder",
        ),
        fmt_url_encode_space_to_plus: mcp_bool_choice(
            arguments,
            "plus_for_space",
            "no_plus_for_space",
        ),
    })
}

fn mcp_custom_bang_update_request(arguments: &Value) -> Result<CustomBangUpdateRequest, KagiError> {
    Ok(CustomBangUpdateRequest {
        target: mcp_required_string(arguments, "target")?,
        name: mcp_string(arguments, "name"),
        trigger: mcp_string(arguments, "trigger"),
        template: mcp_string(arguments, "template"),
        snap_domain: mcp_string(arguments, "snap_domain"),
        regex_pattern: mcp_string(arguments, "regex_pattern"),
        shortcut_menu: mcp_bool_choice(arguments, "shortcut_menu", "no_shortcut_menu"),
        fmt_open_snap_domain: mcp_bool_choice(arguments, "open_snap_domain", "no_open_snap_domain"),
        fmt_open_base_path: mcp_bool_choice(arguments, "open_base_path", "no_open_base_path"),
        fmt_url_encode_placeholder: mcp_bool_choice(
            arguments,
            "encode_placeholder",
            "no_encode_placeholder",
        ),
        fmt_url_encode_space_to_plus: mcp_bool_choice(
            arguments,
            "plus_for_space",
            "no_plus_for_space",
        ),
    })
}

fn mcp_search_options(arguments: &Value) -> Result<SearchRequestOptions, KagiError> {
    Ok(SearchRequestOptions {
        snap: mcp_string(arguments, "snap"),
        lens: mcp_string(arguments, "lens"),
        region: mcp_string(arguments, "region"),
        time: mcp_search_time(arguments)?,
        from_date: mcp_string(arguments, "from_date"),
        to_date: mcp_string(arguments, "to_date"),
        limit: mcp_usize(arguments, "limit")?,
        order: mcp_search_order(arguments)?,
        verbatim: mcp_bool(arguments, "verbatim"),
        personalized: mcp_bool(arguments, "personalized"),
        no_personalized: mcp_bool(arguments, "no_personalized"),
    })
}

fn mcp_news_search_request(arguments: &Value) -> Result<search::NewsSearchRequest, KagiError> {
    Ok(search::NewsSearchRequest {
        query: mcp_required_string(arguments, "query")?,
        region: mcp_string(arguments, "region"),
        freshness: mcp_news_freshness(arguments)?,
        order: mcp_news_order(arguments)?,
        dir_desc: mcp_bool(arguments, "dir_desc"),
        limit: mcp_usize(arguments, "limit")?,
    })
}

fn mcp_validate_url_or_text(
    url: Option<&str>,
    text: Option<&str>,
    tool_name: &str,
) -> Result<(), KagiError> {
    match (
        url.map(str::trim).filter(|value| !value.is_empty()),
        text.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (Some(_), None) | (None, Some(_)) => Ok(()),
        (Some(_), Some(_)) => Err(KagiError::Config(format!(
            "{tool_name} accepts exactly one of url or text, not both"
        ))),
        (None, None) => Err(KagiError::Config(format!(
            "{tool_name} requires url or text"
        ))),
    }
}

fn mcp_required_string(arguments: &Value, key: &str) -> Result<String, KagiError> {
    mcp_string(arguments, key).ok_or_else(|| {
        KagiError::Config(format!(
            "MCP argument `{key}` is required and must be a non-empty string"
        ))
    })
}

fn mcp_string(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn mcp_string_or(arguments: &Value, key: &str, default: &str) -> String {
    mcp_string(arguments, key).unwrap_or_else(|| default.to_string())
}

fn mcp_bool(arguments: &Value, key: &str) -> bool {
    arguments.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn mcp_bool_or(arguments: &Value, key: &str, default: bool) -> bool {
    arguments
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn mcp_bool_option(arguments: &Value, key: &str) -> Option<bool> {
    arguments.get(key).and_then(Value::as_bool)
}

fn mcp_bool_choice(arguments: &Value, enabled: &str, disabled: &str) -> Option<bool> {
    match (mcp_bool(arguments, enabled), mcp_bool(arguments, disabled)) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

fn mcp_u64(arguments: &Value, key: &str) -> Result<Option<u64>, KagiError> {
    match arguments.get(key) {
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            KagiError::Config(format!("MCP argument `{key}` must be a positive integer"))
        }),
        None => Ok(None),
    }
}

fn mcp_u64_or(arguments: &Value, key: &str, default: u64) -> Result<u64, KagiError> {
    Ok(mcp_u64(arguments, key)?.unwrap_or(default))
}

fn mcp_u32(arguments: &Value, key: &str) -> Result<Option<u32>, KagiError> {
    mcp_u64(arguments, key)?
        .map(|value| {
            u32::try_from(value).map_err(|_| {
                KagiError::Config(format!("MCP argument `{key}` is too large for a u32"))
            })
        })
        .transpose()
}

fn mcp_u32_or(arguments: &Value, key: &str, default: u32) -> Result<u32, KagiError> {
    Ok(mcp_u32(arguments, key)?.unwrap_or(default))
}

fn mcp_usize(arguments: &Value, key: &str) -> Result<Option<usize>, KagiError> {
    mcp_u64(arguments, key)?
        .map(|value| {
            usize::try_from(value).map_err(|_| {
                KagiError::Config(format!(
                    "MCP argument `{key}` is too large for this platform"
                ))
            })
        })
        .transpose()
}

fn mcp_usize_or(arguments: &Value, key: &str, default: usize) -> Result<usize, KagiError> {
    Ok(mcp_usize(arguments, key)?.unwrap_or(default))
}

fn mcp_string_array(arguments: &Value, key: &str) -> Result<Vec<String>, KagiError> {
    let values = arguments
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            KagiError::Config(format!("MCP argument `{key}` must be an array of strings"))
        })?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    KagiError::Config(format!(
                        "MCP argument `{key}` must contain only non-empty strings"
                    ))
                })
        })
        .collect()
}

fn mcp_string_array_or_empty(arguments: &Value, key: &str) -> Result<Vec<String>, KagiError> {
    if arguments.get(key).is_none() {
        Ok(Vec::new())
    } else {
        mcp_string_array(arguments, key)
    }
}

fn mcp_path_array(arguments: &Value, key: &str) -> Result<Vec<PathBuf>, KagiError> {
    mcp_string_array_or_empty(arguments, key)
        .map(|values| values.into_iter().map(PathBuf::from).collect())
}

fn mcp_search_time(arguments: &Value) -> Result<Option<SearchTime>, KagiError> {
    mcp_string(arguments, "time")
        .map(|value| match value.as_str() {
            "day" => Ok(SearchTime::Day),
            "week" => Ok(SearchTime::Week),
            "month" => Ok(SearchTime::Month),
            "year" => Ok(SearchTime::Year),
            _ => Err(KagiError::Config(format!(
                "unsupported time `{value}`. Use day, week, month, or year"
            ))),
        })
        .transpose()
}

fn mcp_search_order(arguments: &Value) -> Result<Option<SearchOrder>, KagiError> {
    mcp_string(arguments, "order")
        .map(|value| match value.as_str() {
            "default" => Ok(SearchOrder::Default),
            "recency" => Ok(SearchOrder::Recency),
            "website" => Ok(SearchOrder::Website),
            "trackers" => Ok(SearchOrder::Trackers),
            _ => Err(KagiError::Config(format!(
                "unsupported order `{value}`. Use default, recency, website, or trackers"
            ))),
        })
        .transpose()
}

fn mcp_news_freshness(arguments: &Value) -> Result<Option<search::NewsFreshness>, KagiError> {
    mcp_string(arguments, "freshness")
        .map(|value| match value.as_str() {
            "day" => Ok(search::NewsFreshness::Day),
            "week" => Ok(search::NewsFreshness::Week),
            "month" => Ok(search::NewsFreshness::Month),
            _ => Err(KagiError::Config(format!(
                "unsupported freshness `{value}`. Use day, week, or month"
            ))),
        })
        .transpose()
}

fn mcp_news_order(arguments: &Value) -> Result<Option<search::NewsSearchOrder>, KagiError> {
    mcp_string(arguments, "order")
        .map(|value| match value.as_str() {
            "default" => Ok(search::NewsSearchOrder::Default),
            "recency" => Ok(search::NewsSearchOrder::Recency),
            "website" => Ok(search::NewsSearchOrder::Website),
            _ => Err(KagiError::Config(format!(
                "unsupported news order `{value}`. Use default, recency, or website"
            ))),
        })
        .transpose()
}

fn mcp_news_filter_mode(arguments: &Value) -> Result<Option<NewsFilterMode>, KagiError> {
    mcp_string(arguments, "filter_mode")
        .map(|value| match value.as_str() {
            "hide" => Ok(NewsFilterMode::Hide),
            "blur" => Ok(NewsFilterMode::Blur),
            _ => Err(KagiError::Config(format!(
                "unsupported filter_mode `{value}`. Use hide or blur"
            ))),
        })
        .transpose()
}

fn mcp_news_filter_scope(arguments: &Value) -> Result<Option<NewsFilterScope>, KagiError> {
    mcp_string(arguments, "filter_scope")
        .map(|value| match value.as_str() {
            "title" => Ok(NewsFilterScope::Title),
            "summary" => Ok(NewsFilterScope::Summary),
            "all" => Ok(NewsFilterScope::All),
            _ => Err(KagiError::Config(format!(
                "unsupported filter_scope `{value}`. Use title, summary, or all"
            ))),
        })
        .transpose()
}

fn mcp_lens_template(arguments: &Value) -> Result<Option<String>, KagiError> {
    mcp_string(arguments, "template")
        .map(|value| match value.as_str() {
            "default" => Ok("0".to_string()),
            "news" => Ok("1".to_string()),
            _ => Err(KagiError::Config(format!(
                "unsupported lens template `{value}`. Use default or news"
            ))),
        })
        .transpose()
}

fn mcp_site_pref_mode(arguments: &Value) -> Result<local::SitePreferenceMode, KagiError> {
    match mcp_required_string(arguments, "mode")?.as_str() {
        "block" => Ok(local::SitePreferenceMode::Block),
        "lower" => Ok(local::SitePreferenceMode::Lower),
        "normal" => Ok(local::SitePreferenceMode::Normal),
        "higher" => Ok(local::SitePreferenceMode::Higher),
        "pin" => Ok(local::SitePreferenceMode::Pin),
        value => Err(KagiError::Config(format!(
            "unsupported site preference mode `{value}`. Use block, lower, normal, higher, or pin"
        ))),
    }
}

fn mcp_output_format(arguments: &Value, default: &OutputFormat) -> Result<OutputFormat, KagiError> {
    match mcp_string(arguments, "format").as_deref() {
        None => Ok(default.clone()),
        Some("json") => Ok(OutputFormat::Json),
        Some("toon") => Ok(OutputFormat::Toon),
        Some("pretty") => Ok(OutputFormat::Pretty),
        Some("compact") => Ok(OutputFormat::Compact),
        Some("markdown") => Ok(OutputFormat::Markdown),
        Some("csv") => Ok(OutputFormat::Csv),
        Some(value) => Err(KagiError::Config(format!(
            "unsupported output format `{value}`. Use json, toon, pretty, compact, markdown, or csv"
        ))),
    }
}

fn mcp_quick_format(
    arguments: &Value,
    default: &OutputFormat,
) -> Result<QuickOutputFormat, KagiError> {
    match mcp_output_format(arguments, default)? {
        OutputFormat::Json | OutputFormat::Csv => Ok(QuickOutputFormat::Json),
        OutputFormat::Toon => Ok(QuickOutputFormat::Toon),
        OutputFormat::Pretty => Ok(QuickOutputFormat::Pretty),
        OutputFormat::Compact => Ok(QuickOutputFormat::Compact),
        OutputFormat::Markdown => Ok(QuickOutputFormat::Markdown),
    }
}

fn mcp_assistant_format(
    arguments: &Value,
    default: &OutputFormat,
) -> Result<AssistantOutputFormat, KagiError> {
    match mcp_output_format(arguments, default)? {
        OutputFormat::Json | OutputFormat::Csv => Ok(AssistantOutputFormat::Json),
        OutputFormat::Toon => Ok(AssistantOutputFormat::Toon),
        OutputFormat::Pretty => Ok(AssistantOutputFormat::Pretty),
        OutputFormat::Compact => Ok(AssistantOutputFormat::Compact),
        OutputFormat::Markdown => Ok(AssistantOutputFormat::Markdown),
    }
}

fn output_format_from_assistant(format: AssistantOutputFormat) -> OutputFormat {
    match format {
        AssistantOutputFormat::Json => OutputFormat::Json,
        AssistantOutputFormat::Toon => OutputFormat::Toon,
        AssistantOutputFormat::Pretty => OutputFormat::Pretty,
        AssistantOutputFormat::Compact => OutputFormat::Compact,
        AssistantOutputFormat::Markdown => OutputFormat::Markdown,
    }
}

fn mcp_output<T: Serialize>(value: &T, format: OutputFormat) -> Result<String, KagiError> {
    match format {
        OutputFormat::Toon => {
            let value = serde_json::to_value(value)?;
            Ok(toon::encode(&value, None))
        }
        OutputFormat::Compact => serde_json::to_string(value).map_err(KagiError::from),
        OutputFormat::Json | OutputFormat::Pretty | OutputFormat::Markdown | OutputFormat::Csv => {
            serde_json::to_string_pretty(value).map_err(KagiError::from)
        }
    }
}

fn format_search_response(
    response: &SearchResponse,
    format: &OutputFormat,
    template: Option<&str>,
    use_color: bool,
) -> Result<String, KagiError> {
    if let Some(template) = template {
        return Ok(format_template_response(response, template));
    }
    match format {
        OutputFormat::Pretty => Ok(format_pretty_response(response, use_color)),
        OutputFormat::Toon => {
            let value = serde_json::to_value(response)?;
            Ok(toon::encode(&value, None))
        }
        OutputFormat::Compact => serde_json::to_string(response).map_err(KagiError::from),
        OutputFormat::Markdown => Ok(format_markdown_response(response)),
        OutputFormat::Csv => Ok(format_csv_response(response)),
        OutputFormat::Json => serde_json::to_string_pretty(response).map_err(KagiError::from),
    }
}

fn format_news_search_response(
    response: &NewsSearchResponse,
    format: &OutputFormat,
    use_color: bool,
) -> Result<String, KagiError> {
    match format {
        OutputFormat::Pretty => Ok(format_pretty_news_response(response, use_color)),
        OutputFormat::Markdown => Ok(format_markdown_news_response(response)),
        OutputFormat::Csv => Ok(format_csv_news_response(response)),
        other => mcp_output(response, other.clone()),
    }
}

fn format_quick_response(
    response: &QuickResponse,
    format: QuickOutputFormat,
    use_color: bool,
) -> Result<String, KagiError> {
    match format {
        QuickOutputFormat::Pretty => Ok(format_quick_pretty(response, use_color)),
        QuickOutputFormat::Toon => {
            let value = serde_json::to_value(response)?;
            Ok(toon::encode(&value, None))
        }
        QuickOutputFormat::Compact => serde_json::to_string(response).map_err(KagiError::from),
        QuickOutputFormat::Markdown => Ok(format_quick_markdown(response)),
        QuickOutputFormat::Json => serde_json::to_string_pretty(response).map_err(KagiError::from),
    }
}

fn format_assistant_response(
    response: &crate::types::AssistantPromptResponse,
    format: AssistantOutputFormat,
    use_color: bool,
) -> Result<String, KagiError> {
    match format {
        AssistantOutputFormat::Pretty => Ok(format_assistant_pretty(response, use_color)),
        AssistantOutputFormat::Toon => {
            let value = serde_json::to_value(response)?;
            Ok(toon::encode(&value, None))
        }
        AssistantOutputFormat::Compact => serde_json::to_string(response).map_err(KagiError::from),
        AssistantOutputFormat::Markdown => Ok(format_assistant_markdown(response)),
        AssistantOutputFormat::Json => {
            serde_json::to_string_pretty(response).map_err(KagiError::from)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RateLimiter, SearchRequestOptions, bool_flag_choice, build_search_request,
        format_assistant_markdown, format_assistant_pretty, format_batch_failure_message,
        format_csv_response, format_markdown_response, format_pretty_response,
        is_bare_auth_invocation_from, parse_context_memory_json, print_assistant_response,
        should_fallback_to_session,
    };
    use crate::cli::{AssistantOutputFormat, SearchOrder, SearchTime};
    use crate::error::KagiError;
    use crate::types::{
        AssistantMessage, AssistantMeta, AssistantPromptResponse, AssistantThread, SearchResponse,
        SearchResult,
    };
    use serde_json::json;
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
            related_searches: Vec::new(),
        };

        let output = format_pretty_response(&response, false);

        assert_eq!(
            output,
            "1. Rust Programming Language\n   https://www.rust-lang.org\n\n   A language empowering everyone to build reliable and efficient software.\n\n2. The Rust Book\n   https://doc.rust-lang.org/book/\n\n   Learn Rust with the official book."
        );
    }

    #[test]
    fn formats_batch_failures_with_queries_and_success_count() {
        let message = format_batch_failure_message(
            2,
            &[
                "rust: authentication error: invalid token".to_string(),
                "zig: network error: timeout".to_string(),
            ],
        );

        assert!(message.contains("2 batch queries failed"));
        assert!(message.contains("2 succeeded"));
        assert!(message.contains("rust: authentication error"));
        assert!(message.contains("zig: network error"));
    }

    #[test]
    fn detects_exact_bare_auth_invocation() {
        assert!(is_bare_auth_invocation_from(&["kagi", "auth"]));
        assert!(!is_bare_auth_invocation_from(&["kagi", "auth", "status"]));
        assert!(!is_bare_auth_invocation_from(&["kagi", "auth", "--help"]));
        assert!(!is_bare_auth_invocation_from(&["kagi", "search"]));
    }

    #[test]
    fn formats_pretty_output_for_empty_results() {
        let response = SearchResponse {
            data: vec![],
            related_searches: Vec::new(),
        };
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
            related_searches: Vec::new(),
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
            related_searches: Vec::new(),
        };

        let output = format_pretty_response(&response, true);

        assert!(output.contains("\x1b[1;34m"));
        assert!(output.contains("\x1b[36m"));
        assert!(output.contains("\x1b[0m"));
    }

    #[test]
    fn build_search_request_treats_default_order_as_no_order_filter() {
        let request = build_search_request(
            "rust".to_string(),
            &SearchRequestOptions {
                snap: None,
                lens: None,
                region: None,
                time: Some(SearchTime::Month),
                from_date: None,
                to_date: None,
                limit: None,
                order: Some(SearchOrder::Default),
                verbatim: false,
                personalized: false,
                no_personalized: false,
            },
        );

        assert_eq!(request.time_filter.as_deref(), Some("3"));
        assert_eq!(request.order, None);
        assert!(request.requires_session_auth());
    }

    #[test]
    fn build_search_request_prefixes_snap_shortcut() {
        let request = build_search_request(
            "rust".to_string(),
            &SearchRequestOptions {
                snap: Some("@reddit".to_string()),
                lens: None,
                region: None,
                time: None,
                from_date: None,
                to_date: None,
                limit: None,
                order: None,
                verbatim: false,
                personalized: false,
                no_personalized: false,
            },
        );

        assert_eq!(request.query, "@reddit rust");
    }

    #[test]
    fn resolves_boolean_flag_pairs() {
        assert_eq!(bool_flag_choice(true, false), Some(true));
        assert_eq!(bool_flag_choice(false, true), Some(false));
        assert_eq!(bool_flag_choice(false, false), None);
        assert_eq!(bool_flag_choice(true, true), None);
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
        let rate_limiter = RateLimiter::new(2, 60_000); // 2 tokens, 1000 tokens/sec

        // Acquire both tokens
        rate_limiter.acquire().await.unwrap();
        rate_limiter.acquire().await.unwrap();

        // Bound the wait so the test proves refill behavior without relying on a long sleep.
        let result = tokio::time::timeout(Duration::from_millis(50), rate_limiter.acquire())
            .await
            .expect("rate limiter should refill within timeout");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_throttles_under_contention() {
        let rate_limiter = Arc::new(RateLimiter::new(1, 600)); // 1 token capacity, 10 tokens/sec
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
            elapsed >= Duration::from_millis(150),
            "expected throttling to delay final acquisition by at least ~200ms, got {elapsed:?}"
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
            related_searches: Vec::new(),
        };

        let output = format_markdown_response(&response);

        assert_eq!(
            output,
            "## 1. [Rust Programming Language](https://www.rust-lang.org)\n\nA language empowering everyone to build reliable and efficient software.\n\n"
        );
    }

    fn sample_assistant_response(references_markdown: Option<&str>) -> AssistantPromptResponse {
        AssistantPromptResponse {
            meta: AssistantMeta::default(),
            thread: AssistantThread {
                id: "thread-1".to_string(),
                title: "Greeting".to_string(),
                ack: "2026-03-16T06:19:07Z".to_string(),
                created_at: "2026-03-16T06:19:07Z".to_string(),
                expires_at: "2026-03-16T07:19:07Z".to_string(),
                saved: false,
                shared: false,
                branch_id: "00000000-0000-4000-0000-000000000000".to_string(),
                folder_ids: vec![],
            },
            message: AssistantMessage {
                id: "msg-1".to_string(),
                thread_id: "thread-1".to_string(),
                created_at: "2026-03-16T06:19:07Z".to_string(),
                branch_list: vec![],
                state: "done".to_string(),
                prompt: "Hello".to_string(),
                reply_html: Some("<p>Hello</p>".to_string()),
                markdown: Some("Hello[^1]".to_string()),
                references_html: None,
                references_markdown: references_markdown.map(str::to_string),
                metadata_html: None,
                documents: vec![],
                profile: None,
                trace_id: None,
                usage: None,
            },
        }
    }

    #[test]
    fn formats_assistant_markdown_with_references() {
        let response =
            sample_assistant_response(Some("[^1]: [Example](https://example.com) (100%)"));

        let output = format_assistant_markdown(&response);

        assert_eq!(
            output,
            "Hello[^1]\n\n[^1]: [Example](https://example.com) (100%)"
        );
    }

    #[test]
    fn formats_assistant_pretty_with_references_section() {
        let response =
            sample_assistant_response(Some("[^1]: [Example](https://example.com) (100%)"));

        let output = format_assistant_pretty(&response, false);

        assert!(output.contains("Thread: thread-1"));
        assert!(output.contains("Message: msg-1"));
        assert!(output.contains("Hello[^1]"));
        assert!(output.contains("References"));
        assert!(output.contains("[^1]: [Example](https://example.com) (100%)"));
    }

    #[test]
    fn prints_assistant_markdown_and_pretty_formats() {
        let response = sample_assistant_response(None);

        assert!(
            print_assistant_response(&response, AssistantOutputFormat::Markdown, false).is_ok()
        );
        assert!(print_assistant_response(&response, AssistantOutputFormat::Pretty, false).is_ok());
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
            related_searches: Vec::new(),
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
            related_searches: Vec::new(),
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

    #[test]
    fn parses_context_memory_array_json() {
        let parsed = parse_context_memory_json(Some(r#"[{"kind":"glossary","value":"hello"}]"#))
            .expect("context memory should parse");

        assert_eq!(
            parsed,
            Some(vec![json!({"kind": "glossary", "value": "hello"})])
        );
    }

    #[test]
    fn rejects_non_array_context_memory_json() {
        let error = parse_context_memory_json(Some(r#"{"kind":"glossary"}"#))
            .expect_err("object context memory should fail");

        assert!(error.to_string().contains("JSON array"));
    }
}
