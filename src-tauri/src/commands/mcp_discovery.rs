use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::models::McpTool;

/// Get an enriched PATH that includes common binary directories.
/// On macOS, bundled .app processes don't inherit the shell's PATH,
/// so npx/node/uvx/python3 etc. won't be found without this.
fn get_enriched_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // Cross-platform common dirs
    let mut extra_dirs: Vec<String> = vec![
        format!("{}/.nvm/versions/node/current/bin", home),
        format!("{}/.volta/bin", home),
        format!("{}/.cargo/bin", home),
        format!("{}/.local/bin", home),
        format!("{}/.bun/bin", home),
    ];

    // Platform-specific dirs
    #[cfg(target_os = "macos")]
    {
        extra_dirs.extend([
            "/opt/homebrew/bin".to_string(),
            "/opt/homebrew/sbin".to_string(),
            "/usr/local/bin".to_string(),
            "/usr/local/sbin".to_string(),
            format!("{}/Library/pnpm", home),
            "/usr/bin".to_string(),
            "/bin".to_string(),
            "/usr/sbin".to_string(),
            "/sbin".to_string(),
        ]);
    }

    #[cfg(target_os = "windows")]
    {
        extra_dirs.extend([
            format!("{}\\AppData\\Local\\pnpm", home),
            format!("{}\\AppData\\Roaming\\npm", home),
            "C:\\Program Files\\nodejs".to_string(),
            format!("{}\\AppData\\Local\\Programs\\Python\\Python312", home),
            format!("{}\\AppData\\Local\\Programs\\Python\\Python311", home),
        ]);
    }

    #[cfg(target_os = "linux")]
    {
        extra_dirs.extend([
            "/usr/local/bin".to_string(),
            "/snap/bin".to_string(),
            format!("{}/.local/share/pnpm", home),
            "/usr/bin".to_string(),
            "/bin".to_string(),
        ]);
    }

    // Try to get the real shell PATH via a login shell invocation
    #[cfg(target_os = "macos")]
    let shell_path = Command::new("/bin/zsh")
        .args(["-l", "-c", "echo $PATH"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    #[cfg(target_os = "linux")]
    let shell_path = {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        Command::new(shell)
            .args(["-l", "-c", "echo $PATH"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };

    #[cfg(target_os = "windows")]
    let shell_path = String::new(); // Windows PATH is already inherited correctly

    let path_separator = if cfg!(windows) { ';' } else { ':' };
    let mut parts: Vec<&str> = Vec::new();

    // Start with shell PATH if available (most accurate)
    if !shell_path.is_empty() {
        for p in shell_path.split(path_separator) {
            if !p.is_empty() && !parts.contains(&p) {
                parts.push(p);
            }
        }
    }

    // Add extra dirs
    for dir in &extra_dirs {
        if !parts.contains(&dir.as_str()) && std::path::Path::new(dir).exists() {
            parts.push(dir);
        }
    }

    // Add current PATH entries
    for p in current.split(path_separator) {
        if !p.is_empty() && !parts.contains(&p) {
            parts.push(p);
        }
    }

    let join_sep = if cfg!(windows) { ";" } else { ":" };
    parts.join(join_sep)
}

/// Input for the unified discover command — matches what the frontend extracts from JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// "stdio", "http", "sse", "streamable-http", or "" (auto-detect)
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// For HTTP/SSE/Streamable-HTTP servers
    #[serde(default)]
    pub url: String,
    /// Alternative field name used by some configs (Windsurf)
    #[serde(default, rename = "serverUrl")]
    pub server_url: String,
    /// Environment variables (actual values, not placeholders)
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// HTTP headers (for authenticated remote servers)
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// Unified tool discovery: auto-detects transport type and uses the right method.
///
/// Supports:
/// - **stdio**: Spawns local process, MCP handshake over stdin/stdout
/// - **http / streamable-http**: JSON-RPC POST requests
/// - **sse**: Attempts HTTP POST (modern SSE servers support it), falls back to error
/// - **url-only configs**: Treats as HTTP
#[tauri::command]
pub async fn discover_mcp_tools(config: McpServerConfig) -> Result<Vec<McpTool>, String> {
    let effective_url = if !config.url.is_empty() {
        config.url.clone()
    } else if !config.server_url.is_empty() {
        config.server_url.clone()
    } else {
        String::new()
    };

    let transport = if !config.transport.is_empty() {
        config.transport.to_lowercase()
    } else if !config.command.is_empty() {
        "stdio".to_string()
    } else if !effective_url.is_empty() {
        "http".to_string()
    } else {
        return Err("Cannot determine transport: provide 'command' (stdio) or 'url' (HTTP/SSE)".to_string());
    };

    match transport.as_str() {
        "stdio" => {
            if config.command.is_empty() {
                return Err("'command' is required for stdio transport".to_string());
            }
            let command = config.command.clone();
            let args = config.args.clone();
            let env = config.env.clone();
            tokio::task::spawn_blocking(move || discover_tools_stdio(&command, &args, &env))
                .await
                .map_err(|e| format!("Task join error: {}", e))?
        }
        "http" | "streamable-http" | "sse" => {
            if effective_url.is_empty() {
                return Err("'url' is required for HTTP/SSE transport".to_string());
            }
            discover_tools_http(&effective_url, &config.headers).await
        }
        other => {
            // Unknown transport — try HTTP if url is present, else error
            if !effective_url.is_empty() {
                discover_tools_http(&effective_url, &config.headers).await
            } else if !config.command.is_empty() {
                let command = config.command.clone();
                let args = config.args.clone();
                let env = config.env.clone();
                tokio::task::spawn_blocking(move || discover_tools_stdio(&command, &args, &env))
                    .await
                    .map_err(|e| format!("Task join error: {}", e))?
            } else {
                Err(format!("Unsupported transport '{}' and no command/url provided", other))
            }
        }
    }
}

// Keep the old command signature as an alias for backward compat (frontend already uses it for stdio)
#[tauri::command]
pub async fn discover_mcp_tools_http(
    url: String,
    headers: HashMap<String, String>,
) -> Result<Vec<McpTool>, String> {
    discover_tools_http(&url, &headers).await
}

// ── stdio transport ──────────────────────────────────────────────────────────

fn discover_tools_stdio(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
) -> Result<Vec<McpTool>, String> {
    if command.is_empty() {
        return Err("Command is required for stdio tool discovery".to_string());
    }

    // Bundled apps may not inherit shell PATH.
    // Resolve the full PATH so commands like npx, uvx, node are found.
    let enriched_path = get_enriched_path();

    // On Windows, commands like npx/npm are .cmd batch files.
    // Rust's Command::new doesn't auto-resolve .cmd extensions.
    let resolved_command = crate::utils::platform::resolve_mcp_command(command);

    let mut cmd = Command::new(&resolved_command);
    cmd.args(args)
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    crate::utils::platform::hide_console_window(&mut cmd);

    if !enriched_path.is_empty() {
        cmd.env("PATH", &enriched_path);
    }

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn MCP server '{}': {}", command, e))?;

    let stdin = child.stdin.take().ok_or("Failed to open stdin")?;
    let stdout = child.stdout.take().ok_or("Failed to open stdout")?;

    let result = std::thread::scope(|s| {
        let handle = s.spawn(|| run_stdio_handshake(stdin, stdout));
        match handle.join() {
            Ok(result) => result,
            Err(_) => Err("Tool discovery panicked".to_string()),
        }
    });

    let _ = child.kill();
    let _ = child.wait();

    result
}

fn run_stdio_handshake(
    mut stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
) -> Result<Vec<McpTool>, String> {
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);

    // Initialize
    send_stdio_message(&mut stdin, &json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "agentharbor", "version": "1.0.0" }
        }
    }))?;
    let _init = read_stdio_response(&mut lines, 1, deadline)?;

    // Initialized notification
    send_stdio_message(&mut stdin, &json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }))?;

    // tools/list
    send_stdio_message(&mut stdin, &json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }))?;
    let tools_resp = read_stdio_response(&mut lines, 2, deadline)?;

    parse_tools_from_response(&tools_resp)
}

fn send_stdio_message(stdin: &mut std::process::ChildStdin, msg: &Value) -> Result<(), String> {
    let serialized = serde_json::to_string(msg).map_err(|e| format!("JSON serialize error: {}", e))?;
    stdin.write_all(serialized.as_bytes()).map_err(|e| format!("Write error: {}", e))?;
    stdin.write_all(b"\n").map_err(|e| format!("Write error: {}", e))?;
    stdin.flush().map_err(|e| format!("Flush error: {}", e))?;
    Ok(())
}

fn read_stdio_response(
    lines: &mut std::io::Lines<BufReader<std::process::ChildStdout>>,
    expected_id: u64,
    deadline: std::time::Instant,
) -> Result<Value, String> {
    loop {
        if std::time::Instant::now() > deadline {
            return Err("Tool discovery timed out (15s)".to_string());
        }
        let line = lines
            .next()
            .ok_or("MCP server closed stdout unexpectedly")?
            .map_err(|e| format!("Read error: {}", e))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: Value = serde_json::from_str(line)
            .map_err(|e| format!("Failed to parse MCP response: {}", e))?;
        // Skip notifications (no id)
        if parsed.get("id").is_none() {
            continue;
        }
        if let Some(id) = parsed.get("id").and_then(|v| v.as_u64()) {
            if id == expected_id {
                if let Some(error) = parsed.get("error") {
                    let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown MCP error");
                    return Err(format!("MCP error: {}", msg));
                }
                return Ok(parsed);
            }
        }
    }
}

// ── HTTP transport (Streamable HTTP / SSE fallback) ──────────────────────────

async fn discover_tools_http(
    url: &str,
    headers: &HashMap<String, String>,
) -> Result<Vec<McpTool>, String> {
    if url.is_empty() {
        return Err("URL is required for HTTP tool discovery".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // Initialize
    let init_resp = http_json_rpc(&client, url, headers, &json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "agentharbor", "version": "1.0.0" }
        }
    })).await?;

    // Extract session URL if server returns one (some servers use a different endpoint after init)
    let session_url = init_resp
        .get("result")
        .and_then(|r| r.get("sessionUrl"))
        .and_then(|u| u.as_str())
        .unwrap_or(url);
    let session_url = session_url.to_string();

    // Initialized notification (fire and forget)
    let _ = http_json_rpc(&client, &session_url, headers, &json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })).await;

    // tools/list
    let tools_resp = http_json_rpc(&client, &session_url, headers, &json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    })).await?;

    parse_tools_from_response(&tools_resp)
}

async fn http_json_rpc(
    client: &reqwest::Client,
    url: &str,
    headers: &HashMap<String, String>,
    body: &Value,
) -> Result<Value, String> {
    let mut req = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream");

    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let resp = req
        .json(body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "HTTP request timed out (15s)".to_string()
            } else if e.is_connect() {
                format!("Cannot connect to {}: {}", url, e)
            } else {
                format!("HTTP request failed: {}", e)
            }
        })?;

    let status = resp.status();
    let content_type = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(format!(
            "Authentication required ({}). Add auth headers to the MCP config.",
            status
        ));
    }

    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        return Err(format!("Server returned {}: {}", status, body_text.chars().take(200).collect::<String>()));
    }

    let body_text = resp.text().await.unwrap_or_default();

    // Handle SSE response format (text/event-stream)
    if content_type.contains("text/event-stream") {
        return parse_sse_response(&body_text);
    }

    // Standard JSON response
    serde_json::from_str(&body_text)
        .map_err(|e| format!("Failed to parse response JSON: {}", e))
}

/// Parse an SSE (Server-Sent Events) response body to extract JSON-RPC messages.
/// SSE format: lines starting with "data: " contain JSON payloads.
fn parse_sse_response(body: &str) -> Result<Value, String> {
    for line in body.lines() {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data: ") {
            let data = data.trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                // Return the first JSON-RPC response with an id
                if parsed.get("id").is_some() {
                    return Ok(parsed);
                }
            }
        }
    }
    Err("No JSON-RPC response found in SSE stream".to_string())
}

// ── Shared helpers ───────────────────────────────────────────────────────────

fn parse_tools_from_response(resp: &Value) -> Result<Vec<McpTool>, String> {
    // Check for error
    if let Some(error) = resp.get("error") {
        let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown MCP error");
        return Err(format!("MCP error: {}", msg));
    }

    let tools = resp
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|tool| {
                    let name = tool.get("name")?.as_str()?.to_string();
                    let description = tool
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(McpTool { name, description })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(tools)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_empty_command_error() {
        let result = discover_tools_stdio("", &[], &HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_discover_invalid_command_error() {
        let result = discover_tools_stdio(
            "nonexistent-command-that-does-not-exist",
            &[],
            &HashMap::new(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to spawn"));
    }

    #[test]
    fn test_parse_tools_from_response() {
        let resp = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [
                    { "name": "read_file", "description": "Read a file" },
                    { "name": "search", "description": "Search code" }
                ]
            }
        });
        let tools = parse_tools_from_response(&resp).unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(tools[1].name, "search");
    }

    #[test]
    fn test_parse_tools_error_response() {
        let resp = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "error": { "code": -32601, "message": "Method not found" }
        });
        let result = parse_tools_from_response(&resp);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Method not found"));
    }

    #[test]
    fn test_parse_sse_response() {
        let sse_body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[{\"name\":\"test\",\"description\":\"A test tool\"}]}}\n\n";
        let parsed = parse_sse_response(sse_body).unwrap();
        assert_eq!(parsed["result"]["tools"][0]["name"], "test");
    }

    #[test]
    fn test_parse_sse_response_empty() {
        let result = parse_sse_response("data: [DONE]\n\n");
        assert!(result.is_err());
    }
}
