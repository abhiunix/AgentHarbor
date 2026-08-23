//! Parse Claude Code history.jsonl and sessions/*.json for analytics.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub display: String,
    pub timestamp: u64,
    pub project: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    pub pid: u64,
    pub session_id: String,
    pub cwd: String,
    pub started_at: u64,
    pub is_running: bool,
}

/// Parse ~/.claude/history.jsonl
#[tauri::command]
pub fn get_claude_history(limit: Option<u32>) -> Result<Vec<HistoryEntry>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".claude").join("history.jsonl");
    if !path.exists() {
        return Ok(vec![]);
    }

    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let json: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let display = json.get("display").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let timestamp = json.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0);
        let project = json.get("project").and_then(|v| v.as_str()).map(String::from);
        let session_id = json.get("sessionId").and_then(|v| v.as_str()).map(String::from);

        if display.is_empty() && timestamp == 0 {
            continue;
        }

        entries.push(HistoryEntry {
            display,
            timestamp,
            project,
            session_id,
        });
    }

    // Reverse for newest first
    entries.reverse();

    if let Some(limit) = limit {
        entries.truncate(limit as usize);
    }

    Ok(entries)
}

/// Parse ~/.claude/sessions/*.json and check PID liveness
#[tauri::command]
pub fn get_claude_active_sessions() -> Result<Vec<ActiveSession>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let sessions_dir = home.join(".claude").join("sessions");
    if !sessions_dir.exists() {
        return Ok(vec![]);
    }

    let mut sessions = Vec::new();
    let entries = std::fs::read_dir(&sessions_dir).map_err(|e| e.to_string())?;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let pid = json.get("pid").and_then(|v| v.as_u64()).unwrap_or(0);
        let session_id = json.get("sessionId").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let cwd = json.get("cwd").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let started_at = json.get("startedAt").and_then(|v| v.as_u64()).unwrap_or(0);

        if pid == 0 || session_id.is_empty() {
            continue;
        }

        let is_running = check_pid_alive(pid as u32);

        sessions.push(ActiveSession {
            pid,
            session_id,
            cwd,
            started_at,
            is_running,
        });
    }

    // Sort by started_at descending (newest first)
    sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(sessions)
}

/// Check if a PID is still running
fn check_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // kill(pid, 0) returns 0 if process exists
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(windows)]
    {
        // OpenProcess instead of spawning `tasklist`: the app has no console,
        // so each spawned console process flashes a visible window (one per
        // session file on every analytics refresh).
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            let mut exit_code: u32 = 0;
            let ok = GetExitCodeProcess(handle, &mut exit_code);
            CloseHandle(handle);
            ok != 0 && exit_code == STILL_ACTIVE as u32
        }
    }
}
