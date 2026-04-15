use keyring::Entry;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

const SERVICE_NAME: &str = "com.agentharbor.app";

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

    Ok(())
}

pub fn get_secret(key: &str) -> Result<Option<String>, String> {
    match get_entry(key)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to get secret: {}", e)),
    }
}

pub fn delete_secret(key: &str) -> Result<(), String> {
    match get_entry(key)?.delete_credential() {
        Ok(_) => {}
        Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(format!("Failed to delete secret: {}", e)),
    }

    let mut keys = load_secret_keys();
    keys.remove(key);
    save_secret_keys(&keys)?;

    Ok(())
}

pub fn list_secrets() -> Vec<String> {
    let keys = load_secret_keys();
    let mut sorted: Vec<_> = keys.into_iter().collect();
    sorted.sort();
    sorted
}
