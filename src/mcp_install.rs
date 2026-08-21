use crate::cli::{McpClient, McpSetupArgs};
use crate::error::KagiError;
use jsonc_parser::ParseOptions;
use serde_json::{Map, Value, json};
use std::env;
use std::fs;
use std::io;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

#[derive(Debug, Clone)]
struct SetupResult {
    target: McpClient,
    success: bool,
    detail: String,
}

pub fn run_mcp_setup(args: McpSetupArgs) -> Result<(), KagiError> {
    let targets = resolve_targets(&args)?;
    let kagi_path = args
        .kagi_path
        .clone()
        .map(Ok)
        .unwrap_or_else(current_kagi_exe)?;

    let mut results = Vec::new();
    for target in targets {
        match install_target(target, &args.server_name, &kagi_path, args.dry_run) {
            Ok(detail) => results.push(SetupResult {
                target,
                success: true,
                detail,
            }),
            Err(error) => results.push(SetupResult {
                target,
                success: false,
                detail: error.to_string(),
            }),
        }
    }

    print_setup_results(&results, args.dry_run);
    let failure_count = results.iter().filter(|result| !result.success).count();
    if failure_count > 0 {
        return Err(KagiError::Config(format!(
            "{failure_count} MCP setup target(s) failed"
        )));
    }

    Ok(())
}

pub fn offer_mcp_setup_after_auth() -> Result<(), KagiError> {
    if !supports_interactive_mcp_setup() {
        return Ok(());
    }

    let install = prompt_result(
        cliclack::confirm("Install the Kagi MCP server for an AI agent now?")
            .initial_value(true)
            .interact(),
    )?;

    if !install.unwrap_or(false) {
        return Ok(());
    }

    run_mcp_setup(McpSetupArgs {
        targets: Vec::new(),
        all: false,
        server_name: "kagi-mcp".to_string(),
        kagi_path: None,
        dry_run: false,
    })
}

fn resolve_targets(args: &McpSetupArgs) -> Result<Vec<McpClient>, KagiError> {
    if args.all {
        return Ok(all_supported_targets());
    }

    if !args.targets.is_empty() {
        return Ok(args.targets.clone());
    }

    if supports_interactive_mcp_setup() {
        let selected = prompt_result(
            cliclack::multiselect("Choose AI agents to configure")
                .item(
                    McpClient::ClaudeCode,
                    "Claude Code",
                    "uses `claude mcp add --scope user`",
                )
                .item(McpClient::Codex, "Codex CLI", "writes ~/.codex/config.toml")
                .item(McpClient::Cursor, "Cursor", "writes ~/.cursor/mcp.json")
                .item(
                    McpClient::VsCode,
                    "VS Code Copilot",
                    "writes the user mcp.json",
                )
                .item(
                    McpClient::Windsurf,
                    "Windsurf",
                    "writes ~/.codeium/mcp_config.json",
                )
                .item(
                    McpClient::Gemini,
                    "Gemini CLI",
                    "writes ~/.gemini/settings.json",
                )
                .item(
                    McpClient::Opencode,
                    "OpenCode",
                    "writes ~/.config/opencode/opencode.json",
                )
                .item(McpClient::Cline, "Cline CLI", "writes ~/.cline/mcp.json")
                .item(
                    McpClient::RooCode,
                    "Roo Code",
                    "writes the VS Code extension MCP settings",
                )
                .item(McpClient::Droid, "Droid", "writes ~/.factory/mcp.json")
                .item(
                    McpClient::Antigravity,
                    "Antigravity CLI",
                    "writes ~/.gemini/antigravity-cli/mcp_config.json",
                )
                .item(
                    McpClient::ClaudeDesktop,
                    "Claude Desktop",
                    "writes the desktop MCP JSON config where supported",
                )
                .interact(),
        )?;

        return selected.ok_or_else(|| {
            KagiError::Config("MCP setup canceled. No changes were made.".to_string())
        });
    }

    Err(KagiError::Config(
        "MCP setup needs an interactive terminal or at least one --target. Try `kagi mcp install --target codex`, repeat --target for more clients, or use --all"
            .to_string(),
    ))
}

fn all_supported_targets() -> Vec<McpClient> {
    vec![
        McpClient::ClaudeCode,
        McpClient::Codex,
        McpClient::Cursor,
        McpClient::VsCode,
        McpClient::Windsurf,
        McpClient::Gemini,
        McpClient::Opencode,
        McpClient::Cline,
        McpClient::RooCode,
        McpClient::Droid,
        McpClient::Antigravity,
        McpClient::ClaudeDesktop,
    ]
}

fn install_target(
    target: McpClient,
    server_name: &str,
    kagi_path: &Path,
    dry_run: bool,
) -> Result<String, KagiError> {
    match target {
        McpClient::ClaudeCode => install_claude_code(server_name, kagi_path, dry_run),
        McpClient::Codex => write_codex_config(server_name, kagi_path, dry_run),
        McpClient::VsCode => write_mcp_servers_config(
            vscode_user_mcp_config_path(),
            JsonFlavor::Json,
            "servers",
            server_name,
            stdio_server_entry(kagi_path),
            dry_run,
        ),
        McpClient::Cursor => write_mcp_servers_config(
            home_dir().join(".cursor").join("mcp.json"),
            JsonFlavor::Json,
            "mcpServers",
            server_name,
            stdio_server_entry(kagi_path),
            dry_run,
        ),
        McpClient::Windsurf => write_mcp_servers_config(
            home_dir().join(".codeium").join("mcp_config.json"),
            JsonFlavor::Json,
            "mcpServers",
            server_name,
            stdio_server_entry(kagi_path),
            dry_run,
        ),
        McpClient::Gemini => write_mcp_servers_config(
            home_dir().join(".gemini").join("settings.json"),
            JsonFlavor::Json,
            "mcpServers",
            server_name,
            stdio_server_entry(kagi_path),
            dry_run,
        ),
        McpClient::Opencode => write_mcp_servers_config(
            xdg_config_home().join("opencode").join("opencode.json"),
            JsonFlavor::Jsonc,
            "mcp",
            server_name,
            json!({
                "type": "local",
                "command": [path_string(kagi_path), "mcp"],
                "enabled": true
            }),
            dry_run,
        ),
        McpClient::Cline => write_mcp_servers_config(
            home_dir().join(".cline").join("mcp.json"),
            JsonFlavor::Json,
            "mcpServers",
            server_name,
            cline_stdio_server_entry(kagi_path),
            dry_run,
        ),
        McpClient::RooCode => write_mcp_servers_config(
            roo_code_config_path()?,
            JsonFlavor::Json,
            "mcpServers",
            server_name,
            cline_stdio_server_entry(kagi_path),
            dry_run,
        ),
        McpClient::Droid => write_mcp_servers_config(
            home_dir().join(".factory").join("mcp.json"),
            JsonFlavor::Json,
            "mcpServers",
            server_name,
            droid_stdio_server_entry(kagi_path),
            dry_run,
        ),
        McpClient::Antigravity => write_mcp_servers_config(
            home_dir()
                .join(".gemini")
                .join("antigravity-cli")
                .join("mcp_config.json"),
            JsonFlavor::Json,
            "mcpServers",
            server_name,
            stdio_server_entry(kagi_path),
            dry_run,
        ),
        McpClient::ClaudeDesktop => {
            let path = claude_desktop_config_path()?;
            write_mcp_servers_config(
                path,
                JsonFlavor::Json,
                "mcpServers",
                server_name,
                stdio_server_entry(kagi_path),
                dry_run,
            )
        }
    }
}

fn install_claude_code(
    server_name: &str,
    kagi_path: &Path,
    dry_run: bool,
) -> Result<String, KagiError> {
    let args = vec![
        "mcp".to_string(),
        "add".to_string(),
        "--scope".to_string(),
        "user".to_string(),
        server_name.to_string(),
        "--".to_string(),
        path_string(kagi_path),
        "mcp".to_string(),
    ];

    if dry_run || find_on_path("claude").is_some() {
        return run_client_command("claude", args, dry_run);
    }

    write_mcp_servers_config(
        home_dir().join(".claude.json"),
        JsonFlavor::Json,
        "mcpServers",
        server_name,
        stdio_server_entry(kagi_path),
        false,
    )
}

fn run_client_command(
    program: &str,
    args: Vec<String>,
    dry_run: bool,
) -> Result<String, KagiError> {
    let rendered = format!("{} {}", program, args.join(" "));
    if dry_run {
        return Ok(rendered);
    }

    if find_on_path(program).is_none() {
        return Err(KagiError::Config(format!(
            "`{program}` was not found on PATH. Install that client or choose another --target"
        )));
    }

    let output = ProcessCommand::new(program)
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| KagiError::Config(format!("failed to run `{program}`: {error}")))?;

    if output.status.success() {
        return Ok(rendered);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(KagiError::Config(format!(
        "`{rendered}` failed with status {:?}: {}",
        output.status.code(),
        stderr.trim()
    )))
}

#[derive(Debug, Clone, Copy)]
enum JsonFlavor {
    Json,
    Jsonc,
}

fn write_mcp_servers_config(
    path: PathBuf,
    flavor: JsonFlavor,
    root_key: &str,
    server_name: &str,
    server_entry: Value,
    dry_run: bool,
) -> Result<String, KagiError> {
    let mut config = read_json_object(&path, flavor)?;
    let servers = object_entry(&mut config, root_key)?;
    servers.insert(server_name.to_string(), server_entry);

    if dry_run {
        return Ok(format!("write {}", path.display()));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            KagiError::Config(format!("failed to create {}: {error}", parent.display()))
        })?;
    }

    let raw = serde_json::to_string_pretty(&Value::Object(config))?;
    fs::write(&path, format!("{raw}\n")).map_err(|error| {
        KagiError::Config(format!("failed to write {}: {error}", path.display()))
    })?;

    Ok(format!("wrote {}", path.display()))
}

fn read_json_object(path: &Path, flavor: JsonFlavor) -> Result<Map<String, Value>, KagiError> {
    if !path.exists() {
        return Ok(Map::new());
    }

    let raw = fs::read_to_string(path).map_err(|error| {
        KagiError::Config(format!("failed to read {}: {error}", path.display()))
    })?;
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }

    let value = match flavor {
        JsonFlavor::Json => serde_json::from_str::<Value>(&raw).map_err(KagiError::from)?,
        JsonFlavor::Jsonc => {
            jsonc_parser::parse_to_serde_value::<Value>(&raw, &ParseOptions::default()).map_err(
                |error| KagiError::Config(format!("failed to parse {}: {error}", path.display())),
            )?
        }
    };

    match value {
        Value::Object(object) => Ok(object),
        _ => Err(KagiError::Config(format!(
            "{} must contain a JSON object",
            path.display()
        ))),
    }
}

fn object_entry<'a>(
    config: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>, KagiError> {
    let value = config
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));

    value.as_object_mut().ok_or_else(|| {
        KagiError::Config(format!(
            "cannot update `{key}` because it is not a JSON object"
        ))
    })
}

fn stdio_server_entry(kagi_path: &Path) -> Value {
    json!({
        "command": path_string(kagi_path),
        "args": ["mcp"]
    })
}

fn cline_stdio_server_entry(kagi_path: &Path) -> Value {
    json!({
        "command": path_string(kagi_path),
        "args": ["mcp"],
        "disabled": false,
        "autoApprove": []
    })
}

fn droid_stdio_server_entry(kagi_path: &Path) -> Value {
    json!({
        "type": "stdio",
        "command": path_string(kagi_path),
        "args": ["mcp"],
        "disabled": false
    })
}

fn write_codex_config(
    server_name: &str,
    kagi_path: &Path,
    dry_run: bool,
) -> Result<String, KagiError> {
    let path = home_dir().join(".codex").join("config.toml");
    let mut config = read_toml_table(&path)?;
    let mcp_servers = toml_table_entry(&mut config, "mcp_servers")?;
    let server = toml_table_entry(mcp_servers, server_name)?;
    server.insert(
        "command".to_string(),
        toml::Value::String(path_string(kagi_path)),
    );
    server.insert(
        "args".to_string(),
        toml::Value::Array(vec![toml::Value::String("mcp".to_string())]),
    );

    if dry_run {
        return Ok(format!("write {}", path.display()));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            KagiError::Config(format!("failed to create {}: {error}", parent.display()))
        })?;
    }

    let raw = toml::to_string_pretty(&config).map_err(|error| {
        KagiError::Config(format!("failed to serialize {}: {error}", path.display()))
    })?;
    fs::write(&path, raw).map_err(|error| {
        KagiError::Config(format!("failed to write {}: {error}", path.display()))
    })?;
    Ok(format!("wrote {}", path.display()))
}

fn read_toml_table(path: &Path) -> Result<toml::map::Map<String, toml::Value>, KagiError> {
    if !path.exists() {
        return Ok(toml::map::Map::new());
    }

    let raw = fs::read_to_string(path).map_err(|error| {
        KagiError::Config(format!("failed to read {}: {error}", path.display()))
    })?;
    if raw.trim().is_empty() {
        return Ok(toml::map::Map::new());
    }

    match toml::from_str::<toml::Value>(&raw).map_err(|error| {
        KagiError::Config(format!("failed to parse {}: {error}", path.display()))
    })? {
        toml::Value::Table(table) => Ok(table),
        _ => Err(KagiError::Config(format!(
            "{} must contain a TOML table",
            path.display()
        ))),
    }
}

fn toml_table_entry<'a>(
    table: &'a mut toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<&'a mut toml::map::Map<String, toml::Value>, KagiError> {
    let value = table
        .entry(key.to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));

    value.as_table_mut().ok_or_else(|| {
        KagiError::Config(format!(
            "cannot update `{key}` because it is not a TOML table"
        ))
    })
}

fn current_kagi_exe() -> Result<PathBuf, KagiError> {
    env::current_exe()
        .map_err(|error| KagiError::Config(format!("failed to resolve current kagi path: {error}")))
}

fn vscode_user_mcp_config_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home_dir()
            .join("Library")
            .join("Application Support")
            .join("Code")
            .join("User")
            .join("mcp.json")
    }

    #[cfg(target_os = "windows")]
    {
        return env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(home_dir)
            .join("Code")
            .join("User")
            .join("mcp.json");
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        xdg_config_home().join("Code").join("User").join("mcp.json")
    }
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn claude_desktop_config_path() -> Result<PathBuf, KagiError> {
    #[cfg(target_os = "macos")]
    {
        Ok(home_dir()
            .join("Library")
            .join("Application Support")
            .join("Claude")
            .join("claude_desktop_config.json"))
    }

    #[cfg(target_os = "windows")]
    {
        let appdata = env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| KagiError::Config("APPDATA is not set".to_string()))?;
        return Ok(appdata.join("Claude").join("claude_desktop_config.json"));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(KagiError::Config(
            "Claude Desktop MCP config is only documented for macOS and Windows".to_string(),
        ))
    }
}

fn roo_code_config_path() -> Result<PathBuf, KagiError> {
    let candidates = roo_code_config_candidates()?;
    if let Some(existing) = candidates.iter().find(|path| path.exists()) {
        return Ok(existing.clone());
    }

    candidates.into_iter().next().ok_or_else(|| {
        KagiError::Config("could not resolve a Roo Code MCP settings path".to_string())
    })
}

fn roo_code_config_candidates() -> Result<Vec<PathBuf>, KagiError> {
    let relative = PathBuf::from("User")
        .join("globalStorage")
        .join("rooveterinaryinc.roo-cline")
        .join("settings")
        .join("cline_mcp_settings.json");

    #[cfg(target_os = "macos")]
    {
        let root = home_dir().join("Library").join("Application Support");
        Ok(["Code", "Cursor", "Windsurf", "VSCodium"]
            .iter()
            .map(|name| root.join(name).join(&relative))
            .collect())
    }

    #[cfg(target_os = "windows")]
    {
        let appdata = env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| KagiError::Config("APPDATA is not set".to_string()))?;
        return Ok(["Code", "Cursor", "Windsurf", "VSCodium"]
            .iter()
            .map(|name| appdata.join(name).join(&relative))
            .collect());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let root = xdg_config_home();
        Ok(["Code", "Cursor", "Windsurf", "VSCodium"]
            .iter()
            .map(|name| root.join(name).join(&relative))
            .collect())
    }
}

fn print_setup_results(results: &[SetupResult], dry_run: bool) {
    let heading = if dry_run {
        "MCP setup plan"
    } else {
        "MCP setup complete"
    };
    println!("{heading}:");
    for result in results {
        let status = if result.success { "ok" } else { "failed" };
        println!(
            "- {} ({status}): {}",
            target_label(result.target),
            result.detail
        );
    }
}

fn target_label(target: McpClient) -> &'static str {
    match target {
        McpClient::ClaudeCode => "Claude Code",
        McpClient::ClaudeDesktop => "Claude Desktop",
        McpClient::Codex => "Codex CLI",
        McpClient::Cursor => "Cursor",
        McpClient::VsCode => "VS Code",
        McpClient::Windsurf => "Windsurf",
        McpClient::Gemini => "Gemini CLI",
        McpClient::Opencode => "OpenCode",
        McpClient::Cline => "Cline CLI",
        McpClient::RooCode => "Roo Code",
        McpClient::Droid => "Droid",
        McpClient::Antigravity => "Antigravity CLI",
    }
}

fn supports_interactive_mcp_setup() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal() && io::stderr().is_terminal()
}

fn prompt_result<T>(result: io::Result<T>) -> Result<Option<T>, KagiError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(None),
        Err(error) => Err(KagiError::Config(format!(
            "interactive MCP setup prompt failed: {error}"
        ))),
    }
}

fn find_on_path(program: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|path| path.join(program))
        .find(|candidate| candidate.is_file())
}

fn xdg_config_home() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| home_dir().join(".config"))
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."))
}
