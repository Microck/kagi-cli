use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

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
        "KAGI_CACHE_DIR",
    ] {
        command.env_remove(key);
    }

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
        "KAGI_API_TOKEN",
        "KAGI_SESSION_TOKEN",
        "KAGI_BASE_URL",
        "KAGI_NEWS_BASE_URL",
        "KAGI_TRANSLATE_BASE_URL",
        "KAGI_CACHE_DIR",
    ] {
        command.env_remove(key);
    }

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

fn session_env(server: &MockServer) -> Vec<(&'static str, String)> {
    vec![
        ("KAGI_SESSION_TOKEN", "test-session".to_string()),
        ("KAGI_BASE_URL", server.base_url()),
    ]
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
        "totalStories": 1,
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
fn search_command_limit_truncates_results() {
    let server = MockServer::start();
    let _search = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v0/search")
            .query_param("q", "rust")
            .header("authorization", "Bot test-api-token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "meta": api_meta(),
                "data": [
                    { "t": 0, "url": "https://example.com/a", "title": "A", "snippet": "first" },
                    { "t": 0, "url": "https://example.com/b", "title": "B", "snippet": "second" },
                    { "t": 0, "url": "https://example.com/c", "title": "C", "snippet": "third" }
                ]
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
    assert!(stderr.contains("1 batch query failed"));
    assert!(stderr.contains("1 succeeded"));
    assert!(stderr.contains("broken: authentication error"));
    assert!(stderr.contains("Insufficient credit"));
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
fn batch_command_reads_queries_from_stdin() {
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

#[test]
fn mcp_tools_list_includes_news() {
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
    assert!(
        tools.iter().any(|tool| tool["name"] == "kagi_news"),
        "expected kagi_news in tools list, got {tools:?}"
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
