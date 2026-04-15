//! Test bridge commands — file-based IPC for GUI test automation.
//! The Python test runner writes commands, the React TestBridge reads them via these commands.

use std::fs;

const CMD_FILE: &str = "/tmp/agentharbor_test_cmd.json";
const RESULT_FILE: &str = "/tmp/agentharbor_test_result.json";

/// Read the pending test command (if any). Returns empty string if no command.
#[tauri::command]
pub fn test_bridge_read_cmd() -> Result<String, String> {
    match fs::read_to_string(CMD_FILE) {
        Ok(content) => {
            // Remove the file after reading so we don't re-process it
            let _ = fs::remove_file(CMD_FILE);
            Ok(content)
        }
        Err(_) => Ok(String::new()),
    }
}

/// Write a test result from the React app.
#[tauri::command]
pub fn test_bridge_write_result(data: String) -> Result<(), String> {
    fs::write(RESULT_FILE, data).map_err(|e| e.to_string())
}
