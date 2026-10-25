//! Local-only SMTP protocol tests with ephemeral certificates and fake credentials.
//! Test trust is supplied only to the client fixture; production verification is unchanged.
use super::*;
use lettre::transport::smtp::client::{Certificate, Tls, TlsParameters};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::{TlsAcceptor, rustls};

#[derive(Clone, Copy)]
enum Outcome {
    Accept,
    RejectAuth,
    RejectRecipient,
    RejectData,
    DropAfterData,
}

struct Transcript {
    cleartext: Vec<String>,
    encrypted: Vec<String>,
    body: String,
}

/// Serve exactly one local SMTP connection with verified STARTTLS available.
async fn server(listener: TcpListener, acceptor: TlsAcceptor, outcome: Outcome) -> Transcript {
    let (stream, _) = listener.accept().await.unwrap();
    let mut clear = BufReader::new(stream);
    clear
        .get_mut()
        .write_all(b"220 localhost ESMTP\r\n")
        .await
        .unwrap();
    let mut transcript = Transcript {
        cleartext: vec![],
        encrypted: vec![],
        body: String::new(),
    };
    let mut line = String::new();
    clear.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("EHLO "));
    transcript.cleartext.push(line.clone());
    clear
        .get_mut()
        .write_all(b"250-localhost\r\n250-STARTTLS\r\n250 AUTH PLAIN LOGIN\r\n")
        .await
        .unwrap();
    line.clear();
    clear.read_line(&mut line).await.unwrap();
    assert_eq!(line, "STARTTLS\r\n");
    transcript.cleartext.push(line);
    clear
        .get_mut()
        .write_all(b"220 Begin TLS\r\n")
        .await
        .unwrap();
    let Ok(stream) = acceptor.accept(clear.into_inner()).await else {
        return transcript;
    };
    let mut tls = BufReader::new(stream);
    loop {
        let mut line = String::new();
        match tls.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        transcript.encrypted.push(line.clone());
        let reply: &[u8] = if line.starts_with("EHLO ") {
            b"250-localhost\r\n250 AUTH PLAIN LOGIN\r\n"
        } else if line.starts_with("AUTH PLAIN ") {
            if matches!(outcome, Outcome::RejectAuth) {
                b"535 PRIVATE-RESPONSE\r\n"
            } else {
                b"235 Authentication successful\r\n"
            }
        } else if line.starts_with("MAIL FROM:") {
            b"250 Sender accepted\r\n"
        } else if line.starts_with("RCPT TO:") {
            if matches!(outcome, Outcome::RejectRecipient) {
                b"550 PRIVATE-RESPONSE\r\n"
            } else {
                b"250 Recipient accepted\r\n"
            }
        } else if line == "DATA\r\n" {
            tls.get_mut()
                .write_all(b"354 End with a dot\r\n")
                .await
                .unwrap();
            loop {
                let mut data = String::new();
                let n = tls.read_line(&mut data).await.unwrap();
                assert_ne!(n, 0);
                if data == ".\r\n" {
                    break;
                }
                transcript.body.push_str(&data);
            }
            if matches!(outcome, Outcome::DropAfterData) {
                break;
            }
            if matches!(outcome, Outcome::RejectData) {
                b"451 PRIVATE-RESPONSE\r\n"
            } else {
                b"250 Accepted\r\n"
            }
        } else if line == "NOOP\r\n" {
            b"250 OK\r\n"
        } else if line == "QUIT\r\n" {
            let _ = tls.get_mut().write_all(b"221 Bye\r\n").await;
            break;
        } else {
            panic!("unexpected command in fake SMTP fixture");
        };
        if tls.get_mut().write_all(reply).await.is_err() {
            break;
        }
    }
    transcript
}

/// Start an isolated test listener and configure the client to trust only its test certificate.
async fn fixture(
    outcome: Outcome,
    trust_certificate: bool,
    certificate_name: &str,
) -> (
    AsyncSmtpTransport<Tokio1Executor>,
    tokio::task::JoinHandle<Transcript>,
) {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec![certificate_name.into()]).unwrap();
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
        )
        .unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let handle = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(10), server(listener, acceptor, outcome))
            .await
            .expect("SMTP fixture timed out")
    });
    let mut builder = transport_builder(
        "127.0.0.1",
        port,
        Credentials::new("fake-user".into(), "fake-password".into()),
    )
    .unwrap();
    if trust_certificate {
        let tls = TlsParameters::builder("localhost".into())
            .add_root_certificate(Certificate::from_der(cert.der().as_ref().to_vec()).unwrap())
            .build()
            .unwrap();
        builder = builder.tls(Tls::Required(tls));
    }
    (builder.build::<Tokio1Executor>(), handle)
}

fn message() -> Message {
    MailSendArgs {
        from: "sender@example.com".into(),
        to: "recipient@example.com".into(),
        subject: "SMTP test".into(),
        body: "First\n.\nLast".into(),
    }
    .message()
    .unwrap()
}

/// Verify successful TLS, authenticated envelope submission and dot stuffing on the wire.
#[tokio::test]
async fn smtp_tls_submission_accepts_only_after_authenticated_data() {
    let (transport, handle) = fixture(Outcome::Accept, true, "localhost").await;
    assert_eq!(
        submit(message(), &transport).await.unwrap(),
        json!({"status":"submitted"})
    );
    let trace = handle.await.unwrap();
    assert_eq!(trace.cleartext.len(), 2);
    assert!(trace.encrypted.iter().any(|l| l.starts_with("AUTH PLAIN ")));
    assert!(
        trace
            .encrypted
            .iter()
            .any(|l| l.starts_with("MAIL FROM:<sender@example.com>"))
    );
    assert!(
        trace
            .encrypted
            .iter()
            .any(|l| l.starts_with("RCPT TO:<recipient@example.com>"))
    );
    assert!(trace.body.contains("\r\n..\r\n"));
    assert!(
        trace
            .body
            .contains("Content-Type: text/plain; charset=utf-8")
    );
}

/// The diagnostic command authenticates over TLS but never sends an envelope or body.
#[tokio::test]
async fn smtp_tls_check_never_submits_mail() {
    let (transport, handle) = fixture(Outcome::Accept, true, "localhost").await;
    assert_eq!(
        check_transport(&transport).await.unwrap(),
        json!({"status":"authenticated","message_submitted":false})
    );
    let trace = handle.await.unwrap();
    assert!(trace.encrypted.iter().any(|l| l.starts_with("AUTH PLAIN ")));
    assert!(trace.encrypted.iter().any(|l| l == "NOOP\r\n"));
    assert!(
        !trace
            .encrypted
            .iter()
            .any(|l| l.starts_with("MAIL ") || l.starts_with("RCPT ") || l.starts_with("DATA"))
    );
    assert!(trace.body.is_empty());
}

/// Neither untrusted certificates nor mismatched names may reach SMTP authentication.
#[tokio::test]
async fn smtp_tls_rejects_untrusted_and_wrong_host_certificates() {
    for (trust, name) in [(false, "localhost"), (true, "wrong.example")] {
        let (transport, handle) = fixture(Outcome::Accept, trust, name).await;
        assert!(submit(message(), &transport).await.is_err());
        let trace = handle.await.unwrap();
        assert_eq!(trace.cleartext.len(), 2);
        assert!(trace.encrypted.is_empty());
        assert!(trace.body.is_empty());
    }
}

/// Rejections and a lost final acknowledgement stay errors without leaking server text or retrying.
#[tokio::test]
async fn smtp_tls_failures_are_redacted_and_never_retried() {
    for outcome in [
        Outcome::RejectAuth,
        Outcome::RejectRecipient,
        Outcome::RejectData,
        Outcome::DropAfterData,
    ] {
        let (transport, handle) = fixture(outcome, true, "localhost").await;
        let error = submit(message(), &transport).await.unwrap_err();
        let envelope = crate::error_envelope(&error);
        assert!(!envelope.retryable);
        assert!(!error.to_string().contains("PRIVATE"));
        assert!(!error.to_string().contains("fake-password"));
        let trace = handle.await.unwrap();
        assert_eq!(
            trace
                .encrypted
                .iter()
                .filter(|l| l.starts_with("AUTH PLAIN "))
                .count(),
            1
        );
        if matches!(outcome, Outcome::RejectAuth) {
            assert!(matches!(error, KagiError::Auth(_)));
            assert!(!trace.encrypted.iter().any(|l| l.starts_with("MAIL ")));
        }
        if matches!(outcome, Outcome::RejectRecipient) {
            assert!(trace.body.is_empty());
        }
        if matches!(outcome, Outcome::DropAfterData) {
            assert!(error.to_string().contains("delivery may be unknown"));
        }
    }
}
