use std::env;
use std::io;
use std::io::IsTerminal;
use std::io::Write;

use cliclack::{Theme, ThemeState, log};
use console::{Style, Term, style};

use crate::auth::{
    API_TOKEN_ENV, ConfigAuthSnapshot, Credential, CredentialKind, CredentialSource,
    SESSION_TOKEN_ENV, SearchAuthPreference, load_config_auth_snapshot, load_credential_inventory,
    normalize_api_token, normalize_session_token, save_credentials_with_preference,
};
use crate::error::KagiError;
use crate::search;

const VALIDATION_QUERY: &str = "rust lang";
const GOLD: u8 = 220;
const AUTH_ASCII_ART: &str = r#"  ███████                                                                     ███████                                              ███████    █████    
  ███████                                                                    █████████                                             ███████  █████████  
  ███████                                                                    █████████                                             ███████  █████████  
  ███████                                                          ███████    ███████                                              ███████   ███████   
  ███████                                                        █████████                                                         ███████             
  ███████       █████████    ██████████████            ███████████████████    ███████                            █████████████     ███████   ███████   
  ███████    ██████████    ██████████████████        ██████████████████       ███████                         ██████████████████   ███████   ███████   
  ███████  ██████████    █████████████████████      █████████████████████     ███████                        █████████████████████ ███████   ███████   
  ████████████████       ███████       █████████   ████████       ███████     ███████   █████████████████   ████████       ███████ ███████   ███████   
  ██████████████        ████████         ███████  ████████         ███████    ███████   █████████████████   ███████                ███████   ███████   
  ███████████████       ████████         ███████  ████████         ███████    ███████   █████████████████   ███████                ███████   ███████   
  █████████████████      ███████        ████████   ████████       ███████     ███████                       ████████       ███████ ███████   ███████   
  ███████   █████████    ███████████████████████    █████████████████████     ███████                        █████████████████████ ███████   ███████   
  ███████     █████████    █████████████████████   ████████████████████       ███████                         ██████████████████   ███████   ███████   
  ███████       ██████████   █████████████ █████  ███████████████████         ███████                            █████████████     ███████   ███████   
                                                  ███████                                                                                              
                                                  ███████████████████                                                                                  
                                                  ███████████████████████                                                                              
                                                   ███████████████████████                                                                             
                                                      ████████    ████████                                                                             
                                                                    ███"#;

struct KagiAuthTheme;

impl Theme for KagiAuthTheme {
    fn bar_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Active => Style::new().color256(GOLD).bold(),
            ThemeState::Cancel => Style::new().dim(),
            ThemeState::Submit => Style::new().color256(GOLD),
            ThemeState::Error(_) => Style::new().red(),
        }
    }

    fn state_symbol_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Active => Style::new().color256(GOLD).bold(),
            ThemeState::Cancel => Style::new().dim(),
            ThemeState::Submit => Style::new().color256(GOLD).bold(),
            ThemeState::Error(_) => Style::new().red().bold(),
        }
    }

    fn radio_symbol(&self, state: &ThemeState, selected: bool) -> String {
        match state {
            ThemeState::Active if selected => style("●").color256(GOLD).bold(),
            ThemeState::Active if !selected => style("○").dim(),
            _ => style("").dim(),
        }
        .to_string()
    }

    fn info_symbol(&self) -> String {
        Style::new().color256(GOLD).bold().apply_to("●").to_string()
    }

    fn warning_symbol(&self) -> String {
        Style::new().color256(GOLD).bold().apply_to("▲").to_string()
    }

    fn active_symbol(&self) -> String {
        Style::new().color256(GOLD).bold().apply_to("◆").to_string()
    }

    fn submit_symbol(&self) -> String {
        Style::new().color256(GOLD).bold().apply_to("◇").to_string()
    }
}

pub fn supports_interactive_auth() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal() && io::stderr().is_terminal()
}

pub async fn run_auth_wizard() -> Result<(), KagiError> {
    let _ = ctrlc::set_handler(|| {});
    cliclack::set_theme(KagiAuthTheme);
    let _ = cliclack::clear_screen();

    render_auth_intro()?;

    let inventory = load_credential_inventory()?;
    wizard_io(cliclack::note(
        "Current Auth",
        format_inventory_summary(&inventory),
    ))?;
    wizard_io(log::info("Environment variables override .kagi.toml."))?;

    let Some(kind) = prompt_result(
        cliclack::select("Choose an auth method")
            .item(
                CredentialKind::SessionToken,
                "Session Link",
                "Search, Quick Answer, Assistant, Translate, subscriber Summarizer",
            )
            .item(
                CredentialKind::ApiToken,
                "API Token",
                "FastGPT, enrich, public Summarizer, and API-first base search",
            )
            .interact(),
    )?
    else {
        return cancel_wizard("Auth setup canceled. No changes were made.");
    };

    let config_snapshot = load_config_auth_snapshot()?;
    wizard_io(cliclack::note(
        method_title(kind),
        method_instructions(kind),
    ))?;

    let Some(raw_input) = prompt_result(
        cliclack::password(method_prompt(kind))
            .mask('*')
            .validate(
                move |value: &String| match build_candidate_credential(kind, value) {
                    Ok(_) => Ok(()),
                    Err(error) => Err(error.to_string()),
                },
            )
            .interact(),
    )?
    else {
        return cancel_wizard("Auth setup canceled. No changes were made.");
    };

    if has_config_credential(&config_snapshot, kind) {
        let Some(replace) = prompt_result(
            cliclack::confirm(format!(
                "Replace the existing {} saved in {}?",
                kind_display(kind),
                config_snapshot.config_path.display()
            ))
            .initial_value(false)
            .interact(),
        )?
        else {
            return cancel_wizard("Auth setup canceled. No changes were made.");
        };

        if !replace {
            return cancel_wizard("Kept the existing config value. No changes were made.");
        }
    }

    let credential = build_candidate_credential(kind, &raw_input)?;

    let spinner = cliclack::spinner();
    spinner.start(format!("Validating {}...", kind_display(kind)));
    let validation_result = validate_credential(&credential).await;
    match &validation_result {
        Ok(()) => spinner.stop(format!("Validated {}.", kind_display(kind))),
        Err(_) => spinner.stop(format!("Could not validate {}.", kind_display(kind))),
    }

    let save_after_failure = if let Err(error) = &validation_result {
        wizard_io(cliclack::note(
            "Validation Note",
            validation_warning(kind, error),
        ))?;
        let Some(save_anyway) = prompt_result(
            cliclack::confirm(format!("Save this {} anyway?", kind_display(kind)))
                .initial_value(kind == CredentialKind::ApiToken)
                .interact(),
        )?
        else {
            return cancel_wizard("Auth setup canceled. No changes were made.");
        };

        save_anyway
    } else {
        true
    };

    if !save_after_failure {
        return cancel_wizard("Auth setup canceled. No changes were made.");
    }

    let preferred_auth = if should_prompt_preference(&config_snapshot, kind) {
        let Some(use_selected_method) = prompt_result(
            cliclack::confirm(format!(
                "Both auth methods are configured. Make {} the preferred path for base search?",
                kind_display(kind)
            ))
            .initial_value(true)
            .interact(),
        )?
        else {
            return cancel_wizard("Auth setup canceled. No changes were made.");
        };

        if use_selected_method {
            Some(preference_for_kind(kind))
        } else {
            None
        }
    } else {
        None
    };

    let saved_inventory = match kind {
        CredentialKind::ApiToken => {
            save_credentials_with_preference(Some(&credential.value), None, preferred_auth)?
        }
        CredentialKind::SessionToken => {
            save_credentials_with_preference(None, Some(&credential.value), preferred_auth)?
        }
    };

    if let Some(message) = env_override_notice(kind) {
        wizard_io(cliclack::note("Environment override", message))?;
    }

    wizard_io(cliclack::note(
        "Saved",
        format_saved_summary(&saved_inventory),
    ))?;
    wizard_io(cliclack::note("Try This Next", next_steps(kind)))?;

    match validation_result {
        Ok(()) => {
            wizard_io(cliclack::outro(format!(
                "{} saved and validated.",
                kind_display(kind)
            )))?;
        }
        Err(_) => {
            wizard_io(cliclack::outro(format!(
                "{} saved. Validation still needs follow-up.",
                kind_display(kind)
            )))?;
        }
    }

    Ok(())
}

fn render_auth_intro() -> Result<(), KagiError> {
    let term = Term::stdout();
    let width = term.size_checked().map(|(_rows, cols)| cols).unwrap_or(0);

    if should_render_auth_ascii(width) {
        let mut stdout = io::stdout().lock();
        for line in AUTH_ASCII_ART.lines() {
            writeln!(stdout, "{}", style(line.trim_end()).color256(GOLD).bold()).map_err(
                |error| KagiError::Config(format!("interactive auth output failed: {error}")),
            )?;
        }
        writeln!(stdout).map_err(|error| {
            KagiError::Config(format!("interactive auth output failed: {error}"))
        })?;
        Ok(())
    } else {
        wizard_io(cliclack::intro(
            style(" kagi auth ").black().on_color256(GOLD),
        ))
    }
}

fn should_render_auth_ascii(width: u16) -> bool {
    width >= auth_ascii_width()
}

fn auth_ascii_width() -> u16 {
    AUTH_ASCII_ART
        .lines()
        .map(|line| line.trim_end().chars().count())
        .max()
        .unwrap_or(0)
        .min(u16::MAX as usize) as u16
}

pub async fn validate_credential(credential: &Credential) -> Result<(), KagiError> {
    let request = search::SearchRequest::new(VALIDATION_QUERY.to_string());
    match credential.kind {
        CredentialKind::ApiToken => {
            search::execute_api_search(&request, &credential.value).await?;
        }
        CredentialKind::SessionToken => {
            search::execute_search(&request, &credential.value).await?;
        }
    }

    Ok(())
}

fn prompt_result<T>(result: io::Result<T>) -> Result<Option<T>, KagiError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(None),
        Err(error) => Err(KagiError::Config(format!(
            "interactive auth prompt failed: {error}"
        ))),
    }
}

fn cancel_wizard(message: &str) -> Result<(), KagiError> {
    wizard_io(cliclack::outro_cancel(message))?;
    Ok(())
}

fn wizard_io<T>(result: io::Result<T>) -> Result<T, KagiError> {
    result.map_err(|error| KagiError::Config(format!("interactive auth output failed: {error}")))
}

fn build_candidate_credential(kind: CredentialKind, input: &str) -> Result<Credential, KagiError> {
    let value = match kind {
        CredentialKind::ApiToken => normalize_api_token(input)?,
        CredentialKind::SessionToken => normalize_session_token(input)?,
    };

    Ok(Credential {
        kind,
        source: CredentialSource::Config,
        value,
    })
}

fn kind_display(kind: CredentialKind) -> &'static str {
    match kind {
        CredentialKind::ApiToken => "API token",
        CredentialKind::SessionToken => "Session Link",
    }
}

fn wizard_status_line(label: &str, value: &str) -> String {
    format!("{label:<13} {value}")
}

fn configured_from(source: CredentialSource) -> String {
    format!("configured via {}", source.as_str())
}

fn inventory_value_line(credential: Option<&Credential>) -> String {
    match credential {
        Some(credential) => configured_from(credential.source),
        None => "not configured".to_string(),
    }
}

fn format_inventory_summary(inventory: &crate::auth::CredentialInventory) -> String {
    [
        wizard_status_line(
            "Selected",
            &inventory
                .preferred_for_status()
                .map(|credential| {
                    format!(
                        "{} ({})",
                        credential.kind.as_str(),
                        credential.source.as_str()
                    )
                })
                .unwrap_or_else(|| "none".to_string()),
        ),
        wizard_status_line("Base Search", inventory.search_preference.as_str()),
        wizard_status_line(
            "Session Link",
            &inventory_value_line(inventory.session_token.as_ref()),
        ),
        wizard_status_line(
            "API Token",
            &inventory_value_line(inventory.api_token.as_ref()),
        ),
        wizard_status_line("Config File", &inventory.config_path.display().to_string()),
    ]
    .join("\n")
}

fn format_saved_summary(inventory: &crate::auth::CredentialInventory) -> String {
    [
        wizard_status_line(
            "Selected",
            &inventory
                .preferred_for_status()
                .map(|credential| {
                    format!(
                        "{} ({})",
                        credential.kind.as_str(),
                        credential.source.as_str()
                    )
                })
                .unwrap_or_else(|| "none".to_string()),
        ),
        wizard_status_line("Base Search", inventory.search_preference.as_str()),
        wizard_status_line(
            "Session Link",
            &inventory_value_line(inventory.session_token.as_ref()),
        ),
        wizard_status_line(
            "API Token",
            &inventory_value_line(inventory.api_token.as_ref()),
        ),
    ]
    .join("\n")
}

fn method_title(kind: CredentialKind) -> &'static str {
    match kind {
        CredentialKind::ApiToken => "API Token Setup",
        CredentialKind::SessionToken => "Session Link Setup",
    }
}

fn method_prompt(kind: CredentialKind) -> &'static str {
    match kind {
        CredentialKind::ApiToken => "Paste your API token",
        CredentialKind::SessionToken => "Paste your Session Link or raw session token",
    }
}

fn method_instructions(kind: CredentialKind) -> String {
    match kind {
        CredentialKind::ApiToken => [
            "Open: https://kagi.com/settings/api",
            "Then copy your API token and paste it here.",
            "",
        ]
        .join("\n"),
        CredentialKind::SessionToken => [
            "Open: https://kagi.com/settings/user_details",
            "Then copy your Session Link and paste the full link or raw token here.",
            "",
        ]
        .join("\n"),
    }
}

fn validation_warning(kind: CredentialKind, error: &KagiError) -> String {
    let mut message = format!("Validation Error:\n{error}");
    if kind == CredentialKind::ApiToken {
        message.push_str(
            "\n\nThis CLI validates API tokens through the Search API path. Some accounts may still want to save the token and test FastGPT or enrich directly.",
        );
    }
    message
}

fn env_override_notice(kind: CredentialKind) -> Option<String> {
    let env_var = env_var_name(kind);

    env::var_os(env_var).map(|_| env_override_message(env_var))
}

fn has_config_credential(snapshot: &ConfigAuthSnapshot, kind: CredentialKind) -> bool {
    match kind {
        CredentialKind::ApiToken => snapshot.api_token.is_some(),
        CredentialKind::SessionToken => snapshot.session_token.is_some(),
    }
}

fn should_prompt_preference(snapshot: &ConfigAuthSnapshot, kind: CredentialKind) -> bool {
    should_prompt_preference_with_other_method(
        snapshot,
        kind,
        other_method_configured(snapshot, kind) || env_credential_present(other_kind(kind)),
    )
}

fn should_prompt_preference_with_other_method(
    snapshot: &ConfigAuthSnapshot,
    kind: CredentialKind,
    other_method_configured: bool,
) -> bool {
    other_method_configured && snapshot.search_preference != preference_for_kind(kind)
}

fn preference_for_kind(kind: CredentialKind) -> SearchAuthPreference {
    match kind {
        CredentialKind::ApiToken => SearchAuthPreference::Api,
        CredentialKind::SessionToken => SearchAuthPreference::Session,
    }
}

fn other_kind(kind: CredentialKind) -> CredentialKind {
    match kind {
        CredentialKind::ApiToken => CredentialKind::SessionToken,
        CredentialKind::SessionToken => CredentialKind::ApiToken,
    }
}

fn other_method_configured(snapshot: &ConfigAuthSnapshot, kind: CredentialKind) -> bool {
    has_config_credential(snapshot, other_kind(kind))
}

fn env_var_name(kind: CredentialKind) -> &'static str {
    match kind {
        CredentialKind::ApiToken => API_TOKEN_ENV,
        CredentialKind::SessionToken => SESSION_TOKEN_ENV,
    }
}

fn env_credential_present(kind: CredentialKind) -> bool {
    env::var(env_var_name(kind))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .is_some()
}

fn env_override_message(env_var: &str) -> String {
    format!(
        "{env_var} is set in the environment, so it will override the config value you just saved until you unset it."
    )
}

fn next_steps(kind: CredentialKind) -> String {
    match kind {
        CredentialKind::ApiToken => [
            "kagi auth check",
            "kagi fastgpt \"what changed in rust 1.86?\"",
            "kagi enrich web \"local-first software\"",
        ]
        .join("\n"),
        CredentialKind::SessionToken => [
            "kagi auth check",
            "kagi search --format pretty \"rust programming language\"",
            "kagi quick --format pretty \"what is rust\"",
        ]
        .join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(
        api_token: Option<&str>,
        session_token: Option<&str>,
        search_preference: SearchAuthPreference,
    ) -> ConfigAuthSnapshot {
        ConfigAuthSnapshot {
            config_path: ".kagi.toml".into(),
            api_token: api_token.map(str::to_string),
            session_token: session_token.map(str::to_string),
            search_preference,
        }
    }

    #[test]
    fn prompts_for_preference_when_both_methods_exist_and_choice_changes_it() {
        let config = snapshot(Some("api"), None, SearchAuthPreference::Session);
        assert!(!should_prompt_preference_with_other_method(
            &config,
            CredentialKind::ApiToken,
            false
        ));

        let config = snapshot(Some("api"), Some("session"), SearchAuthPreference::Session);
        assert!(should_prompt_preference_with_other_method(
            &config,
            CredentialKind::ApiToken,
            true
        ));
        assert!(!should_prompt_preference_with_other_method(
            &config,
            CredentialKind::SessionToken,
            true
        ));
    }

    #[test]
    fn prompts_for_preference_when_other_method_exists_via_environment() {
        let config = snapshot(None, None, SearchAuthPreference::Session);
        assert!(should_prompt_preference_with_other_method(
            &config,
            CredentialKind::ApiToken,
            true
        ));
        assert!(!should_prompt_preference_with_other_method(
            &config,
            CredentialKind::SessionToken,
            false
        ));
    }

    #[test]
    fn builds_session_instructions_with_official_settings_page() {
        let instructions = method_instructions(CredentialKind::SessionToken);
        assert!(instructions.contains("https://kagi.com/settings/user_details"));
        assert!(instructions.contains("Session Link"));
    }

    #[test]
    fn builds_api_instructions_with_official_settings_page() {
        let instructions = method_instructions(CredentialKind::ApiToken);
        assert!(instructions.contains("https://kagi.com/settings/api"));
        assert!(instructions.contains("API token"));
    }

    #[test]
    fn api_validation_warning_mentions_search_api_behavior() {
        let warning = validation_warning(
            CredentialKind::ApiToken,
            &KagiError::Auth("403 Forbidden".to_string()),
        );
        assert!(warning.contains("Search API"));
    }

    #[test]
    fn session_validation_warning_stays_generic() {
        let warning = validation_warning(
            CredentialKind::SessionToken,
            &KagiError::Auth("401 Unauthorized".to_string()),
        );
        assert!(!warning.contains("Search API"));
    }

    #[test]
    fn env_override_message_mentions_environment_precedence() {
        let message = env_override_message(API_TOKEN_ENV);
        assert!(message.contains(API_TOKEN_ENV));
        assert!(message.contains("override the config value"));
    }

    #[test]
    fn builds_session_candidate_from_full_session_link() {
        let credential = build_candidate_credential(
            CredentialKind::SessionToken,
            "https://kagi.com/search?token=session-demo-token",
        )
        .expect("session link should normalize");

        assert_eq!(credential.kind, CredentialKind::SessionToken);
        assert_eq!(credential.value, "session-demo-token");
    }

    #[test]
    fn renders_ascii_intro_only_on_wide_terminals() {
        let width = auth_ascii_width();
        assert!(width > 0);
        assert!(!should_render_auth_ascii(width - 1));
        assert!(should_render_auth_ascii(width));
        assert!(should_render_auth_ascii(width.saturating_add(20)));
    }
}
