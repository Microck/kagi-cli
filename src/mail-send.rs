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
const SMTP_PORT: u16 = 587;
const SMTP_USERNAME: &str = "KAGI_MAIL_SMTP_USERNAME";
const SMTP_PASSWORD: &str = "KAGI_MAIL_SMTP_PASSWORD";

#[derive(Args)]
#[command(
    after_help = "Requires KAGI_MAIL_SMTP_USERNAME and KAGI_MAIL_SMTP_PASSWORD in the environment.\nUses mail.kagimail.com:587 with required, certificate-verified STARTTLS.\nMCP OAuth tokens and saved profiles are never used for sending."
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
    // starttls_relay requires TLS before AUTH/MAIL and verifies the certificate
    // against host using Mozilla roots. No plaintext or insecure fallback.
    AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
        .map(|builder| {
            builder
                .port(port)
                .credentials(credentials)
                .authentication(vec![Mechanism::Plain, Mechanism::Login])
                .timeout(Some(Duration::from_secs(30)))
        })
        .map_err(|_| KagiError::MailSend("could not initialize verified SMTP STARTTLS".into()))
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

pub async fn send(args: MailSendArgs) -> Result<Value, KagiError> {
    // Validate the whole message before loading credentials or opening a socket.
    let message = args.message()?;
    let credentials = credentials(|name| env::var(name))?;
    let transport = transport_builder(SMTP_HOST, SMTP_PORT, credentials)?.build::<Tokio1Executor>();
    transport.send(message).await.map_err(submission_error)?;
    // Acceptance by SMTP is not proof of recipient delivery. Do not echo content.
    Ok(json!({"status": "submitted"}))
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, BufReader, Write},
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

    #[tokio::test]
    async fn refuses_missing_or_rejected_starttls_before_authentication_or_message() {
        for advertises_starttls in [false, true] {
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
                stream.write_all(b"220 localhost ESMTP\r\n").unwrap();
                let mut reader = BufReader::new(stream);
                let mut transcript = String::new();
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    transcript.push_str(&line);
                    if line.starts_with("EHLO ") {
                        let reply = if advertises_starttls {
                            "250-localhost\r\n250-STARTTLS\r\n250 AUTH PLAIN LOGIN XOAUTH2\r\n"
                        } else {
                            "250-localhost\r\n250 AUTH PLAIN LOGIN XOAUTH2\r\n"
                        };
                        reader.get_mut().write_all(reply.as_bytes()).unwrap();
                    } else if line == "STARTTLS\r\n" {
                        reader
                            .get_mut()
                            .write_all(b"454 PRIVATE-RESPONSE\r\n")
                            .unwrap();
                    } else if line == "QUIT\r\n" {
                        reader.get_mut().write_all(b"221 Bye\r\n").unwrap();
                        break;
                    } else {
                        panic!("sent sensitive SMTP command before TLS: {line:?}");
                    }
                }
                transcript
            });
            let transport = transport_builder(
                "127.0.0.1",
                port,
                Credentials::new("PRIVATE-USER".into(), "PRIVATE-PASSWORD".into()),
            )
            .unwrap()
            .build::<Tokio1Executor>();
            let smtp_error = transport.send(args().message().unwrap()).await.unwrap_err();
            let smtp_error_details = format!("{smtp_error:?}");
            let error = submission_error(smtp_error);
            let envelope = crate::error_envelope(&error);
            assert!(!envelope.retryable);
            assert!(!envelope.message.contains("PRIVATE"));
            let transcript = server.join().unwrap();
            assert!(
                transcript.starts_with("EHLO "),
                "SMTP transcript: {transcript:?}; raw submission error: {smtp_error_details}"
            );
            assert_eq!(transcript.contains("STARTTLS\r\n"), advertises_starttls);
            assert!(!transcript.contains("AUTH "));
            assert!(!transcript.contains("MAIL FROM"));
            assert!(!transcript.contains("RCPT TO"));
            assert!(!transcript.contains("DATA"));
        }
    }
}
