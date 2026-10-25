//! Explicit SMTP submission. Never loads MCP OAuth or persists credentials/mail.

use std::{env, fmt, time::Duration};

use clap::Args;
use lettre::{
    Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
    transport::smtp::{
        AsyncSmtpTransportBuilder,
        authentication::{Credentials, Mechanism},
    },
};
use serde_json::{Value, json};

use crate::error::KagiError;

const SMTP_HOST: &str = "mail.kagimail.com";
const SMTP_PORT: u16 = 465;
const SMTP_USERNAME: &str = "KAGI_MAIL_SMTP_USERNAME";
const SMTP_PASSWORD: &str = "KAGI_MAIL_SMTP_PASSWORD";

#[derive(Args)]
#[command(
    after_help = "Requires KAGI_MAIL_SMTP_USERNAME (full email address) and KAGI_MAIL_SMTP_PASSWORD (App Specific Password) in the environment.\nUses mail.kagimail.com:465 with certificate-verified implicit TLS (SSL).\nMCP OAuth tokens and saved profiles are never used for sending."
)]
pub struct MailSendArgs {
    /// Sender email address (one bare address, no display name)
    #[arg(long, value_name = "ADDRESS")]
    pub from: String,
    /// Recipient email address (one bare address, no display name)
    #[arg(long, value_name = "ADDRESS")]
    pub to: String,
    /// Subject text (no control characters)
    #[arg(long, value_name = "TEXT")]
    pub subject: String,
    /// Plain-text message body; quote multiline text in your shell
    #[arg(long, value_name = "TEXT", allow_hyphen_values = true)]
    pub body: String,
}

// Cli derives Debug; keep message content out of any diagnostic formatting.
impl fmt::Debug for MailSendArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailSendArgs").finish_non_exhaustive()
    }
}

impl MailSendArgs {
    fn message(self) -> Result<Message, KagiError> {
        let from = address(&self.from, "--from")?;
        let to = address(&self.to, "--to")?;
        if self.subject.chars().any(char::is_control) {
            return Err(KagiError::Config(
                "mail send --subject must not contain control characters".into(),
            ));
        }
        Message::builder()
            .from(Mailbox::new(None, from))
            .to(Mailbox::new(None, to))
            .subject(self.subject)
            .header(ContentType::TEXT_PLAIN)
            .body(self.body)
            .map_err(|_| KagiError::Config("mail send could not construct the message".into()))
    }
}

fn address(value: &str, flag: &str) -> Result<Address, KagiError> {
    if value.chars().any(char::is_control) {
        return Err(KagiError::Config(format!(
            "mail send {flag} requires one valid bare email address"
        )));
    }
    value.parse().map_err(|_| {
        KagiError::Config(format!(
            "mail send {flag} requires one valid bare email address"
        ))
    })
}

fn credentials(
    mut lookup: impl FnMut(&str) -> Result<String, env::VarError>,
) -> Result<Credentials, KagiError> {
    let mut read = |name: &str| {
        lookup(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                KagiError::Config(format!(
                    "missing credentials: set {name} to a nonempty UTF-8 value for SMTP; mail login tokens cannot be used"
                ))
            })
    };
    Ok(Credentials::new(read(SMTP_USERNAME)?, read(SMTP_PASSWORD)?))
}

fn transport_builder(
    host: &str,
    port: u16,
    credentials: Credentials,
) -> Result<AsyncSmtpTransportBuilder, KagiError> {
    // relay establishes implicit TLS before any SMTP commands and verifies the certificate
    // against host using Mozilla roots. No plaintext or insecure fallback.
    AsyncSmtpTransport::<Tokio1Executor>::relay(host)
        .map(|builder| {
            builder
                .port(port)
                .credentials(credentials)
                .authentication(vec![Mechanism::Plain, Mechanism::Login])
                .timeout(Some(Duration::from_secs(30)))
        })
        .map_err(|_| KagiError::MailSend("could not initialize verified implicit SMTP TLS".into()))
}

fn submission_error(error: lettre::transport::smtp::Error) -> KagiError {
    // SMTP responses and underlying errors can echo credentials or message text.
    // Inspect only the status code, never format or log the upstream error.
    if error
        .status()
        .is_some_and(|status| matches!(u16::from(status), 530 | 534 | 535))
    {
        KagiError::Auth(
            "SMTP authentication rejected; check KAGI_MAIL_SMTP_USERNAME and KAGI_MAIL_SMTP_PASSWORD; MCP OAuth tokens cannot be used".into(),
        )
    } else {
        KagiError::MailSend(
            "SMTP submission failed; delivery may be unknown. Check your mailbox before retrying to avoid duplicate mail".into(),
        )
    }
}

/// Validate and submit one message; never automatically retry an uncertain send.
pub async fn send(args: MailSendArgs) -> Result<Value, KagiError> {
    let message = args.message()?;
    let credentials = credentials(|name| env::var(name))?;
    let transport = transport_builder(SMTP_HOST, SMTP_PORT, credentials)?.build::<Tokio1Executor>();
    submit(message, &transport).await
}

/// Bound the entire SMTP submission, not only individual protocol commands.
async fn submit(
    message: Message,
    transport: &AsyncSmtpTransport<Tokio1Executor>,
) -> Result<Value, KagiError> {
    tokio::time::timeout(Duration::from_secs(90), transport.send(message))
        .await
        .map_err(|_| KagiError::MailSend("SMTP submission timed out; delivery may be unknown. Check your mailbox before retrying to avoid duplicate mail".into()))?
        .map_err(submission_error)?;
    Ok(json!({"status": "submitted"}))
}

/// Check verified TLS and SMTP authentication with NOOP, without MAIL/RCPT/DATA.
pub async fn check() -> Result<Value, KagiError> {
    let credentials = credentials(|name| env::var(name))?;
    let transport = transport_builder(SMTP_HOST, SMTP_PORT, credentials)?.build::<Tokio1Executor>();
    check_transport(&transport).await
}

/// Run the authentication-only probe with a bounded total deadline.
async fn check_transport(
    transport: &AsyncSmtpTransport<Tokio1Executor>,
) -> Result<Value, KagiError> {
    let connected = tokio::time::timeout(Duration::from_secs(90), transport.test_connection())
        .await
        .map_err(|_| {
            KagiError::MailSend("SMTP connection check timed out; no message was submitted".into())
        })?
        .map_err(|error| match submission_error(error) {
            auth @ KagiError::Auth(_) => auth,
            _ => {
                KagiError::MailSend("SMTP connection check failed; no message was submitted".into())
            }
        })?;
    if !connected {
        return Err(KagiError::MailSend(
            "SMTP connection check failed; no message was submitted".into(),
        ));
    }
    Ok(json!({"status": "authenticated", "message_submitted": false}))
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Instant,
    };

    use super::*;

    fn args() -> MailSendArgs {
        MailSendArgs {
            from: "sender@example.com".into(),
            to: "recipient@example.com".into(),
            subject: "A plain-text message".into(),
            body: "First line\n.\nBcc: not-a-header@example.com\nLast line".into(),
        }
    }

    #[test]
    fn serializes_plain_text_with_matching_envelope_and_normalized_newlines() {
        let message = args().message().unwrap();
        assert_eq!(
            message.envelope().from().unwrap().to_string(),
            "sender@example.com"
        );
        assert_eq!(
            message.envelope().to(),
            &["recipient@example.com".parse::<Address>().unwrap()]
        );
        let formatted = String::from_utf8(message.formatted()).unwrap();
        let (headers, body) = formatted.split_once("\r\n\r\n").unwrap();
        assert!(headers.contains("From: sender@example.com\r\n"));
        assert!(headers.contains("To: recipient@example.com\r\n"));
        assert!(headers.contains("Subject: A plain-text message\r\n"));
        assert!(headers.contains("Content-Type: text/plain; charset=utf-8"));
        assert!(!headers.contains("Bcc:"));
        assert!(!headers.contains("Cc:"));
        assert!(!headers.contains("Reply-To:"));
        assert_eq!(
            body,
            "First line\r\n.\r\nBcc: not-a-header@example.com\r\nLast line"
        );
    }

    #[test]
    fn rejects_invalid_addresses_and_header_injection_without_echoing_content() {
        for invalid in [
            "",
            "PRIVATE",
            "PRIVATE <sender@example.com>",
            "sender@example.com,PRIVATE@example.com",
            "sender@example.com\r\nBcc: PRIVATE@example.com",
        ] {
            for sender in [true, false] {
                let mut input = args();
                if sender {
                    input.from = invalid.into();
                } else {
                    input.to = invalid.into();
                }
                let error = input.message().unwrap_err().to_string();
                assert!(error.contains(if sender { "--from" } else { "--to" }));
                assert!(!error.contains("PRIVATE"));
            }
        }
        for subject in [
            "PRIVATE\r\nBcc: other@example.com",
            "PRIVATE\0",
            "PRIVATE\t",
        ] {
            let mut input = args();
            input.subject = subject.into();
            let error = input.message().unwrap_err().to_string();
            assert!(error.contains("--subject"));
            assert!(!error.contains("PRIVATE"));
        }
    }

    #[test]
    fn rejects_missing_empty_and_non_utf8_smtp_credentials() {
        for name in [SMTP_USERNAME, SMTP_PASSWORD] {
            for value in [None, Some(""), Some(" \t\n")] {
                let error = credentials(|key| {
                    if key == name {
                        value.map(str::to_owned).ok_or(env::VarError::NotPresent)
                    } else {
                        Ok("PRIVATE".into())
                    }
                })
                .unwrap_err()
                .to_string();
                assert!(error.contains(name));
                assert!(!error.contains("PRIVATE"));
            }
            let error = credentials(|key| {
                if key == name {
                    Err(env::VarError::NotUnicode("PRIVATE".into()))
                } else {
                    Ok("PRIVATE".into())
                }
            })
            .unwrap_err()
            .to_string();
            assert!(error.contains(name));
            assert!(!error.contains("PRIVATE"));
        }
    }

    #[test]
    fn preserves_whitespace_in_nonempty_passwords() {
        let credentials = credentials(|name| {
            Ok(if name == SMTP_USERNAME {
                "user".into()
            } else {
                " password ".into()
            })
        })
        .unwrap();
        assert_eq!(
            Mechanism::Plain.response(&credentials, None).unwrap(),
            "\0user\0 password "
        );
    }

    #[test]
    fn uses_kagi_implicit_tls_endpoint() {
        assert_eq!(SMTP_HOST, "mail.kagimail.com");
        assert_eq!(SMTP_PORT, 465);
    }

    #[tokio::test]
    async fn refuses_plaintext_server_before_smtp_commands() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "local SMTP connection timed out");
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("local SMTP accept failed: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            // Capture the initial client record before offering a plaintext greeting.
            let mut buffer = [0u8; 8192];
            let n = stream.read(&mut buffer).unwrap();
            assert!(n > 0);
            let _ = stream.write_all(b"220 localhost ESMTP\r\n");
            buffer[..n].to_vec()
        });
        let transport = transport_builder(
            "127.0.0.1",
            port,
            Credentials::new("PRIVATE-USER".into(), "PRIVATE-PASSWORD".into()),
        )
        .unwrap()
        .build::<Tokio1Executor>();
        let error = submission_error(transport.send(args().message().unwrap()).await.unwrap_err());
        let envelope = crate::error_envelope(&error);
        assert!(!envelope.retryable);
        assert!(!envelope.message.contains("PRIVATE"));
        let wire = server.join().unwrap();
        assert_eq!(wire[0], 0x16, "first record must be a TLS handshake");
        for forbidden in [
            "EHLO ",
            "STARTTLS",
            "AUTH ",
            "MAIL FROM",
            "RCPT TO",
            "PRIVATE-USER",
            "PRIVATE-PASSWORD",
        ] {
            assert!(
                !wire
                    .windows(forbidden.len())
                    .any(|w| w == forbidden.as_bytes())
            );
        }
    }
}

#[cfg(test)]
#[path = "mail-send-tls-tests.rs"]
mod tls_tests;
