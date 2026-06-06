use crate::utils::keychain;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretInfo {
    pub key: String,
    pub has_value: bool,
    pub reserved: bool,
}

#[tauri::command]
pub fn store_secret(key: String, value: String) -> Result<(), String> {
    keychain::store_secret(&key, &value)
}

#[tauri::command]
pub fn get_secret(key: String) -> Result<Option<String>, String> {
    keychain::get_secret(&key)
}

#[tauri::command]
pub fn delete_secret(key: String) -> Result<(), String> {
    keychain::delete_secret(&key)
}

#[tauri::command]
pub fn list_secrets() -> Vec<SecretInfo> {
    let stored: Vec<String> = keychain::list_secrets();

    // Reserved secrets always appear, even when unset, and sort first.
    let mut out: Vec<SecretInfo> = keychain::RESERVED_SECRETS
        .iter()
        .map(|&key| SecretInfo {
            key: key.to_string(),
            has_value: keychain::is_known(key),
            reserved: true,
        })
        .collect();

    for key in stored {
        if keychain::is_reserved(&key) {
            continue;
        }
        out.push(SecretInfo {
            key,
            has_value: true,
            reserved: false,
        });
    }

    out
}

#[tauri::command]
pub fn get_secrets_count() -> u32 {
    list_secrets().iter().filter(|s| s.has_value).count() as u32
}
