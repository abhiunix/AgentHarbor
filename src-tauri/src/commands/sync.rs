use crate::registry::{
    get_community_registry_path, get_current_sync_status, is_polling_active,
    start_background_polling, stop_background_polling, sync_registry, SyncConfig, SyncResult,
    SyncState,
};
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub fn sync_registry_now(config: SyncConfig, app: AppHandle) -> SyncResult {
    let result = sync_registry(&config);
    
    if result.success && result.updated {
        let _ = app.emit("registry-updated", &result);
        
        if let Ok(app_handle) = tauri::async_runtime::block_on(async {
            Ok::<_, ()>(app.clone())
        }) {
            use tauri_plugin_notification::NotificationExt;
            let _ = app_handle
                .notification()
                .builder()
                .title("AgentHarbor")
                .body(format!(
                    "{} new capabilities, {} new agents available",
                    result.new_capabilities, result.new_agents
                ))
                .show();
        }
    }
    
    result
}

#[tauri::command]
pub fn get_sync_status() -> SyncState {
    get_current_sync_status()
}

#[tauri::command]
pub fn start_registry_polling(config: SyncConfig) {
    let interval = config.polling_interval_minutes;
    start_background_polling(config, interval);
}

#[tauri::command]
pub fn stop_registry_polling() {
    stop_background_polling();
}

#[tauri::command]
pub fn is_registry_polling_active() -> bool {
    is_polling_active()
}

#[tauri::command]
pub fn get_community_registry_dir() -> String {
    get_community_registry_path().to_string_lossy().to_string()
}
