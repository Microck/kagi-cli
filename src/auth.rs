use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use reqwest::Url;

use serde::Deserialize;

use crate::error::KagiError;

const DEFAULT_CONFIG_PATH: &str = ".kagi.toml";
pub const API_TOKEN_ENV: &str = "KAGI_API_TOKEN";
pub const SESSION_TOKEN_ENV: &str = "KAGI_SESSION_TOKEN";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialKind {
    ApiToken,
    SessionToken,
}

impl CredentialKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApiToken => "api-token",
            Self::SessionToken => "session-token",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    Env,
    Config,
}

impl CredentialSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Config => "config",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchAuthPreference {
    Session,
    Api,
}

impl SearchAuthPreference {
    fn parse(raw: &str) -> Result<Self, KagiError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "session" => Ok(Self::Session),
            "api" => Ok(Self::Api),
            other => Err(KagiError::Config(format!(
                "invalid [auth.preferred_auth] value `{other}`; expected `session` or `api`"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Api => "api",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    pub kind: CredentialKind,
    pub source: CredentialSource,
    pub value: String,
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
            .field("kind", &self.kind)
            .field("source", &self.source)
            .field("value", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct SearchCredentials {
    pub primary: Credential,
    pub fallback_session: Option<Credential>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchAuthRequirement {
    Base,
    Lens,
    Filtered,
}

#[derive(Debug, Clone)]
pub struct CredentialInventory {
    pub api_token: Option<Credential>,
    pub session_token: Option<Credential>,
    pub search_preference: SearchAuthPreference,
    pub config_path: PathBuf,
}

impl CredentialInventory {
    pub fn resolve_for_search(
        &self,
        requirement: SearchAuthRequirement,
    ) -> Result<SearchCredentials, KagiError> {
        match requirement {
            SearchAuthRequirement::Lens | SearchAuthRequirement::Filtered => {
                let session = self.session_token.clone().ok_or_else(|| {
                    KagiError::Config(match requirement {
                        SearchAuthRequirement::Lens => {
                            "lens search requires KAGI_SESSION_TOKEN (env or .kagi.toml [auth.session_token])"
                                .to_string()
                        }
                        SearchAuthRequirement::Filtered => {
                            "search filters require KAGI_SESSION_TOKEN (env or .kagi.toml [auth.session_token])"
                                .to_string()
                        }
                        SearchAuthRequirement::Base => unreachable!(),
                    })
                })?;

                return Ok(SearchCredentials {
                    primary: session,
                    fallback_session: None,
                });
            }
            SearchAuthRequirement::Base => {}
        }

        match self.search_preference {
            SearchAuthPreference::Session => {
                if let Some(session_token) = self.session_token.clone() {
                    return Ok(SearchCredentials {
                        primary: session_token,
                        fallback_session: self.api_token.clone(),
                    });
                }

                if let Some(api_token) = self.api_token.clone() {
                    return Ok(SearchCredentials {
                        primary: api_token,
                        fallback_session: None,
                    });
                }
            }
            SearchAuthPreference::Api => {
                if let Some(api_token) = self.api_token.clone() {
                    return Ok(SearchCredentials {
                        primary: api_token,
                        fallback_session: self.session_token.clone(),
                    });
                }

                if let Some(session_token) = self.session_token.clone() {
                    return Ok(SearchCredentials {
                        primary: session_token,
                        fallback_session: None,
                    });
                }
            }
        }

        Err(KagiError::Config(
            "missing credentials: set KAGI_API_TOKEN or KAGI_SESSION_TOKEN (env), or add [auth] api_token/session_token to .kagi.toml".to_string(),
        ))
    }

    pub fn preferred_for_status(&self) -> Option<&Credential> {
        match self.search_preference {
            SearchAuthPreference::Session => {
                self.session_token.as_ref().or(self.api_token.as_ref())
            }
            SearchAuthPreference::Api => self.api_token.as_ref().or(self.session_token.as_ref()),
        }
    }
}

#[derive(Debug, Default, Deserialize, serde::Serialize)]
struct ConfigFile {
    auth: Option<AuthConfig>,
}

#[derive(Debug, Default, Deserialize, serde::Serialize)]
struct AuthConfig {
    api_token: Option<String>,
    session_token: Option<String>,
    preferred_auth: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConfigAuthSnapshot {
    pub config_path: PathBuf,
    pub api_token: Option<String>,
    pub session_token: Option<String>,
    pub search_preference: SearchAuthPreference,
}

pub fn load_credential_inventory() -> Result<CredentialInventory, KagiError> {
    load_credential_inventory_from_path(Path::new(DEFAULT_CONFIG_PATH))
}

fn load_credential_inventory_from_path(
    config_path: &Path,
) -> Result<CredentialInventory, KagiError> {
    let config = read_config_file(config_path)?;
    let search_preference = config
        .auth
        .as_ref()
        .and_then(|auth| auth.preferred_auth.as_deref())
        .map(SearchAuthPreference::parse)
        .transpose()?
        .unwrap_or(SearchAuthPreference::Session);

    let env_api = read_env_credential(API_TOKEN_ENV).map(|value| Credential {
        kind: CredentialKind::ApiToken,
        source: CredentialSource::Env,
        value,
    });
    let env_session = read_env_credential(SESSION_TOKEN_ENV)
        .map(|value| build_session_credential(&value, CredentialSource::Env))
        .transpose()?;

    let config_api = config
        .auth
        .as_ref()
        .and_then(|auth| auth.api_token.as_ref())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| Credential {
            kind: CredentialKind::ApiToken,
            source: CredentialSource::Config,
            value,
        });

    let config_session = config
        .auth
        .as_ref()
        .and_then(|auth| auth.session_token.as_ref())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| build_session_credential(&value, CredentialSource::Config))
        .transpose()?;

    Ok(CredentialInventory {
        api_token: env_api.or(config_api),
        session_token: env_session.or(config_session),
        search_preference,
        config_path: config_path.to_path_buf(),
    })
}

pub fn format_status(inventory: &CredentialInventory) -> String {
    let selected = inventory.preferred_for_status();
    let selected_line = if let Some(credential) = selected {
        format!(
            "selected: {} ({})",
            credential.kind.as_str(),
            credential.source.as_str()
        )
    } else {
        "selected: none".to_string()
    };

    let api_line = format_status_line("api token", inventory.api_token.as_ref());
    let session_line = format_status_line("session token", inventory.session_token.as_ref());

    format!(
        "{selected_line}\npreferred auth for base search: {}\n{api_line}\n{session_line}\nconfig path: {}\nprecedence: env > config; base search defaults to session unless [auth.preferred_auth] = \"api\"; lens search requires session token",
        inventory.search_preference.as_str(),
        inventory.config_path.display(),
    )
}

pub fn load_config_auth_snapshot() -> Result<ConfigAuthSnapshot, KagiError> {
    load_config_auth_snapshot_from_path(Path::new(DEFAULT_CONFIG_PATH))
}

fn load_config_auth_snapshot_from_path(
    config_path: &Path,
) -> Result<ConfigAuthSnapshot, KagiError> {
    let config = read_config_file(config_path)?;
    let search_preference = config
        .auth
        .as_ref()
        .and_then(|auth| auth.preferred_auth.as_deref())
        .map(SearchAuthPreference::parse)
        .transpose()?
        .unwrap_or(SearchAuthPreference::Session);

    let api_token = config
        .auth
        .as_ref()
        .and_then(|auth| auth.api_token.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let session_token = config
        .auth
        .as_ref()
        .and_then(|auth| auth.session_token.as_deref())
        .map(normalize_session_token)
        .transpose()?;

    Ok(ConfigAuthSnapshot {
        config_path: config_path.to_path_buf(),
        api_token,
        session_token,
        search_preference,
    })
}

fn format_status_line(label: &str, credential: Option<&Credential>) -> String {
    match credential {
        Some(credential) => format!("{label}: configured via {}", credential.source.as_str()),
        None => format!("{label}: not configured"),
    }
}

fn read_env_credential(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_session_credential(
    raw_value: &str,
    source: CredentialSource,
) -> Result<Credential, KagiError> {
    Ok(Credential {
        kind: CredentialKind::SessionToken,
        source,
        value: normalize_session_token(raw_value)?,
    })
}

pub fn normalize_api_token(input: &str) -> Result<String, KagiError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(KagiError::Config("api token cannot be empty".to_string()));
    }

    Ok(trimmed.to_string())
}

pub fn save_credentials(
    api_token: Option<&str>,
    session_input: Option<&str>,
) -> Result<CredentialInventory, KagiError> {
    save_credentials_with_preference(api_token, session_input, None)
}

pub fn save_credentials_with_preference(
    api_token: Option<&str>,
    session_input: Option<&str>,
    preferred_auth: Option<SearchAuthPreference>,
) -> Result<CredentialInventory, KagiError> {
    save_credentials_with_preference_to_path(
        Path::new(DEFAULT_CONFIG_PATH),
        api_token,
        session_input,
        preferred_auth,
    )
}

fn save_credentials_with_preference_to_path(
    config_path: &Path,
    api_token: Option<&str>,
    session_input: Option<&str>,
    preferred_auth: Option<SearchAuthPreference>,
) -> Result<CredentialInventory, KagiError> {
    if api_token.is_none() && session_input.is_none() {
        return Err(KagiError::Config(
            "nothing to save: provide --api-token, --session-token, or both".to_string(),
        ));
    }

    let mut config = read_config_file(config_path)?;
    let auth = config.auth.get_or_insert_with(AuthConfig::default);

    if let Some(api_token) = api_token {
        auth.api_token = Some(normalize_api_token(api_token)?);
    }

    if let Some(session_input) = session_input {
        let normalized = normalize_session_token(session_input)?;
        auth.session_token = Some(normalized);
    }

    if let Some(preferred_auth) = preferred_auth {
        auth.preferred_auth = Some(preferred_auth.as_str().to_string());
    }

    let raw = toml::to_string(&config).map_err(|error| {
        KagiError::Config(format!(
            "failed to serialize config file {}: {error}",
            config_path.display()
        ))
    })?;
    fs::write(config_path, raw).map_err(|error| {
        KagiError::Config(format!(
            "failed to write config file {}: {error}",
            config_path.display()
        ))
    })?;
    secure_config_permissions(config_path)?;

    load_credential_inventory_from_path(config_path)
}

#[cfg(test)]
fn normalize_optional_session_token(input: Option<String>) -> Result<Option<String>, KagiError> {
    input
        .map(|value| normalize_session_token(&value))
        .transpose()
}

pub fn normalize_session_token(input: &str) -> Result<String, KagiError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(KagiError::Config(
            "session token cannot be empty".to_string(),
        ));
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let url = Url::parse(trimmed)
            .map_err(|error| KagiError::Config(format!("invalid session link URL: {error}")))?;
        if let Some(token) = url
            .query_pairs()
            .find_map(|(key, value)| (key == "token").then(|| value.into_owned()))
        {
            if token.trim().is_empty() {
                return Err(KagiError::Config(
                    "session link URL contained an empty token parameter".to_string(),
                ));
            }
            return Ok(token);
        }

        return Err(KagiError::Config(
            "session link URL must include a non-empty token= query parameter".to_string(),
        ));
    }

    Ok(trimmed.to_string())
}

fn read_config_file(path: &Path) -> Result<ConfigFile, KagiError> {
    if !path.exists() {
        return Ok(ConfigFile::default());
    }

    let raw = fs::read_to_string(path).map_err(|error| {
        KagiError::Config(format!(
            "failed to read config file {}: {error}",
            path.display()
        ))
    })?;

    toml::from_str(&raw).map_err(|error| {
        KagiError::Config(format!(
            "failed to parse config file {}: {error}",
            path.display()
        ))
    })
}

#[cfg(unix)]
fn secure_config_permissions(path: &Path) -> Result<(), KagiError> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, permissions).map_err(|error| {
        KagiError::Config(format!(
            "failed to secure config file permissions for {}: {error}",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
fn secure_config_permissions(_path: &Path) -> Result<(), KagiError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        env::temp_dir().join(format!("kagi-auth-test-{nanos}.toml"))
    }

    fn set_env_var(key: &str, value: &str) {
        unsafe { env::set_var(key, value) }
    }

    fn remove_env_var(key: &str) {
        unsafe { env::remove_var(key) }
    }

    #[test]
    fn prefers_env_over_config_values() {
        let path = unique_path();
        fs::write(
            &path,
            "[auth]\napi_token = \"config-api\"\nsession_token = \"config-session\"\n",
        )
        .expect("write config");

        set_env_var(API_TOKEN_ENV, "env-api");
        set_env_var(SESSION_TOKEN_ENV, "env-session");

        let config = read_config_file(&path).expect("config parses");

        let inventory = CredentialInventory {
            api_token: read_env_credential(API_TOKEN_ENV)
                .map(|value| Credential {
                    kind: CredentialKind::ApiToken,
                    source: CredentialSource::Env,
                    value,
                })
                .or_else(|| {
                    config
                        .auth
                        .as_ref()
                        .and_then(|auth| auth.api_token.as_ref())
                        .map(|value| Credential {
                            kind: CredentialKind::ApiToken,
                            source: CredentialSource::Config,
                            value: value.clone(),
                        })
                }),
            session_token: read_env_credential(SESSION_TOKEN_ENV)
                .map(|value| Credential {
                    kind: CredentialKind::SessionToken,
                    source: CredentialSource::Env,
                    value,
                })
                .or_else(|| {
                    config
                        .auth
                        .as_ref()
                        .and_then(|auth| auth.session_token.as_ref())
                        .map(|value| Credential {
                            kind: CredentialKind::SessionToken,
                            source: CredentialSource::Config,
                            value: value.clone(),
                        })
                }),
            search_preference: SearchAuthPreference::Session,
            config_path: path.clone(),
        };

        assert_eq!(inventory.api_token.unwrap().source, CredentialSource::Env);
        assert_eq!(
            inventory.session_token.unwrap().source,
            CredentialSource::Env
        );

        remove_env_var(API_TOKEN_ENV);
        remove_env_var(SESSION_TOKEN_ENV);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_empty_api_token_input() {
        let error = normalize_api_token("   ").expect_err("empty api token should fail");
        assert!(error.to_string().contains("api token cannot be empty"));
    }

    #[test]
    fn requires_session_for_lens_search() {
        let inventory = CredentialInventory {
            api_token: Some(Credential {
                kind: CredentialKind::ApiToken,
                source: CredentialSource::Env,
                value: "api".to_string(),
            }),
            session_token: None,
            search_preference: SearchAuthPreference::Session,
            config_path: PathBuf::from(DEFAULT_CONFIG_PATH),
        };

        let error = inventory
            .resolve_for_search(SearchAuthRequirement::Lens)
            .expect_err("lens should require session token");
        assert!(matches!(error, KagiError::Config(_)));
        assert!(error.to_string().contains("requires KAGI_SESSION_TOKEN"));
    }

    #[test]
    fn requires_session_for_filtered_search() {
        let inventory = CredentialInventory {
            api_token: Some(Credential {
                kind: CredentialKind::ApiToken,
                source: CredentialSource::Env,
                value: "api".to_string(),
            }),
            session_token: None,
            search_preference: SearchAuthPreference::Session,
            config_path: PathBuf::from(DEFAULT_CONFIG_PATH),
        };

        let error = inventory
            .resolve_for_search(SearchAuthRequirement::Filtered)
            .expect_err("filtered search should require session token");
        assert!(matches!(error, KagiError::Config(_)));
        assert!(error.to_string().contains("search filters require"));
    }

    #[test]
    fn base_search_keeps_api_token_as_fallback_when_session_is_preferred() {
        let inventory = CredentialInventory {
            api_token: Some(Credential {
                kind: CredentialKind::ApiToken,
                source: CredentialSource::Env,
                value: "api".to_string(),
            }),
            session_token: Some(Credential {
                kind: CredentialKind::SessionToken,
                source: CredentialSource::Env,
                value: "session".to_string(),
            }),
            search_preference: SearchAuthPreference::Session,
            config_path: PathBuf::from(DEFAULT_CONFIG_PATH),
        };

        let credentials = inventory
            .resolve_for_search(SearchAuthRequirement::Base)
            .expect("base search resolves credential");
        assert_eq!(credentials.primary.kind, CredentialKind::SessionToken);
        assert_eq!(
            credentials
                .fallback_session
                .expect("api fallback exists")
                .kind,
            CredentialKind::ApiToken
        );
    }

    #[test]
    fn prefers_session_for_base_search_by_default() {
        let inventory = CredentialInventory {
            api_token: Some(Credential {
                kind: CredentialKind::ApiToken,
                source: CredentialSource::Env,
                value: "api".to_string(),
            }),
            session_token: Some(Credential {
                kind: CredentialKind::SessionToken,
                source: CredentialSource::Env,
                value: "session".to_string(),
            }),
            search_preference: SearchAuthPreference::Session,
            config_path: PathBuf::from(DEFAULT_CONFIG_PATH),
        };

        let credentials = inventory
            .resolve_for_search(SearchAuthRequirement::Base)
            .expect("base search resolves credential");
        assert_eq!(credentials.primary.kind, CredentialKind::SessionToken);
    }

    #[test]
    fn prefers_api_for_base_search_when_configured() {
        let inventory = CredentialInventory {
            api_token: Some(Credential {
                kind: CredentialKind::ApiToken,
                source: CredentialSource::Env,
                value: "api".to_string(),
            }),
            session_token: Some(Credential {
                kind: CredentialKind::SessionToken,
                source: CredentialSource::Env,
                value: "session".to_string(),
            }),
            search_preference: SearchAuthPreference::Api,
            config_path: PathBuf::from(DEFAULT_CONFIG_PATH),
        };

        let credentials = inventory
            .resolve_for_search(SearchAuthRequirement::Base)
            .expect("base search resolves credential");
        assert_eq!(credentials.primary.kind, CredentialKind::ApiToken);
    }

    #[test]
    fn rejects_invalid_preferred_auth_value() {
        let path = unique_path();
        fs::write(&path, "[auth]\npreferred_auth = \"weird\"\n").expect("write config");

        let raw = fs::read_to_string(&path).expect("read config");
        let config: ConfigFile = toml::from_str(&raw).expect("parse config");
        let error = config
            .auth
            .as_ref()
            .and_then(|auth| auth.preferred_auth.as_deref())
            .map(SearchAuthPreference::parse)
            .transpose()
            .expect_err("invalid config should fail");

        assert!(error.to_string().contains("expected `session` or `api`"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn status_output_redacts_values() {
        let inventory = CredentialInventory {
            api_token: Some(Credential {
                kind: CredentialKind::ApiToken,
                source: CredentialSource::Env,
                value: "secret-api".to_string(),
            }),
            session_token: None,
            search_preference: SearchAuthPreference::Session,
            config_path: PathBuf::from(DEFAULT_CONFIG_PATH),
        };

        let status = format_status(&inventory);
        assert!(status.contains("selected: api-token (env)"));
        assert!(status.contains("preferred auth for base search: session"));
        assert!(!status.contains("secret-api"));
    }

    #[test]
    fn credential_debug_redacts_secret_values() {
        let credential = Credential {
            kind: CredentialKind::ApiToken,
            source: CredentialSource::Env,
            value: "super-secret".to_string(),
        };

        let rendered = format!("{credential:?}");
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("super-secret"));
    }

    #[test]
    fn extracts_token_from_session_link_url() {
        let token = normalize_session_token("https://kagi.com/search?token=abc123.def456&foo=bar")
            .expect("session link parses");
        assert_eq!(token, "abc123.def456");
    }

    #[test]
    fn keeps_raw_session_token_input() {
        let token = normalize_session_token("abc123.def456").expect("raw token accepted");
        assert_eq!(token, "abc123.def456");
    }

    #[test]
    fn builds_env_session_credential_from_session_link() {
        let credential = build_session_credential(
            "https://kagi.com/search?token=abc123.def456",
            CredentialSource::Env,
        )
        .expect("session link should normalize");

        assert_eq!(credential.kind, CredentialKind::SessionToken);
        assert_eq!(credential.source, CredentialSource::Env);
        assert_eq!(credential.value, "abc123.def456");
    }

    #[test]
    fn builds_config_session_credential_from_session_link() {
        let credential = build_session_credential(
            "https://kagi.com/search?token=abc123.def456",
            CredentialSource::Config,
        )
        .expect("session link should normalize");

        assert_eq!(credential.kind, CredentialKind::SessionToken);
        assert_eq!(credential.source, CredentialSource::Config);
        assert_eq!(credential.value, "abc123.def456");
    }

    #[test]
    fn rejects_session_link_without_token_param() {
        let error = normalize_session_token("https://kagi.com/search?q=test")
            .expect_err("missing token param should fail");
        assert!(error.to_string().contains("token="));
    }

    #[test]
    fn normalizes_session_link_from_environment_style_input() {
        let normalized = normalize_optional_session_token(Some(
            "https://kagi.com/search?token=env-session-token".to_string(),
        ))
        .expect("session token should normalize");

        assert_eq!(normalized.as_deref(), Some("env-session-token"));
    }

    #[test]
    fn load_config_auth_snapshot_normalizes_session_link_and_preference() {
        let path = unique_path();
        fs::write(
            &path,
            "[auth]\npreferred_auth = \"api\"\nsession_token = \"https://kagi.com/search?token=session-from-link\"\n",
        )
        .expect("write config");

        let snapshot =
            load_config_auth_snapshot_from_path(&path).expect("config snapshot should load");

        assert_eq!(snapshot.search_preference, SearchAuthPreference::Api);
        assert_eq!(snapshot.session_token.as_deref(), Some("session-from-link"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_credentials_preserves_existing_values_when_only_one_is_updated() {
        let path = unique_path();
        fs::write(
            &path,
            "[auth]\napi_token = \"existing-api\"\nsession_token = \"existing-session\"\npreferred_auth = \"api\"\n",
        )
        .expect("write config");

        save_credentials_with_preference_to_path(
            &path,
            None,
            Some("https://kagi.com/search?token=new-session"),
            None,
        )
        .expect("save should succeed");
        let snapshot =
            load_config_auth_snapshot_from_path(&path).expect("config snapshot should load");

        assert_eq!(snapshot.api_token.as_deref(), Some("existing-api"));
        assert_eq!(snapshot.session_token.as_deref(), Some("new-session"));
        assert_eq!(snapshot.search_preference, SearchAuthPreference::Api);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_credentials_updates_preference_when_requested() {
        let path = unique_path();

        let inventory = save_credentials_with_preference_to_path(
            &path,
            Some("new-api"),
            Some("https://kagi.com/search?token=new-session"),
            Some(SearchAuthPreference::Api),
        )
        .expect("save should succeed");

        assert_eq!(inventory.search_preference, SearchAuthPreference::Api);

        let raw = fs::read_to_string(&path).expect("read saved config");
        assert!(raw.contains("preferred_auth = \"api\""));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let _ = fs::remove_file(path);
    }
}
