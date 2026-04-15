use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::models::SkillFile;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedSkill {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub model: Option<String>,
    pub context: Option<String>,
    pub agent: Option<String>,
    pub argument_hint: Option<String>,
    pub files: Vec<SkillFile>,
    pub has_scripts: bool,
    pub rate_limit_remaining: Option<u32>,
    pub rate_limit_reset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialSkillEntry {
    pub name: String,
    pub description: String,
    pub github_url: String,
    pub has_scripts: bool,
    pub file_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedIndex {
    pub entries: Vec<OfficialSkillEntry>,
    pub fetched_at: u64,
}

#[derive(Debug, Deserialize)]
struct GitHubContent {
    name: String,
    path: String,
    #[serde(rename = "type")]
    content_type: String,
    download_url: Option<String>,
    // content field is base64 when fetching individual files via contents API
    content: Option<String>,
    encoding: Option<String>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn cache_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.agentharbor.app")
        .join("official_skills_cache.json")
}

/// Parse a GitHub URL like:
///   https://github.com/owner/repo/tree/branch/path/to/skill
///   https://github.com/owner/repo/blob/branch/path/to/file
/// Returns (owner, repo, branch, path)
fn parse_github_url(url: &str) -> Result<(String, String, String, String), String> {
    let url = url.trim().trim_end_matches('/');

    // Strip scheme + host
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .ok_or_else(|| "URL must start with https://github.com/".to_string())?;

    let parts: Vec<&str> = path.splitn(5, '/').collect();
    // parts: [owner, repo, tree|blob, branch, rest_path]
    if parts.len() < 4 {
        return Err("URL must contain owner/repo/tree|blob/branch/path".to_string());
    }

    let owner = parts[0].to_string();
    let repo = parts[1].to_string();
    // parts[2] is "tree" or "blob"
    let branch = parts[3].to_string();
    let rest_path = if parts.len() >= 5 { parts[4].to_string() } else { String::new() };

    Ok((owner, repo, branch, rest_path))
}

fn build_client(github_token: Option<&str>) -> Result<reqwest::Client, String> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("User-Agent", "AgentHarbor/1.0".parse().unwrap());
    headers.insert("Accept", "application/vnd.github.v3+json".parse().unwrap());
    if let Some(token) = github_token {
        if !token.is_empty() {
            headers.insert(
                "Authorization",
                format!("token {}", token).parse().map_err(|e| format!("Invalid token: {}", e))?,
            );
        }
    }
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

struct RateLimitInfo {
    remaining: Option<u32>,
    reset: Option<u64>,
}

fn extract_rate_limit(headers: &reqwest::header::HeaderMap) -> RateLimitInfo {
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let reset = headers
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    RateLimitInfo { remaining, reset }
}

/// Parse SKILL.md frontmatter to extract metadata fields.
fn parse_skill_frontmatter(content: &str) -> HashMap<String, String> {
    let mut meta = HashMap::new();
    let trimmed = content.trim();
    if !trimmed.starts_with("---") {
        return meta;
    }

    let after_first = &trimmed[3..];
    if let Some(end) = after_first.find("---") {
        let frontmatter = &after_first[..end];
        for line in frontmatter.lines() {
            let line = line.trim();
            if let Some(pos) = line.find(':') {
                let key = line[..pos].trim().to_string();
                let value = line[pos + 1..].trim().trim_matches('"').to_string();
                if !key.is_empty() && !value.is_empty() {
                    meta.insert(key, value);
                }
            }
        }
    }
    meta
}

/// Extract the body content after frontmatter.
fn extract_skill_body(content: &str) -> String {
    let trimmed = content.trim();
    if !trimmed.starts_with("---") {
        return content.to_string();
    }
    let after_first = &trimmed[3..];
    if let Some(end) = after_first.find("---") {
        after_first[end + 3..].trim_start().to_string()
    } else {
        content.to_string()
    }
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fetch_github_skill(url: String, github_token: Option<String>) -> Result<FetchedSkill, String> {
    let (owner, repo, branch, path) = parse_github_url(&url)?;
    let client = build_client(github_token.as_deref())?;

    // Fetch directory listing
    let api_url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
        owner, repo, path, branch
    );

    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| format!("GitHub API request failed: {}", e))?;

    let rate_limit = extract_rate_limit(resp.headers());

    if resp.status() == 403 {
        if let Some(0) = rate_limit.remaining {
            let reset_ts = rate_limit.reset.unwrap_or(0);
            return Err(format!(
                "RATE_LIMITED:{}",
                reset_ts
            ));
        }
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        // Parse GitHub error for more helpful messages
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            let msg = json.get("message").and_then(|v| v.as_str()).unwrap_or("");
            if msg.contains("SAML") || msg.contains("SSO") {
                return Err(format!(
                    "This organization requires SAML SSO. Authorize your token at: {}",
                    json.get("documentation_url").and_then(|v| v.as_str()).unwrap_or("https://github.com/settings/tokens")
                ));
            }
            if !msg.is_empty() {
                return Err(format!("GitHub API {} — {}", status, msg.lines().next().unwrap_or(msg)));
            }
        }
        return Err(format!("GitHub API returned status {}", status));
    }

    let entries: Vec<GitHubContent> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitHub response: {}", e))?;

    // Collect all files (recursively for subdirectories)
    let mut files: Vec<SkillFile> = Vec::new();
    let mut has_scripts = false;

    for entry in &entries {
        if entry.content_type == "dir" {
            if entry.name == "scripts" {
                has_scripts = true;
            }
            // Fetch subdirectory contents
            let sub_url = format!(
                "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
                owner, repo, entry.path, branch
            );
            let sub_resp = client.get(&sub_url).send().await.map_err(|e| format!("Failed to fetch subdir: {}", e))?;
            if sub_resp.status().is_success() {
                let sub_entries: Vec<GitHubContent> = sub_resp.json().await.unwrap_or_default();
                for sub_entry in &sub_entries {
                    if sub_entry.content_type == "file" {
                        if let Some(ref dl_url) = sub_entry.download_url {
                            let file_content = client.get(dl_url).send().await
                                .map_err(|e| format!("Failed to download {}: {}", sub_entry.name, e))?
                                .text().await.unwrap_or_default();
                            // Relative path within skill directory
                            let rel_path = format!("{}/{}", entry.name, sub_entry.name);
                            files.push(SkillFile { path: rel_path, content: file_content });
                        }
                    }
                }
            }
        } else if entry.content_type == "file" {
            if let Some(ref dl_url) = entry.download_url {
                let file_content = client.get(dl_url).send().await
                    .map_err(|e| format!("Failed to download {}: {}", entry.name, e))?
                    .text().await.unwrap_or_default();
                files.push(SkillFile { path: entry.name.clone(), content: file_content });
            }
        }
    }

    // Parse SKILL.md frontmatter
    let skill_md = files.iter().find(|f| f.path == "SKILL.md" || f.path == "skill.md");
    let meta = skill_md.map(|f| parse_skill_frontmatter(&f.content)).unwrap_or_default();

    let name = meta.get("name").cloned().unwrap_or_else(|| {
        // Fall back to last segment of path
        path.split('/').last().unwrap_or("unknown").to_string()
    });
    let description = meta.get("description").cloned().unwrap_or_default();
    let license = meta.get("license").cloned();
    let allowed_tools: Option<Vec<String>> = meta.get("allowed-tools").map(|s| {
        s.split_whitespace().map(String::from).collect()
    });
    let model = meta.get("model").cloned();
    let context = meta.get("context").cloned();
    let agent = meta.get("agent").cloned();
    let argument_hint = meta.get("argument-hint").cloned();

    // Rewrite SKILL.md to only contain the body (frontmatter will be regenerated on deploy)
    let files: Vec<SkillFile> = files
        .into_iter()
        .map(|f| {
            if f.path == "SKILL.md" || f.path == "skill.md" {
                SkillFile {
                    path: "SKILL.md".to_string(),
                    content: extract_skill_body(&f.content),
                }
            } else {
                f
            }
        })
        .collect();

    Ok(FetchedSkill {
        name,
        description,
        license,
        allowed_tools,
        model,
        context,
        agent,
        argument_hint,
        files,
        has_scripts,
        rate_limit_remaining: rate_limit.remaining,
        rate_limit_reset: rate_limit.reset,
    })
}

#[tauri::command]
pub async fn get_official_skills_index(
    force_refresh: bool,
    github_token: Option<String>,
) -> Result<Vec<OfficialSkillEntry>, String> {
    let cache_file = cache_path();

    // Check cache unless force refresh
    if !force_refresh {
        if let Ok(data) = fs::read_to_string(&cache_file) {
            if let Ok(cached) = serde_json::from_str::<CachedIndex>(&data) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                // Cache valid for 24 hours
                if now - cached.fetched_at < 86400 {
                    return Ok(cached.entries);
                }
            }
        }
    }

    let client = build_client(github_token.as_deref())?;

    // Fetch Anthropic skills directory listing
    let api_url = "https://api.github.com/repos/anthropics/skills/contents/skills?ref=main";
    let resp = client
        .get(api_url)
        .send()
        .await
        .map_err(|e| format!("GitHub API request failed: {}", e))?;

    let rate_limit = extract_rate_limit(resp.headers());

    if resp.status() == 403 {
        if let Some(0) = rate_limit.remaining {
            // Return stale cache if available
            if let Ok(data) = fs::read_to_string(&cache_file) {
                if let Ok(cached) = serde_json::from_str::<CachedIndex>(&data) {
                    return Ok(cached.entries);
                }
            }
            let reset_ts = rate_limit.reset.unwrap_or(0);
            return Err(format!("RATE_LIMITED:{}", reset_ts));
        }
    }

    if !resp.status().is_success() {
        // Return stale cache if available
        if let Ok(data) = fs::read_to_string(&cache_file) {
            if let Ok(cached) = serde_json::from_str::<CachedIndex>(&data) {
                return Ok(cached.entries);
            }
        }
        return Err(format!("GitHub API returned status {}", resp.status()));
    }

    let entries: Vec<GitHubContent> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitHub response: {}", e))?;

    let skill_dirs: Vec<&GitHubContent> = entries.iter().filter(|e| e.content_type == "dir").collect();

    let mut skills: Vec<OfficialSkillEntry> = Vec::new();

    for dir in &skill_dirs {
        // Fetch each skill's directory to get file count and SKILL.md
        let dir_url = format!(
            "https://api.github.com/repos/anthropics/skills/contents/{}?ref=main",
            dir.path
        );
        let dir_resp = client.get(&dir_url).send().await;
        if let Ok(resp) = dir_resp {
            if resp.status().is_success() {
                let dir_entries: Vec<GitHubContent> = resp.json().await.unwrap_or_default();
                let file_count = dir_entries.len() as u32;
                let has_scripts = dir_entries.iter().any(|e| e.name == "scripts" && e.content_type == "dir");

                // Fetch SKILL.md for name + description
                let skill_md_entry = dir_entries.iter().find(|e| e.name == "SKILL.md");
                let (name, description) = if let Some(entry) = skill_md_entry {
                    if let Some(ref dl_url) = entry.download_url {
                        let content = client.get(dl_url).send().await
                            .ok()
                            .and_then(|r| if r.status().is_success() { Some(r) } else { None });
                        if let Some(resp) = content {
                            let text = resp.text().await.unwrap_or_default();
                            let meta = parse_skill_frontmatter(&text);
                            (
                                meta.get("name").cloned().unwrap_or_else(|| dir.name.clone()),
                                meta.get("description").cloned().unwrap_or_default(),
                            )
                        } else {
                            (dir.name.clone(), String::new())
                        }
                    } else {
                        (dir.name.clone(), String::new())
                    }
                } else {
                    (dir.name.clone(), String::new())
                };

                skills.push(OfficialSkillEntry {
                    name,
                    description,
                    github_url: format!("https://github.com/anthropics/skills/tree/main/{}", dir.path),
                    has_scripts,
                    file_count,
                });
            }
        }
    }

    // Save cache
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cached = CachedIndex {
        entries: skills.clone(),
        fetched_at: now,
    };
    if let Some(parent) = cache_file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&cache_file, serde_json::to_string_pretty(&cached).unwrap_or_default());

    Ok(skills)
}

// ── OpenClaw Skills Registry ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawSkillEntry {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub category: String,
    pub author: String,
    pub author_github: String,
    pub github_url: String,
    pub github_stars: u64,
    pub has_scripts: bool,
    pub file_count: u32,
    pub tags: Vec<String>,
    pub version: String,
    pub license: String,
    pub compatible_adapters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedOpenClawIndex {
    entries: Vec<OpenClawSkillEntry>,
    fetched_at: u64,
}

fn openclaw_cache_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.agentharbor.app")
        .join("openclaw_skills_cache.json")
}

fn openclaw_registry_skills_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.agentharbor.app")
        .join("registry")
        .join("community")
        .join("skills")
}

/// Scan the local community registry for OpenClaw skills with metadata.json
fn load_openclaw_skills_from_registry(min_stars: u64) -> Vec<OpenClawSkillEntry> {
    let skills_dir = openclaw_registry_skills_path();
    if !skills_dir.exists() {
        return vec![];
    }

    let mut entries = Vec::new();

    // Walk category dirs: skills/<category>/<skill-name>/metadata.json
    let cat_entries = match fs::read_dir(&skills_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    for cat_entry in cat_entries.flatten() {
        let cat_path = cat_entry.path();
        if !cat_path.is_dir() {
            continue;
        }

        let skill_entries = match fs::read_dir(&cat_path) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for skill_entry in skill_entries.flatten() {
            let skill_path = skill_entry.path();
            if !skill_path.is_dir() {
                continue;
            }

            let metadata_file = skill_path.join("metadata.json");
            if !metadata_file.exists() {
                continue;
            }

            let content = match fs::read_to_string(&metadata_file) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let meta: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let stars = meta.get("stats")
                .and_then(|s| s.get("github_stars"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            if stars < min_stars {
                continue;
            }

            let name = meta.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let display_name = meta.get("display_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let description = meta.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let category = meta.get("category").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let author = meta.get("author").and_then(|v| v.as_str()).unwrap_or("openclaw").to_string();
            let author_github = meta.get("author_github").and_then(|v| v.as_str()).unwrap_or("openclaw").to_string();
            let version = meta.get("version").and_then(|v| v.as_str()).unwrap_or("1.0.0").to_string();
            let license = meta.get("license").and_then(|v| v.as_str()).unwrap_or("MIT").to_string();
            let has_scripts = meta.get("has_scripts").and_then(|v| v.as_bool()).unwrap_or(false);
            let file_count = meta.get("file_count").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

            let tags: Vec<String> = meta.get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let compatible_adapters: Vec<String> = meta.get("compatible_adapters")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_else(|| vec!["claude-code".into(), "cursor".into(), "windsurf".into()]);

            let github_url = meta.get("source")
                .and_then(|s| s.get("url"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            entries.push(OpenClawSkillEntry {
                name, display_name, description, category, author, author_github,
                github_url, github_stars: stars, has_scripts, file_count,
                tags, version, license, compatible_adapters,
            });
        }
    }

    // Sort by stars descending, then name ascending
    entries.sort_by(|a, b| b.github_stars.cmp(&a.github_stars).then(a.name.cmp(&b.name)));
    entries
}

#[tauri::command]
pub async fn get_openclaw_skills_index(
    force_refresh: bool,
    min_stars: Option<u64>,
) -> Result<Vec<OpenClawSkillEntry>, String> {
    let min_stars = min_stars.unwrap_or(5000);
    let cache_file = openclaw_cache_path();

    // Try cache first (24h TTL)
    if !force_refresh {
        if let Ok(data) = fs::read_to_string(&cache_file) {
            if let Ok(cached) = serde_json::from_str::<CachedOpenClawIndex>(&data) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if now - cached.fetched_at < 86400 {
                    // Filter by min_stars in case it changed
                    let filtered: Vec<_> = cached.entries.into_iter()
                        .filter(|e| e.github_stars >= min_stars)
                        .collect();
                    return Ok(filtered);
                }
            }
        }
    }

    // Load from local community registry
    let entries = load_openclaw_skills_from_registry(min_stars);

    // Cache results
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cached = CachedOpenClawIndex {
        entries: entries.clone(),
        fetched_at: now,
    };
    if let Some(parent) = cache_file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&cache_file, serde_json::to_string_pretty(&cached).unwrap_or_default());

    Ok(entries)
}

#[tauri::command]
pub async fn search_openclaw_skills(
    query: String,
    min_stars: Option<u64>,
) -> Result<Vec<OpenClawSkillEntry>, String> {
    let min_stars = min_stars.unwrap_or(5000);
    let all = load_openclaw_skills_from_registry(min_stars);

    if query.trim().is_empty() {
        return Ok(all);
    }

    let q = query.to_lowercase();
    let filtered: Vec<_> = all.into_iter().filter(|entry| {
        entry.name.to_lowercase().contains(&q)
            || entry.display_name.to_lowercase().contains(&q)
            || entry.description.to_lowercase().contains(&q)
            || entry.category.to_lowercase().contains(&q)
            || entry.tags.iter().any(|t| t.to_lowercase().contains(&q))
    }).collect();

    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_url_tree() {
        let (owner, repo, branch, path) =
            parse_github_url("https://github.com/anthropics/skills/tree/main/skills/pdf").unwrap();
        assert_eq!(owner, "anthropics");
        assert_eq!(repo, "skills");
        assert_eq!(branch, "main");
        assert_eq!(path, "skills/pdf");
    }

    #[test]
    fn test_parse_github_url_blob() {
        let (owner, repo, branch, path) =
            parse_github_url("https://github.com/owner/repo/blob/dev/some/path").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
        assert_eq!(branch, "dev");
        assert_eq!(path, "some/path");
    }

    #[test]
    fn test_parse_github_url_trailing_slash() {
        let (_, _, _, path) =
            parse_github_url("https://github.com/anthropics/skills/tree/main/skills/pdf/").unwrap();
        assert_eq!(path, "skills/pdf");
    }

    #[test]
    fn test_parse_github_url_invalid() {
        assert!(parse_github_url("https://gitlab.com/owner/repo").is_err());
        assert!(parse_github_url("not a url").is_err());
    }

    #[test]
    fn test_parse_skill_frontmatter() {
        let content = r#"---
name: pdf
description: "Extract PDF text, fill forms, merge files"
license: Apache-2.0
allowed-tools: Bash Read Write
---
# PDF Processing
Instructions here."#;

        let meta = parse_skill_frontmatter(content);
        assert_eq!(meta.get("name").unwrap(), "pdf");
        assert_eq!(meta.get("description").unwrap(), "Extract PDF text, fill forms, merge files");
        assert_eq!(meta.get("license").unwrap(), "Apache-2.0");
        assert_eq!(meta.get("allowed-tools").unwrap(), "Bash Read Write");
    }

    #[test]
    fn test_parse_skill_frontmatter_empty() {
        let meta = parse_skill_frontmatter("No frontmatter here");
        assert!(meta.is_empty());
    }

    #[test]
    fn test_extract_skill_body() {
        let content = "---\nname: test\n---\n# Body\nContent here.";
        let body = extract_skill_body(content);
        assert_eq!(body, "# Body\nContent here.");
    }

    #[test]
    fn test_extract_skill_body_no_frontmatter() {
        let content = "# No frontmatter\nJust content.";
        let body = extract_skill_body(content);
        assert_eq!(body, content);
    }
}
