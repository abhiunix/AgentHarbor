use keyring::Entry;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

const SERVICE_NAME: &str = "com.agentharbor.app";

/// Secret names that AgentHarbor features depend on. These always appear in the
/// Secrets Manager (even when unset), can be edited/revealed, but cannot be
/// deleted. Keep in sync with the consumers (debate.rs, CapabilityEditor, etc.).
pub const RESERVED_SECRETS: [&str; 3] = ["GITHUB_TOKEN", "ANTHROPIC_API_KEY", "OPENAI_API_KEY"];

pub fn is_reserved(key: &str) -> bool {
    RESERVED_SECRETS.contains(&key)
}

/// Process-lifetime cache for successful keychain reads.
///
/// macOS prompts the user every time an unsigned (or re-built dev) binary
/// reads a key from the Keychain — "Always Allow" only sticks for a stable
/// code-signed binary. Without caching, a single debate run can trigger 6+
/// prompts (check_credentials × 2 keys, start_debate × 2 keys, the Secrets
/// modal's Reveal, etc.). Caching collapses that to one prompt per key per
/// app launch.
///
/// `None` means "we asked and the OS confirmed there is no entry" (also
/// cached so we don't re-prompt). The cache is invalidated on store/delete.
lazy_static::lazy_static! {
    static ref SECRET_CACHE: Mutex<HashMap<String, Option<String>>> = Mutex::new(HashMap::new());
}

fn cache_get(key: &str) -> Option<Option<String>> {
    SECRET_CACHE.lock().ok().and_then(|g| g.get(key).cloned())
}

fn cache_put(key: &str, value: Option<String>) {
    if let Ok(mut g) = SECRET_CACHE.lock() {
        g.insert(key.to_string(), value);
    }
}

fn cache_invalidate(key: &str) {
    if let Ok(mut g) = SECRET_CACHE.lock() {
        g.remove(key);
    }
}

fn get_secrets_index_path() -> PathBuf {
    crate::utils::paths::app_data_dir().join("secrets-index.json")
}

fn load_secret_keys() -> HashSet<String> {
    let path = get_secrets_index_path();
    if !path.exists() {
        return HashSet::new();
    }

    fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_secret_keys(keys: &HashSet<String>) -> Result<(), String> {
    let path = get_secrets_index_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let json = serde_json::to_string_pretty(keys).map_err(|e| e.to_string())?;
    crate::utils::paths::atomic_write_str(&path, &json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn get_entry(key: &str) -> Result<Entry, String> {
    Entry::new(SERVICE_NAME, key)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))
}

pub fn store_secret(key: &str, value: &str) -> Result<(), String> {
    get_entry(key)?
        .set_password(value)
        .map_err(|e| format!("Failed to store secret: {}", e))?;

    let mut keys = load_secret_keys();
    keys.insert(key.to_string());
    save_secret_keys(&keys)?;

    // Refresh the cache with the just-written value so the next read
    // doesn't re-prompt the user.
    cache_put(key, Some(value.to_string()));

    Ok(())
}

pub fn get_secret(key: &str) -> Result<Option<String>, String> {
    // Process-lifetime cache: avoids one OS keychain prompt per call after the
    // first successful read.
    if let Some(cached) = cache_get(key) {
        return Ok(cached);
    }
    let value = match get_entry(key)?.get_password() {
        Ok(password) => Some(password),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => return Err(format!("Failed to get secret: {}", e)),
    };
    cache_put(key, value.clone());
    Ok(value)
}

pub fn delete_secret(key: &str) -> Result<(), String> {
    if is_reserved(key) {
        return Err(format!("'{}' is a reserved secret and cannot be deleted.", key));
    }
    match get_entry(key)?.delete_credential() {
        Ok(_) => {}
        Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(format!("Failed to delete secret: {}", e)),
    }

    let mut keys = load_secret_keys();
    keys.remove(key);
    save_secret_keys(&keys)?;

    cache_invalidate(key);

    Ok(())
}

pub fn list_secrets() -> Vec<String> {
    let keys = load_secret_keys();
    let mut sorted: Vec<_> = keys.into_iter().collect();
    sorted.sort();
    sorted
}

/// Lightweight "does this key exist?" check that consults the local index
/// (`secrets-index.json`) instead of probing the OS keychain. Returns true
/// when the key was written via `store_secret`; the value is NOT loaded, so
/// this triggers ZERO keychain prompts. Use this for UI existence badges;
/// for actual value access call `get_secret`.
pub fn is_known(key: &str) -> bool {
    load_secret_keys().contains(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_secrets_are_recognized() {
        assert!(is_reserved("GITHUB_TOKEN"));
        assert!(is_reserved("ANTHROPIC_API_KEY"));
        assert!(is_reserved("OPENAI_API_KEY"));
        assert!(!is_reserved("MY_CUSTOM_KEY"));
    }

    #[test]
    fn delete_reserved_secret_is_rejected() {
        // Returns early before touching the keychain.
        let err = delete_secret("GITHUB_TOKEN").unwrap_err();
        assert!(err.contains("reserved"));
    }
}
