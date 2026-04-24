mod api;
mod auth;
mod auth_wizard;
mod cli;
mod error;
mod http;
mod parser;
mod quick;
mod search;
#[cfg(test)]
#[path = "test-support.rs"]
mod test_support;
mod types;

use clap::{CommandFactory, Parser};
use clap_complete::{generate, shells};

use crate::api::{
    NewsFilterRequest, execute_ask_page, execute_assistant_prompt, execute_assistant_thread_delete,
    execute_assistant_thread_export, execute_assistant_thread_get, execute_assistant_thread_list,
    execute_custom_assistant_create, execute_custom_assistant_delete, execute_custom_assistant_get,
    execute_custom_assistant_list, execute_custom_assistant_update, execute_custom_bang_create,
    execute_custom_bang_delete, execute_custom_bang_get, execute_custom_bang_list,
    execute_custom_bang_update, execute_enrich_news, execute_enrich_web, execute_fastgpt,
    execute_lens_create, execute_lens_delete, execute_lens_get, execute_lens_list,
    execute_lens_set_enabled, execute_lens_update, execute_news, execute_news_categories,
    execute_news_chaos, execute_news_filter_presets, execute_redirect_create,
    execute_redirect_delete, execute_redirect_get, execute_redirect_list,
    execute_redirect_set_enabled, execute_redirect_update, execute_smallweb,
    execute_subscriber_summarize, execute_summarize, execute_translate,
};
use crate::auth::{
    Credential, CredentialKind, SearchAuthRequirement, SearchCredentials, format_status,
    load_credential_inventory, save_credentials,
};
use crate::auth_wizard::{run_auth_wizard, supports_interactive_auth, validate_credential};
use crate::cli::{
    AssistantCustomSubcommand, AssistantOutputFormat, AssistantSubcommand,
    AssistantThreadExportFormat, AssistantThreadSubcommand, AuthSetArgs, AuthSubcommand,
    BangSubcommand, Cli, Commands, CompletionShell, CustomBangSubcommand, EnrichSubcommand,
    SearchOrder, SearchTime, TranslateArgs,
};
use crate::error::KagiError;
use crate::quick::{execute_quick, format_quick_markdown, format_quick_pretty};
use crate::types::{
    AskPageRequest, AssistantProfileCreateRequest, AssistantProfileUpdateRequest,
    AssistantPromptRequest, CustomBangCreateRequest, CustomBangUpdateRequest, FastGptRequest,
    LensCreateRequest, LensUpdateRequest, QuickResponse, RedirectRuleCreateRequest,
    RedirectRuleUpdateRequest, SearchResponse, SubscriberSummarizeRequest, SummarizeRequest,
    TranslateCommandRequest,
};
use serde_json::Value;
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};
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
    order: Option<SearchOrder>,
    verbatim: bool,
    personalized: bool,
    no_personalized: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    init_tracing();
    if let Err(error) = run().await {
        error!(error = %error, "kagi exited with error");
        eprintln!("{error}");
        std::process::exit(1);
    }
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
            "kagi auth is interactive on TTYs; use `kagi auth set`, `kagi auth status`, or `kagi auth check` in non-interactive environments"
                .to_string(),
        ));
    }

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
            let options = SearchRequestOptions {
                snap: args.snap,
                lens: args.lens,
                region: args.region,
                time: args.time,
                from_date: args.from_date,
                to_date: args.to_date,
                order: args.order,
                verbatim: args.verbatim,
                personalized: args.personalized,
                no_personalized: args.no_personalized,
            };
            let request = build_search_request(args.query, &options);
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
            args.validate().map_err(KagiError::Config)?;

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
            let token = resolve_session_token()?;
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
                let query = args.query.ok_or_else(|| {
                    KagiError::Config(
                        "assistant prompt mode requires a QUERY unless a thread subcommand is used"
                            .to_string(),
                    )
                })?;
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
                let response = execute_assistant_prompt(&request, &token).await?;
                print_assistant_response(&response, args.format, !args.no_color)
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
        Commands::Quick(args) => {
            let token = resolve_session_token()?;
            let request = search::SearchRequest::new(args.query.trim().to_string());
            let request = if let Some(lens) = args.lens {
                request.with_lens(lens)
            } else {
                request
            };
            let format_str = match args.format {
                cli::QuickOutputFormat::Json => "json",
                cli::QuickOutputFormat::Pretty => "pretty",
                cli::QuickOutputFormat::Compact => "compact",
                cli::QuickOutputFormat::Markdown => "markdown",
            };
            let response = execute_quick(&request, &token).await?;
            print_quick_response(&response, format_str, !args.no_color)
        }
        Commands::Translate(args) => {
            let token = resolve_session_token()?;
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
        Commands::Lens(command) => {
            let token = resolve_session_token()?;
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
            let token = resolve_session_token()?;
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
            let token = resolve_session_token()?;
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
        Commands::Batch(args) => {
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
                SearchRequestOptions {
                    snap: args.snap,
                    lens: args.lens,
                    region: args.region,
                    time: args.time,
                    from_date: args.from_date,
                    to_date: args.to_date,
                    order: args.order,
                    verbatim: args.verbatim,
                    personalized: args.personalized,
                    no_personalized: args.no_personalized,
                },
            )
            .await
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
    let mut cmd = Cli::command();

    match shell {
        CompletionShell::Bash => generate(shells::Bash, &mut cmd, "kagi", &mut std::io::stdout()),
        CompletionShell::Zsh => generate(shells::Zsh, &mut cmd, "kagi", &mut std::io::stdout()),
        CompletionShell::Fish => generate(shells::Fish, &mut cmd, "kagi", &mut std::io::stdout()),
        CompletionShell::PowerShell => {
            generate(shells::PowerShell, &mut cmd, "kagi", &mut std::io::stdout());
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
    let credentials = inventory.resolve_for_search(SearchAuthRequirement::Base)?;

    let selected_kind = credentials.primary.kind;
    let selected_source = credentials.primary.source;
    validate_credential(&credentials.primary).await?;

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

const fn should_fallback_to_session(error: &KagiError) -> bool {
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

fn build_translate_request(args: TranslateArgs) -> Result<TranslateCommandRequest, KagiError> {
    Ok(TranslateCommandRequest {
        text: args.text.trim().to_string(),
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
    } else if request.has_runtime_filters() {
        SearchAuthRequirement::Filtered
    } else {
        SearchAuthRequirement::Base
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), KagiError> {
    let output = serde_json::to_string_pretty(value)
        .map_err(|error| KagiError::Parse(format!("failed to serialize JSON output: {error}")))?;
    println!("{output}");
    Ok(())
}

fn print_compact_json<T: serde::Serialize>(value: &T) -> Result<(), KagiError> {
    let output = serde_json::to_string(value)
        .map_err(|error| KagiError::Parse(format!("failed to serialize JSON output: {error}")))?;
    println!("{output}");
    Ok(())
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

async fn run_search(
    request: search::SearchRequest,
    format: String,
    use_color: bool,
) -> Result<(), KagiError> {
    let inventory = load_credential_inventory()?;
    let credentials = inventory.resolve_for_search(search_auth_requirement(&request))?;

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

async fn run_batch_search(
    queries: Vec<String>,
    concurrency: usize,
    rate_limit: u32,
    format: String,
    use_color: bool,
    options: SearchRequestOptions,
) -> Result<(), KagiError> {
    let inventory = load_credential_inventory()?;
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
    let mut had_errors = false;

    for (query, handle) in handles {
        match handle.await {
            Ok((completed_query, Ok(output))) => results.push((completed_query, output)),
            Ok((completed_query, Err(e))) => {
                error!(query = %completed_query, error = %e, "batch query failed");
                had_errors = true;
            }
            Err(e) => {
                error!(query = %query, error = %e, "batch worker task failed");
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
        } else {
            println!("{}", serde_json::to_string_pretty(&results_json)?);
        }
    } else {
        // For human-readable formats, output with headers
        for (query, response) in results {
            let output = match format.as_str() {
                "pretty" => format_pretty_response(&response, use_color),
                "markdown" => format_markdown_response(&response),
                "csv" => format_csv_response(&response),
                _ => serde_json::to_string_pretty(&response).map_err(|error| {
                    KagiError::Parse(format!("failed to serialize search response: {error}"))
                })?,
            };
            println!("=== Results for: {query} ===");
            println!("{output}");
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
        RateLimiter, SearchRequestOptions, bool_flag_choice, build_search_request,
        format_assistant_markdown, format_assistant_pretty, format_csv_response,
        format_markdown_response, format_pretty_response, is_bare_auth_invocation_from,
        parse_context_memory_json, print_assistant_response, should_fallback_to_session,
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
        };

        let output = format_pretty_response(&response, false);

        assert_eq!(
            output,
            "1. Rust Programming Language\n   https://www.rust-lang.org\n\n   A language empowering everyone to build reliable and efficient software.\n\n2. The Rust Book\n   https://doc.rust-lang.org/book/\n\n   Learn Rust with the official book."
        );
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
                order: Some(SearchOrder::Default),
                verbatim: false,
                personalized: false,
                no_personalized: false,
            },
        );

        assert_eq!(request.time_filter.as_deref(), Some("3"));
        assert_eq!(request.order, None);
        assert!(request.has_runtime_filters());
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
            "expected throttling to delay final acquisition by at least ~200ms, got {:?}",
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
                tag_ids: vec![],
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
