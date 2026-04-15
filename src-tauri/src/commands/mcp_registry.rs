use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRegistryEnvVar {
    pub name: String,
    pub description: String,
    pub is_required: bool,
    pub is_secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRegistryEntry {
    pub name: String,
    pub title: String,
    pub description: String,
    pub version: String,
    pub icon_url: Option<String>,
    pub website_url: Option<String>,
    pub repository_url: Option<String>,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env_vars: Vec<McpRegistryEnvVar>,
    pub is_official: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRegistryPage {
    pub entries: Vec<McpRegistryEntry>,
    pub next_cursor: Option<String>,
    pub page: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedRegistry {
    pub entries: Vec<McpRegistryEntry>,
    pub next_cursor: Option<String>,
    pub fetched_at: u64,
}

// ── Raw API response types ───────────────────────────────────────────────────
// Actual shape: { servers: [ { server: {...}, _meta: {...} }, ... ] }

#[derive(Debug, Deserialize)]
struct ApiResponse {
    servers: Option<Vec<ApiServerWrapper>>,
    metadata: Option<ApiMetadata>,
}

#[derive(Debug, Deserialize)]
struct ApiMetadata {
    #[serde(rename = "nextCursor")]
    next_cursor: Option<String>,
}

/// Each entry in the servers array wraps the actual server + _meta
#[derive(Debug, Deserialize)]
struct ApiServerWrapper {
    server: Option<ApiServer>,
    #[serde(rename = "_meta")]
    meta: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct ApiServer {
    name: Option<String>,
    title: Option<String>,
    description: Option<String>,
    version: Option<String>,
    #[serde(rename = "websiteUrl")]
    website_url: Option<String>,
    repository: Option<ApiRepository>,
    packages: Option<Vec<ApiPackage>>,
    remotes: Option<Vec<ApiRemote>>,
    icons: Option<Vec<ApiIcon>>,
}

#[derive(Debug, Deserialize)]
struct ApiRepository {
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiPackage {
    #[serde(rename = "registryType")]
    registry_type: Option<String>,
    identifier: Option<String>,
    name: Option<String>,
    version: Option<String>,
    transport: Option<ApiPackageTransport>,
    #[serde(rename = "environmentVariables")]
    environment_variables: Option<Vec<ApiEnvVar>>,
}

#[derive(Debug, Deserialize)]
struct ApiPackageTransport {
    #[serde(rename = "type")]
    transport_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiRemote {
    #[serde(rename = "type")]
    transport_type: Option<String>,
    url: Option<String>,
    #[serde(rename = "environmentVariables")]
    environment_variables: Option<Vec<ApiEnvVar>>,
}

#[derive(Debug, Deserialize)]
struct ApiEnvVar {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "isRequired")]
    is_required: Option<bool>,
    #[serde(rename = "isSecret")]
    is_secret: Option<bool>,
    // Also accept "required" as a fallback key
    required: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ApiIcon {
    url: Option<String>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn cache_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.agentharbor.app")
        .join("mcp_registry_cache.json")
}

fn build_client() -> Result<reqwest::Client, String> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("User-Agent", "AgentHarbor/1.0".parse().unwrap());
    headers.insert("Accept", "application/json".parse().unwrap());
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

fn parse_server_wrapper(wrapper: &ApiServerWrapper) -> Option<McpRegistryEntry> {
    let server = wrapper.server.as_ref()?;
    let name = server.name.clone().unwrap_or_default();
    if name.is_empty() {
        return None;
    }

    let description = server.description.clone().unwrap_or_default();
    let repository_url = server.repository.as_ref().and_then(|r| r.url.clone());
    let version = server.version.clone().unwrap_or_else(|| "latest".to_string());
    let icon_url = server.icons.as_ref().and_then(|icons| icons.first().and_then(|i| i.url.clone()));
    let website_url = server.website_url.clone();

    // Check _meta for official status
    // Key is "io.modelcontextprotocol.registry/official", value has "status": "active"
    let is_official = wrapper.meta.as_ref().map(|meta| {
        meta.get("io.modelcontextprotocol.registry/official")
            .and_then(|v| v.get("status"))
            .and_then(|s| s.as_str())
            .map(|s| s == "active")
            .unwrap_or(false)
    }).unwrap_or(false);

    // Use explicit title if available, otherwise derive from name
    let title = server.title.clone().unwrap_or_else(|| {
        name.split('/').last().unwrap_or(&name)
            .replace('-', " ")
            .split_whitespace()
            .map(|w| {
                let mut chars = w.chars();
                match chars.next() {
                    Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    });

    // Check packages first (stdio transport)
    if let Some(packages) = &server.packages {
        if let Some(pkg) = packages.first() {
            let transport_type = pkg.transport.as_ref()
                .and_then(|t| t.transport_type.clone())
                .unwrap_or_else(|| "stdio".to_string());

            if transport_type == "stdio" {
                // Infer command from registryType
                let command = match pkg.registry_type.as_deref() {
                    Some("npm") => Some("npx".to_string()),
                    Some("pypi") => Some("uvx".to_string()),
                    _ => None,
                };

                let mut args: Vec<String> = Vec::new();
                // For npx, add -y flag
                if command.as_deref() == Some("npx") {
                    args.push("-y".to_string());
                }
                // Add the package identifier
                if let Some(ref identifier) = pkg.identifier {
                    args.push(identifier.clone());
                } else if let Some(ref pkg_name) = pkg.name {
                    args.push(pkg_name.clone());
                }

                let env_vars = parse_env_vars(pkg.environment_variables.as_ref());

                return Some(McpRegistryEntry {
                    name,
                    title,
                    description,
                    version,
                    icon_url,
                    website_url,
                    repository_url,
                    transport: "stdio".to_string(),
                    command,
                    args,
                    url: None,
                    env_vars,
                    is_official,
                });
            }
        }
    }

    // Check remotes (http/sse transport)
    if let Some(remotes) = &server.remotes {
        if let Some(remote) = remotes.first() {
            let transport = remote.transport_type.clone().unwrap_or_else(|| "streamable-http".to_string());
            let url = remote.url.clone();
            let env_vars = parse_env_vars(remote.environment_variables.as_ref());

            return Some(McpRegistryEntry {
                name,
                title,
                description,
                version,
                icon_url,
                website_url,
                repository_url,
                transport,
                command: None,
                args: vec![],
                url,
                env_vars,
                is_official,
            });
        }
    }

    // Fallback: entry exists but no packages or remotes
    Some(McpRegistryEntry {
        name,
        title,
        description,
        version,
        icon_url,
        website_url,
        repository_url,
        transport: "unknown".to_string(),
        command: None,
        args: vec![],
        url: None,
        env_vars: vec![],
        is_official,
    })
}

fn parse_env_vars(vars: Option<&Vec<ApiEnvVar>>) -> Vec<McpRegistryEnvVar> {
    vars.map(|evs| {
        evs.iter().filter_map(|ev| {
            let name = ev.name.clone()?;
            Some(McpRegistryEnvVar {
                name,
                description: ev.description.clone().unwrap_or_default(),
                is_required: ev.is_required.or(ev.required).unwrap_or(false),
                is_secret: ev.is_secret.unwrap_or(true),
            })
        }).collect()
    }).unwrap_or_default()
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn search_mcp_registry(
    query: String,
    limit: Option<u32>,
    cursor: Option<String>,
    page: Option<u32>,
) -> Result<McpRegistryPage, String> {
    let client = build_client()?;
    let limit = limit.unwrap_or(30);
    let page = page.unwrap_or(1);

    let encoded_query: String = query.chars().map(|c| {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
            c.to_string()
        } else {
            format!("%{:02X}", c as u32)
        }
    }).collect();

    let mut url = format!(
        "https://registry.modelcontextprotocol.io/v0.1/servers?search={}&version=latest&limit={}",
        encoded_query,
        limit
    );
    if let Some(ref c) = cursor {
        url.push_str(&format!("&cursor={}", c));
    }

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("MCP Registry request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("MCP Registry returned status {}", resp.status()));
    }

    let api_resp: ApiResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse MCP Registry response: {}", e))?;

    let next_cursor = api_resp.metadata.and_then(|m| m.next_cursor);

    let entries = api_resp
        .servers
        .unwrap_or_default()
        .iter()
        .filter_map(parse_server_wrapper)
        .collect();

    Ok(McpRegistryPage { entries, next_cursor, page })
}

#[tauri::command]
pub async fn get_mcp_registry_popular(
    force_refresh: bool,
    cursor: Option<String>,
    page: Option<u32>,
) -> Result<McpRegistryPage, String> {
    let cache_file = cache_path();
    let page = page.unwrap_or(1);

    // Only use cache for page 1 without cursor
    if !force_refresh && cursor.is_none() {
        if let Ok(data) = fs::read_to_string(&cache_file) {
            if let Ok(cached) = serde_json::from_str::<CachedRegistry>(&data) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                // Cache valid for 24 hours
                if now - cached.fetched_at < 86400 {
                    return Ok(McpRegistryPage {
                        entries: cached.entries,
                        next_cursor: cached.next_cursor,
                        page: 1,
                    });
                }
            }
        }
    }

    let client = build_client()?;

    let mut url = "https://registry.modelcontextprotocol.io/v0.1/servers?version=latest&limit=50".to_string();
    if let Some(ref c) = cursor {
        url.push_str(&format!("&cursor={}", c));
    }

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| {
            format!("MCP Registry request failed: {}", e)
        })?;

    if !resp.status().is_success() {
        // Return stale cache if available (only for page 1)
        if cursor.is_none() {
            if let Ok(data) = fs::read_to_string(&cache_file) {
                if let Ok(cached) = serde_json::from_str::<CachedRegistry>(&data) {
                    return Ok(McpRegistryPage {
                        entries: cached.entries,
                        next_cursor: cached.next_cursor,
                        page: 1,
                    });
                }
            }
        }
        return Err(format!("MCP Registry returned status {}", resp.status()));
    }

    let api_resp: ApiResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse MCP Registry response: {}", e))?;

    let next_cursor = api_resp.metadata.and_then(|m| m.next_cursor);

    let entries: Vec<McpRegistryEntry> = api_resp
        .servers
        .unwrap_or_default()
        .iter()
        .filter_map(parse_server_wrapper)
        .collect();

    // Save cache only for page 1
    if cursor.is_none() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cached = CachedRegistry {
            entries: entries.clone(),
            next_cursor: next_cursor.clone(),
            fetched_at: now,
        };
        if let Some(parent) = cache_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(
            &cache_file,
            serde_json::to_string_pretty(&cached).unwrap_or_default(),
        );
    }

    Ok(McpRegistryPage { entries, next_cursor, page })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_vars_empty() {
        let result = parse_env_vars(None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_env_vars_with_data() {
        let vars = vec![
            ApiEnvVar {
                name: Some("API_KEY".to_string()),
                description: Some("Your API key".to_string()),
                is_required: Some(true),
                required: None,
                is_secret: Some(true),
            },
            ApiEnvVar {
                name: Some("BASE_URL".to_string()),
                description: None,
                is_required: None,
                required: None,
                is_secret: Some(false),
            },
        ];
        let result = parse_env_vars(Some(&vars));
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "API_KEY");
        assert!(result[0].is_required);
        assert!(result[0].is_secret);
        assert_eq!(result[1].name, "BASE_URL");
        assert!(!result[1].is_required);
        assert!(!result[1].is_secret);
    }

    #[test]
    fn test_parse_env_vars_skips_unnamed() {
        let vars = vec![
            ApiEnvVar {
                name: None,
                description: Some("No name".to_string()),
                is_required: Some(true),
                required: None,
                is_secret: None,
            },
        ];
        let result = parse_env_vars(Some(&vars));
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_server_stdio() {
        let wrapper = ApiServerWrapper {
            server: Some(ApiServer {
                name: Some("io.github.user/my-server".to_string()),
                title: None,
                description: Some("A test server".to_string()),
                version: Some("1.0.0".to_string()),
                website_url: None,
                repository: Some(ApiRepository {
                    url: Some("https://github.com/user/my-server".to_string()),
                }),
                packages: Some(vec![ApiPackage {
                    registry_type: Some("npm".to_string()),
                    identifier: Some("@user/my-server".to_string()),
                    name: None,
                    version: Some("1.0.0".to_string()),
                    environment_variables: Some(vec![ApiEnvVar {
                        name: Some("TOKEN".to_string()),
                        description: Some("Auth token".to_string()),
                        is_required: Some(true),
                        required: None,
                        is_secret: Some(true),
                    }]),
                    transport: Some(ApiPackageTransport {
                        transport_type: Some("stdio".to_string()),
                    }),
                }]),
                remotes: None,
                icons: Some(vec![ApiIcon {
                    url: Some("https://example.com/icon.png".to_string()),
                }]),
            }),
            meta: {
                let mut m = HashMap::new();
                m.insert(
                    "io.modelcontextprotocol.registry/official".to_string(),
                    serde_json::json!({"status": "active"}),
                );
                Some(m)
            },
        };

        let entry = parse_server_wrapper(&wrapper).unwrap();
        assert_eq!(entry.name, "io.github.user/my-server");
        assert_eq!(entry.title, "My Server");
        assert_eq!(entry.transport, "stdio");
        assert_eq!(entry.command, Some("npx".to_string()));
        assert_eq!(entry.args, vec!["-y", "@user/my-server"]);
        assert_eq!(entry.env_vars.len(), 1);
        assert_eq!(entry.env_vars[0].name, "TOKEN");
        assert!(entry.is_official);
        assert_eq!(entry.icon_url, Some("https://example.com/icon.png".to_string()));
    }

    #[test]
    fn test_parse_server_remote() {
        let wrapper = ApiServerWrapper {
            server: Some(ApiServer {
                name: Some("example/remote-server".to_string()),
                title: Some("Remote Server".to_string()),
                description: Some("A remote server".to_string()),
                version: Some("2.0.0".to_string()),
                website_url: Some("https://example.com".to_string()),
                repository: None,
                packages: None,
                remotes: Some(vec![ApiRemote {
                    transport_type: Some("streamable-http".to_string()),
                    url: Some("https://api.example.com/mcp".to_string()),
                    environment_variables: None,
                }]),
                icons: None,
            }),
            meta: None,
        };

        let entry = parse_server_wrapper(&wrapper).unwrap();
        assert_eq!(entry.title, "Remote Server");
        assert_eq!(entry.transport, "streamable-http");
        assert_eq!(entry.url, Some("https://api.example.com/mcp".to_string()));
        assert!(entry.command.is_none());
        assert!(!entry.is_official);
        assert_eq!(entry.website_url, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_parse_server_no_server() {
        let wrapper = ApiServerWrapper {
            server: None,
            meta: None,
        };
        assert!(parse_server_wrapper(&wrapper).is_none());
    }

    #[test]
    fn test_parse_server_no_name() {
        let wrapper = ApiServerWrapper {
            server: Some(ApiServer {
                name: None,
                title: None,
                description: None,
                version: None,
                website_url: None,
                repository: None,
                packages: None,
                remotes: None,
                icons: None,
            }),
            meta: None,
        };
        assert!(parse_server_wrapper(&wrapper).is_none());
    }

    #[test]
    fn test_parse_server_pypi() {
        let wrapper = ApiServerWrapper {
            server: Some(ApiServer {
                name: Some("test/python-server".to_string()),
                title: None,
                description: Some("Python server".to_string()),
                version: Some("0.1.0".to_string()),
                website_url: None,
                repository: None,
                packages: Some(vec![ApiPackage {
                    registry_type: Some("pypi".to_string()),
                    identifier: Some("python-server".to_string()),
                    name: None,
                    version: None,
                    environment_variables: None,
                    transport: Some(ApiPackageTransport {
                        transport_type: Some("stdio".to_string()),
                    }),
                }]),
                remotes: None,
                icons: None,
            }),
            meta: None,
        };

        let entry = parse_server_wrapper(&wrapper).unwrap();
        assert_eq!(entry.command, Some("uvx".to_string()));
        assert_eq!(entry.args, vec!["python-server"]);
    }

    #[test]
    fn test_cache_path() {
        let path = cache_path();
        assert!(path.to_string_lossy().contains("mcp_registry_cache.json"));
    }
}
