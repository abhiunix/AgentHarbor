//! Provider token storage — file-based primary, keychain as fallback.
//! Tokens are stored in a JSON file in the app data directory to avoid
//! triggering macOS Keychain password prompts on every access.

use crate::utils::keychain;
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

const PREFIX: &str = "analytics";

/// File-based token cache path
fn token_file_path() -> std::path::PathBuf {
    crate::utils::paths::app_data_dir().join("provider-tokens.json")
}

lazy_static::lazy_static! {
    /// In-memory cache to avoid reading the file on every call
    static ref TOKEN_CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
}

fn provider_key(provider_id: &str, key_type: &str) -> String {
    format!("{}:{}:{}", PREFIX, provider_id, key_type)
}

/// Load tokens from file into memory cache
fn load_file_tokens() -> HashMap<String, String> {
    let path = token_file_path();
    if !path.exists() {
        return HashMap::new();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Save tokens from memory cache to file
fn save_file_tokens(tokens: &HashMap<String, String>) -> Result<(), String> {
    let path = token_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(tokens).map_err(|e| e.to_string())?;
    crate::utils::paths::atomic_write_str(&path, &json)?;
    // Restrict permissions to owner-only (0600) on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Get the in-memory cache, loading from file if needed
fn get_cache() -> HashMap<String, String> {
    let mut guard = TOKEN_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(load_file_tokens());
    }
    guard.as_ref().unwrap().clone()
}

/// Store a token for a provider. Writes to file (no keychain prompt).
pub fn store_provider_token(provider_id: &str, key_type: &str, value: &str) -> Result<(), String> {
    let key = provider_key(provider_id, key_type);
    let mut tokens = get_cache();
    tokens.insert(key.clone(), value.to_string());
    save_file_tokens(&tokens)?;
    // Update in-memory cache
    if let Ok(mut guard) = TOKEN_CACHE.lock() {
        *guard = Some(tokens);
    }
    // Also store in keychain as backup (ignore errors — not critical)
    let _ = keychain::store_secret(&key, value);
    Ok(())
}

/// Retrieve a token for a provider. Checks file first (no prompt), then keychain.
pub fn get_provider_token(provider_id: &str, key_type: &str) -> Result<Option<String>, String> {
    let key = provider_key(provider_id, key_type);
    // 1. File-based cache (instant, no prompt)
    let tokens = get_cache();
    if let Some(val) = tokens.get(&key) {
        if !val.is_empty() {
            return Ok(Some(val.clone()));
        }
    }
    // 2. Do NOT fall back to keychain here — that triggers the password prompt.
    // Keychain is only accessed via explicit user action (Import from Keychain button).
    Ok(None)
}

/// Delete a provider's stored token from both file and keychain.
pub fn delete_provider_token(provider_id: &str, key_type: &str) -> Result<(), String> {
    let key = provider_key(provider_id, key_type);
    let mut tokens = get_cache();
    tokens.remove(&key);
    save_file_tokens(&tokens)?;
    if let Ok(mut guard) = TOKEN_CACHE.lock() {
        *guard = Some(tokens);
    }
    let _ = keychain::delete_secret(&key);
    Ok(())
}

/// Check if a provider has a stored token (file-based, no keychain prompt).
pub fn has_provider_token(provider_id: &str, key_type: &str) -> bool {
    matches!(get_provider_token(provider_id, key_type), Ok(Some(_)))
}

/// List all analytics-related keys.
pub fn list_provider_tokens() -> Vec<String> {
    get_cache()
        .keys()
        .filter(|k| k.starts_with(PREFIX))
        .cloned()
        .collect()
}
