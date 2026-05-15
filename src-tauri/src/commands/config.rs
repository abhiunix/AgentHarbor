use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

static SETTINGS_MUTEX: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    pub theme: String,
    pub launch_at_login: bool,
    pub username: String,
    #[serde(default)]
    pub show_in_menu_bar: bool,
    #[serde(default)]
    pub keep_running_on_close: bool,
    #[serde(default)]
    pub author_id: Option<String>,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            launch_at_login: false,
            username: "user".to_string(),
            show_in_menu_bar: false,
            keep_running_on_close: false,
            author_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySettings {
    pub github_repo: String,
    pub github_branch: String,
    pub poll_interval_minutes: u32,
    pub auto_update: bool,
    pub last_sync: Option<String>,
}

impl Default for RegistrySettings {
    fn default() -> Self {
        Self {
            github_repo: "https://github.com/abhiunix/community-registry".to_string(),
            github_branch: "main".to_string(),
            poll_interval_minutes: 30,
            auto_update: true,
            last_sync: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploySettings {
    pub default_strategy: String,
    pub create_backups: bool,
    pub default_adapters: Vec<String>,
}

impl Default for DeploySettings {
    fn default() -> Self {
        Self {
            default_strategy: "merge".to_string(),
            create_backups: true,
            default_adapters: vec!["claude-code".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecretsInfo {
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsSettings {
    pub refresh_interval_minutes: u64,
    /// Show Anthropic internal / experimental usage buckets (omelette, tangelo, …) in Claude analytics.
    #[serde(default)]
    pub show_internal_usage_buckets: bool,
    /// Emit native macOS notifications when usage limits change state.
    #[serde(default = "default_limit_notifications_enabled")]
    pub limit_notifications_enabled: bool,
}

fn default_limit_notifications_enabled() -> bool {
    true
}

impl Default for AnalyticsSettings {
    fn default() -> Self {
        Self {
            refresh_interval_minutes: 5,
            show_internal_usage_buckets: false,
            limit_notifications_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ClaudeCodeProvider {
    #[default]
    Anthropic,
    Ollama,
}


fn default_ollama_base_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_ollama_auth_token() -> String {
    "ollama".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeSettings {
    #[serde(default)]
    pub provider: ClaudeCodeProvider,
    #[serde(default = "default_ollama_base_url")]
    pub ollama_base_url: String,
    #[serde(default)]
    pub ollama_model: String,
    #[serde(default = "default_ollama_auth_token")]
    pub ollama_auth_token: String,
}

impl Default for ClaudeCodeSettings {
    fn default() -> Self {
        Self {
            provider: ClaudeCodeProvider::default(),
            ollama_base_url: default_ollama_base_url(),
            ollama_model: String::new(),
            ollama_auth_token: default_ollama_auth_token(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub general: GeneralSettings,
    pub registry: RegistrySettings,
    pub deploy: DeploySettings,
    pub secrets: SecretsInfo,
    #[serde(default)]
    pub analytics: AnalyticsSettings,
    #[serde(default)]
    pub claude_code: ClaudeCodeSettings,
}

fn get_settings_file_path() -> PathBuf {
    crate::utils::paths::app_data_dir().join("settings.json")
}

/// Read settings from disk (used by analytics background tasks).
pub(crate) fn load_settings() -> AppSettings {
    let path = get_settings_file_path();
    if !path.exists() {
        return AppSettings::default();
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = get_settings_file_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let content = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    crate::utils::paths::atomic_write_str(&path, &content)?;

    Ok(())
}

#[tauri::command]
pub fn get_settings() -> AppSettings {
    let mut settings = load_settings();
    settings.secrets.count = crate::utils::keychain::list_secrets().len() as u32;
    settings
}

#[tauri::command]
pub fn update_settings(settings: AppSettings) -> Result<AppSettings, String> {
    let _lock = SETTINGS_MUTEX.lock().map_err(|e| format!("Settings lock error: {}", e))?;
    save_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn update_general_settings(general: GeneralSettings) -> Result<AppSettings, String> {
    let _lock = SETTINGS_MUTEX.lock().map_err(|e| format!("Settings lock error: {}", e))?;
    let mut settings = load_settings();
    settings.general = general;
    save_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn update_registry_settings(registry: RegistrySettings) -> Result<AppSettings, String> {
    let _lock = SETTINGS_MUTEX.lock().map_err(|e| format!("Settings lock error: {}", e))?;
    let mut settings = load_settings();
    settings.registry = registry;
    save_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn update_deploy_settings(deploy: DeploySettings) -> Result<AppSettings, String> {
    let _lock = SETTINGS_MUTEX.lock().map_err(|e| format!("Settings lock error: {}", e))?;
    let mut settings = load_settings();
    settings.deploy = deploy;
    save_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn update_analytics_settings(analytics: AnalyticsSettings) -> Result<AppSettings, String> {
    let _lock = SETTINGS_MUTEX.lock().map_err(|e| format!("Settings lock error: {}", e))?;
    let mut settings = load_settings();
    settings.analytics = analytics;
    save_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn get_username() -> String {
    load_settings().general.username
}

#[tauri::command]
pub fn get_author_id() -> String {
    let _lock = SETTINGS_MUTEX.lock().ok();
    let mut settings = load_settings();
    if settings.general.author_id.as_deref().is_none_or(|s| s.is_empty()) {
        let new_id = uuid::Uuid::new_v4().to_string();
        settings.general.author_id = Some(new_id.clone());
        let _ = save_settings(&settings);
        return new_id;
    }
    settings.general.author_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

#[tauri::command]
pub fn get_claude_code_settings() -> ClaudeCodeSettings {
    load_settings().claude_code
}

#[tauri::command]
pub fn apply_claude_code_provider(cc: ClaudeCodeSettings) -> Result<AppSettings, String> {
    let _lock = SETTINGS_MUTEX
        .lock()
        .map_err(|e| format!("Settings lock error: {}", e))?;

    if cc.provider == ClaudeCodeProvider::Ollama {
        if cc.ollama_model.trim().is_empty() {
            return Err("Model is required for Ollama".to_string());
        }
        let url = cc.ollama_base_url.trim();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("Base URL must start with http:// or https://".to_string());
        }
    }

    crate::commands::global_config::mutate_claude_settings_env(&cc)?;

    let mut settings = load_settings();
    settings.claude_code = cc;
    save_settings(&settings)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = AppSettings::default();
        assert_eq!(settings.general.theme, "dark");
        assert!(!settings.general.launch_at_login);
        assert_eq!(settings.deploy.default_strategy, "merge");
        assert!(settings.deploy.create_backups);
    }

    #[test]
    fn test_default_adapters() {
        let settings = AppSettings::default();
        assert!(settings.deploy.default_adapters.contains(&"claude-code".to_string()));
    }

    #[test]
    fn test_default_claude_code_settings() {
        let cc = ClaudeCodeSettings::default();
        assert_eq!(cc.provider, ClaudeCodeProvider::Anthropic);
        assert_eq!(cc.ollama_base_url, "http://localhost:11434");
        assert_eq!(cc.ollama_auth_token, "ollama");
        assert_eq!(cc.ollama_model, "");
    }
}
