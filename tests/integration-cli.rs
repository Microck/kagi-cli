use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use httpmock::Method::{GET, POST};
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
        "KAGI_NEWS_BASE_URL",
        "KAGI_TRANSLATE_BASE_URL",
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
        "KAGI_NEWS_BASE_URL",
        "KAGI_TRANSLATE_BASE_URL",
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
        stdout.contains("kagi skills get kagi"),
        "expected help to mention kagi skills get kagi, got:\n{stdout}"
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
    let output = run_kagi(&["skills", "get", "kagi"], &[], tempdir.path());

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("# Kagi CLI"),
        "expected markdown skill guide, got:\n{stdout}"
    );
    assert!(
        stdout.contains("kagi skills get kagi"),
        "expected skills command guidance, got:\n{stdout}"
    );
}

#[test]
fn skills_list_and_path_are_auth_free() {
    let tempdir = TempDir::new().expect("tempdir");

    let list = run_kagi(&["skills"], &[], tempdir.path());
    assert_success(&list);
    assert!(
        String::from_utf8_lossy(&list.stdout).contains("kagi                 Core CLI usage guide"),
        "expected core skill listing, got:\n{}",
        String::from_utf8_lossy(&list.stdout)
    );

    let path = run_kagi(&["skills", "path"], &[], tempdir.path());
    assert_success(&path);
    assert_eq!(
        String::from_utf8_lossy(&path.stdout).trim(),
        "embedded://skills"
    );

    let skill_path = run_kagi(&["skills", "path", "kagi"], &[], tempdir.path());
    assert_success(&skill_path);
    assert_eq!(
        String::from_utf8_lossy(&skill_path.stdout).trim(),
        "embedded://skills/kagi"
    );
}

#[test]
fn skills_get_full_prints_body_without_frontmatter() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi(&["skills", "get", "kagi", "--full"], &[], tempdir.path());

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

fn session_env(server: &MockServer) -> Vec<(&'static str, String)> {
    vec![
        ("KAGI_SESSION_TOKEN", "test-session".to_string()),
        ("KAGI_BASE_URL", server.base_url()),
    ]
}

#[test]
fn assistant_prompt_stream_reads_query_from_stdin() {
    let server = MockServer::start();
    let prompt = server.mock(|when, then| {
        when.method(POST)
            .path("/assistant/prompt")
            .header("cookie", "kagi_session=test-session")
            .header("accept", "application/vnd.kagi.stream")
            .json_body(json!({
                "focus": {
                    "thread_id": null,
                    "branch_id": "00000000-0000-4000-0000-000000000000",
                    "prompt": "do a little dance",
                    "message_id": null,
                },
                "profile": {},
            }));
        then.status(200)
            .header("content-type", "application/vnd.kagi.stream")
            .body(concat!(
                "hi:{\"v\":\"test\",\"trace\":\"trace-stdin\"}\0\n",
                "thread.json:{\"id\":\"thread-stdin\",\"title\":\"Stdin test\",\"ack\":\"2026-06-07T00:00:00Z\",\"created_at\":\"2026-06-07T00:00:00Z\",\"saved\":false,\"shared\":false,\"branch_id\":\"00000000-0000-4000-0000-000000000000\",\"tag_ids\":[]}\0\n",
                "new_message.json:{\"id\":\"msg-stdin\",\"thread_id\":\"thread-stdin\",\"created_at\":\"2026-06-07T00:00:00Z\",\"state\":\"streaming\",\"prompt\":\"do a little dance\",\"md\":\"dance\",\"documents\":[]}\0\n",
                "new_message.json:{\"id\":\"msg-stdin\",\"thread_id\":\"thread-stdin\",\"created_at\":\"2026-06-07T00:00:00Z\",\"state\":\"done\",\"prompt\":\"do a little dance\",\"md\":\"dance-ok\",\"documents\":[]}\0\n",
            ));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi_with_stdin(
        &["assistant", "--stream"],
        "do a little dance\n",
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    prompt.assert_calls(1);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "dance-ok\n");
}

fn api_meta() -> Value {
    json!({
        "id": "req-1",
        "node": "test",
        "ms": 12
    })
}

fn assistant_form_html(profile_id: &str, name: &str) -> String {
    format!(
        r#"
        <form class="s-form" action="/settings/ast/profiles/update" method="POST">
          <input type="hidden" name="profile_id" value="{profile_id}">
          <input type="text" name="name" value="{name}">
          <input type="text" name="bang_trigger" value="">
          <input type="checkbox" name="internet_access" checked value="on">
          <input type="hidden" name="internet_access" value="false">
          <input type="radio" name="selected_lens" value="0" checked class="hidden">
          <input type="checkbox" name="personalizations" checked value="on">
          <input type="hidden" name="personalizations" value="false">
          <input type="radio" name="base_model" value="gpt-5-mini" aria-label="GPT 5 Mini" checked class="hidden">
          <input type="radio" name="base_model" value="claude-4-7-opus" aria-label="Claude Opus" class="hidden">
          <textarea name="custom_instructions"></textarea>
        </form>
        <form action="/settings/ast/profiles/delete" method="POST"></form>
        "#
    )
}

fn assistant_list_html() -> &'static str {
    r#"
    <div id="custom_mode_table">
      <ul id="items_p">
        <li class="item" id="profile-once">
          <div class="item-name">
            <a href="/assistant?profile=profile-once">Once</a>
          </div>
          <dl class="item-details">
            <div><dt>Model:</dt><dd>GPT 5 Mini</dd></div>
            <div></div>
            <div><dt>Internet Access:</dt><dd>On</dd></div>
          </dl>
          <div class="edit">
            <a href="/settings/custom_assistant?id=profile-once">Edit</a>
          </div>
        </li>
      </ul>
    </div>
    "#
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
    fs::write(
        tempdir.path().join(".kagi.toml"),
        "[auth]\npreferred_auth = \"api\"\n",
    )
    .expect("config should write");
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
    let raw = fs::read_to_string(tempdir.path().join(".kagi.toml")).expect("config should exist");
    assert!(raw.contains("api_key = \"current-key\""));
    assert!(raw.contains("api_token = \"legacy-token\""));
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
    let _first_page = server.mock(|when, then| {
        when.method(POST)
            .path("/assistant/thread_list")
            .header("cookie", "kagi_session=test-session")
            .header("accept", "application/vnd.kagi.stream")
            .header("content-type", "application/json")
            .json_body(json!({ "limit": 100 }));
        then.status(200)
            .header("content-type", "application/vnd.kagi.stream")
            .body(concat!(
                "hi:{\"v\":\"test\",\"trace\":\"trace-list\"}\0\n",
                "tags.json:[]\0\n",
                "thread_list.html:{\"html\":\"<div class=\\\"hide-if-no-threads\\\"><ul class=\\\"thread-list\\\"><li class=\\\"thread\\\" data-code=\\\"thread-1\\\" data-saved=\\\"false\\\" data-public=\\\"false\\\" data-tags='[]' data-snippet=\\\"First snippet\\\"><a href=\\\"/assistant/thread-1\\\"><div class=\\\"title\\\">First Thread</div><div class=\\\"excerpt\\\">First snippet</div></a></li></ul></div>\",\"next_cursor\":{\"ack\":\"2026-02-11T16:22:13Z\",\"created_at\":\"2026-02-11T16:22:13Z\",\"id\":\"cursor-123\"},\"has_more\":true,\"count\":1,\"total_counts\":{\"all\":2}}\0\n"
            ));
    });
    let _second_page = server.mock(|when, then| {
        when.method(POST)
            .path("/assistant/thread_list")
            .header("cookie", "kagi_session=test-session")
            .header("accept", "application/vnd.kagi.stream")
            .header("content-type", "application/json")
            .json_body(json!({
                "limit": 100,
                "cursor": {
                    "ack": "2026-02-11T16:22:13Z",
                    "created_at": "2026-02-11T16:22:13Z",
                    "id": "cursor-123"
                }
            }));
        then.status(200)
            .header("content-type", "application/vnd.kagi.stream")
            .body(concat!(
                "hi:{\"v\":\"test\",\"trace\":\"trace-list\"}\0\n",
                "tags.json:[]\0\n",
                "thread_list.html:{\"html\":\"<div class=\\\"hide-if-no-threads\\\"><ul class=\\\"thread-list\\\"><li class=\\\"thread\\\" data-code=\\\"thread-2\\\" data-saved=\\\"false\\\" data-public=\\\"false\\\" data-tags='[]' data-snippet=\\\"Second snippet\\\"><a href=\\\"/assistant/thread-2\\\"><div class=\\\"title\\\">Second Thread</div><div class=\\\"excerpt\\\">Second snippet</div></a></li></ul></div>\",\"next_cursor\":null,\"has_more\":false,\"count\":1,\"total_counts\":null}\0\n"
            ));
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
    assert_eq!(body["meta"]["trace"], "trace-list");
    assert_eq!(body["threads"][0]["id"], "thread-1");
    assert_eq!(body["threads"][1]["id"], "thread-2");
    assert_eq!(body["pagination"]["count"], 2);
    assert_eq!(body["pagination"]["total_counts"]["all"], 2);
}

#[test]
fn assistant_models_prints_json_catalog() {
    let server = MockServer::start();
    let _form = server.mock(|when, then| {
        when.method(GET)
            .path("/settings/custom_assistant")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .body(assistant_form_html("profile-once", "Once"));
    });

    let tempdir = TempDir::new().expect("tempdir");
    let env = session_env(&server);
    let output = run_kagi(&["assistant", "models"], &env_refs(&env), tempdir.path());

    assert_success(&output);
    let body: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(body["models"][0]["id"], "gpt-5-mini");
    assert_eq!(body["models"][0]["label"], "GPT 5 Mini");
    assert_eq!(body["models"][0]["selected"], true);
    assert_eq!(body["models"][1]["id"], "claude-4-7-opus");
}

#[test]
fn assistant_stream_prints_text_deltas_by_default() {
    let server = MockServer::start();
    let _prompt = server.mock(|when, then| {
        when.method(POST)
            .path("/assistant/prompt")
            .header("cookie", "kagi_session=test-session")
            .header("accept", "application/vnd.kagi.stream")
            .header("content-type", "application/json");
        then.status(200)
            .header("content-type", "application/vnd.kagi.stream")
            .body(concat!(
                "hi:{\"v\":\"test\",\"trace\":\"trace-stream\"}\0\n",
                "thread.json:{\"id\":\"thread-1\",\"title\":\"Greeting\",\"ack\":\"2026-03-16T06:19:07Z\",\"created_at\":\"2026-03-16T06:19:07Z\",\"saved\":false,\"shared\":false,\"branch_id\":\"00000000-0000-4000-0000-000000000000\",\"tag_ids\":[]}\0\n",
                "new_message.json:{\"id\":\"msg-1\",\"thread_id\":\"thread-1\",\"created_at\":\"2026-03-16T06:19:07Z\",\"state\":\"streaming\",\"prompt\":\"Hello\",\"md\":\"Hel\",\"documents\":[]}\0\n",
                "new_message.json:{\"id\":\"msg-1\",\"thread_id\":\"thread-1\",\"created_at\":\"2026-03-16T06:19:07Z\",\"state\":\"done\",\"prompt\":\"Hello\",\"md\":\"Hello\",\"documents\":[]}\0\n"
            ));
    });

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
    let _prompt = server.mock(|when, then| {
        when.method(POST)
            .path("/assistant/prompt")
            .header("cookie", "kagi_session=test-session")
            .header("accept", "application/vnd.kagi.stream")
            .header("content-type", "application/json");
        then.status(200)
            .header("content-type", "application/vnd.kagi.stream")
            .body(concat!(
                "hi:{\"v\":\"test\",\"trace\":\"trace-stream\"}\0\n",
                "thread.json:{\"id\":\"thread-1\",\"title\":\"Greeting\",\"ack\":\"2026-03-16T06:19:07Z\",\"created_at\":\"2026-03-16T06:19:07Z\",\"saved\":false,\"shared\":false,\"branch_id\":\"00000000-0000-4000-0000-000000000000\",\"tag_ids\":[]}\0\n",
                "new_message.json:{\"id\":\"msg-1\",\"thread_id\":\"thread-1\",\"created_at\":\"2026-03-16T06:19:07Z\",\"state\":\"streaming\",\"prompt\":\"Hello\",\"md\":\"Hel\",\"documents\":[]}\0\n",
                "new_message.json:{\"id\":\"msg-1\",\"thread_id\":\"thread-1\",\"created_at\":\"2026-03-16T06:19:07Z\",\"state\":\"done\",\"prompt\":\"Hello\",\"md\":\"Hello\",\"documents\":[]}\0\n"
            ));
    });

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
    let _new_form = server.mock(|when, then| {
        when.method(GET)
            .path("/settings/custom_assistant")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .body(assistant_form_html("profile-once", "Once"));
    });
    let _create = server.mock(|when, then| {
        when.method(POST)
            .path("/settings/ast/profiles/update")
            .header("cookie", "kagi_session=test-session")
            .body_includes("base_model=gpt-5-mini");
        then.status(303)
            .header("location", "/settings/custom_assistant?id=profile-once");
    });
    let _list = server.mock(|when, then| {
        when.method(GET)
            .path("/html/settings/assistant")
            .header("cookie", "kagi_session=test-session");
        then.status(200).body(assistant_list_html());
    });
    let _edit_form = server.mock(|when, then| {
        when.method(GET)
            .path("/settings/custom_assistant")
            .query_param("id", "profile-once")
            .header("cookie", "kagi_session=test-session");
        then.status(200)
            .body(assistant_form_html("profile-once", "Once"));
    });
    let _prompt = server.mock(|when, then| {
        when.method(POST)
            .path("/assistant/prompt")
            .header("cookie", "kagi_session=test-session")
            .header("accept", "application/vnd.kagi.stream");
        then.status(200)
            .header("content-type", "application/vnd.kagi.stream")
            .body(concat!(
                "hi:{\"v\":\"test\",\"trace\":\"trace-once\"}\0\n",
                "thread.json:{\"id\":\"thread-once\",\"title\":\"Once\",\"ack\":\"2026-03-16T06:19:07Z\",\"created_at\":\"2026-03-16T06:19:07Z\",\"saved\":false,\"shared\":false,\"branch_id\":\"00000000-0000-4000-0000-000000000000\",\"tag_ids\":[]}\0\n",
                "new_message.json:{\"id\":\"msg-once\",\"thread_id\":\"thread-once\",\"created_at\":\"2026-03-16T06:19:07Z\",\"state\":\"done\",\"prompt\":\"Hi\",\"md\":\"ok\",\"documents\":[]}\0\n"
            ));
    });
    let _delete = server.mock(|when, then| {
        when.method(POST)
            .path("/settings/ast/profiles/delete")
            .header("cookie", "kagi_session=test-session")
            .body_includes("profile_id=profile-once");
        then.status(200).body("");
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
fn mcp_initialize_returns_server_info() {
    let tempdir = TempDir::new().expect("tempdir");
    let output = run_kagi_with_stdin(
        &["mcp"],
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n",
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["serverInfo"]["name"], "kagi-cli");
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
    let output = run_kagi_with_stdin(
        &["mcp"],
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n",
        &[],
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    let tools = response["result"]["tools"].as_array().expect("tools array");

    for tool in tools {
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "schema type for {tool:?}");
        let properties = schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("expected schema properties for {tool:?}"));
        assert!(
            !properties.is_empty(),
            "expected non-empty schema properties for {tool:?}"
        );
    }

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
    let output = run_kagi_with_stdin(
        &["mcp"],
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kagi_extract","arguments":{"url":"https://example.com/article"}}}"#,
        &env_refs(&env),
        tempdir.path(),
    );

    assert_success(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("mcp json parses");
    assert_eq!(
        response["result"]["content"][0]["text"],
        "# Article\n\nExtracted content."
    );
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
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "kagi_news",
            "arguments": { "category": "tech", "lang": "en", "limit": 3 }
        }
    });
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
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "kagi_news_search",
            "arguments": {
                "query": "iran",
                "freshness": "day",
                "order": "recency"
            }
        }
    });
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
    let failing_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "kagi_search",
            "arguments": { "query": "test" }
        }
    });
    let succeeding_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "kagi_news",
            "arguments": { "category": "tech", "lang": "en", "limit": 3 }
        }
    });

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

    // First response: the failed tool call should be a JSON-RPC error, not a crash.
    let error_resp = &responses[0];
    assert_eq!(error_resp["id"], 1);
    assert!(
        error_resp.get("error").is_some(),
        "expected JSON-RPC error for failed tool call, got: {error_resp}"
    );
    assert_eq!(error_resp["error"]["code"], -32000);
    assert!(
        !error_resp["error"]["message"].as_str().unwrap().is_empty(),
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
