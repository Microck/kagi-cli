//! Mail commands and the small Streamable HTTP MCP client they share.

use clap::{Args, Subcommand, ValueEnum};
use reqwest::{Client, Response, Url, header::HeaderValue};
use serde_json::{Value, json};

use crate::{error::KagiError, mail_auth};

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  kagi mail login\n  kagi mail boxes\n  kagi mail search --mailbox Inbox --unread\n  kagi mail search \"contract renewal\" --semantic\n  kagi mail read MESSAGE_ID --format pretty"
)]
pub struct MailCommand {
    #[command(subcommand)]
    pub command: MailSubcommand,
    /// Output format
    #[arg(long, global = true, value_enum, default_value = "json")]
    pub format: MailFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MailFormat {
    Json,
    Compact,
    Toon,
    Pretty,
}

#[derive(Debug, Subcommand)]
pub enum MailSubcommand {
    /// Sign in using a browser verification code and save OAuth tokens
    Login,
    /// Show mail credential configuration without contacting the service
    Status,
    /// Remove saved mail tokens (does not revoke access or clear environment variables)
    Logout,
    /// List mailbox paths and total/unread message counts
    #[command(visible_alias = "mailboxes")]
    Boxes,
    /// Search literal text and filters, or use --semantic to search by meaning
    Search(MailSearchArgs),
    /// Read one message or every message in a thread
    Read(MailReadArgs),
}

#[derive(Debug, Args)]
pub struct MailSearchArgs {
    /// Text to match; omit to list recent mail using the selected filters
    #[arg(value_name = "QUERY")]
    pub query: Option<String>,
    /// Search by meaning instead of literal text (requires QUERY)
    #[arg(long, requires = "query")]
    pub semantic: bool,
    /// Match sender address or display name
    #[arg(long)]
    pub from: Option<String>,
    /// Match recipient address or display name
    #[arg(long)]
    pub to: Option<String>,
    /// Match a cc recipient
    #[arg(long)]
    pub cc: Option<String>,
    /// Match words in the subject
    #[arg(long)]
    pub subject: Option<String>,
    /// Full mailbox path from `kagi mail boxes`
    #[arg(long)]
    pub mailbox: Option<String>,
    /// Received at or after this date (YYYY-MM-DD or RFC 3339)
    #[arg(long, value_name = "DATE")]
    pub after: Option<String>,
    /// Received before this date (YYYY-MM-DD or RFC 3339)
    #[arg(long, value_name = "DATE")]
    pub before: Option<String>,
    /// Only unread messages
    #[arg(long)]
    pub unread: bool,
    /// Only messages with attachments
    #[arg(long)]
    pub has_attachment: bool,
    /// Only messages at least this many bytes in size
    #[arg(long, value_name = "BYTES")]
    pub min_size: Option<u64>,
    /// Maximum messages to return (1-50; the service has no pagination)
    #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u8).range(1..=50))]
    pub limit: u8,
}

impl MailSearchArgs {
    fn tool_call(self) -> Result<(&'static str, Value), KagiError> {
        if self
            .query
            .as_ref()
            .is_some_and(|query| query.trim().is_empty())
        {
            return Err(KagiError::Config(
                "mail search query is empty; omit it to list recent mail, or enter search text"
                    .into(),
            ));
        }
        let mut arguments = serde_json::Map::new();
        for (key, value) in [
            (if self.semantic { "query" } else { "text" }, self.query),
            ("from", self.from),
            ("to", self.to),
            ("cc", self.cc),
            ("subject", self.subject),
            ("mailbox", self.mailbox),
            ("after", self.after),
            ("before", self.before),
        ] {
            if let Some(value) = value {
                arguments.insert(key.into(), value.into());
            }
        }
        arguments.insert("limit".into(), self.limit.into());
        if self.unread {
            arguments.insert("unreadOnly".into(), true.into());
        }
        if self.has_attachment {
            arguments.insert("hasAttachment".into(), true.into());
        }
        if let Some(size) = self.min_size {
            arguments.insert("minSizeBytes".into(), size.into());
        }
        Ok((
            if self.semantic {
                "semantic_search"
            } else {
                "search_email"
            },
            arguments.into(),
        ))
    }
}

#[derive(Debug, Args)]
pub struct MailReadArgs {
    /// Message ID returned by a search
    #[arg(
        value_name = "MESSAGE_ID",
        required_unless_present = "thread",
        conflicts_with = "thread"
    )]
    pub message_id: Option<String>,
    /// Read all messages in this thread, oldest first
    #[arg(long, value_name = "THREAD_ID")]
    pub thread: Option<String>,
    /// Drop quoted reply history and keep only newly written text
    #[arg(long)]
    pub new_text_only: bool,
}

pub async fn run(args: MailCommand, profile: Option<&str>) -> Result<(), KagiError> {
    let value = match args.command {
        MailSubcommand::Login => mail_auth::login(profile).await?,
        MailSubcommand::Status => mail_auth::MailConfig::load(profile)?.status(),
        MailSubcommand::Logout => mail_auth::logout(profile)?,
        command => {
            let (tool, arguments) = match command {
                MailSubcommand::Boxes => ("list_mailboxes", json!({})),
                MailSubcommand::Search(search) => search.tool_call()?,
                MailSubcommand::Read(read) => {
                    let (key, id) = match (read.message_id, read.thread) {
                        (Some(id), None) => ("emailId", id),
                        (None, Some(id)) => ("threadId", id),
                        _ => unreachable!("clap requires exactly one message or thread ID"),
                    };
                    if id.trim().is_empty() {
                        return Err(KagiError::Config("mail read requires a nonempty message or thread ID from a search result".into()));
                    }
                    (
                        "get_email",
                        json!({key: id, "newTextOnly": read.new_text_only}),
                    )
                }
                _ => unreachable!("local mail commands were handled above"),
            };
            let client = mail_auth::client()?;
            let (endpoint, token) = mail_auth::access_token(&client, profile).await?;
            let mut rpc = MailRpc {
                client,
                endpoint,
                token,
                session: None,
                protocol: None,
            };
            let response = rpc.call(tool, arguments).await;
            rpc.close().await;
            response?
        }
    };
    match args.format {
        MailFormat::Json => crate::print_json(&value),
        MailFormat::Compact => crate::print_compact_json(&value),
        MailFormat::Toon => crate::print_toon(&value),
        MailFormat::Pretty => {
            println!("{}", format_pretty(&value));
            Ok(())
        }
    }
}

struct MailRpc {
    client: Client,
    endpoint: Url,
    token: String,
    session: Option<HeaderValue>,
    protocol: Option<String>,
}

impl MailRpc {
    fn request(&self, method: reqwest::Method) -> reqwest::RequestBuilder {
        let mut request = self
            .client
            .request(method, self.endpoint.clone())
            .bearer_auth(&self.token)
            .header("Accept", "application/json, text/event-stream");
        if let Some(session) = &self.session {
            request = request.header("Mcp-Session-Id", session);
        }
        if let Some(protocol) = &self.protocol {
            request = request.header("MCP-Protocol-Version", protocol);
        }
        request
    }

    async fn post(&mut self, body: Value) -> Result<Response, KagiError> {
        let response = self
            .request(reqwest::Method::POST)
            .json(&body)
            .send()
            .await
            .map_err(mail_auth::transport_error)?;
        mail_auth::check_status(&response)?;
        if self.session.is_none() {
            self.session = response
                .headers()
                .get("Mcp-Session-Id")
                .cloned()
                .map(|mut value| {
                    value.set_sensitive(true);
                    value
                });
        }
        Ok(response)
    }

    async fn call(&mut self, tool: &str, arguments: Value) -> Result<Value, KagiError> {
        let response = self
            .post(json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"protocolVersion": "2025-11-25", "capabilities": {},
                    "clientInfo": {"name": "kagi-cli", "version": env!("CARGO_PKG_VERSION")}}
            }))
            .await?;
        let initialized = rpc_response(response, 1).await?;
        let protocol = initialized
            .get("protocolVersion")
            .and_then(Value::as_str)
            .filter(|version| matches!(*version, "2025-03-26" | "2025-06-18" | "2025-11-25"))
            .ok_or_else(|| {
                KagiError::Parse("mail service selected an unsupported MCP protocol version".into())
            })?;
        self.protocol = Some(protocol.into());
        self.post(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await?;
        let response = self
            .post(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": tool, "arguments": arguments}}))
            .await?;
        tool_result(rpc_response(response, 2).await?, tool)
    }

    async fn close(&self) {
        if self.session.is_some() {
            // End only this MCP session, never mailbox state. A server may not
            // implement DELETE; cleanup must not discard a successful read.
            let _ = self
                .request(reqwest::Method::DELETE)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await;
        }
    }
}

fn parse_error(message: &str) -> KagiError {
    KagiError::Parse(message.into())
}

fn rpc_result(mut value: Value, id: u64) -> Result<Value, KagiError> {
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || value.get("id") != Some(&json!(id))
    {
        return Err(parse_error(
            "mail service returned an unexpected MCP response",
        ));
    }
    if let Some(error) = value.get("error") {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        return Err(parse_error(&format!(
            "mail MCP request failed (code {code}); check the service configuration and supported tools"
        )));
    }
    value
        .get_mut("result")
        .map(Value::take)
        .ok_or_else(|| parse_error("mail MCP response has no result"))
}

/// Read JSON or SSE until the matching response arrives. SSE connections may
/// remain open after a result, so waiting for the entire HTTP body would hang.
async fn rpc_response(mut response: Response, id: u64) -> Result<Value, KagiError> {
    let sse = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|s| s.split(';').next() == Some("text/event-stream"));
    let mut buffer = Vec::new();
    let mut event = String::new();
    let mut received = 0usize;
    while let Some(chunk) = response.chunk().await.map_err(mail_auth::transport_error)? {
        received += chunk.len();
        if received > 16 * 1024 * 1024 {
            return Err(parse_error("mail MCP response exceeded 16 MiB"));
        }
        buffer.extend_from_slice(&chunk);
        if !sse {
            continue;
        }
        let mut consumed = 0;
        for line in buffer.split_inclusive(|byte| *byte == b'\n') {
            if !line.ends_with(b"\n") {
                break;
            }
            consumed += line.len();
            let line = std::str::from_utf8(line)
                .map_err(|_| parse_error("mail MCP stream is not UTF-8"))?
                .trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if !event.is_empty() {
                    let value: Value = serde_json::from_str(&event)
                        .map_err(|_| parse_error("mail MCP stream contains invalid JSON"))?;
                    event.clear();
                    if value.get("id") == Some(&json!(id)) {
                        return rpc_result(value, id);
                    }
                }
            } else if let Some(text) = line.strip_prefix("data:") {
                if !event.is_empty() {
                    event.push('\n');
                }
                event.push_str(text.strip_prefix(' ').unwrap_or(text));
            }
        }
        buffer.drain(..consumed);
    }
    if sse {
        return Err(parse_error(
            "mail MCP stream ended before returning a result",
        ));
    }
    let value = serde_json::from_slice(&buffer)
        .map_err(|_| parse_error("mail MCP response is not valid JSON"))?;
    rpc_result(value, id)
}

fn tool_result(mut value: Value, tool: &str) -> Result<Value, KagiError> {
    if value.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(parse_error(
            "mail tool failed; check your search filters or message ID and retry",
        ));
    }
    // MCP supports structured content and JSON text content. Preserve the
    // service's result in either representation, without the protocol wrapper.
    let payload = if let Some(content) = value.get_mut("structuredContent") {
        content.take()
    } else {
        let text = value
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        serde_json::from_str(&text)
            .map_err(|_| parse_error("mail tool did not return a JSON result"))?
    };
    let key = if tool == "list_mailboxes" {
        "mailboxes"
    } else {
        "emails"
    };
    if !payload
        .get(key)
        .is_some_and(|v| v.is_null() || v.is_array())
    {
        return Err(parse_error("mail tool response is missing its result list"));
    }
    Ok(payload)
}

/// Keep IDs and server notes visible so terminal output supports the next step.
fn format_pretty(value: &Value) -> String {
    let mut lines = Vec::new();
    if value.get("mailboxes").is_some() {
        lines.push("UNREAD\tTOTAL\tMAILBOX".into());
        for mailbox in value["mailboxes"].as_array().into_iter().flatten() {
            lines.push(format!(
                "{}\t{}\t{}",
                mailbox["unreadEmails"],
                mailbox["totalEmails"],
                text(mailbox, "name")
            ));
        }
    } else if value.get("emails").is_some() {
        for email in value["emails"].as_array().into_iter().flatten() {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!(
                "{}{}",
                if email["unread"] == true { "* " } else { "" },
                text(email, "subject")
            ));
            for (label, key) in [("ID", "id"), ("Thread", "threadId"), ("Date", "receivedAt")] {
                if email.get(key).is_some() {
                    lines.push(format!("{label}: {}", text(email, key)));
                }
            }
            for (label, key) in [("From", "from"), ("To", "to"), ("Cc", "cc")] {
                if let Some(addresses) = email[key].as_array() {
                    lines.push(format!(
                        "{label}: {}",
                        addresses
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
            let body = ["body", "snippet", "preview"]
                .iter()
                .find_map(|key| email[key].as_str())
                .unwrap_or_default();
            if !body.is_empty() {
                lines.push(body.into());
            }
            if email["truncated"] == true {
                lines.push("[Message truncated by the service]".into());
            }
            for attachment in email["attachments"].as_array().into_iter().flatten() {
                lines.push(format!(
                    "Attachment: {} ({} bytes)",
                    text(attachment, "name"),
                    attachment["sizeBytes"]
                ));
            }
        }
        if lines.is_empty() {
            lines.push("No messages found.".into());
        }
    } else if let Some(fields) = value.as_object() {
        for (key, value) in fields {
            lines.push(format!("{}: {value}", key.replace('_', " ")));
        }
    }
    if let Some(note) = value["note"].as_str() {
        lines.push(format!("Note: {note}"));
    }
    // Mail is untrusted terminal input. Retain line breaks and tabs, but never
    // let control bytes in a subject/body issue terminal commands.
    lines
        .join("\n")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}
