use crate::utils::drift::{
    self, DriftInfo,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftDiff {
    pub expected: String,
    pub current: String,
}

#[tauri::command]
pub fn detect_drift(project_path: String) -> DriftInfo {
    drift::detect_drift(&project_path)
}

#[tauri::command]
pub fn accept_drift(project_path: String) -> Result<(), String> {
    drift::accept_drift(&project_path)
}

#[tauri::command]
pub fn restore_drift(project_path: String) -> Result<(), String> {
    drift::restore_drift(&project_path)
}

#[tauri::command]
pub fn get_drift_diff(project_path: String, file_path: String) -> Option<DriftDiff> {
    drift::get_drift_diff(&project_path, &file_path).map(|(expected, current)| DriftDiff {
        expected,
        current,
    })
}
