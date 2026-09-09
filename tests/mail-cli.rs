//! End-to-end mail contract tests against a local HTTP service. The service
//! implements OAuth device/refresh grants and the MCP handshake using fixtures;
//! no live credentials, network services, or mocking framework are involved.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;

#[derive(Clone, Copy, Default)]
enum Mode {
    #[default]
    Json,
    Sse,
    Text,
    Unauthorized,
    RpcError,
    ToolError,
    WrongId,
    BadPayload,
    Denied,
    Expired,
    NoExpiry,
    DeviceError(u16, &'static str),
    TokenError(u16, &'static str),
}

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: String,
}

struct MailService {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl MailService {
    fn start(mode: Mode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (thread_url, thread_requests, thread_stop) =
            (url.clone(), requests.clone(), stop.clone());
        let worker = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        let request = read_request(&stream);
                        let reply = serve(&thread_url, &request, mode);
                        thread_requests.lock().unwrap().push(request);
                        if reply.content_type == "text/event-stream" {
                            write!(stream, "HTTP/1.1 {}\r\nContent-Type: text/event-stream\r\nConnection: close\r\n{}Transfer-Encoding: chunked\r\n\r\n", reply.status, reply.headers).unwrap();
                            for chunk in reply.body.as_bytes().chunks(7) {
                                write!(stream, "{:x}\r\n", chunk.len()).unwrap();
                                stream.write_all(chunk).unwrap();
                                stream.write_all(b"\r\n").unwrap();
                            }
                            stream.flush().unwrap();
                            // Leave the HTTP stream open. The CLI must return the
                            // matching RPC result without waiting for end-of-body.
                            let mut byte = [0];
                            assert_eq!(stream.read(&mut byte).unwrap(), 0);
                        } else {
                            write!(stream, "HTTP/1.1 {}\r\nContent-Type: {}\r\nConnection: close\r\n{}Content-Length: {}\r\n\r\n{}",
                                reply.status, reply.content_type, reply.headers, reply.body.len(), reply.body).unwrap();
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("fixture listener failed: {error}"),
                }
            }
        });
        Self {
            url,
            requests,
            stop,
            worker: Some(worker),
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/mcp", self.url)
    }
    fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("KAGI_MAIL_ENDPOINT", self.endpoint()),
            ("KAGI_MAIL_ACCESS_TOKEN", "fixture-access".into()),
        ]
    }
    fn calls(&self) -> Vec<Value> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter_map(|r| serde_json::from_str::<Value>(&r.body).ok())
            .filter(|v| v["method"] == "tools/call")
            .collect()
    }
}

impl Drop for MailService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn read_request(stream: &TcpStream) -> Request {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap().into();
    let path = parts.next().unwrap().into();
    let mut headers = BTreeMap::new();
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        let (key, value) = line.split_once(':').unwrap();
        headers.insert(key.to_ascii_lowercase(), value.trim().into());
    }
    let length = headers
        .get("content-length")
        .map(|s: &String| s.parse().unwrap())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    Request {
        method,
        path,
        headers,
        body: String::from_utf8(body).unwrap(),
    }
}

struct Reply {
    status: u16,
    content_type: &'static str,
    headers: String,
    body: String,
}
fn reply(status: u16, value: Value) -> Reply {
    Reply {
        status,
        content_type: "application/json",
        headers: String::new(),
        body: value.to_string(),
    }
}

fn serve(url: &str, request: &Request, mode: Mode) -> Reply {
    assert_eq!(
        request.headers["user-agent"],
        "OpenAI File Downloader, XaiImageApiFetch/1.0"
    );
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/mcp") => {
            assert!(!request.headers.contains_key("authorization"));
            let mut response = reply(401, json!({}));
            response.headers =
                format!("WWW-Authenticate: Bearer resource_metadata=\"{url}/resource\"\r\n");
            response
        }
        ("GET", "/resource") => reply(
            200,
            json!({"resource": format!("{url}/mcp"), "authorization_servers": [url], "scopes_supported": ["openid"]}),
        ),
        ("GET", "/.well-known/oauth-authorization-server") => reply(404, json!({})),
        ("GET", "/.well-known/openid-configuration") => reply(
            200,
            json!({"issuer": url, "token_endpoint": format!("{url}/token"), "device_authorization_endpoint": format!("{url}/device")}),
        ),
        ("POST", "/device") => {
            let form = form(&request.body);
            assert_eq!(form["client_id"], "fixture-client");
            assert_eq!(form["scope"], "openid offline_access");
            if let Mode::DeviceError(status, error) = mode {
                return reply(
                    status,
                    json!({"error": error, "error_description": "PRIVATE"}),
                );
            }
            reply(
                200,
                json!({"device_code": "fixture-device", "user_code": "\u{0000}AB\tCD\r\n", "verification_uri": format!("{url}/verify"), "interval": 1, "expires_in": if matches!(mode, Mode::Expired) { 0 } else { 30 }}),
            )
        }
        ("POST", "/token") => {
            let form = form(&request.body);
            assert_eq!(form["client_id"], "fixture-client");
            if matches!(mode, Mode::Denied) {
                return reply(
                    400,
                    json!({"error": "access_denied", "error_description": "PRIVATE"}),
                );
            }
            let refresh = form["grant_type"] == "refresh_token";
            if refresh {
                assert_eq!(form["refresh_token"], "fixture-refresh");
                assert_eq!(form["resource"], format!("{url}/mcp"));
            } else {
                assert_eq!(
                    form["grant_type"],
                    "urn:ietf:params:oauth:grant-type:device_code"
                );
                assert_eq!(form["device_code"], "fixture-device");
            }
            if let Mode::TokenError(status, error) = mode {
                return reply(
                    status,
                    json!({"error": error, "error_description": "PRIVATE"}),
                );
            }
            let mut token = json!({
                "access_token": "fixture-access",
                "refresh_token": if refresh { "rotated-refresh" } else { "fixture-refresh" },
                "token_type": "Bearer",
            });
            if !matches!(mode, Mode::NoExpiry) {
                token["expires_in"] = json!(3600);
            }
            reply(200, token)
        }
        ("DELETE", "/mcp") => reply(200, json!({})),
        ("POST", "/mcp") => {
            if matches!(mode, Mode::Unauthorized) {
                return reply(401, json!({"detail": "PRIVATE"}));
            }
            assert_eq!(request.headers["authorization"], "Bearer fixture-access");
            let rpc: Value = serde_json::from_str(&request.body).unwrap();
            assert_eq!(rpc["jsonrpc"], "2.0");
            let result = match rpc["method"].as_str().unwrap() {
                "initialize" => {
                    assert_eq!(rpc["params"]["clientInfo"]["name"], "kagi-cli");
                    json!({"protocolVersion": "2025-11-25", "capabilities": {"tools": {}}, "serverInfo": {"name": "mail-fixture", "version": "1"}})
                }
                "notifications/initialized" => {
                    assert_eq!(request.headers["mcp-session-id"], "fixture-session");
                    assert_eq!(request.headers["mcp-protocol-version"], "2025-11-25");
                    return reply(202, json!({}));
                }
                "tools/call" => {
                    assert_eq!(request.headers["mcp-session-id"], "fixture-session");
                    assert_eq!(request.headers["mcp-protocol-version"], "2025-11-25");
                    if matches!(mode, Mode::RpcError) {
                        return reply(
                            200,
                            json!({"jsonrpc": "2.0", "id": 2, "error": {"code": -32602, "message": "PRIVATE"}}),
                        );
                    }
                    if matches!(mode, Mode::ToolError) {
                        return reply(
                            200,
                            json!({"jsonrpc": "2.0", "id": 2, "result": {"isError": true, "content": [{"type": "text", "text": "PRIVATE"}]}}),
                        );
                    }
                    let payload = match rpc["params"]["name"].as_str().unwrap() {
                        "list_mailboxes" => {
                            json!({"mailboxes": [{"name": "Inbox", "role": "inbox", "totalEmails": 4, "unreadEmails": 2}]})
                        }
                        "search_email" | "semantic_search" => {
                            json!({"emails": [{"id": "m1", "threadId": "t1", "subject": "Invoice", "from": ["sender@example.com"], "preview": "Example message", "unread": true}], "note": "fixture note"})
                        }
                        "get_email" => {
                            json!({"emails": [{"id": "m1", "threadId": "t1", "subject": "Invoice\u{001b}[31m", "body": "Hello world, Résumé", "truncated": true, "attachments": [{"name": "invoice.pdf", "sizeBytes": 42}]}]})
                        }
                        name => panic!("unexpected mail tool {name}"),
                    };
                    match mode {
                        Mode::Text => {
                            json!({"content": [{"type": "text", "text": payload.to_string()}]})
                        }
                        Mode::BadPayload => json!({"structuredContent": {"emails": "invalid"}}),
                        _ => json!({"structuredContent": payload}),
                    }
                }
                method => panic!("unexpected MCP method {method}"),
            };
            let id = if matches!(mode, Mode::WrongId) {
                json!(999)
            } else {
                rpc["id"].clone()
            };
            let envelope = json!({"jsonrpc": "2.0", "id": id, "result": result});
            let mut response = reply(200, envelope.clone());
            response.headers = "Mcp-Session-Id: fixture-session\r\n".into();
            if matches!(mode, Mode::Sse) {
                response.content_type = "text/event-stream";
                response.body = format!(
                    ": keepalive\r\n\r\ndata: {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}}\r\n\r\nevent: message\r\ndata: {envelope}\r\n\r\n"
                );
            }
            response
        }
        route => panic!("unexpected service route {route:?}"),
    }
}

fn form(body: &str) -> BTreeMap<String, String> {
    reqwest::Url::parse(&format!("http://localhost/?{body}"))
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect()
}

fn run(args: &[&str], directory: &Path, env: &[(&str, String)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kagi"));
    command.args(args).current_dir(directory);
    for key in [
        "KAGI_MAIL_ENDPOINT",
        "KAGI_MAIL_CLIENT_ID",
        "KAGI_MAIL_ACCESS_TOKEN",
        "KAGI_API_KEY",
        "KAGI_API_TOKEN",
        "KAGI_SESSION_TOKEN",
        "KAGI_ERROR_FORMAT",
        "RUST_LOG",
    ] {
        command.env_remove(key);
    }
    command.env("KAGI_CONFIG", directory.join("config.toml"));
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().unwrap()
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn maps_all_search_filters_and_semantics() {
    let service = MailService::start(Mode::Json);
    let dir = TempDir::new().unwrap();
    let value = success(run(
        &[
            "mail",
            "search",
            "invoice",
            "--from",
            "a@example.com",
            "--to",
            "b@example.com",
            "--cc",
            "c@example.com",
            "--subject",
            "bill",
            "--mailbox",
            "Projects/2026",
            "--after",
            "2026-01-01",
            "--before",
            "2026-02-01",
            "--unread",
            "--has-attachment",
            "--min-size",
            "1000",
            "--limit",
            "5",
        ],
        dir.path(),
        &service.env(),
    ));
    assert_eq!(value["emails"][0]["id"], "m1");
    assert_eq!(value["note"], "fixture note");
    let calls = service.calls();
    assert_eq!(
        calls[0]["params"],
        json!({"name":"search_email", "arguments": {"text":"invoice", "from":"a@example.com", "to":"b@example.com", "cc":"c@example.com", "subject":"bill", "mailbox":"Projects/2026", "after":"2026-01-01", "before":"2026-02-01", "unreadOnly":true, "hasAttachment":true, "minSizeBytes":1000, "limit":5}})
    );
    success(run(
        &[
            "mail",
            "search",
            "renewal discussion",
            "--semantic",
            "--from",
            "a@example.com",
        ],
        dir.path(),
        &service.env(),
    ));
    assert_eq!(
        service.calls()[1]["params"],
        json!({"name":"semantic_search", "arguments":{"query":"renewal discussion", "from":"a@example.com", "limit":20}})
    );
    success(run(&["mail", "search"], dir.path(), &service.env()));
    assert_eq!(
        service.calls()[2]["params"]["arguments"],
        json!({"limit":20})
    );
}

#[test]
fn reads_messages_threads_and_boxes_with_both_mcp_representations() {
    for mode in [Mode::Json, Mode::Sse, Mode::Text] {
        let service = MailService::start(mode);
        let dir = TempDir::new().unwrap();
        let boxes = success(run(
            &["mail", "--format", "compact", "boxes"],
            dir.path(),
            &service.env(),
        ));
        assert_eq!(boxes["mailboxes"][0]["unreadEmails"], 2);
        success(run(&["mail", "read", "m1"], dir.path(), &service.env()));
        assert_eq!(
            service.calls()[1]["params"]["arguments"],
            json!({"emailId":"m1", "newTextOnly":false})
        );
        success(run(
            &["mail", "read", "--thread", "t1", "--new-text-only"],
            dir.path(),
            &service.env(),
        ));
        assert_eq!(
            service.calls()[2]["params"]["arguments"],
            json!({"threadId":"t1", "newTextOnly":true})
        );
    }
}

#[test]
fn validates_arguments_before_authentication() {
    let dir = TempDir::new().unwrap();
    for args in [
        vec!["mail"],
        vec!["mail", "search", "--semantic"],
        vec!["mail", "search", "--limit", "0"],
        vec!["mail", "search", "--limit", "51"],
        vec!["mail", "read"],
        vec!["mail", "read", "m1", "--thread", "t1"],
    ] {
        let output = run(&args, dir.path(), &[]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    let help = run(&["mail", "--help"], dir.path(), &[]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--semantic"));
}

#[test]
fn errors_are_redacted_and_machine_readable() {
    let dir = TempDir::new().unwrap();
    for mode in [
        Mode::Unauthorized,
        Mode::RpcError,
        Mode::ToolError,
        Mode::WrongId,
        Mode::BadPayload,
    ] {
        let service = MailService::start(mode);
        let output = run(
            &["mail", "search", "--error-format", "json"],
            dir.path(),
            &service.env(),
        );
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(!stderr.contains("PRIVATE"));
        assert!(!stderr.contains(&service.url));
        assert!(!stderr.contains("fixture-access"));
        let error: Value = serde_json::from_str(&stderr).unwrap();
        if matches!(mode, Mode::Unauthorized) {
            assert_eq!(error["required_auth"], "KAGI_MAIL_ACCESS_TOKEN");
            assert!(
                error["suggested_commands"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("kagi mail login"))
            );
        }
    }
}

#[test]
fn pretty_output_keeps_ids_notes_and_truncation_without_terminal_escapes() {
    let service = MailService::start(Mode::Json);
    let dir = TempDir::new().unwrap();
    let output = run(
        &["mail", "read", "m1", "--format", "pretty"],
        dir.path(),
        &service.env(),
    );
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "ID: m1",
        "Thread: t1",
        "Hello world",
        "[Message truncated",
        "invoice.pdf",
    ] {
        assert!(text.contains(expected));
    }
    assert!(!text.contains('\u{001b}'));
    let output = run(
        &["mail", "search", "--format", "pretty"],
        dir.path(),
        &service.env(),
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Note: fixture note"));
}

fn write_config(directory: &Path, service: &MailService, mail_tokens: &str) {
    std::fs::write(
        directory.join("config.toml"),
        format!(
            r#"
[auth]
api_key = 'search-key'
[mail]
endpoint = '{base}/mcp'
client_id = 'fixture-client'
issuer = '{base}'
{mail_tokens}
[profiles.work.auth]
api_token = 'other-search-token'
[profiles.work.mail]
endpoint = '{base}/mcp'
client_id = 'fixture-client'
access_token = 'work-token'
"#,
            base = service.url
        ),
    )
    .unwrap();
}

#[test]
fn refresh_rotates_tokens_and_preserves_other_credentials() {
    let service = MailService::start(Mode::Json);
    let dir = TempDir::new().unwrap();
    write_config(
        dir.path(),
        &service,
        "access_token = 'expired'\nrefresh_token = 'fixture-refresh'\nexpires_at = 1",
    );
    success(run(&["mail", "boxes"], dir.path(), &[]));
    let saved: toml::Value =
        toml::from_str(&std::fs::read_to_string(dir.path().join("config.toml")).unwrap()).unwrap();
    assert_eq!(
        saved["mail"]["refresh_token"].as_str(),
        Some("rotated-refresh")
    );
    assert_eq!(saved["auth"]["api_key"].as_str(), Some("search-key"));
    assert_eq!(
        saved["profiles"]["work"]["mail"]["access_token"].as_str(),
        Some("work-token")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.path().join("config.toml"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    success(run(&["mail", "boxes"], dir.path(), &[]));
    assert_eq!(
        service
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.path == "/token")
            .count(),
        1
    );
    // Saving a search key must not erase the newly stored mail credentials.
    assert!(
        run(
            &["auth", "set", "--api-key", "replacement-key"],
            dir.path(),
            &[]
        )
        .status
        .success()
    );
    let saved = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(saved.contains("rotated-refresh"));
}

#[test]
fn login_saves_tokens_and_logout_preserves_connection_and_profiles() {
    let service = MailService::start(Mode::Json);
    let dir = TempDir::new().unwrap();
    write_config(dir.path(), &service, "");
    let output = run(&["mail", "login"], dir.path(), &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({"authenticated":true})
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Code: ABCD\nWaiting for approval."));
    assert!(!stderr.chars().any(|c| c.is_control() && c != '\n'));
    success(run(&["mail", "boxes"], dir.path(), &[]));
    let status = success(run(&["mail", "status"], dir.path(), &[]));
    assert_eq!(status["access_token_configured"], true);
    assert_eq!(status["expired"], false);
    success(run(&["mail", "logout"], dir.path(), &[]));
    let saved: toml::Value =
        toml::from_str(&std::fs::read_to_string(dir.path().join("config.toml")).unwrap()).unwrap();
    assert!(saved["mail"].get("access_token").is_none());
    assert_eq!(
        saved["mail"]["endpoint"].as_str(),
        Some(service.endpoint().as_str())
    );
    assert_eq!(
        saved["profiles"]["work"]["mail"]["access_token"].as_str(),
        Some("work-token")
    );
}

#[test]
fn failed_login_keeps_saved_credentials() {
    for (mode, category, retryable) in [
        (Mode::Denied, "auth", false),
        (Mode::Expired, "auth", false),
        (Mode::TokenError(400, "invalid_grant"), "auth", false),
        (
            Mode::TokenError(429, "temporarily_unavailable"),
            "network",
            true,
        ),
        (Mode::TokenError(500, "server_error"), "network", true),
        (
            Mode::TokenError(400, "invalid_client"),
            "configuration",
            false,
        ),
        (
            Mode::TokenError(400, "unauthorized_client"),
            "configuration",
            false,
        ),
        (
            Mode::TokenError(401, "invalid_client"),
            "configuration",
            false,
        ),
        (
            Mode::DeviceError(400, "invalid_client"),
            "configuration",
            false,
        ),
        (
            Mode::DeviceError(401, "invalid_client"),
            "configuration",
            false,
        ),
        (
            Mode::TokenError(400, "temporarily_unavailable"),
            "network",
            true,
        ),
    ] {
        let service = MailService::start(mode);
        let dir = TempDir::new().unwrap();
        write_config(dir.path(), &service, "access_token = 'keep-me'");
        let before = std::fs::read(dir.path().join("config.toml")).unwrap();
        let output = run(
            &["mail", "login", "--error-format", "json"],
            dir.path(),
            &[],
        );
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE"));
        // Device instructions precede the final structured error on stderr.
        let stderr = String::from_utf8(output.stderr).unwrap();
        let error: Value = serde_json::from_str(stderr.lines().last().unwrap()).unwrap();
        assert_eq!(error["category"], category);
        assert_eq!(error["retryable"], retryable);
        if category == "auth" {
            assert_eq!(error["required_auth"], "KAGI_MAIL_ACCESS_TOKEN");
            assert_eq!(
                error["suggested_commands"],
                json!(["kagi mail status", "kagi mail login"])
            );
            assert!(
                error["message"]
                    .as_str()
                    .unwrap()
                    .contains("run `kagi mail login`")
            );
        } else if let Mode::TokenError(status, _) | Mode::DeviceError(status, _) = mode {
            assert_eq!(error["http_status"], status);
            assert_eq!(error["suggested_commands"], json!([]));
            if category == "configuration" {
                assert!(
                    error["message"]
                        .as_str()
                        .unwrap()
                        .contains("KAGI_MAIL_CLIENT_ID")
                );
            }
        }
        assert_eq!(
            std::fs::read(dir.path().join("config.toml")).unwrap(),
            before
        );
    }
}

#[test]
fn profile_and_environment_overrides_do_not_reuse_another_accounts_tokens() {
    let service = MailService::start(Mode::Json);
    let dir = TempDir::new().unwrap();
    write_config(dir.path(), &service, "access_token = 'default-token'");
    let status = success(run(
        &["mail", "status", "--profile", "missing"],
        dir.path(),
        &[],
    ));
    assert_eq!(status["access_token_configured"], false);
    assert_eq!(status["endpoint_configured"], false);
    let status = success(run(
        &["mail", "status"],
        dir.path(),
        &[("KAGI_MAIL_ENDPOINT", "https://different.example/mcp".into())],
    ));
    assert_eq!(status["access_token_configured"], false);
    success(run(
        &["mail", "boxes", "--profile", "work"],
        dir.path(),
        &[("KAGI_MAIL_ACCESS_TOKEN", "fixture-access".into())],
    ));
    let output = run(&["mail", "status"], dir.path(), &[]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("default-token"));
    assert!(!text.contains("fixture-client"));
    assert!(!text.contains(&service.url));
}

#[test]
fn refresh_rejects_a_different_issuer_without_sending_tokens() {
    let service = MailService::start(Mode::Json);
    let dir = TempDir::new().unwrap();
    write_config(
        dir.path(),
        &service,
        "access_token = 'expired'\nrefresh_token = 'fixture-refresh'\nexpires_at = 1",
    );
    let path = dir.path().join("config.toml");
    let config = std::fs::read_to_string(&path).unwrap().replace(
        &format!("issuer = '{}'", service.url),
        "issuer = 'https://other.example'",
    );
    std::fs::write(&path, &config).unwrap();
    let output = run(
        &["mail", "boxes", "--error-format", "json"],
        dir.path(),
        &[],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("issuer changed"));
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["required_auth"], "KAGI_MAIL_ACCESS_TOKEN");
    assert_eq!(
        error["suggested_commands"],
        json!(["kagi mail status", "kagi mail login"])
    );
    assert!(
        !service
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.path == "/token")
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), config);
}

#[test]
fn refresh_errors_preserve_tokens_and_distinguish_retryable_failures() {
    for (status, oauth_error, category, retryable) in [
        (429, "temporarily_unavailable", "network", true),
        (500, "server_error", "network", true),
        (400, "temporarily_unavailable", "network", true),
        (400, "invalid_grant", "auth", false),
        (400, "invalid_client", "configuration", false),
        (400, "unauthorized_client", "configuration", false),
        (401, "invalid_client", "configuration", false),
        (400, "invalid_request", "configuration", false),
    ] {
        let service = MailService::start(Mode::TokenError(status, oauth_error));
        let dir = TempDir::new().unwrap();
        write_config(
            dir.path(),
            &service,
            "access_token = 'expired'\nrefresh_token = 'fixture-refresh'\nexpires_at = 1",
        );
        let path = dir.path().join("config.toml");
        let before = std::fs::read(&path).unwrap();
        let output = run(
            &["mail", "boxes", "--error-format", "json"],
            dir.path(),
            &[],
        );
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(!stderr.contains("PRIVATE"));
        assert!(!stderr.contains(&service.url));
        let error: Value = serde_json::from_str(&stderr).unwrap();
        assert_eq!(error["category"], category);
        assert_eq!(error["retryable"], retryable);
        if matches!(oauth_error, "invalid_client" | "unauthorized_client") {
            assert!(
                error["message"]
                    .as_str()
                    .unwrap()
                    .contains("KAGI_MAIL_CLIENT_ID")
            );
        }
        if category == "network" {
            assert_eq!(error["http_status"], status);
            assert_eq!(error["suggested_commands"], json!([]));
        }
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn login_without_expiry_refreshes_before_reading_mail() {
    let service = MailService::start(Mode::NoExpiry);
    let dir = TempDir::new().unwrap();
    write_config(dir.path(), &service, "");
    let login = run(&["mail", "login"], dir.path(), &[]);
    assert!(
        login.status.success(),
        "{}",
        String::from_utf8_lossy(&login.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&login.stdout).unwrap(),
        json!({"authenticated": true})
    );
    let saved: toml::Value =
        toml::from_str(&std::fs::read_to_string(dir.path().join("config.toml")).unwrap()).unwrap();
    assert!(saved["mail"].get("expires_at").is_none());
    success(run(&["mail", "boxes"], dir.path(), &[]));
    let grants: Vec<_> = service
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| request.path == "/token")
        .map(|request| form(&request.body)["grant_type"].clone())
        .collect();
    assert_eq!(
        grants,
        [
            "urn:ietf:params:oauth:grant-type:device_code",
            "refresh_token"
        ]
    );
}

#[test]
fn malformed_config_and_insecure_urls_do_not_echo_private_values() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        "[mail]\naccess_token = PRIVATE-SECRET",
    )
    .unwrap();
    let output = run(&["mail", "status"], dir.path(), &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE-SECRET"));
    std::fs::write(dir.path().join("config.toml"), "").unwrap();
    for url in [
        "http://private.example/mcp",
        "https://user:PRIVATE@private.example/mcp",
        "https://private.example/mcp#PRIVATE",
    ] {
        let output = run(
            &["mail", "boxes"],
            dir.path(),
            &[
                ("KAGI_MAIL_ENDPOINT", url.into()),
                ("KAGI_MAIL_ACCESS_TOKEN", "PRIVATE".into()),
            ],
        );
        assert_eq!(output.status.code(), Some(1));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private.example"));
    }
}
