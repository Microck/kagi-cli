use std::path::Path;
use std::process::{Command, Output};

use httpmock::Method::{GET, POST};
use httpmock::MockServer;
use serde_json::{Value, json};
use tempfile::TempDir;

const API_TOKEN: &str = "test-api-token";

fn run_kagi(args: &[&str], envs: &[(&str, &str)], cwd: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kagi"));
    command.args(args).current_dir(cwd);

    for key in [
        "KAGI_API_TOKEN",
        "KAGI_SESSION_TOKEN",
        "KAGI_BASE_URL",
        "KAGI_NEWS_BASE_URL",
        "KAGI_TRANSLATE_BASE_URL",
    ] {
        command.env_remove(key);
    }

    for (key, value) in envs {
        command.env(key, value);
    }

    command.output().expect("command should run")
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

fn test_env(server: &MockServer) -> Vec<(&'static str, String)> {
    vec![
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

fn api_meta() -> Value {
    json!({
        "id": "req-1",
        "node": "test",
        "ms": 12
    })
}

fn search_payload(title: &str, url: &str, snippet: &str) -> Value {
    json!({
        "meta": api_meta(),
        "data": [
            {
                "t": 0,
                "url": url,
                "title": title,
                "snippet": snippet
            }
        ]
    })
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
        "totalStories": "1",
        "domains": [],
        "readCount": 10
    })
}

#[test]
fn search_command_returns_json_from_mock_api() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v0/search")
            .query_param("q", "rust programming")
            .header("authorization", "Bot test-api-token");
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
fn search_command_pretty_format_prints_ranked_results() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v0/search")
            .query_param("q", "rust programming")
            .header("authorization", "Bot test-api-token");
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
fn batch_command_returns_queries_and_results() {
    let server = MockServer::start();
    let _rust = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v0/search")
            .query_param("q", "rust")
            .header("authorization", "Bot test-api-token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust",
                "https://www.rust-lang.org",
                "Rust homepage.",
            ));
    });
    let _zig = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v0/search")
            .query_param("q", "zig")
            .header("authorization", "Bot test-api-token");
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
        when.method(GET)
            .path("/api/v0/search")
            .query_param("q", "rust")
            .header("authorization", "Bot test-api-token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(search_payload(
                "Rust",
                "https://www.rust-lang.org",
                "Rust homepage.",
            ));
    });
    let _fail = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v0/search")
            .query_param("q", "broken")
            .header("authorization", "Bot test-api-token");
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
    assert!(stderr.contains("One or more batch queries failed"));
}

#[test]
fn auth_check_validates_credentials_without_live_network() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v0/search")
            .query_param("q", "rust lang")
            .header("authorization", "Bot test-api-token");
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
    assert!(stdout.contains("auth check passed: api-token (env)"));
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
