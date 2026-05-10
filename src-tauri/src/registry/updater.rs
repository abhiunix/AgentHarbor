use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub last_etag: Option<String>,
    pub last_sync_time: Option<String>,
    pub last_error: Option<String>,
    pub is_syncing: bool,
    pub capabilities_count: usize,
    pub agents_count: usize,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            last_etag: None,
            last_sync_time: None,
            last_error: None,
            is_syncing: false,
            capabilities_count: 0,
            agents_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub repo_url: String,
    pub branch: String,
    pub polling_interval_minutes: u32,
    pub auto_update: bool,
    pub github_pat: Option<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            repo_url: "https://github.com/abhiunix/community-registry".to_string(),
            branch: "main".to_string(),
            polling_interval_minutes: 60,
            auto_update: true,
            github_pat: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub success: bool,
    pub message: String,
    pub new_capabilities: usize,
    pub new_agents: usize,
    pub updated: bool,
}

lazy_static::lazy_static! {
    static ref SYNC_STATE: Arc<Mutex<SyncState>> = Arc::new(Mutex::new(SyncState::default()));
    static ref POLLING_ACTIVE: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
}

pub fn get_community_registry_path() -> PathBuf {
    crate::utils::paths::app_data_dir()
        .join("registry")
        .join("community")
}

pub fn get_sync_state_path() -> PathBuf {
    crate::utils::paths::app_data_dir().join("sync_state.json")
}

pub fn load_sync_state() -> SyncState {
    let path = get_sync_state_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(state) = serde_json::from_str(&content) {
                return state;
            }
        }
    }
    SyncState::default()
}

pub fn save_sync_state(state: &SyncState) -> Result<(), String> {
    let path = get_sync_state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn check_for_updates(config: &SyncConfig, last_etag: Option<&str>) -> Result<(bool, Option<String>, Option<Vec<u8>>), String> {
    let (owner, repo) = parse_github_url(&config.repo_url)?;
    let api_url = format!(
        "https://api.github.com/repos/{}/{}/zipball/{}",
        owner, repo, config.branch
    );

    let client = reqwest::blocking::Client::new();
    let mut request = client
        .get(&api_url)
        .header("User-Agent", "AgentHarbor/1.0")
        .header("Accept", "application/vnd.github.v3+json");

    if let Some(etag) = last_etag {
        request = request.header("If-None-Match", etag);
    }

    if let Some(ref pat) = config.github_pat {
        if !pat.is_empty() {
            request = request.header("Authorization", format!("token {}", pat));
        }
    }

    let response = request.send().map_err(|e| format!("Network error: {}", e))?;

    let status = response.status();
    
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok((false, last_etag.map(|s| s.to_string()), None));
    }

    if !status.is_success() {
        return Err(format!("GitHub API error: {}", status));
    }

    let new_etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let bytes = response.bytes().map_err(|e| format!("Download error: {}", e))?;
    
    Ok((true, new_etag, Some(bytes.to_vec())))
}

fn parse_github_url(url: &str) -> Result<(String, String), String> {
    // Trim whitespace and trailing punctuation (e.g. comma from paste)
    let url = url.trim().trim_end_matches(|c| c == '/' || c == ',' || c == ';');
    let url = url.strip_prefix("https://github.com/").unwrap_or(url);
    let url = url.strip_prefix("http://github.com/").unwrap_or(url);
    let url = url.strip_prefix("github.com/").unwrap_or(url);
    // Trim again in case of space before trailing comma
    let url = url.trim().trim_end_matches(|c| c == ',' || c == ';');
    let parts: Vec<&str> = url.split('/').collect();
    if parts.len() >= 2 {
        let owner = parts[0].trim();
        let repo = parts[1].trim().trim_end_matches(|c| c == ',' || c == ';');
        if owner.is_empty() || repo.is_empty() {
            return Err("Invalid GitHub URL format".to_string());
        }
        // Validate characters: GitHub allows alphanumeric, hyphens, dots, and underscores
        if !owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_') {
            return Err("Invalid GitHub owner format".to_string());
        }
        if !repo.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_') {
            return Err("Invalid GitHub repo format".to_string());
        }
        Ok((owner.to_string(), repo.to_string()))
    } else {
        Err("Invalid GitHub URL format".to_string())
    }
}

pub fn extract_registry_archive(data: &[u8], dest: &PathBuf) -> Result<(usize, usize), String> {
    let temp_dir = dest.parent()
        .ok_or("Invalid destination path")?
        .join("temp_extract");
    
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).ok();
    }
    fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let cursor = Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| format!("Failed to open zip: {}", e))?;

    let mut root_prefix: Option<String> = None;
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| format!("Failed to read archive entry: {}", e))?;
        
        let name = file.name().to_string();
        
        if root_prefix.is_none() && name.contains('/') {
            root_prefix = Some(name.split('/').next().unwrap_or("").to_string() + "/");
        }
        
        let stripped_name = if let Some(ref prefix) = root_prefix {
            name.strip_prefix(prefix).unwrap_or(&name)
        } else {
            &name
        };
        
        if stripped_name.is_empty() {
            continue;
        }
        
        let outpath = temp_dir.join(stripped_name);

        // Zip-slip protection: ensure extracted path stays within temp_dir
        if !outpath.starts_with(&temp_dir) {
            continue; // Skip malicious entries with path traversal
        }

        if file.is_dir() {
            fs::create_dir_all(&outpath).ok();
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).ok();
            }
            let mut outfile = fs::File::create(&outpath)
                .map_err(|e| format!("Failed to create file: {}", e))?;
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("Failed to write file: {}", e))?;
        }
    }

    if dest.exists() {
        fs::remove_dir_all(dest).ok();
    }
    fs::create_dir_all(dest).map_err(|e| format!("Failed to create dest dir: {}", e))?;

    // GitHub zipball root is owner-repo-<sha>/, with registry/ inside. Support:
    // - community-registry/registry (legacy)
    // - registry/ (GitHub zipball: owner-repo-sha/registry/capabilities|agents)
    // - root level capabilities/ and agents/
    let registry_base = if temp_dir.join("community-registry").join("registry").exists() {
        temp_dir.join("community-registry").join("registry")
    } else if temp_dir.join("registry").exists() {
        temp_dir.join("registry")
    } else {
        temp_dir.clone()
    };

    let caps_src = registry_base.join("capabilities");
    let agents_src = registry_base.join("agents");
    let caps_dest = dest.join("capabilities");
    let agents_dest = dest.join("agents");

    let mut caps_count = 0;
    let mut agents_count = 0;

    if caps_src.exists() {
        copy_dir_recursive(&caps_src, &caps_dest)?;
        caps_count = count_json_files(&caps_dest);
    }

    if agents_src.exists() {
        copy_dir_recursive(&agents_src, &agents_dest)?;
        agents_count = count_md_files(&agents_dest);
    }

    // Also copy root-level skills/ directory (agentskills.io format: skills/<name>/SKILL.md)
    // Check both inside registry_base and at the extracted root level
    let skills_src = if registry_base.join("skills").exists() {
        Some(registry_base.join("skills"))
    } else if temp_dir.join("skills").exists() {
        Some(temp_dir.join("skills"))
    } else {
        None
    };
    if let Some(ref src) = skills_src {
        let skills_dest = dest.join("skills");
        copy_dir_recursive(src, &skills_dest)?;
        // Count SKILL.md files as capabilities
        caps_count += count_skill_dirs(&skills_dest);
    }

    // Copy new community registry format: root-level mcps/, rules/, hooks/, agents/ with category subdirs
    for type_name in &["mcps", "rules", "hooks"] {
        let type_src = if temp_dir.join(type_name).exists() {
            Some(temp_dir.join(type_name))
        } else if registry_base.join(type_name).exists() {
            Some(registry_base.join(type_name))
        } else {
            None
        };
        if let Some(ref src) = type_src {
            let type_dest = dest.join(type_name);
            copy_dir_recursive(src, &type_dest)?;
            caps_count += count_json_files(&type_dest);
        }
    }

    // Copy new agents/ with category subdirs (if not already copied from registry_base/agents)
    let new_agents_src = if !agents_src.exists() && temp_dir.join("agents").exists() {
        Some(temp_dir.join("agents"))
    } else {
        None
    };
    if let Some(ref src) = new_agents_src {
        let new_agents_dest = dest.join("agents");
        copy_dir_recursive(src, &new_agents_dest)?;
        agents_count += count_md_files(&new_agents_dest);
    }

    fs::remove_dir_all(&temp_dir).ok();

    Ok((caps_count, agents_count))
}

fn copy_dir_recursive(src: &PathBuf, dest: &PathBuf) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("Failed to create directory: {}", e))?;
    
    let entries = fs::read_dir(src).map_err(|e| format!("Failed to read directory: {}", e))?;
    
    for entry in entries.flatten() {
        let path = entry.path();
        let dest_path = dest.join(entry.file_name());
        
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path).map_err(|e| format!("Failed to copy file: {}", e))?;
        }
    }
    
    Ok(())
}

fn count_json_files(dir: &PathBuf) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_json_files(&path);
            } else if path.extension().map(|e| e == "json").unwrap_or(false) {
                count += 1;
            }
        }
    }
    count
}

fn count_skill_dirs(dir: &PathBuf) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() && entry.path().join("SKILL.md").exists() {
                count += 1;
            }
        }
    }
    count
}

fn count_md_files(dir: &PathBuf) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_md_files(&path);
            } else if path.extension().map(|e| e == "md").unwrap_or(false) {
                count += 1;
            }
        }
    }
    count
}

pub fn sync_registry(config: &SyncConfig) -> SyncResult {
    let mut state = load_sync_state();
    state.is_syncing = true;
    state.last_error = None;

    // If community dir is empty but we have a cached ETag, clear it to force re-download
    let community_path = get_community_registry_path();
    if state.last_etag.is_some() && (state.capabilities_count == 0 && state.agents_count == 0) {
        let has_files = community_path.exists()
            && fs::read_dir(&community_path)
                .map(|entries| entries.count() > 0)
                .unwrap_or(false);
        if !has_files {
            state.last_etag = None;
        }
    }

    let _ = save_sync_state(&state);

    let result = match check_for_updates(config, state.last_etag.as_deref()) {
        Ok((updated, new_etag, data)) => {
            if updated {
                if let Some(archive_data) = data {
                    let dest = get_community_registry_path();
                    match extract_registry_archive(&archive_data, &dest) {
                        Ok(_) => {
                            // File-count heuristics (README/index.json/non-recursive skill walk) are
                            // brittle. Use the real loader output so the displayed totals match what
                            // the user sees in the registry browser.
                            let caps = crate::registry::loader::load_capabilities(&[dest.clone()]).items.len();
                            let agents = crate::registry::loader::load_agents(&[dest.clone()]).items.len();

                            state.last_etag = new_etag;
                            state.capabilities_count = caps;
                            state.agents_count = agents;
                            state.last_sync_time = Some(chrono::Utc::now().to_rfc3339());

                            SyncResult {
                                success: true,
                                message: format!("Synced {} capabilities and {} agents", caps, agents),
                                new_capabilities: caps,
                                new_agents: agents,
                                updated: true,
                            }
                        }
                        Err(e) => {
                            state.last_error = Some(e.clone());
                            SyncResult {
                                success: false,
                                message: e,
                                new_capabilities: 0,
                                new_agents: 0,
                                updated: false,
                            }
                        }
                    }
                } else {
                    SyncResult {
                        success: true,
                        message: "No data to extract".to_string(),
                        new_capabilities: 0,
                        new_agents: 0,
                        updated: false,
                    }
                }
            } else {
                state.last_sync_time = Some(chrono::Utc::now().to_rfc3339());
                SyncResult {
                    success: true,
                    message: "Registry is up to date".to_string(),
                    new_capabilities: state.capabilities_count,
                    new_agents: state.agents_count,
                    updated: false,
                }
            }
        }
        Err(e) => {
            state.last_error = Some(e.clone());
            SyncResult {
                success: false,
                message: e,
                new_capabilities: 0,
                new_agents: 0,
                updated: false,
            }
        }
    };

    state.is_syncing = false;
    let _ = save_sync_state(&state);

    result
}

pub fn get_current_sync_status() -> SyncState {
    load_sync_state()
}

pub fn start_background_polling(config: SyncConfig, interval_minutes: u32) {
    let mut active = POLLING_ACTIVE.lock().unwrap();
    if *active {
        return;
    }
    *active = true;
    drop(active);

    thread::spawn(move || {
        // Run one sync immediately so the user doesn't wait a full interval
        {
            let active = POLLING_ACTIVE.lock().unwrap();
            if !*active {
                return;
            }
        }
        let _ = sync_registry(&config);

        loop {
            {
                let active = POLLING_ACTIVE.lock().unwrap();
                if !*active {
                    break;
                }
            }

            thread::sleep(Duration::from_secs((interval_minutes as u64) * 60));

            {
                let active = POLLING_ACTIVE.lock().unwrap();
                if !*active {
                    break;
                }
            }

            let _ = sync_registry(&config);
        }
    });
}

pub fn stop_background_polling() {
    let mut active = POLLING_ACTIVE.lock().unwrap();
    *active = false;
}

pub fn is_polling_active() -> bool {
    *POLLING_ACTIVE.lock().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_url_full() {
        let (owner, repo) = parse_github_url("https://github.com/abhiunix/community-registry").unwrap();
        assert_eq!(owner, "abhiunix");
        assert_eq!(repo, "community-registry");
    }

    #[test]
    fn test_parse_github_url_short() {
        let (owner, repo) = parse_github_url("abhiunix/community-registry").unwrap();
        assert_eq!(owner, "abhiunix");
        assert_eq!(repo, "community-registry");
    }

    #[test]
    fn test_parse_github_url_trailing_comma() {
        let (owner, repo) = parse_github_url("https://github.com/abhiunix/community-registry,").unwrap();
        assert_eq!(owner, "abhiunix");
        assert_eq!(repo, "community-registry");
    }

    #[test]
    fn test_parse_github_url_trim_whitespace() {
        let (owner, repo) = parse_github_url("  https://github.com/owner/repo  ").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_sync_state_default() {
        let state = SyncState::default();
        assert!(!state.is_syncing);
        assert!(state.last_etag.is_none());
    }

    #[test]
    fn test_sync_config_default() {
        let config = SyncConfig::default();
        assert_eq!(config.branch, "main");
        assert!(config.auto_update);
    }
}
