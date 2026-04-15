use crate::utils::keychain;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretInfo {
    pub key: String,
    pub has_value: bool,
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
    keychain::list_secrets()
        .into_iter()
        .map(|key| SecretInfo {
            key,
            has_value: true,
        })
        .collect()
}

#[tauri::command]
pub fn get_secrets_count() -> u32 {
    keychain::list_secrets().len() as u32
}
