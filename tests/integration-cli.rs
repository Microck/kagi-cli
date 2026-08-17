use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use httpmock::Method::{DELETE, GET, PATCH, POST};
use httpmock::MockServer;
use serde_json::{Value, json};
use tempfile::TempDir;

const API_KEY: &str = "test-api-key";
const API_TOKEN: &str = "test-api-token";

fn run_kagi(args: &[&str], envs: &[(&str, &str)], cwd: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kagi"));
    command.args(args).current_dir(cwd);

    for key in [
        "KAGI_API_KEY",
        "KAGI_API_TOKEN",
        "KAGI_SESSION_TOKEN",
        "KAGI_BASE_URL",
        "KAGI_ASSISTANT_BASE_URL",
        "KAGI_NEWS_BASE_URL",
        "KAGI_TRANSLATE_BASE_URL",
        "KAGI_ERROR_FORMAT",
        "KAGI_CACHE_DIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "SHELL",
    ] {
        command.env_remove(key);
    }

    isolate_command_home(&mut command, cwd);

    for (key, value) in envs {
        command.env(key, value);
    }

    command.output().expect("command should run")
}

fn run_kagi_with_stdin(args: &[&str], stdin: &str, envs: &[(&str, &str)], cwd: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kagi"));
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for key in [
        "KAGI_API_KEY",
        "KAGI_API_TOKEN",
        "KAGI_SESSION_TOKEN",
        "KAGI_BASE_URL",
        "KAGI_ASSISTANT_BASE_URL",
        "KAGI_NEWS_BASE_URL",
        "KAGI_TRANSLATE_BASE_URL",
        "KAGI_ERROR_FORMAT",
        "KAGI_CACHE_DIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "SHELL",
    ] {
        command.env_remove(key);
    }

    isolate_command_home(&mut command, cwd);

    for (key, value) in envs {
        command.env(key, value);
    }

    let mut child = command.spawn().expect("command should spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(stdin.as_bytes())
        .expect("stdin should write");
    child.wait_with_output().expect("command should run")
}

fn isolate_command_home(command: &mut Command, cwd: &Path) {
    command
        .env("HOME", cwd)
        .env("XDG_CONFIG_HOME", cwd.join(".config"))
        .env("XDG_DATA_HOME", cwd.join(".local").join("share"));
}

/// Path to the kagi config file for a sandboxed run, matching the isolated
/// `XDG_CONFIG_HOME` set by `isolate_command_home`.
fn config_path(cwd: &Path) -> std::path::PathBuf {
    cwd.join(".config").join("kagi-cli").join("config.toml")
}

/// Seeds the kagi config file for a sandboxed run, creating parent directories.
fn write_config(cwd: &Path, contents: &str) {
    let path = config_path(cwd);
    fs::create_dir_all(path.parent().expect("config path has a parent"))
        .expect("config directory should create");
    fs::write(path, contents).expect("config should write");
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success, got status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn help_points_agents_to_agent_guide() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(&["--help"], &[], tempdir.path());

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Agent usage:"),
        "expected agent help section, got:\n{stdout}"
    );
    assert!(
        stdout.contains("kagi skills get kagi-usage"),
        "expected help to mention kagi skills get kagi-usage, got:\n{stdout}"
    );
    assert!(
        stdout.contains("skills [list]"),
        "expected help to mention skills [list], got:\n{stdout}"
    );
}

#[test]
fn default_failure_stderr_has_single_user_facing_error() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(&["search", "rust"], &[], tempdir.path());

    assert!(
        !output.status.success(),
        "expected search without auth to fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing credentials"),
        "expected missing credentials error, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("kagi exited with error"),
        "expected no tracing duplicate in default stderr, got:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("missing credentials").count(),
        1,
        "expected one user-facing missing credentials error, got:\n{stderr}"
    );
}

#[test]
fn json_error_format_prints_structured_stderr() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(
        &["--error-format", "json", "search", "rust"],
        &[],
        tempdir.path(),
    );

    assert!(
        !output.status.success(),
        "expected search without auth to fail"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let envelope: Value =
        serde_json::from_slice(&output.stderr).expect("stderr should be a JSON error envelope");
    assert_eq!(envelope["code"], "missing_credentials");
    assert_eq!(envelope["category"], "configuration");
    assert_eq!(envelope["retryable"], false);
    assert_eq!(
        envelope["required_auth"],
        "KAGI_API_KEY or KAGI_SESSION_TOKEN"
    );
    assert!(
        envelope["suggested_commands"]
            .as_array()
            .expect("suggested_commands should be an array")
            .iter()
            .any(|command| command == "kagi auth status")
    );

    let env_tempdir = TempDir::new().expect("tempdir");
    let env_output = run_kagi(
        &["search", "rust"],
        &[("KAGI_ERROR_FORMAT", "json")],
        env_tempdir.path(),
    );
    assert!(
        !env_output.status.success(),
        "expected search without auth to fail"
    );
    let env_envelope: Value = serde_json::from_slice(&env_output.stderr)
        .expect("env-selected stderr should be a JSON error envelope");
    assert_eq!(env_envelope["code"], "missing_credentials");
}

#[test]
fn agent_command_prints_embedded_skill_guide_without_auth() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(&["agent"], &[], tempdir.path());

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("# Kagi CLI"),
        "expected markdown skill guide, got:\n{stdout}"
    );
    assert!(
        stdout.contains("kagi auth status"),
        "expected auth discovery guidance, got:\n{stdout}"
    );
    assert!(
        stdout.contains("--format toon"),
        "expected structured output guidance, got:\n{stdout}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).is_empty(),
        "expected no stderr, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn skills_get_prints_core_guide_without_auth() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(&["skills", "get", "kagi-usage"], &[], tempdir.path());

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("# Kagi CLI"),
        "expected markdown skill guide, got:\n{stdout}"
    );
    assert!(
        stdout.contains("kagi skills get <name>"),
        "expected skills command guidance, got:\n{stdout}"
    );
}

#[test]
fn skills_list_and_path_are_auth_free() {
    let tempdir = TempDir::new().expect("tempdir");

    let list = run_kagi(&["skills"], &[], tempdir.path());
    assert_success(&list);
    let stdout = String::from_utf8_lossy(&list.stdout);
    for skill in [
        "kagi-usage",
        "kagi-ai",
        "kagi-assistant",
        "kagi-monitoring",
        "kagi-account-config",
    ] {
        assert!(
            stdout.contains(skill),
            "expected {skill} in skill listing, got:\n{stdout}"
        );
    }

    let path = run_kagi(&["skills", "path"], &[], tempdir.path());
    assert_success(&path);
    assert_eq!(
        String::from_utf8_lossy(&path.stdout).trim(),
        "embedded://skills"
    );

    let skill_path = run_kagi(&["skills", "path", "kagi-usage"], &[], tempdir.path());
    assert_success(&skill_path);
    assert_eq!(
        String::from_utf8_lossy(&skill_path.stdout).trim(),
        "embedded://skills/kagi-usage"
    );
}

#[test]
fn skills_get_full_prints_body_without_frontmatter() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(
        &["skills", "get", "kagi-usage", "--full"],
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("# Kagi CLI"),
        "expected skill body without frontmatter, got:\n{stdout}"
    );
    assert!(
        !stdout.starts_with("---"),
        "expected frontmatter to be stripped, got:\n{stdout}"
    );
}

fn test_env(server: &MockServer) -> Vec<(&'static str, String)> {
    vec![
        ("KAGI_API_KEY", API_KEY.to_string()),
        ("KAGI_API_TOKEN", API_TOKEN.to_string()),
        ("KAGI_BASE_URL", server.base_url()),
        ("KAGI_NEWS_BASE_URL", server.base_url()),
    ]
}

fn env_refs(values: &[(impl AsRef<str>, impl AsRef<str>)]) -> Vec<(&str, &str)> {
    values
        .iter()
        .map(|(key, value)| (key.as_ref(), value.as_ref()))
        .collect()
}

fn mcp_request(id: Value, method: &str, params: Value) -> Value {
    mcp_request_with_version(id, method, params, "2026-07-28")
}

fn mcp_request_with_version(id: Value, method: &str, params: Value, version: &str) -> Value {
    let mut params = params
        .as_object()
        .expect("MCP test params should be an object")
        .clone();
    params.insert(
        "_meta".to_string(),
        json!({
            "io.modelcontextprotocol/protocolVersion": version,
            "io.modelcontextprotocol/clientInfo": {
                "name": "kagi-cli-integration-tests",
                "version": "1.0.0"
            },
            "io.modelcontextprotocol/clientCapabilities": {}
        }),
    );

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": Value::Object(params),
    })
}

fn session_env(server: &MockServer) -> Vec<(&'static str, String)> {
    vec![
        ("KAGI_SESSION_TOKEN", "test-session".to_string()),
        ("KAGI_BASE_URL", server.base_url()),
        ("KAGI_ASSISTANT_BASE_URL", server.base_url()),
    ]
}

#[test]
fn assistant_prompt_stream_reads_query_from_stdin() {
    let server = MockServer::start();
    let current_prompt = mock_current_assistant_prompt(
        &server,
        "do a little dance",
        Some("dance"),
        "dance-ok",
        None,
    );
    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi_with_stdin(
        &["assistant", "--stream"],
        "do a little dance\n",
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    current_prompt.assert_calls(1);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "dance-ok\n");
}

fn api_meta() -> Value {
    json!({
        "id": "req-1",
        "node": "test",
        "ms": 12
    })
}

fn assistant_init_json(custom_assistants: Value) -> Value {
    json!({
        "models": {
            "models": [{
                "id": "gpt-5-mini",
                "display_name": "GPT 5 Mini"
            }],
            "default": "gpt-5-mini",
            "assistants": [{
                "id": "assistant:code",
                "name": "Code",
                "underlying_model": "gpt-5-mini",
                "internet_access": true
            }]
        },
        "custom_assistants": custom_assistants
    })
}

fn custom_assistant_json(id: &str, name: &str, model: &str) -> Value {
    json!({
        "uuid": id,
        "name": name,
        "llm_id": model,
        "instructions": "",
        "internet_access": true,
        "personalizations": false,
        "lens_id": null,
        "bang_trigger": null
    })
}

fn mock_current_assistant_prompt<'a>(
    server: &'a MockServer,
    expected_message: &str,
    first_markdown: Option<&str>,
    final_markdown: &str,
    expected_profile_id: Option<&str>,
) -> httpmock::Mock<'a> {
    server.mock(|when, then| {
        when.method(POST)
            .path("/api/conversations")
            .header("cookie", "kagi_session=test-session")
            .header("accept", "application/json");
        then.status(200).json_body(json!({
            "conversation": {
                "uuid": "thread-current",
                "title": "New chat",
                "created_at": "2026-07-25T00:00:00Z",
                "updated_at": "2026-07-25T00:00:00Z",
                "is_saved": false,
                "is_shared": false,
                "folder_uuid": null
            },
            "default_branch": {
                "uuid": "branch-current",
                "is_default": true
            }
        }));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/api/conversations/thread-current/init")
            .header("cookie", "kagi_session=test-session")
            .header("accept", "application/json");
        then.status(200).json_body(json!({
            "conversation": {
                "uuid": "thread-current",
                "title": "Current Assistant",
                "created_at": "2026-07-25T00:00:00Z",
                "updated_at": "2026-07-25T00:00:00Z",
                "is_saved": false,
                "is_shared": false,
                "folder_uuid": null
            },
            "active_branch": {
                "uuid": "branch-current",
                "is_default": true
            },
            "branches": [{
                "uuid": "branch-current",
                "is_default": true
            }],
            "messages": {
                "items": [],
                "has_more": false
            }
        }));
    });
    let expected_profile_fragment = expected_profile_id
        .map(|profile_id| format!("\"profile_uuid\":\"{profile_id}\""))
        .unwrap_or_default();
    let message = server.mock(|when, then| {
        when.method(POST)
            .path("/api/branches/branch-current/messages")
            .header("cookie", "kagi_session=test-session")
            .header("content-type", "application/json")
            .body_includes(expected_message)
            .body_includes(&expected_profile_fragment);
        then.status(200).json_body(json!({
            "conversation": {
                "uuid": "thread-current",
                "title": "New chat",
                "created_at": "2026-07-25T00:00:00Z",
                "updated_at": "2026-07-25T00:00:00Z",
                "is_saved": false,
                "is_shared": false,
                "folder_uuid": null
            },
            "branch": {
                "uuid": "branch-current",
                "is_default": true
            },
            "user_message": {
                "uuid": "user-current",
                "role": "user",
                "content": expected_message,
                "created_at": "2026-07-25T00:00:00Z",
                "references": [],
                "attachments": []
            }
        }));
    });
    let mut frames = String::new();
    if let Some(first_markdown) = first_markdown {
        frames.push_str(&format!(
            "data: {}\n\n",
            json!({"text": first_markdown, "is_final": false})
        ));
    }
    frames.push_str(&format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({
            "text": final_markdown,
            "html_content": format!("<p>{final_markdown}</p>"),
            "conversation_title": "Current Assistant",
            "assistant_message_uuid": "assistant-current",
            "usage": {
                "input_tokens": 4314,
                "output_tokens": 2,
                "total_tokens": 4316,
                "cost_usd": 0.006192
            },
            "is_final": true
        })
    ));
    server.mock(move |when, then| {
        when.method(GET)
            .path("/api/branches/branch-current/stream")
            .header("cookie", "kagi_session=test-session")
            .header("accept", "text/event-stream");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(frames);
    });
    message
}

fn search_payload(title: &str, url: &str, snippet: &str) -> Value {
    json!({
        "meta": api_meta(),
        "data": {
            "search": [
                {
                    "url": url,
                    "title": title,
                    "snippet": snippet
                }
            ]
        }
    })
}

fn search_html_fixture() -> &'static str {
    r#"
    <html><body>
      <div class="search-result">
        <a class="__sri_title_link" href="https://example.com/session">Session Result</a>
        <div class="__sri-desc">Served by session fallback.</div>
      </div>
    </body></html>
    "#
}

fn lens_settings_html_fixture() -> &'static str {
    r#"
    <form class="__lens_item" action="/lenses/move" method="POST">
      <input type="hidden" name="lens_id" value="22524">
      <input type="hidden" name="active_index" value="2">
      <a class="lens_title" href="/settings/update_lens?id=22524"><div>Rust Docs</div></a>
      <div class="lens_edit_lens">
        <a aria-label="Edit lens" href="/settings/update_lens?id=22524">Edit</a>
      </div>
    </form>
    "#
}

fn news_latest_batch() -> Value {
    json!({
        "createdAt": "2026-04-06T00:00:00Z",
        "dateSlug": "2026-04-06",
        "id": "batch-1",
        "languageCode": "en",
        "processingTime": 14,
        "totalArticles": 120,
        "totalCategories": 8,
        "totalClusters": 64,
        "totalReadCount": 90
    })
}

fn news_category_metadata() -> Value {
    json!({
        "categories": [
            {
                "categoryId": "tech",
                "categoryType": "topic",
                "displayName": "Tech",
                "isCore": true,
                "sourceLanguage": "en"
            }
        ]
    })
}

fn news_batch_categories() -> Value {
    json!({
        "batchId": "batch-1",
        "createdAt": "2026-04-06T00:00:00Z",
        "hasOnThisDay": false,
        "categories": [
            {
                "id": "category-1",
                "categoryId": "tech",
                "categoryName": "Tech",
                "sourceLanguage": "en",
                "timestamp": 1712361600,
                "readCount": 42,
                "clusterCount": 3
            }
        ]
    })
}

fn news_stories() -> Value {
    json!({
        "batchId": "batch-1",
        "categoryId": "tech",
        "categoryName": "Tech",
        "timestamp": 1712361600,
        "stories": [
            {
                "title": "Rust ships new release",
                "url": "https://example.com/rust-release"
            }
        ],
        "totalStories": 1,
        "domains": [],
        "readCount": 10
    })
}

#[test]
fn search_command_returns_json_from_mock_api() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust programming" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust Programming Language",
                "https://www.rust-lang.org",
                "Reliable systems programming.",
            ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &["search", "rust programming", "--format", "json"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["data"][0]["title"], "Rust Programming Language");
}

#[test]
fn search_command_surfaces_related_searches_from_mock_api() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust programming" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "meta": api_meta(),
                "data": {
                    "search": [
                        {
                            "url": "https://www.rust-lang.org",
                            "title": "Rust Programming Language",
                            "snippet": "Reliable systems programming."
                        }
                    ],
                    "related_searches": [
                        {
                            "query": "rust ownership",
                            "rank": 1,
                            "source": "api"
                        }
                    ]
                }
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &["search", "rust programming", "--format", "json"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["related_searches"][0]["query"], "rust ownership");
    assert_eq!(body["related_searches"][0]["source"], "api");
}

#[test]
fn search_command_sends_v1_filters_with_api_key() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({
                "query": "rust programming",
                "filters": {
                    "region": "us",
                    "after": "2026-01-01",
                    "before": "2026-02-01"
                }
            }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust Programming Language",
                "https://www.rust-lang.org",
                "Reliable systems programming.",
            ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &[
            "search",
            "rust programming",
            "--region",
            "us",
            "--from-date",
            "2026-01-01",
            "--to-date",
            "2026-02-01",
            "--format",
            "json",
        ],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["data"][0]["title"], "Rust Programming Language");
}

#[test]
fn search_command_returns_toon_from_mock_api() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust programming" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust Programming Language",
                "https://www.rust-lang.org",
                "Reliable systems programming.",
            ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &["search", "rust programming", "--format", "toon"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("data"));
    assert!(stdout.contains("Rust Programming Language"));
    assert!(stdout.contains("https://www.rust-lang.org"));
}

#[test]
fn search_command_pretty_format_prints_ranked_results() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust programming" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust Book",
                "https://doc.rust-lang.org/book/",
                "Learn Rust with the official book.",
            ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &[
            "search",
            "rust programming",
            "--format",
            "pretty",
            "--no-color",
        ],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1. Rust Book"));
    assert!(stdout.contains("https://doc.rust-lang.org/book/"));
    assert!(stdout.contains("Learn Rust with the official book."));
}

#[test]
fn search_command_limit_truncates_results() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust", "limit": 2 }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "meta": api_meta(),
                "data": {
                    "search": [
                        { "url": "https://example.com/a", "title": "A", "snippet": "first" },
                        { "url": "https://example.com/b", "title": "B", "snippet": "second" },
                        { "url": "https://example.com/c", "title": "C", "snippet": "third" }
                    ]
                }
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &["search", "rust", "--limit", "2", "--format", "json"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    let data = body["data"].as_array().expect("data should be an array");
    assert_eq!(data.len(), 2);
    assert_eq!(data[0]["title"], "A");
    assert_eq!(data[1]["title"], "B");
}

#[test]
fn search_command_falls_back_to_session_when_api_is_rate_limited() {
    let server = MockServer::start();
    let api_search = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust programming" }))
            .header("authorization", "Bearer test-api-key");
        then.status(429)
            .header("content-type", "application/json")
            .json_body(json!({
                "error": [{ "msg": "rate limit exceeded" }]
            }));
    });
    let session_search = server.mock(|when, then| {
        when.method(GET)
            .path("/html/search")
            .query_param("q", "rust programming")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .header("content-type", "text/html")
            .body(search_html_fixture());
    });

    let tempdir = TempDir::new().expect("tempdir");
    write_config(tempdir.path(), "[auth]\npreferred_auth = \"api\"\n");
    let env = vec![
        ("KAGI_API_KEY", API_KEY.to_string()),
        ("KAGI_SESSION_TOKEN", "test-session".to_string()),
        ("KAGI_BASE_URL", server.base_url()),
    ];
    let output = run_kagi(
        &["search", "rust programming", "--format", "json"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    api_search.assert_calls(1);
    session_search.assert_calls(1);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["data"][0]["title"], "Session Result");
}

#[test]
fn search_lens_name_resolves_to_current_position() {
    let server = MockServer::start();
    let lenses = server.mock(|when, then| {
        when.method(GET)
            .path("/html/settings/lenses")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .header("content-type", "text/html")
            .body(lens_settings_html_fixture());
    });
    let search = server.mock(|when, then| {
        when.method(GET)
            .path("/html/search")
            .query_param("q", "rust ownership")
            .query_param("l", "2")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .header("content-type", "text/html")
            .body(search_html_fixture());
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(
        &[
            "search",
            "rust ownership",
            "--lens",
            "Rust Docs",
            "--format",
            "json",
        ],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    lenses.assert_calls(1);
    search.assert_calls(1);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["data"][0]["title"], "Session Result");
}

#[test]
fn batch_command_returns_queries_and_results() {
    let server = MockServer::start();
    let _rust = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust",
                "https://www.rust-lang.org",
                "Rust homepage.",
            ));
    });
    let _zig = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "zig" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Zig",
                "https://ziglang.org",
                "Zig homepage.",
            ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &[
            "batch",
            "rust",
            "zig",
            "--format",
            "json",
            "--concurrency",
            "2",
            "--rate-limit",
            "60",
        ],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["queries"], json!(["rust", "zig"]));
    assert_eq!(body["results"][0]["data"][0]["title"], "Rust");
    assert_eq!(body["results"][1]["data"][0]["title"], "Zig");
}

#[test]
fn batch_command_reports_partial_failures_in_json_mode() {
    let server = MockServer::start();
    let _ok = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust",
                "https://www.rust-lang.org",
                "Rust homepage.",
            ));
    });
    let _fail = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "broken" }))
            .header("authorization", "Bearer test-api-key");
        then.status(403)
            .header("content-type", "application/json")
            .json_body(json!({
                "error": [{ "msg": "Insufficient credit" }]
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &[
            "batch",
            "rust",
            "broken",
            "--format",
            "json",
            "--concurrency",
            "2",
            "--rate-limit",
            "60",
        ],
        &env_refs(&env),
        tempdir.path(),
    );

    assert!(!output.status.success(), "batch command should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("1 batch query failed"));
    assert!(stderr.contains("1 succeeded"));
    assert!(stderr.contains("broken: authentication error"));
    assert!(stderr.contains("Insufficient credit"));
}

#[test]
fn auth_check_validates_credentials_without_live_network() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust lang" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust",
                "https://www.rust-lang.org",
                "Rust homepage.",
            ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(&["auth", "check"], &env_refs(&env), tempdir.path());

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("auth check passed: api-key (env)"));
}

#[test]
fn auth_check_uses_current_search_api_for_api_keys() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust lang" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust",
                "https://www.rust-lang.org",
                "Rust homepage.",
            ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(&["auth", "check"], &env_refs(&env), tempdir.path());

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("auth check passed: api-key (env)"));
    assert_eq!(_search.calls(), 1, "auth check should call v1 Search API");
}

#[test]
fn auth_check_validates_legacy_api_token_with_fastgpt() {
    let server = MockServer::start();
    let _fastgpt = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v0/fastgpt")
            .json_body(json!({
                "query": "2+2",
                "cache": true,
                "web_search": false
            }))
            .header("authorization", "Bot test-api-token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "meta": api_meta(),
                "data": {
                    "output": "4",
                    "tokens": 4,
                    "references": []
                }
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = vec![
        ("KAGI_API_TOKEN", API_TOKEN.to_string()),
        ("KAGI_BASE_URL", server.base_url()),
    ];
    let output = run_kagi(&["auth", "check"], &env_refs(&env), tempdir.path());

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("auth check passed: api-token (env)"));
}

#[test]
fn auth_set_saves_api_key_and_legacy_api_token_separately() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(
        &[
            "auth",
            "set",
            "--api-key",
            "current-key",
            "--api-token",
            "legacy-token",
        ],
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let raw = fs::read_to_string(config_path(tempdir.path())).expect("config should exist");
    assert!(raw.contains("api_key = \"current-key\""));
    assert!(raw.contains("api_token = \"legacy-token\""));
}

#[test]
fn mcp_install_writes_cursor_global_config() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(
        &[
            "mcp",
            "install",
            "--target",
            "cursor",
            "--kagi-path",
            "/usr/local/bin/kagi",
        ],
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MCP setup complete"));

    let raw = fs::read_to_string(tempdir.path().join(".cursor").join("mcp.json"))
        .expect("cursor MCP config should be written");
    let config: Value = serde_json::from_str(&raw).expect("cursor MCP config should parse");
    assert_eq!(
        config["mcpServers"]["kagi-mcp"],
        json!({
            "command": "/usr/local/bin/kagi",
            "args": ["mcp"]
        })
    );
}

#[test]
fn mcp_install_preserves_existing_json_mcp_servers() {
    let tempdir = TempDir::new().expect("tempdir");
    let path = tempdir.path().join(".cursor").join("mcp.json");
    fs::create_dir_all(path.parent().expect("cursor config has parent"))
        .expect("cursor config directory should create");
    fs::write(
        &path,
        r#"{
  "mcpServers": {
    "github": {
      "command": "docker",
      "args": ["run", "github-mcp"]
    }
  }
}
"#,
    )
    .expect("cursor config should seed");

    let output = run_kagi(
        &[
            "mcp",
            "install",
            "--target",
            "cursor",
            "--kagi-path",
            "/usr/local/bin/kagi",
        ],
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let raw = fs::read_to_string(path).expect("cursor MCP config should be written");
    let config: Value = serde_json::from_str(&raw).expect("cursor MCP config should parse");
    assert_eq!(
        config["mcpServers"]["github"],
        json!({
            "command": "docker",
            "args": ["run", "github-mcp"]
        })
    );
    assert_eq!(
        config["mcpServers"]["kagi-mcp"],
        json!({
            "command": "/usr/local/bin/kagi",
            "args": ["mcp"]
        })
    );
}

#[test]
fn mcp_install_writes_opencode_config_without_removing_existing_settings() {
    let tempdir = TempDir::new().expect("tempdir");
    let path = tempdir
        .path()
        .join(".config")
        .join("opencode")
        .join("opencode.json");
    fs::create_dir_all(path.parent().expect("opencode config has parent"))
        .expect("opencode config directory should create");
    fs::write(
        &path,
        r#"{
  // OpenCode accepts JSONC, so setup should parse this instead of failing.
  "$schema": "https://opencode.ai/config.json",
  "model": "anthropic/claude-sonnet-4-5",
}
"#,
    )
    .expect("opencode config should seed");

    let output = run_kagi(
        &[
            "mcp",
            "setup",
            "--target",
            "opencode",
            "--kagi-path",
            "/opt/kagi/bin/kagi",
        ],
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let raw = fs::read_to_string(path).expect("opencode config should be written");
    let config: Value = serde_json::from_str(&raw).expect("opencode config should parse");
    assert_eq!(
        config["mcp"]["kagi-mcp"],
        json!({
            "type": "local",
            "command": ["/opt/kagi/bin/kagi", "mcp"],
            "enabled": true
        })
    );
    assert_eq!(config["model"], "anthropic/claude-sonnet-4-5");
}

#[test]
fn mcp_install_writes_codex_config_without_client_cli() {
    let tempdir = TempDir::new().expect("tempdir");
    let path = tempdir.path().join(".codex").join("config.toml");
    fs::create_dir_all(path.parent().expect("codex config has parent"))
        .expect("codex config directory should create");
    fs::write(
        &path,
        r#"[mcp_servers.github]
command = "docker"
args = ["run", "github-mcp"]

[model_providers.local]
name = "local"
"#,
    )
    .expect("Codex config should seed");

    let output = run_kagi(
        &[
            "mcp",
            "install",
            "--target",
            "codex",
            "--kagi-path",
            "/tmp/kagi",
        ],
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let raw = fs::read_to_string(path).expect("Codex config should be written");
    let config: toml::Value = toml::from_str(&raw).expect("Codex config should parse");
    assert_eq!(
        config["mcp_servers"]["github"]["command"].as_str(),
        Some("docker")
    );
    assert_eq!(
        config["mcp_servers"]["kagi-mcp"]["command"].as_str(),
        Some("/tmp/kagi")
    );
    assert_eq!(
        config["model_providers"]["local"]["name"].as_str(),
        Some("local")
    );
    assert_eq!(
        config["mcp_servers"]["kagi-mcp"]["args"]
            .as_array()
            .expect("args should be an array")
            .iter()
            .map(|value| value.as_str().expect("arg should be a string"))
            .collect::<Vec<_>>(),
        vec!["mcp"]
    );
}

#[test]
fn mcp_install_writes_vs_code_user_config_without_client_cli() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(
        &[
            "mcp",
            "install",
            "--target",
            "vs-code",
            "--kagi-path",
            "/usr/bin/kagi",
        ],
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let raw = fs::read_to_string(
        tempdir
            .path()
            .join(".config")
            .join("Code")
            .join("User")
            .join("mcp.json"),
    )
    .expect("VS Code MCP config should be written");
    let config: Value = serde_json::from_str(&raw).expect("VS Code MCP config should parse");
    assert_eq!(
        config["servers"]["kagi-mcp"],
        json!({
            "command": "/usr/bin/kagi",
            "args": ["mcp"]
        })
    );
}

#[test]
fn mcp_install_writes_roo_code_extension_config() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(
        &[
            "mcp",
            "install",
            "--target",
            "roo-code",
            "--kagi-path",
            "/usr/bin/kagi",
        ],
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let path = tempdir
        .path()
        .join(".config")
        .join("Code")
        .join("User")
        .join("globalStorage")
        .join("rooveterinaryinc.roo-cline")
        .join("settings")
        .join("cline_mcp_settings.json");
    let raw = fs::read_to_string(path).expect("Roo Code MCP config should be written");
    let config: Value = serde_json::from_str(&raw).expect("Roo Code MCP config should parse");
    assert_eq!(
        config["mcpServers"]["kagi-mcp"],
        json!({
            "command": "/usr/bin/kagi",
            "args": ["mcp"],
            "disabled": false,
            "autoApprove": []
        })
    );
}

#[test]
fn mcp_install_writes_droid_user_config() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(
        &[
            "mcp",
            "install",
            "--target",
            "droid",
            "--kagi-path",
            "/usr/local/bin/kagi",
        ],
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let raw = fs::read_to_string(tempdir.path().join(".factory").join("mcp.json"))
        .expect("Droid MCP config should be written");
    let config: Value = serde_json::from_str(&raw).expect("Droid MCP config should parse");
    assert_eq!(
        config["mcpServers"]["kagi-mcp"],
        json!({
            "type": "stdio",
            "command": "/usr/local/bin/kagi",
            "args": ["mcp"],
            "disabled": false
        })
    );
}

#[test]
fn mcp_install_writes_antigravity_cli_config() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(
        &[
            "mcp",
            "install",
            "--target",
            "antigravity",
            "--kagi-path",
            "/opt/kagi",
        ],
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let raw = fs::read_to_string(
        tempdir
            .path()
            .join(".gemini")
            .join("antigravity-cli")
            .join("mcp_config.json"),
    )
    .expect("Antigravity MCP config should be written");
    let config: Value = serde_json::from_str(&raw).expect("Antigravity MCP config should parse");
    assert_eq!(
        config["mcpServers"]["kagi-mcp"],
        json!({
            "command": "/opt/kagi",
            "args": ["mcp"]
        })
    );
}

#[test]
fn mcp_install_dry_run_does_not_require_client_cli() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(
        &[
            "mcp",
            "install",
            "--target",
            "codex",
            "--kagi-path",
            "/tmp/kagi",
            "--dry-run",
        ],
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MCP setup plan"));
    assert!(stdout.contains("Codex CLI (ok): write"));
    assert!(stdout.contains(".codex/config.toml"));
}

#[test]
fn mcp_install_requires_target_when_not_interactive() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(&["mcp", "install"], &[], tempdir.path());

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("MCP setup needs an interactive terminal"));
    assert!(stderr.contains("--target"));
}

#[test]
fn summarize_url_command_prints_structured_json() {
    let server = MockServer::start();
    let _summarize = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v0/summarize")
            .header("authorization", "Bot test-api-token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "meta": api_meta(),
                "data": {
                    "output": "A concise summary.",
                    "tokens": 42
                }
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &["summarize", "--url", "https://example.com/article"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["data"]["output"], "A concise summary.");
}

#[test]
fn extract_command_prints_markdown_from_mock_api() {
    let server = MockServer::start();
    let _extract = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/extract")
            .header("authorization", "Bearer test-api-key")
            .json_body(json!({
                "pages": [
                    {
                        "url": "https://example.com/article"
                    }
                ],
                "format": "json"
            }));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "meta": {
                    "trace": "trace-1",
                    "node": "test",
                    "ms": 12
                },
                "data": [
                    {
                        "url": "https://example.com/article",
                        "markdown": "# Article\n\nExtracted content."
                    }
                ]
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &["extract", "https://example.com/article"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "# Article\n\nExtracted content.\n");
}

#[test]
fn extract_command_json_surfaces_retained_links_from_mock_api() {
    let server = MockServer::start();
    let _extract = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/extract")
            .header("authorization", "Bearer test-api-key")
            .json_body(json!({
                "pages": [
                    {
                        "url": "https://example.com/article"
                    }
                ],
                "format": "json"
            }));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "meta": {
                    "trace": "trace-1",
                    "node": "test",
                    "ms": 12
                },
                "data": [
                    {
                        "url": "https://example.com/article",
                        "markdown": "# Article\n\nExtracted content.",
                        "links": [
                            {
                                "text": "Source",
                                "url": "https://example.com/source"
                            }
                        ]
                    }
                ]
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &["extract", "https://example.com/article", "--format", "json"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["data"][0]["links"][0]["text"], "Source");
    assert_eq!(
        body["data"][0]["links"][0]["url"],
        "https://example.com/source"
    );
}

#[test]
fn extract_command_rejects_non_https_urls() {
    let tempdir = TempDir::new().expect("tempdir");
    let env = [("KAGI_API_KEY", API_KEY)];
    let output = run_kagi(&["extract", "http://example.com"], &env, tempdir.path());

    assert!(
        !output.status.success(),
        "expected non-zero exit for non-HTTPS extract URL"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("extract URL must use the https scheme"),
        "expected HTTPS validation in stderr: {stderr}"
    );
}

#[test]
fn extract_command_requires_api_key_with_session_only_auth() {
    let server = MockServer::start();
    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(
        &["extract", "https://example.com/article"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert!(
        !output.status.success(),
        "expected session-only extract to fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("extract requires KAGI_API_KEY"),
        "expected API key requirement in stderr: {stderr}"
    );
}

#[test]
fn extract_filter_prints_ordered_jsonl_records() {
    let server = MockServer::start();
    let extract = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/extract")
            .header("authorization", "Bearer test-api-key")
            .json_body(json!({
                "pages": [
                    {
                        "url": "https://example.com/one"
                    }
                ],
                "format": "json"
            }));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "meta": {
                    "trace": "trace-1",
                    "node": "test",
                    "ms": 12
                },
                "data": [
                    {
                        "url": "https://example.com/one",
                        "markdown": "# One"
                    }
                ]
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi_with_stdin(
        &["extract", "--filter"],
        "https://example.com/one\nhttp://example.com/two\n",
        &env_refs(&env),
        tempdir.path(),
    );

    assert!(
        !output.status.success(),
        "expected nonzero exit when one filter item fails"
    );
    extract.assert_calls(1);
    let records = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSONL record should parse"))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["url"], "https://example.com/one");
    assert_eq!(records[0]["ok"], true);
    assert_eq!(records[0]["response"]["data"][0]["markdown"], "# One");
    assert_eq!(records[1]["url"], "http://example.com/two");
    assert_eq!(records[1]["ok"], false);
    assert_eq!(records[1]["error"]["code"], "configuration_error");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("extract --filter completed with 1 failed item"),
        "expected aggregate failure on stderr, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn news_search_html_fixture() -> &'static str {
    r#"<html><body>
        <div class="newsResultItem _0_SRI">
          <span class="newsResultTime">2 hours ago</span>
          <h3 class="__sri-title-box">
            <a class="_0_TITLE" data-domain="cnn.com" href="https://www.cnn.com/lead">Lead Story</a>
          </h3>
          <div class="trigger paywall-icon"></div>
          <div class="newsResultContent">Lead snippet.</div>
        </div>
        <div class="newsResultGroup">
          <div class="newsResultItem _0_SRI">
            <span class="newsResultTime">3 hours ago</span>
            <h3 class="__sri-title-box">
              <a class="_0_TITLE" data-domain="theguardian.com" href="https://theguardian.com/a">First in Cluster</a>
            </h3>
            <div class="newsResultContent">First cluster snippet.</div>
          </div>
          <div class="newsResultItem _0_SRI">
            <span class="newsResultTime">4 hours ago</span>
            <h3 class="__sri-title-box">
              <a class="_0_TITLE" data-domain="bbc.com" href="https://bbc.com/b">Follower</a>
            </h3>
          </div>
        </div>
      </body></html>"#
}

#[test]
fn search_news_returns_clustered_json() {
    let server = MockServer::start();
    let _news = server.mock(|when, then| {
        when.method(GET)
            .path("/news")
            .query_param("q", "iran")
            .query_param("freshness", "day")
            .query_param("order", "2")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .header("content-type", "text/html")
            .body(news_search_html_fixture());
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(
        &[
            "search", "iran", "--news", "--time", "day", "--order", "recency", "--format", "json",
        ],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["query"], "iran");
    let clusters = body["clusters"].as_array().expect("clusters array");
    assert_eq!(clusters.len(), 2, "expected ungrouped + grouped clusters");
    assert_eq!(clusters[0]["items"][0]["title"], "Lead Story");
    assert_eq!(clusters[0]["items"][0]["source"], "cnn.com");
    assert_eq!(clusters[0]["items"][0]["time_relative"], "2 hours ago");
    assert_eq!(clusters[0]["items"][0]["paywall"], true);
    let cluster_items = clusters[1]["items"].as_array().expect("cluster items");
    assert_eq!(cluster_items.len(), 2);
    assert_eq!(cluster_items[1]["source"], "bbc.com");
    assert_eq!(cluster_items[1]["time_relative"], "4 hours ago");
}

#[test]
fn search_news_local_cache_reuses_cached_response() {
    let server = MockServer::start();
    let news = server.mock(|when, then| {
        when.method(GET)
            .path("/news")
            .query_param("q", "iran")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .header("content-type", "text/html")
            .body(news_search_html_fixture());
    });

    let tempdir = TempDir::new().expect("tempdir");
    let cache_dir = tempdir.path().join("cache");
    let cache_dir_value = cache_dir.to_string_lossy().to_string();
    let mut env = session_env(&server);
    env.push(("KAGI_CACHE_DIR", cache_dir_value));

    let first = run_kagi(
        &[
            "search",
            "iran",
            "--news",
            "--local-cache",
            "--format",
            "json",
        ],
        &env_refs(&env),
        tempdir.path(),
    );
    assert_success(&first);

    let second = run_kagi(
        &[
            "search",
            "iran",
            "--news",
            "--local-cache",
            "--format",
            "json",
        ],
        &env_refs(&env),
        tempdir.path(),
    );
    assert_success(&second);

    news.assert_calls(1);
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn search_news_rejects_lens_combination() {
    let tempdir = TempDir::new().expect("tempdir");
    let env = [("KAGI_SESSION_TOKEN", "test-session")];
    let output = run_kagi(
        &["search", "iran", "--news", "--lens", "1"],
        &env,
        tempdir.path(),
    );
    assert!(
        !output.status.success(),
        "expected non-zero exit for --news --lens"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--lens"),
        "expected --lens conflict in stderr: {stderr}"
    );
}

#[test]
fn search_news_rejects_time_year() {
    let tempdir = TempDir::new().expect("tempdir");
    let env = [("KAGI_SESSION_TOKEN", "test-session")];
    let output = run_kagi(
        &["search", "iran", "--news", "--time", "year"],
        &env,
        tempdir.path(),
    );
    assert!(
        !output.status.success(),
        "expected non-zero exit for --news --time year"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--time year"),
        "expected --time year rejection in stderr: {stderr}"
    );
}

#[test]
fn news_command_resolves_category_and_prints_json() {
    let server = MockServer::start();
    let _latest = server.mock(|when, then| {
        when.method(GET)
            .path("/api/batches/latest")
            .query_param("lang", "en");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_latest_batch());
    });
    let _metadata = server.mock(|when, then| {
        when.method(GET).path("/api/categories/metadata");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_category_metadata());
    });
    let _categories = server.mock(|when, then| {
        when.method(GET)
            .path("/api/batches/batch-1/categories")
            .query_param("lang", "en");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_batch_categories());
    });
    let _stories = server.mock(|when, then| {
        when.method(GET)
            .path("/api/batches/batch-1/categories/category-1/stories")
            .query_param("limit", "12")
            .query_param("lang", "en");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_stories());
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &["news", "--category", "tech", "--lang", "en"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["category"]["category_name"], "Tech");
    assert_eq!(body["stories"][0]["title"], "Rust ships new release");
}

#[test]
fn assistant_thread_list_paginates_with_cursor_id() {
    let server = MockServer::start();
    let _folders = server.mock(|when, then| {
        when.method(GET)
            .path("/api/folders")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!([]));
    });
    let _first_page = server.mock(|when, then| {
        when.method(GET)
            .path("/api/conversations")
            .header("cookie", "kagi_session=test-session")
            .query_param("limit", "100")
            .query_param("all", "true")
            .query_param_missing("after_id");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "items": [{
                    "conversation": {
                        "uuid": "thread-1",
                        "title": "First Thread",
                        "created_at": "2026-02-11T16:22:13Z",
                        "updated_at": "2026-02-11T16:22:13Z",
                        "is_saved": false,
                        "is_shared": false,
                        "folder_uuid": null
                    },
                    "rank": 0,
                    "snippet": "First snippet"
                }],
                "has_more": true
            }));
    });
    let _second_page = server.mock(|when, then| {
        when.method(GET)
            .path("/api/conversations")
            .header("cookie", "kagi_session=test-session")
            .query_param("limit", "100")
            .query_param("all", "true")
            .query_param("after_id", "thread-1");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "items": [{
                    "conversation": {
                        "uuid": "thread-2",
                        "title": "Second Thread",
                        "created_at": "2026-02-12T16:22:13Z",
                        "updated_at": "2026-02-12T16:22:13Z",
                        "is_saved": false,
                        "is_shared": false,
                        "folder_uuid": null
                    },
                    "rank": 1,
                    "snippet": "Second snippet"
                }],
                "has_more": false
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(
        &["assistant", "thread", "list"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["threads"][0]["id"], "thread-1");
    assert_eq!(body["threads"][1]["id"], "thread-2");
    assert_eq!(body["pagination"]["count"], 2);
    assert_eq!(body["pagination"]["total_counts"]["all"], 2);
}

#[test]
fn assistant_models_prints_json_catalog() {
    let server = MockServer::start();
    let _catalog = server.mock(|when, then| {
        when.method(GET)
            .path("/api/init")
            .header("cookie", "kagi_session=test-session")
            .header("accept", "application/json");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "models": {
                    "models": [
                        { "id": "ki_quick", "display_name": "Quick" },
                        { "id": "claude-5-opus-thinking", "display_name": "Claude Opus 5 (reasoning)" }
                    ],
                    "default": "ki_quick"
                }
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(&["assistant", "models"], &env_refs(&env), tempdir.path());

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["models"][0]["id"], "ki_quick");
    assert_eq!(body["models"][0]["label"], "Quick");
    assert_eq!(body["models"][1]["id"], "claude-5-opus-thinking");
    assert_eq!(body["default"], "ki_quick");
}

#[test]
fn assistant_custom_create_uses_current_assistant_api() {
    let server = MockServer::start();
    let _create = server.mock(|when, then| {
        when.method(POST)
            .path("/api/assistants")
            .header("cookie", "kagi_session=test-session")
            .header("content-type", "application/json")
            .body_includes("\"name\":\"Release Notes\"")
            .body_includes("\"llm_id\":\"gpt-5-4-nano\"")
            .body_includes("\"internet_access\":true")
            .body_includes("\"lens_id\":\"2\"")
            .body_includes("\"instructions\":\"Focus on release diffs and migration notes.\"");
        then.status(200).json_body(custom_assistant_json(
            "profile-release-notes",
            "Release Notes",
            "gpt-5-4-nano",
        ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(
        &[
            "assistant",
            "custom",
            "create",
            "Release Notes",
            "--model",
            "gpt-5-4-nano",
            "--web-access",
            "--lens",
            "2",
            "--instructions",
            "Focus on release diffs and migration notes.",
        ],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["profile_id"], "profile-release-notes");
    assert_eq!(body["name"], "Release Notes");
    assert_eq!(body["base_model"], "gpt-5-4-nano");
}

#[test]
fn assistant_custom_update_uses_current_assistant_api() {
    let server = MockServer::start();
    let _profiles = server.mock(|when, then| {
        when.method(GET)
            .path("/api/init")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .json_body(assistant_init_json(json!([custom_assistant_json(
                "profile-writer",
                "Writer",
                "gpt-5-mini"
            )])));
    });
    let _update = server.mock(|when, then| {
        when.method(PATCH)
            .path("/api/assistants/profile-writer")
            .header("cookie", "kagi_session=test-session")
            .header("content-type", "application/json")
            .body_includes("\"name\":\"Release Notes\"")
            .body_includes("\"llm_id\":\"gpt-5-4-nano\"")
            .body_includes("\"internet_access\":false")
            .body_includes("\"lens_id\":\"2\"")
            .body_includes("\"instructions\":\"Use migration notes.\"");
        then.status(200).json_body(json!({
            "uuid": "profile-writer",
            "name": "Release Notes",
            "llm_id": "gpt-5-4-nano",
            "instructions": "Use migration notes.",
            "internet_access": false,
            "personalizations": false,
            "lens_id": "2",
            "bang_trigger": null
        }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(
        &[
            "assistant",
            "custom",
            "update",
            "Writer",
            "--name",
            "Release Notes",
            "--model",
            "gpt-5-4-nano",
            "--no-web-access",
            "--lens",
            "2",
            "--instructions",
            "Use migration notes.",
        ],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["profile_id"], "profile-writer");
    assert_eq!(body["name"], "Release Notes");
    assert_eq!(body["internet_access"], false);
    assert_eq!(body["selected_lens"], "2");
}

#[test]
fn assistant_stream_prints_text_deltas_by_default() {
    let server = MockServer::start();
    mock_current_assistant_prompt(&server, "Hello", Some("Hel"), "Hello", None);
    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(
        &["assistant", "--stream", "Hello"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello\n");
}

#[test]
fn assistant_stream_can_print_ndjson_updates() {
    let server = MockServer::start();
    mock_current_assistant_prompt(&server, "Hello", Some("Hel"), "Hello", None);
    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(
        &["assistant", "--stream", "--stream-output", "json", "Hello"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let lines = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("line should parse as json"))
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["md_delta"], "Hel");
    assert_eq!(lines[1]["md_delta"], "lo");
    assert_eq!(lines[1]["message"]["state"], "done");
    assert_eq!(lines[1]["message"]["usage"]["prompt_tokens"], 4314);
    assert_eq!(lines[1]["message"]["usage"]["completion_tokens"], 2);
    assert_eq!(lines[1]["message"]["usage"]["total_tokens"], 4316);
    assert_eq!(lines[1]["message"]["usage"]["cost_usd"], 0.006192);
}

#[test]
fn assistant_resolves_name_to_profile_uuid() {
    let server = MockServer::start();
    let message =
        mock_current_assistant_prompt(&server, "Hello", None, "Hello", Some("profile-once"));
    let _profiles = server.mock(|when, then| {
        when.method(GET)
            .path("/api/init")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .json_body(assistant_init_json(json!([custom_assistant_json(
                "profile-once",
                "Once",
                "gpt-5-mini"
            )])));
    });
    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(
        &["assistant", "--assistant", "Once", "Hello"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    message.assert_calls(1);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["message"]["profile"]["id"], "profile-once");
    assert_eq!(body["message"]["profile"]["name"], "Once");
}

#[test]
fn assistant_contract_decision_prints_validated_json() {
    let server = MockServer::start();
    let current_prompt = mock_current_assistant_prompt(
        &server,
        "Assistant contract",
        None,
        r#"{"decision":"ship","rationale":"tests pass","next_actions":["open PR"]}"#,
        None,
    );
    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(
        &["assistant", "--contract", "decision", "Should I ship?"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    current_prompt.assert_calls(1);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["decision"], "ship");
    assert_eq!(body["rationale"], "tests pass");
    assert_eq!(body["next_actions"], json!(["open PR"]));
    assert!(
        body.get("message").is_none(),
        "contract mode should print the contract JSON only"
    );
}

#[test]
fn assistant_contract_file_rejects_missing_required_key() {
    let server = MockServer::start();
    let current_prompt = mock_current_assistant_prompt(
        &server,
        "Assistant contract",
        None,
        r#"{"summary":"not enough"}"#,
        None,
    );
    let tempdir = TempDir::new().expect("tempdir");
    let contract_path = tempdir.path().join("verdict-contract.json");
    fs::write(
        &contract_path,
        r#"{
      "type": "object",
      "required": ["summary", "verdict"],
      "properties": {
        "summary": { "type": "string" },
        "verdict": { "type": "string" }
      }
    }"#,
    )
    .expect("contract file should write");
    let env = session_env(&server);
    let contract_path_string = contract_path.to_string_lossy().to_string();
    let output = run_kagi(
        &[
            "assistant",
            "--contract-file",
            &contract_path_string,
            "Summarize the release state",
        ],
        &env_refs(&env),
        tempdir.path(),
    );

    assert!(
        !output.status.success(),
        "expected invalid contract output to fail"
    );
    current_prompt.assert_calls(2);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("assistant contract"),
        "expected contract error, got:\n{stderr}"
    );
    assert!(
        stderr.contains("missing required key 'verdict'"),
        "expected missing key detail, got:\n{stderr}"
    );
}

#[test]
fn completion_install_detects_fish_and_writes_completion_file() {
    let tempdir = TempDir::new().expect("tempdir");
    let config_home = tempdir.path().join("config");
    let config_home_value = config_home.to_string_lossy().to_string();
    let env = vec![
        ("SHELL", "/usr/bin/fish".to_string()),
        ("XDG_CONFIG_HOME", config_home_value),
    ];

    let output = run_kagi(&["completion", "install"], &env_refs(&env), tempdir.path());

    assert_success(&output);
    let target = config_home
        .join("fish")
        .join("completions")
        .join("kagi.fish");
    let completion = fs::read_to_string(&target).expect("completion file should exist");
    assert!(
        completion.contains("complete"),
        "expected fish completion script, got:\n{completion}"
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(target.to_string_lossy().as_ref()));
}

#[test]
fn assistant_once_creates_prompts_and_deletes_temporary_profile() {
    let server = MockServer::start();
    mock_current_assistant_prompt(&server, "Hi", None, "ok", None);
    let _create = server.mock(|when, then| {
        when.method(POST)
            .path("/api/assistants")
            .header("cookie", "kagi_session=test-session")
            .body_includes("\"llm_id\":\"gpt-5-mini\"");
        then.status(200)
            .json_body(custom_assistant_json("profile-once", "Once", "gpt-5-mini"));
    });
    let _list = server.mock(|when, then| {
        when.method(GET)
            .path("/api/init")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .json_body(assistant_init_json(json!([custom_assistant_json(
                "profile-once",
                "Once",
                "gpt-5-mini"
            )])));
    });
    let _delete = server.mock(|when, then| {
        when.method(DELETE)
            .path("/api/assistants/profile-once")
            .header("cookie", "kagi_session=test-session");
        then.status(204);
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(
        &["assistant", "--once", "--model", "gpt-5-mini", "Hi"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["message"]["markdown"], "ok");
}

#[test]
fn batch_command_reads_queries_from_stdin() {
    let server = MockServer::start();
    let _rust = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust",
                "https://www.rust-lang.org",
                "Rust homepage.",
            ));
    });
    let _zig = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "zig" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Zig",
                "https://ziglang.org",
                "Zig homepage.",
            ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi_with_stdin(
        &["batch", "--format", "json", "--concurrency", "2"],
        "rust\nzig\n",
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["queries"], json!(["rust", "zig"]));
}

#[test]
fn search_template_renders_result_fields() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust",
                "https://www.rust-lang.org",
                "Rust homepage.",
            ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let output = run_kagi(
        &["search", "rust", "--template", "{{rank}} {{title}} {{url}}"],
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "1 Rust https://www.rust-lang.org"
    );
}

#[test]
fn site_pref_and_history_use_local_cache_dir() {
    let tempdir = TempDir::new().expect("tempdir");
    let cache_dir = tempdir.path().join("cache");
    let cache_dir_value = cache_dir.to_string_lossy().to_string();
    let env = [("KAGI_CACHE_DIR", cache_dir_value.as_str())];

    let set_output = run_kagi(
        &["site-pref", "set", "Example.COM/path", "--mode", "pin"],
        &env,
        tempdir.path(),
    );
    assert_success(&set_output);

    let list_output = run_kagi(&["site-pref", "list"], &env, tempdir.path());
    assert_success(&list_output);
    let prefs: Value = serde_json::from_slice(&list_output.stdout).expect("prefs json parses");
    assert_eq!(prefs["domains"]["example.com"], "pin");

    let history_output = run_kagi(&["history", "stats"], &env, tempdir.path());
    assert_success(&history_output);
    let stats: Value = serde_json::from_slice(&history_output.stdout).expect("history json parses");
    assert_eq!(stats["total"], 0);
}

#[test]
fn mcp_server_discover_returns_modern_server_info() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_request(json!(1), "server/discover", json!({}));
    let output = run_kagi_with_stdin(
        &["mcp"],
        &format!(
            "{}\n",
            serde_json::to_string(&request).expect("request serializes")
        ),
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(
        response["result"]["supportedVersions"],
        json!(["2026-07-28"])
    );
    assert_eq!(
        response["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "kagi-cli"
    );
    assert_eq!(response["result"]["ttlMs"], 3600000);
    assert_eq!(response["result"]["cacheScope"], "public");
}

fn tool_named<'a>(tools: &'a [Value], name: &str) -> &'a Value {
    tools
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap_or_else(|| panic!("expected {name} in tools list, got {tools:?}"))
}

#[test]
fn mcp_tools_list_declares_input_schemas() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_request(json!(1), "tools/list", json!({}));
    let output = run_kagi_with_stdin(
        &["mcp"],
        &format!(
            "{}\n",
            serde_json::to_string(&request).expect("request serializes")
        ),
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(response["result"]["cacheScope"], "public");
    assert_eq!(response["result"]["ttlMs"], 3600000);
    let tools = response["result"]["tools"].as_array().expect("tools array");

    for tool in tools {
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "schema type for {tool:?}");
        assert_eq!(
            schema["$schema"], "https://json-schema.org/draft/2020-12/schema",
            "schema dialect for {tool:?}"
        );
        assert!(
            schema["properties"].is_object(),
            "expected schema properties object for {tool:?}"
        );
    }

    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();
    assert!(
        names.windows(2).all(|pair| pair[0] < pair[1]),
        "tools should be returned in deterministic sorted order: {names:?}"
    );
    assert_eq!(
        tool_named(tools, "kagi_search")["annotations"],
        json!({ "readOnlyHint": true, "openWorldHint": true })
    );

    assert_eq!(
        tool_named(tools, "kagi_search")["inputSchema"]["required"],
        json!(["query"])
    );
    assert_eq!(
        tool_named(tools, "kagi_quick")["inputSchema"]["required"],
        json!(["query"])
    );
    assert_eq!(
        tool_named(tools, "kagi_extract")["inputSchema"]["required"],
        json!(["url"])
    );
    assert_eq!(
        tool_named(tools, "kagi_news_search")["inputSchema"]["required"],
        json!(["query"])
    );
    assert_eq!(
        tool_named(tools, "kagi_summarize")["inputSchema"]["anyOf"],
        json!([{ "required": ["url"] }, { "required": ["text"] }])
    );
    assert_eq!(
        tool_named(tools, "kagi_news")["inputSchema"]["properties"]["category"]["default"],
        json!("world")
    );
    assert!(
        tool_named(tools, "kagi_assistant_models").is_object(),
        "expected assistant model catalog tool"
    );
    assert!(
        tool_named(tools, "kagi_fastgpt").is_object(),
        "expected FastGPT tool"
    );
    assert!(
        tool_named(tools, "kagi_translate").is_object(),
        "expected translate tool"
    );
    assert!(
        !tools.iter().any(|tool| tool["name"] == "kagi_assistant"),
        "assistant prompts should not be exposed by default because they create or extend threads"
    );
    assert!(
        !tools.iter().any(|tool| tool["name"] == "kagi_ask_page"),
        "page questions should not be exposed by default because they create Assistant messages"
    );
    assert!(
        !tools.iter().any(|tool| tool["name"] == "kagi_lens_create"),
        "mutating tools should not be exposed by default"
    );
}

#[test]
fn mcp_tools_list_exposes_mutating_tools_when_enabled() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_request(json!(1), "tools/list", json!({}));
    let output = run_kagi_with_stdin(
        &["mcp", "--enable-mutating-tools"],
        &format!(
            "{}\n",
            serde_json::to_string(&request).expect("request serializes")
        ),
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    let tools = response["result"]["tools"].as_array().expect("tools array");
    assert!(
        tool_named(tools, "kagi_lens_create").is_object(),
        "expected mutating lens tool when enabled"
    );
    assert!(
        tool_named(tools, "kagi_site_pref_set").is_object(),
        "expected local-state mutating tool when enabled"
    );
    assert!(
        tool_named(tools, "kagi_cli").is_object(),
        "expected CLI parity escape hatch when mutating tools are enabled"
    );
    assert_eq!(
        tool_named(tools, "kagi_assistant")["annotations"]["readOnlyHint"],
        false
    );
    assert_eq!(
        tool_named(tools, "kagi_ask_page")["annotations"]["readOnlyHint"],
        false
    );
    assert!(
        tool_named(tools, "kagi_assistant")["annotations"]
            .get("destructiveHint")
            .is_none()
    );
    assert_eq!(
        tool_named(tools, "kagi_lens_create")["annotations"]["readOnlyHint"],
        false
    );
    assert_eq!(
        tool_named(tools, "kagi_lens_delete")["annotations"]["destructiveHint"],
        true
    );
    assert!(
        tool_named(tools, "kagi_lens_update")["annotations"]
            .get("destructiveHint")
            .is_none()
    );
}

#[test]
fn mcp_tools_list_treats_null_cursor_as_absent() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_request(json!(1), "tools/list", json!({ "cursor": null }));
    let output = run_kagi_with_stdin(
        &["mcp"],
        &format!(
            "{}\n",
            serde_json::to_string(&request).expect("request serializes")
        ),
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    assert_eq!(response["result"]["resultType"], "complete");
}

#[test]
fn mcp_malformed_json_returns_parse_error_and_keeps_server_alive() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_request(json!(2), "server/discover", json!({}));
    let output = run_kagi_with_stdin(
        &["mcp"],
        &format!(
            "not json\n{}\n",
            serde_json::to_string(&request).expect("request serializes")
        ),
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let responses: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is valid JSON"))
        .collect();
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["id"], Value::Null);
    assert_eq!(responses[0]["error"]["code"], -32700);
    assert_eq!(responses[1]["id"], 2);
    assert_eq!(
        responses[1]["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "kagi-cli"
    );
}

#[test]
fn mcp_requires_modern_request_metadata() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi_with_stdin(
        &["mcp"],
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{}}\n",
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    assert_eq!(response["error"]["code"], -32602);
    assert!(
        response["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("clientCapabilities")
    );
}

#[test]
fn mcp_rejects_unsupported_protocol_versions() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_request_with_version(json!(1), "server/discover", json!({}), "2025-11-25");
    let output = run_kagi_with_stdin(
        &["mcp"],
        &format!(
            "{}\n",
            serde_json::to_string(&request).expect("request serializes")
        ),
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    assert_eq!(response["error"]["code"], -32022);
    assert_eq!(response["error"]["data"]["requested"], "2025-11-25");
    assert_eq!(
        response["error"]["data"]["supported"],
        json!(["2026-07-28"])
    );
}

#[test]
fn mcp_unknown_tool_returns_json_rpc_error() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_nope",
            "arguments": {}
        }),
    );
    let mut stdin = serde_json::to_string(&request).expect("request serializes");
    stdin.push('\n');

    let output = run_kagi_with_stdin(&["mcp"], &stdin, &[], tempdir.path());

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    assert_eq!(response["id"], 1);
    assert_eq!(response["error"]["code"], -32602);
    assert!(
        response["error"]["message"]
            .as_str()
            .expect("message")
            .contains("Unknown tool")
    );
}

#[test]
fn mcp_cli_passthrough_runs_auth_free_commands_when_enabled() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_cli",
            "arguments": { "args": ["--help"] }
        }),
    );
    let mut stdin = serde_json::to_string(&request).expect("request serializes");
    stdin.push('\n');

    let output = run_kagi_with_stdin(
        &["mcp", "--enable-mutating-tools"],
        &stdin,
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    let payload: Value = serde_json::from_str(text).expect("inner json parses");
    assert_eq!(payload["success"], true);
    assert!(
        payload["stdout"]
            .as_str()
            .expect("stdout")
            .contains("Agent usage:"),
        "expected passthrough help output, got:\n{payload}"
    );
}

#[test]
fn mcp_cli_passthrough_closes_stdin_when_payload_is_omitted() {
    let tempdir = TempDir::new().expect("tempdir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_kagi"));
    child
        .args(["mcp", "--enable-mutating-tools"])
        .current_dir(tempdir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_command_home(&mut child, tempdir.path());
    let mut child = child.spawn().expect("mcp server should spawn");
    let mut stdin = child.stdin.take().expect("mcp stdin should be piped");
    let stdout = child.stdout.take().expect("mcp stdout should be piped");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let read = reader.read_line(&mut line);
        let _ = tx.send(read.map(|_| line));
    });

    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_cli",
            "arguments": { "args": ["batch"] }
        }),
    );
    writeln!(stdin, "{request}").expect("request should write");
    stdin.flush().expect("request should flush");

    let line = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("kagi_cli should return without inheriting the open MCP stdin")
        .expect("mcp stdout should read");
    drop(stdin);
    let status = child
        .wait()
        .expect("mcp server should exit after stdin closes");
    assert!(status.success(), "mcp server should exit successfully");

    let response: Value = serde_json::from_str(&line).expect("mcp response should parse");
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    let payload: Value = serde_json::from_str(text).expect("inner json parses");
    assert_eq!(payload["success"], false);
    assert!(
        payload["stderr"]
            .as_str()
            .expect("stderr")
            .contains("no queries were provided"),
        "expected batch EOF error, got:\n{payload}"
    );
}

#[test]
fn mcp_extract_tool_call_returns_markdown() {
    let server = MockServer::start();
    let _extract = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/extract")
            .header("authorization", "Bearer test-api-key")
            .json_body(json!({
                "pages": [
                    {
                        "url": "https://example.com/article"
                    }
                ],
                "format": "json"
            }));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "meta": {
                    "trace": "trace-1",
                    "node": "test",
                    "ms": 12
                },
                "data": [
                    {
                        "url": "https://example.com/article",
                        "markdown": "# Article\n\nExtracted content."
                    }
                ]
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_extract",
            "arguments": { "url": "https://example.com/article" }
        }),
    );
    let output = run_kagi_with_stdin(
        &["mcp"],
        &format!(
            "{}\n",
            serde_json::to_string(&request).expect("request serializes")
        ),
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(response["result"]["isError"], false);
    assert_eq!(
        response["result"]["content"][0]["text"],
        "# Article\n\nExtracted content."
    );
}

#[test]
fn mcp_extract_explicit_json_overrides_default_output() {
    let server = MockServer::start();
    let _extract = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/extract")
            .header("authorization", "Bearer test-api-key")
            .json_body(json!({
                "pages": [
                    {
                        "url": "https://example.com/article"
                    }
                ],
                "format": "json"
            }));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "meta": {
                    "trace": "trace-1",
                    "node": "test",
                    "ms": 12
                },
                "data": [
                    {
                        "url": "https://example.com/article",
                        "markdown": "# Article\n\nExtracted content."
                    }
                ]
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_extract",
            "arguments": {
                "url": "https://example.com/article",
                "format": "json"
            }
        }),
    );
    let mut stdin = serde_json::to_string(&request).expect("request serializes");
    stdin.push('\n');

    let output = run_kagi_with_stdin(
        &["mcp", "--default-output", "toon"],
        &stdin,
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    let body: Value = serde_json::from_str(text).expect("inner extract should stay JSON");
    assert_eq!(
        body["data"][0]["markdown"],
        "# Article\n\nExtracted content."
    );
    assert_eq!(
        response["result"]["structuredContent"]["data"][0]["markdown"],
        "# Article\n\nExtracted content."
    );
}

#[test]
fn mcp_search_uses_default_output_format() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/search")
            .json_body(json!({ "query": "rust" }))
            .header("authorization", "Bearer test-api-key");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust",
                "https://www.rust-lang.org",
                "Rust homepage.",
            ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_search",
            "arguments": { "query": "rust" }
        }),
    );
    let mut stdin = serde_json::to_string(&request).expect("request serializes");
    stdin.push('\n');

    let output = run_kagi_with_stdin(
        &["mcp", "--default-output", "toon"],
        &stdin,
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    assert!(
        text.contains("data["),
        "expected TOON output from default format, got:\n{text}"
    );
    assert!(
        serde_json::from_str::<Value>(text).is_err(),
        "expected TOON text, not JSON"
    );
}

#[test]
fn mcp_only_exposes_structured_content_for_json_output() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_history_stats",
            "arguments": {}
        }),
    );
    let mut stdin = serde_json::to_string(&request).expect("request serializes");
    stdin.push('\n');

    let output = run_kagi_with_stdin(
        &["mcp", "--default-output", "pretty"],
        &stdin,
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    let result = &response["result"];
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().expect("text content");
    serde_json::from_str::<Value>(text).expect("pretty output should remain valid JSON text");
    assert!(
        result.get("structuredContent").is_none(),
        "pretty output must not be duplicated as structured content"
    );
}

#[test]
fn mcp_explicit_json_format_exposes_structured_content_for_news_metadata() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_news_filter_presets",
            "arguments": { "format": "json" }
        }),
    );
    let mut stdin = serde_json::to_string(&request).expect("request serializes");
    stdin.push('\n');

    let output = run_kagi_with_stdin(
        &["mcp", "--default-output", "pretty"],
        &stdin,
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    let result = &response["result"];
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().expect("text content");
    serde_json::from_str::<Value>(text).expect("explicit JSON output should parse");
    assert!(
        result.get("structuredContent").is_some(),
        "explicit JSON output must be exposed as structured content"
    );
}

#[test]
fn mcp_batch_search_rejects_zero_concurrency() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_batch_search",
            "arguments": {
                "queries": ["rust"],
                "concurrency": 0
            }
        }),
    );
    let mut stdin = serde_json::to_string(&request).expect("request serializes");
    stdin.push('\n');

    let output = run_kagi_with_stdin(
        &["mcp"],
        &stdin,
        &[("KAGI_API_KEY", API_KEY)],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(response["result"]["isError"], true);
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("message")
            .contains("concurrency must be at least 1")
    );
}

#[test]
fn mcp_assistant_thread_export_json_overrides_default_output() {
    let server = MockServer::start();
    let _folders = server.mock(|when, then| {
        when.method(GET)
            .path("/api/folders")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!([]));
    });
    let _thread = server.mock(|when, then| {
        when.method(GET)
            .path("/api/conversations/thread-1/init")
            .header("cookie", "kagi_session=test-session")
            .header("accept", "application/json");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "conversation": {
                    "uuid": "thread-1",
                    "title": "Greeting",
                    "created_at": "2026-03-16T06:19:07Z",
                    "updated_at": "2026-03-16T06:20:07Z",
                    "is_saved": false,
                    "is_shared": false,
                    "folder_uuid": null
                },
                "active_branch": {
                    "uuid": "branch-1",
                    "conversation_uuid": "thread-1",
                    "is_default": true,
                    "message_count": 2,
                    "updated_at": "2026-03-16T06:20:07Z"
                },
                "branches": [],
                "messages": {
                    "items": [
                        {
                            "uuid": "msg-user",
                            "role": "user",
                            "content": "Hello",
                            "created_at": "2026-03-16T06:19:07Z"
                        },
                        {
                            "uuid": "msg-assistant",
                            "role": "assistant",
                            "content": "Hello back",
                            "html_content": "<p>Hello back</p>",
                            "created_at": "2026-03-16T06:20:07Z",
                            "input_tokens": 4314,
                            "output_tokens": 2,
                            "cost_usd": 0.006192,
                            "references": []
                        }
                    ],
                    "has_more": false
                }
            }));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_assistant_thread_export",
            "arguments": {
                "thread_id": "thread-1",
                "format": "json"
            }
        }),
    );
    let mut stdin = serde_json::to_string(&request).expect("request serializes");
    stdin.push('\n');

    let output = run_kagi_with_stdin(
        &["mcp", "--default-output", "toon"],
        &stdin,
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    let body: Value = serde_json::from_str(text).expect("inner export should stay JSON");
    assert_eq!(body["thread"]["id"], "thread-1");
    assert_eq!(body["messages"][0]["prompt"], "Hello");
    assert_eq!(body["messages"][0]["markdown"], "Hello back");
    assert_eq!(body["messages"][0]["usage"]["prompt_tokens"], 4314);
    assert_eq!(body["messages"][0]["usage"]["completion_tokens"], 2);
    assert_eq!(body["messages"][0]["usage"]["total_tokens"], 4316);
    assert_eq!(body["messages"][0]["usage"]["cost_usd"], 0.006192);
}

#[test]
fn mcp_news_tool_call_returns_stories() {
    let server = MockServer::start();
    let _latest = server.mock(|when, then| {
        when.method(GET)
            .path("/api/batches/latest")
            .query_param("lang", "en");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_latest_batch());
    });
    let _metadata = server.mock(|when, then| {
        when.method(GET).path("/api/categories/metadata");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_category_metadata());
    });
    let _categories = server.mock(|when, then| {
        when.method(GET)
            .path("/api/batches/batch-1/categories")
            .query_param("lang", "en");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_batch_categories());
    });
    let _stories = server.mock(|when, then| {
        when.method(GET)
            .path("/api/batches/batch-1/categories/category-1/stories")
            .query_param("limit", "3")
            .query_param("lang", "en");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_stories());
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = test_env(&server);
    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_news",
            "arguments": { "category": "tech", "lang": "en", "limit": 3 }
        }),
    );
    let mut stdin = serde_json::to_string(&request).expect("request serializes");
    stdin.push('\n');

    let output = run_kagi_with_stdin(&["mcp"], &stdin, &env_refs(&env), tempdir.path());

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    let body: Value = serde_json::from_str(text).expect("inner json parses");
    assert_eq!(body["category"]["category_name"], "Tech");
    assert_eq!(body["stories"][0]["title"], "Rust ships new release");
}

#[test]
fn mcp_news_search_tool_call_returns_clusters() {
    let server = MockServer::start();
    let _news = server.mock(|when, then| {
        when.method(GET)
            .path("/news")
            .query_param("q", "iran")
            .query_param("freshness", "day")
            .query_param("order", "2")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .header("content-type", "text/html")
            .body(news_search_html_fixture());
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_news_search",
            "arguments": {
                "query": "iran",
                "freshness": "day",
                "order": "recency"
            }
        }),
    );
    let mut stdin = serde_json::to_string(&request).expect("request serializes");
    stdin.push('\n');

    let output = run_kagi_with_stdin(&["mcp"], &stdin, &env_refs(&env), tempdir.path());

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    let body: Value = serde_json::from_str(text).expect("inner json parses");
    assert_eq!(body["query"], "iran");
    let clusters = body["clusters"].as_array().expect("clusters array");
    assert_eq!(clusters.len(), 2);
    assert_eq!(clusters[0]["items"][0]["title"], "Lead Story");
    assert_eq!(clusters[0]["items"][0]["paywall"], true);
    assert_eq!(clusters[1]["items"].as_array().unwrap().len(), 2);
}

#[test]
fn mcp_tool_call_error_returns_json_rpc_error_and_keeps_server_alive() {
    let server = MockServer::start();

    // Mock search endpoint to return a 500 error, simulating a backend failure.
    let _search = server.mock(|when, then| {
        when.method(GET).path("/search");
        then.status(500).body("Internal Server Error");
    });

    // Mock news endpoints so kagi_news succeeds -- proving the server survived.
    let _latest = server.mock(|when, then| {
        when.method(GET).path("/api/batches/latest");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_latest_batch());
    });
    let _metadata = server.mock(|when, then| {
        when.method(GET).path("/api/categories/metadata");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_category_metadata());
    });
    let _categories = server.mock(|when, then| {
        when.method(GET).path("/api/batches/batch-1/categories");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_batch_categories());
    });
    let _stories = server.mock(|when, then| {
        when.method(GET)
            .path("/api/batches/batch-1/categories/category-1/stories");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(news_stories());
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);

    // Send a search tool call (will fail) followed by a news tool call (should succeed).
    let failing_request = mcp_request(
        json!(1),
        "tools/call",
        json!({
            "name": "kagi_search",
            "arguments": { "query": "test" }
        }),
    );
    let succeeding_request = mcp_request(
        json!(2),
        "tools/call",
        json!({
            "name": "kagi_news",
            "arguments": { "category": "tech", "lang": "en", "limit": 3 }
        }),
    );

    let stdin = format!(
        "{}\n{}\n",
        serde_json::to_string(&failing_request).unwrap(),
        serde_json::to_string(&succeeding_request).unwrap(),
    );

    let output = run_kagi_with_stdin(&["mcp"], &stdin, &env_refs(&env), tempdir.path());

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let responses: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is valid JSON"))
        .collect();

    assert_eq!(responses.len(), 2, "expected two JSON-RPC responses");

    // First response: the failed tool call should be an MCP tool error result, not a crash.
    let error_resp = &responses[0];
    assert_eq!(error_resp["id"], 1);
    assert!(
        error_resp.get("result").is_some(),
        "expected MCP tool result for failed tool call, got: {error_resp}"
    );
    assert_eq!(error_resp["result"]["resultType"], "complete");
    assert_eq!(error_resp["result"]["isError"], true);
    assert!(
        !error_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .is_empty(),
        "error message should be non-empty"
    );

    // Second response: the server stayed alive and processed the next request.
    let success_resp = &responses[1];
    assert_eq!(success_resp["id"], 2);
    assert!(
        success_resp.get("result").is_some(),
        "expected successful result for second tool call, got: {success_resp}"
    );
}
