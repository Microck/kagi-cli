from __future__ import annotations

import re
import subprocess
from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count != 1:
        raise RuntimeError(
            f"expected exactly one match in {path}, found {count} for {old[:160]!r}"
        )
    file.write_text(text.replace(old, new, 1))


def insert_once(path: str, marker: str, insertion: str) -> None:
    replace_once(path, marker, insertion + marker)


def upstream_file(path: str) -> str:
    return subprocess.check_output(
        ["git", "show", f"upstream/main:{path}"], text=True
    )


# Resolve the expected changelog conflict against current upstream main, then
# place the entry under the repository's shared numeric release heading.
cargo_toml = Path("Cargo.toml").read_text()
version_match = re.search(r'^version\s*=\s*"([^"]+)"', cargo_toml, re.MULTILINE)
if version_match is None:
    raise RuntimeError("could not determine package version from Cargo.toml")
version = version_match.group(1)
changelog = upstream_file("CHANGELOG.md")
release_heading = f"## [{version}]\n\n"
if changelog.count(release_heading) != 1:
    raise RuntimeError(
        f"expected exactly one {release_heading.strip()!r} heading in CHANGELOG.md"
    )
changelog_entry = (
    "### Added\n\n"
    "- `kagi mcp` now auto-negotiates the wire protocol per request: only "
    "`params._meta[\"io.modelcontextprotocol/protocolVersion\"]` selects the "
    "draft `2026-07-28` protocol with `server/discover` and cache hints; requests "
    "without that namespaced selector remain on the stable MCP specification, "
    "including requests whose `_meta` contains unrelated fields such as "
    "`progressToken`.\n\n"
)
Path("CHANGELOG.md").write_text(
    changelog.replace(release_heading, release_heading + changelog_entry, 1)
)


# Make protocol selection and stable request validation explicit before dispatch.
replace_once(
    "src/main.rs",
    '''        let speaks_draft = request_speaks_draft(&request);
        let draft_validation = if speaks_draft {
            validate_mcp_request(&request)
        } else {
            Ok(())
        };

        let response = match draft_validation {
''',
    '''        let speaks_draft = request_speaks_draft(&request);
        let request_validation = if speaks_draft {
            validate_mcp_request(&request)
        } else {
            validate_mcp_stable_request(&request, method, &config)
        };

        let response = match request_validation {
''',
)
replace_once(
    "src/main.rs",
    '                "initialize" => serde_json::json!({\n',
    '                "initialize" if !speaks_draft => serde_json::json!({\n',
)
replace_once(
    "src/main.rs",
    '                "ping" => serde_json::json!({\n',
    '                "ping" if !speaks_draft => serde_json::json!({\n',
)

# Add API documentation to the new helpers and centralize validation.
replace_once(
    "src/main.rs",
    "async fn run_mcp(args: McpArgs, profile: Option<&str>) -> Result<(), KagiError> {\n",
    "/// Runs the stdio MCP server and negotiates the protocol independently per request.\n"
    "async fn run_mcp(args: McpArgs, profile: Option<&str>) -> Result<(), KagiError> {\n",
)
replace_once(
    "src/main.rs",
    '''async fn mcp_tools_call_response(
''',
    '''/// Validates and executes one MCP tool call, returning a JSON-RPC response.
async fn mcp_tools_call_response(
''',
)
replace_once(
    "src/main.rs",
    '''fn mcp_stable_initialize_result(request: &Value) -> Value {
''',
    '''/// Builds a stable MCP initialize result using the negotiated protocol version.
fn mcp_stable_initialize_result(request: &Value) -> Value {
''',
)
replace_once(
    "src/main.rs",
    '''fn mcp_stable_tools_list_result(config: &McpServerConfig) -> Value {
''',
    '''/// Builds the stable MCP tools/list result without draft cache metadata.
fn mcp_stable_tools_list_result(config: &McpServerConfig) -> Value {
''',
)
replace_once(
    "src/main.rs",
    '''fn request_speaks_draft(request: &Value) -> bool {
''',
    '''/// Returns true only when the namespaced draft protocol selector is present.
fn request_speaks_draft(request: &Value) -> bool {
''',
)
replace_once(
    "src/main.rs",
    "fn mcp_discover_result() -> Value {\n",
    "/// Builds the draft server/discover response with cache metadata.\n"
    "fn mcp_discover_result() -> Value {\n",
)
replace_once(
    "src/main.rs",
    "fn mcp_tools_list_result(\n",
    "/// Validates and builds a draft tools/list response with cache metadata.\n"
    "fn mcp_tools_list_result(\n",
)
replace_once(
    "src/main.rs",
    "fn validate_mcp_tool_call(\n",
    "/// Validates tool name and arguments for an MCP tools/call request.\n"
    "fn validate_mcp_tool_call(\n",
)

stable_validation = '''/// Validates the common JSON-RPC envelope shared by both MCP protocol eras.
fn validate_mcp_json_rpc_request(request: &Value) -> Result<(), McpProtocolError> {
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(McpProtocolError {
            code: -32600,
            message: "Invalid Request: jsonrpc must be \\"2.0\\"".to_string(),
            data: None,
        });
    }

    Ok(())
}

/// Validates a stable MCP request before method dispatch.
fn validate_mcp_stable_request(
    request: &Value,
    method: &str,
    config: &McpServerConfig,
) -> Result<(), McpProtocolError> {
    validate_mcp_json_rpc_request(request)?;

    match method {
        "initialize" => validate_mcp_stable_initialize(request),
        "ping" => validate_mcp_optional_params_object(request),
        "tools/list" => {
            validate_mcp_optional_params_object(request)?;
            if request
                .get("params")
                .and_then(Value::as_object)
                .and_then(|params| params.get("cursor"))
                .is_some_and(|cursor| !cursor.is_null())
            {
                return Err(McpProtocolError::invalid_params(
                    "MCP tools/list does not issue pagination cursors because the complete tool catalog fits in one response",
                ));
            }
            Ok(())
        }
        "tools/call" => validate_mcp_tool_call(request, config),
        _ => Ok(()),
    }
}

/// Validates the required parameters of a stable MCP initialize request.
fn validate_mcp_stable_initialize(request: &Value) -> Result<(), McpProtocolError> {
    let params = request
        .get("params")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            McpProtocolError::invalid_params("MCP initialize params must be an object")
        })?;

    if !params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .is_some_and(|version| !version.trim().is_empty())
    {
        return Err(McpProtocolError::invalid_params(
            "MCP initialize requires a non-empty protocolVersion",
        ));
    }

    if !params.get("capabilities").is_some_and(Value::is_object) {
        return Err(McpProtocolError::invalid_params(
            "MCP initialize requires capabilities to be an object",
        ));
    }

    let client_info = params
        .get("clientInfo")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            McpProtocolError::invalid_params("MCP initialize requires clientInfo to be an object")
        })?;
    for field in ["name", "version"] {
        if !client_info
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(McpProtocolError::invalid_params(format!(
                "MCP initialize clientInfo.{field} is required and must be non-empty"
            )));
        }
    }

    Ok(())
}

/// Ensures optional stable MCP params are objects when supplied.
fn validate_mcp_optional_params_object(request: &Value) -> Result<(), McpProtocolError> {
    if request.get("params").is_some_and(|params| !params.is_object()) {
        return Err(McpProtocolError::invalid_params(
            "MCP request params must be an object",
        ));
    }
    Ok(())
}

'''
insert_once("src/main.rs", "fn validate_mcp_request(request: &Value)", stable_validation)
replace_once(
    "src/main.rs",
    '''fn validate_mcp_request(request: &Value) -> Result<(), McpProtocolError> {
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(McpProtocolError {
            code: -32600,
            message: "Invalid Request: jsonrpc must be \\"2.0\\"".to_string(),
            data: None,
        });
    }

''',
    '''/// Validates the stateless draft MCP envelope and its namespaced metadata.
fn validate_mcp_request(request: &Value) -> Result<(), McpProtocolError> {
    validate_mcp_json_rpc_request(request)?;

''',
)
replace_once(
    "src/main.rs",
    '''const MCP_STABLE_LATEST_VERSION: &str = "2025-11-25";
''',
    '''/// Latest stable MCP version used when a requested version is unsupported.
const MCP_STABLE_LATEST_VERSION: &str = "2025-11-25";
''',
)


# Clarify per-request protocol selection throughout the command reference.
replace_once(
    "docs/commands/mcp.mdx",
    '''Run a stdio MCP server that exposes Kagi tools for agents. The server
auto-negotiates the wire protocol per request: it speaks the stateless draft
`2026-07-28` protocol with per-request metadata, and also answers the stable
MCP specification's `initialize` handshake, `ping`, `tools/list`, and
`tools/call` for clients that do not support the draft.
''',
    '''Run a stdio MCP server that exposes Kagi tools for agents. The server
selects the wire protocol independently for every request. A request carrying
`params._meta["io.modelcontextprotocol/protocolVersion"]` uses the stateless
draft `2026-07-28` protocol; every other request uses the stable MCP protocol,
even when `_meta` contains unrelated fields such as `progressToken`.
''',
)
replace_once(
    "docs/commands/mcp.mdx",
    '''The installed entry runs plain `kagi mcp`, which auto-negotiates the protocol with each client.
''',
    '''The installed entry runs plain `kagi mcp`, which auto-negotiates the protocol for every request.
''',
)
replace_once(
    "docs/commands/mcp.mdx",
    '''Requests without the draft `_meta` key — including `initialize` — speak the
stable MCP specification.
''',
    '''Requests without the namespaced draft protocol-version selector — including
`initialize` and requests with only `_meta.progressToken` — speak the stable MCP
specification.
''',
)


# Document helpers and add regression coverage for the review findings.
replace_once(
    "tests/integration-cli.rs",
    "#[test]\nfn mcp_requires_modern_request_metadata() {\n",
    "/// Confirms draft requests require client capabilities metadata.\n#[test]\nfn mcp_requires_modern_request_metadata() {\n",
)
replace_once(
    "tests/integration-cli.rs",
    '''fn mcp_stable_request(id: Value, method: &str, params: Value) -> Value {
''',
    '''/// Builds a stable MCP JSON-RPC request without the draft selector.
fn mcp_stable_request(id: Value, method: &str, params: Value) -> Value {
''',
)
replace_once(
    "tests/integration-cli.rs",
    '''fn mcp_responses(stdout: &[u8]) -> Vec<Value> {
''',
    '''/// Parses newline-delimited MCP JSON-RPC responses.
fn mcp_responses(stdout: &[u8]) -> Vec<Value> {
''',
)
for test_name, doc in [
    ("mcp_auto_answers_initialize_ping_and_tools_list", "Exercises the stable lifecycle and tool-list surface."),
    ("mcp_auto_falls_back_to_latest_supported_version", "Confirms unsupported stable versions negotiate to the latest supported version."),
    ("mcp_auto_answers_initialize_without_draft_metadata", "Guards the stable initialize compatibility regression."),
    ("mcp_auto_unknown_tool_without_draft_metadata_returns_json_rpc_error", "Confirms stable unknown tools remain JSON-RPC invalid-parameter errors."),
    ("mcp_auto_negotiates_per_request_in_one_session", "Confirms draft and stable requests can be interleaved in one process."),
]:
    replace_once(
        "tests/integration-cli.rs",
        f"#[test]\nfn {test_name}() {{\n",
        f"/// {doc}\n#[test]\nfn {test_name}() {{\n",
    )

new_tests = r'''/// Rejects malformed JSON-RPC envelopes on the stable path.
#[test]
fn mcp_stable_rejects_non_2_0_json_rpc() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = json!({
        "jsonrpc": "1.0",
        "id": 1,
        "method": "ping",
        "params": {}
    });
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
    assert_eq!(response["error"]["code"], -32600);
}

/// Rejects stable initialize calls that omit required MCP fields.
#[test]
fn mcp_stable_initialize_requires_protocol_capabilities_and_client_info() {
    let tempdir = TempDir::new().expect("tempdir");
    let requests = [
        mcp_stable_request(json!(1), "initialize", json!({})),
        mcp_stable_request(
            json!(2),
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "clientInfo": { "name": "test", "version": "1.0" }
            }),
        ),
        mcp_stable_request(
            json!(3),
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {}
            }),
        ),
    ];
    let stdin = requests
        .iter()
        .map(|request| serde_json::to_string(request).expect("request serializes"))
        .collect::<Vec<_>>()
        .join("\n");
    let output = run_kagi_with_stdin(&["mcp"], &format!("{stdin}\n"), &[], tempdir.path());

    assert_success(&output);
    let responses = mcp_responses(&output.stdout);
    assert_eq!(responses.len(), 3);
    for response in responses {
        assert_eq!(response["error"]["code"], -32602, "{response:?}");
    }
}

/// Keeps unrelated stable metadata from selecting the draft protocol.
#[test]
fn mcp_progress_token_metadata_remains_stable() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_stable_request(
        json!(1),
        "initialize",
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1.0" },
            "_meta": { "progressToken": "progress-1" }
        }),
    );
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
    assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
}

/// Prevents draft-tagged lifecycle methods from using stable handlers.
#[test]
fn mcp_draft_requests_reject_stable_lifecycle_methods() {
    let tempdir = TempDir::new().expect("tempdir");
    let requests = [
        mcp_request(
            json!(1),
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0" }
            }),
        ),
        mcp_request(json!(2), "ping", json!({})),
    ];
    let stdin = requests
        .iter()
        .map(|request| serde_json::to_string(request).expect("request serializes"))
        .collect::<Vec<_>>()
        .join("\n");
    let output = run_kagi_with_stdin(&["mcp"], &format!("{stdin}\n"), &[], tempdir.path());

    assert_success(&output);
    let responses = mcp_responses(&output.stdout);
    assert_eq!(responses.len(), 2);
    for response in responses {
        assert_eq!(response["error"]["code"], -32601, "{response:?}");
    }
}

/// Applies tools/list cursor validation to stable requests too.
#[test]
fn mcp_stable_tools_list_rejects_non_null_cursor() {
    let tempdir = TempDir::new().expect("tempdir");
    let request = mcp_stable_request(json!(1), "tools/list", json!({ "cursor": "next" }));
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
    assert_eq!(response["error"]["code"], -32602);
}

'''
insert_once(
    "tests/integration-cli.rs",
    "/// Confirms stable unknown tools remain JSON-RPC invalid-parameter errors.\n#[test]\nfn mcp_auto_unknown_tool_without_draft_metadata_returns_json_rpc_error()",
    new_tests,
)
