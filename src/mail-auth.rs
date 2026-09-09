//! OAuth device login and token storage for the mail service.
//!
//! Connection details belong to private configuration. Search credentials are
//! never sent to the mail service, and transport errors never print its URLs.

use std::env;
use std::fs::{self, File, OpenOptions};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use reqwest::{Client, Response, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{auth, error::KagiError};

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct MailConfig {
    pub endpoint: Option<String>,
    pub client_id: Option<String>,
    pub issuer: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
}

impl std::fmt::Debug for MailConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MailConfig { <redacted> }")
    }
}

impl MailConfig {
    pub fn load(profile: Option<&str>) -> Result<Self, KagiError> {
        let mut config = auth::load_mail_config(profile)?;
        // An endpoint/client override selects a different connection. Do not
        // carry a saved account's tokens over to that connection.
        for (field, key) in [
            (&mut config.endpoint, "KAGI_MAIL_ENDPOINT"),
            (&mut config.client_id, "KAGI_MAIL_CLIENT_ID"),
        ] {
            if let Some(value) = env_value(key) {
                if field.as_ref() != Some(&value) {
                    config.access_token = None;
                    config.refresh_token = None;
                    config.expires_at = None;
                }
                *field = Some(value);
            }
        }
        if let Some(token) = env_value("KAGI_MAIL_ACCESS_TOKEN") {
            config.access_token = Some(token);
            config.refresh_token = None;
            config.expires_at = None;
        }
        Ok(config)
    }

    pub fn endpoint(&self) -> Result<Url, KagiError> {
        let endpoint = self.endpoint.as_deref().filter(|s| !s.trim().is_empty())
            .ok_or_else(|| KagiError::Config("mail requires KAGI_MAIL_ENDPOINT or [mail].endpoint in the selected profile's config".into()))?;
        private_url(endpoint)
    }

    fn client_id(&self) -> Result<&str, KagiError> {
        self.client_id.as_deref().filter(|s| !s.trim().is_empty())
            .ok_or_else(|| KagiError::Config("mail login requires KAGI_MAIL_CLIENT_ID or [mail].client_id in the selected profile's config".into()))
    }

    fn needs_refresh(&self) -> bool {
        self.expires_at
            .map_or(self.refresh_token.is_some(), |expiry| {
                expiry <= now().saturating_add(30)
            })
    }

    pub fn status(&self) -> Value {
        json!({
            "endpoint_configured": self.endpoint.is_some(),
            "client_id_configured": self.client_id.is_some(),
            "access_token_configured": self.access_token.is_some(),
            "refresh_token_configured": self.refresh_token.is_some(),
            "expired": self.expires_at.map(|expiry| expiry <= now()),
            "token_source": if env_value("KAGI_MAIL_ACCESS_TOKEN").is_some() { "env" } else { "config" },
        })
    }
}

fn env_value(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn client() -> Result<Client, KagiError> {
    Client::builder()
        .user_agent("OpenAI File Downloader, XaiImageApiFetch/1.0")
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(transport_error)
}

/// HTTP is allowed only on loopback, for local services and contract tests.
/// Reject credentials in URLs, and never include the rejected value in errors.
pub fn private_url(value: &str) -> Result<Url, KagiError> {
    let url = Url::parse(value).map_err(|_| {
        KagiError::Config("mail service URL is invalid; use an absolute HTTPS URL".into())
    })?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"));
    if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(KagiError::Config("mail service URLs require HTTPS without embedded credentials or fragments (HTTP is allowed on loopback)".into()));
    }
    Ok(url)
}

pub fn transport_error(error: reqwest::Error) -> KagiError {
    let message = if error.is_timeout() {
        "mail request timed out; retry or check your connection"
    } else {
        "mail request failed; check your connection and private mail configuration"
    };
    KagiError::Network(message.into())
}

pub fn login_required() -> KagiError {
    KagiError::MailAuth("mail authorization is missing or expired; run `kagi mail login` (include --profile when using a named profile), or set KAGI_MAIL_ACCESS_TOKEN".into())
}

pub fn check_status(response: &Response) -> Result<(), KagiError> {
    match response.status().as_u16() {
        200..=299 => Ok(()),
        401 | 403 => Err(login_required()),
        status => Err(KagiError::Network(format!(
            "mail service returned HTTP {status}; retry or check your mail configuration"
        ))),
    }
}

#[derive(Deserialize)]
struct ResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    #[serde(default)]
    scopes_supported: Vec<String>,
}

#[derive(Deserialize)]
struct OAuthMetadata {
    issuer: String,
    token_endpoint: String,
    device_authorization_endpoint: Option<String>,
}

struct Discovery {
    oauth: OAuthMetadata,
    resource: String,
    scope: String,
}

async fn json_response(response: Response) -> Result<Value, KagiError> {
    response
        .json()
        .await
        .map_err(|_| KagiError::Parse("mail service returned invalid JSON".into()))
}

/// OAuth endpoints use HTTP 400/401 for protocol errors. Preserve grant-specific
/// responses, but classify client configuration and temporary failures first.
async fn oauth_response(response: Response) -> Result<(reqwest::StatusCode, Value), KagiError> {
    let status = response.status();
    if !matches!(status.as_u16(), 400 | 401) {
        check_status(&response)?;
    }
    let value = json_response(response).await?;
    match value.get("error").and_then(Value::as_str) {
        Some("invalid_grant") => return Err(login_required()),
        Some("temporarily_unavailable") => {
            return Err(KagiError::Network(format!(
                "mail authorization service is temporarily unavailable (HTTP {}); retry later",
                status.as_u16()
            )));
        }
        Some("invalid_client" | "unauthorized_client") => {
            return Err(KagiError::Config(format!(
                "mail OAuth client was rejected (HTTP {}); check KAGI_MAIL_CLIENT_ID or [mail].client_id",
                status.as_u16()
            )));
        }
        _ => {}
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(login_required());
    }
    Ok((status, value))
}

async fn metadata(client: &Client, url: Url) -> Result<Response, KagiError> {
    client.get(url).send().await.map_err(transport_error)
}

async fn discover(client: &Client, endpoint: Url) -> Result<Discovery, KagiError> {
    let response = client
        .get(endpoint.clone())
        .header("Accept", "application/json, text/event-stream")
        .send()
        .await
        .map_err(transport_error)?;
    let challenge = response
        .headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .and_then(|header| header.to_str().ok())
        .unwrap_or_default();
    let metadata_url = challenge
        .split("resource_metadata=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .ok_or_else(|| {
            KagiError::MailAuth(
                "mail service did not advertise OAuth resource metadata; check [mail].endpoint"
                    .into(),
            )
        })?;
    let response = metadata(client, private_url(metadata_url)?).await?;
    check_status(&response)?;
    let resource: ResourceMetadata = serde_json::from_value(json_response(response).await?)
        .map_err(|_| KagiError::Parse("mail OAuth resource metadata is incomplete".into()))?;
    if private_url(&resource.resource)? != endpoint {
        return Err(KagiError::MailAuth(
            "mail OAuth metadata identifies a different resource; check [mail].endpoint".into(),
        ));
    }
    let issuer = resource.authorization_servers.first().ok_or_else(|| {
        KagiError::MailAuth("mail service did not advertise an OAuth issuer".into())
    })?;
    let issuer_url = private_url(issuer)?;
    let issuer_path = issuer_url.path().trim_end_matches('/');
    let mut discovery_url = issuer_url.clone();
    discovery_url.set_path(&format!(
        "/.well-known/oauth-authorization-server{issuer_path}"
    ));
    discovery_url.set_query(None);
    let mut response = metadata(client, discovery_url).await?;
    // OAuth issuers may publish their endpoints through OpenID Connect
    // discovery instead of RFC 8414. Both are standard discovery mechanisms.
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        let mut oidc_url = issuer_url.clone();
        oidc_url.set_path(&format!("{issuer_path}/.well-known/openid-configuration"));
        oidc_url.set_query(None);
        response = metadata(client, oidc_url).await?;
    }
    check_status(&response)?;
    let oauth: OAuthMetadata = serde_json::from_value(json_response(response).await?)
        .map_err(|_| KagiError::Parse("mail OAuth issuer metadata is incomplete".into()))?;
    if private_url(&oauth.issuer)? != issuer_url {
        return Err(KagiError::MailAuth(
            "mail OAuth discovery returned a different issuer".into(),
        ));
    }
    private_url(&oauth.token_endpoint)?;
    let mut scopes = resource.scopes_supported;
    if !scopes.iter().any(|scope| scope == "offline_access") {
        scopes.push("offline_access".into());
    }
    Ok(Discovery {
        oauth,
        resource: resource.resource,
        scope: scopes.join(" "),
    })
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

fn apply_token(config: &mut MailConfig, value: Value) -> Result<(), KagiError> {
    let token: TokenResponse = serde_json::from_value(value)
        .map_err(|_| KagiError::Parse("mail OAuth response is missing a bearer token".into()))?;
    if !token.token_type.eq_ignore_ascii_case("bearer") || token.access_token.trim().is_empty() {
        return Err(KagiError::MailAuth(
            "mail OAuth response did not provide a valid bearer token".into(),
        ));
    }
    config.access_token = Some(token.access_token);
    if let Some(refresh) = token.refresh_token {
        config.refresh_token = Some(refresh);
    }
    config.expires_at = token
        .expires_in
        .map(|seconds| now().saturating_add(seconds));
    Ok(())
}

/// Serialize token rotation across CLI processes. The file contains no secrets;
/// dropping it releases the OS lock even after cancellation or a failed request.
fn token_lock() -> Result<File, KagiError> {
    let lock_path = auth::default_config_path().with_extension("mail.lock");
    if let Some(parent) = lock_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|_| KagiError::Config("could not create the mail config directory".into()))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)
        .map_err(|_| KagiError::Config("could not open the mail token lock".into()))?;
    file.lock_exclusive()
        .map_err(|_| KagiError::Config("could not lock mail tokens".into()))?;
    Ok(file)
}

pub async fn access_token(
    client: &Client,
    profile: Option<&str>,
) -> Result<(Url, String), KagiError> {
    let mut config = MailConfig::load(profile)?;
    let mut endpoint = config.endpoint()?;
    if config.needs_refresh() {
        let _lock = token_lock()?;
        // Another process may have refreshed while this process waited.
        config = MailConfig::load(profile)?;
        endpoint = config.endpoint()?;
        if config.needs_refresh() {
            let refresh = config.refresh_token.as_deref().ok_or_else(login_required)?;
            let discovery = discover(client, endpoint.clone()).await?;
            // Bind saved refresh credentials to the issuer that granted them.
            // Resource metadata alone cannot authorize a different issuer.
            if config.issuer.as_deref() != Some(discovery.oauth.issuer.as_str()) {
                return Err(KagiError::MailAuth("mail OAuth issuer changed or is missing; run `kagi mail login` before refreshing credentials".into()));
            }
            let response = client
                .post(private_url(&discovery.oauth.token_endpoint)?)
                .form(&[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh),
                    ("client_id", config.client_id()?),
                    ("resource", &discovery.resource),
                ])
                .send()
                .await
                .map_err(transport_error)?;
            let (status, value) = oauth_response(response).await?;
            if status == reqwest::StatusCode::BAD_REQUEST {
                return Err(KagiError::Config(
                    "mail token refresh failed (HTTP 400); check your OAuth client configuration"
                        .into(),
                ));
            }
            apply_token(&mut config, value)?;
            auth::save_mail_config(profile, config.clone())?;
        }
    }
    let token = config
        .access_token
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(login_required)?;
    Ok((endpoint, token))
}

#[derive(Deserialize)]
struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    #[serde(default = "default_poll_interval")]
    interval: u64,
}

fn default_poll_interval() -> u64 {
    5
}

pub async fn login(profile: Option<&str>) -> Result<Value, KagiError> {
    let client = client()?;
    let mut config = MailConfig::load(profile)?;
    if env_value("KAGI_MAIL_ACCESS_TOKEN").is_some() {
        return Err(KagiError::Config("unset KAGI_MAIL_ACCESS_TOKEN before running `kagi mail login`; it overrides saved tokens".into()));
    }
    let client_id = config.client_id()?.to_owned();
    let discovery = discover(&client, config.endpoint()?).await?;
    let device_endpoint = discovery.oauth.device_authorization_endpoint
        .ok_or_else(|| KagiError::MailAuth("mail OAuth issuer does not support device login; supply KAGI_MAIL_ACCESS_TOKEN from an authorized OAuth client".into()))?;
    let response = client
        .post(private_url(&device_endpoint)?)
        .form(&[
            ("client_id", client_id.as_str()),
            ("scope", &discovery.scope),
            ("resource", &discovery.resource),
        ])
        .send()
        .await
        .map_err(transport_error)?;
    let (status, value) = oauth_response(response).await?;
    if !status.is_success() {
        return Err(KagiError::Config(format!(
            "mail device login request was rejected (HTTP {}); check your OAuth client configuration",
            status.as_u16()
        )));
    }
    let device: DeviceAuthorization = serde_json::from_value(value)
        .map_err(|_| KagiError::Parse("mail OAuth device response is incomplete".into()))?;
    let verification_url = private_url(
        device
            .verification_uri_complete
            .as_deref()
            .unwrap_or(&device.verification_uri),
    )?;
    eprintln!(
        "Open {verification_url}\nCode: {}\nWaiting for approval. Press Ctrl-C to cancel.",
        device
            .user_code
            .chars()
            .filter(|c| !c.is_control())
            .collect::<String>()
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(device.expires_in.min(600));
    let mut interval = device.interval.max(1);
    let token_url = private_url(&discovery.oauth.token_endpoint)?;
    loop {
        let poll = async {
            tokio::time::sleep(Duration::from_secs(interval)).await;
            client
                .post(token_url.clone())
                .form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", device.device_code.as_str()),
                    ("client_id", client_id.as_str()),
                ])
                .send()
                .await
                .map_err(transport_error)
        };
        let response = tokio::time::timeout_at(deadline, poll)
            .await
            .map_err(|_| {
                KagiError::MailAuth("mail login timed out; run `kagi mail login` again".into())
            })??;
        let (status, value) = oauth_response(response).await?;
        if status.is_success() {
            // A new login must not retain a previous account's refresh token.
            config.refresh_token = None;
            config.issuer = Some(discovery.oauth.issuer.clone());
            apply_token(&mut config, value)?;
            let _lock = token_lock()?;
            auth::save_mail_config(profile, config)?;
            return Ok(json!({"authenticated": true}));
        }
        match value.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => {}
            Some("slow_down") => interval = interval.saturating_add(5),
            Some("access_denied") => {
                return Err(KagiError::MailAuth(
                    "mail login was denied; run `kagi mail login` to try again".into(),
                ));
            }
            Some("expired_token") => {
                return Err(KagiError::MailAuth(
                    "mail login code expired; run `kagi mail login` again".into(),
                ));
            }
            _ => {
                return Err(KagiError::MailAuth(format!(
                    "mail login failed (HTTP {}); check your OAuth client configuration",
                    status.as_u16()
                )));
            }
        }
    }
}

pub fn logout(profile: Option<&str>) -> Result<Value, KagiError> {
    let _lock = token_lock()?;
    let mut config = auth::load_mail_config(profile)?;
    config.access_token = None;
    config.refresh_token = None;
    config.expires_at = None;
    auth::save_mail_config(profile, config)?;
    Ok(
        json!({"saved_tokens_removed": true, "environment_token_configured": env_value("KAGI_MAIL_ACCESS_TOKEN").is_some()}),
    )
}
