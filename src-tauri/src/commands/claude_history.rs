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

// ── Transcript retention status ─────────────────────────────────────────────
//
// Claude Code prunes `~/.claude/projects/**` on startup using the
// `cleanupPeriodDays` setting (default 30). The deleted transcripts are the
// only place per-message token and cost data lives, so analytics silently
// loses history. `~/.claude/history.jsonl` is NOT pruned, so comparing the
// sessions it lists against the transcripts still on disk quantifies the loss.

#[derive(serde::Serialize, Debug, Default)]
pub struct ClaudeRetentionStatus {
    /// `cleanupPeriodDays` from settings.json, if explicitly set.
    pub cleanup_period_days: Option<u32>,
    /// Effective retention window (the default when unset).
    pub effective_days: u32,
    /// True when no explicit value is set and the default applies.
    pub using_default: bool,
    /// Distinct sessions ever recorded in history.jsonl.
    pub sessions_in_history: usize,
    /// Sessions whose transcript still exists on disk.
    pub transcripts_on_disk: usize,
    /// Sessions whose transcript has been pruned.
    pub sessions_pruned: usize,
    /// Earliest session date in history.jsonl (`YYYY-MM-DD`).
    pub earliest_session: Option<String>,
    /// Earliest date still backed by a transcript (`YYYY-MM-DD`).
    pub earliest_transcript: Option<String>,
}

const DEFAULT_CLEANUP_DAYS: u32 = 30;

/// Report how much session history has been pruned by Claude Code's cleanup.
#[tauri::command]
pub async fn get_claude_retention_status() -> Result<ClaudeRetentionStatus, String> {
    tauri::async_runtime::spawn_blocking(claude_retention_status_sync)
        .await
        .map_err(|error| format!("Retention status worker failed: {error}"))?
}

fn claude_retention_status_sync() -> Result<ClaudeRetentionStatus, String> {
    use std::collections::HashSet;
    use std::io::BufRead;

    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let claude = home.join(".claude");

    let cleanup_period_days = std::fs::read_to_string(claude.join("settings.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|json| {
            json.get("cleanupPeriodDays")
                .and_then(serde_json::Value::as_u64)
                .map(|value| value as u32)
        });

    // Transcripts still on disk: ~/.claude/projects/<project>/<sessionId>.jsonl
    let mut on_disk: HashSet<String> = HashSet::new();
    let mut earliest_transcript: Option<String> = None;
    if let Ok(projects) = std::fs::read_dir(claude.join("projects")) {
        for project in projects.flatten() {
            let Ok(files) = std::fs::read_dir(project.path()) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    on_disk.insert(stem.to_string());
                }
                if let Some(date) = std::fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| {
                        let dt: chrono::DateTime<chrono::Local> = t.into();
                        dt.format("%Y-%m-%d").to_string()
                    })
                {
                    if earliest_transcript.as_ref().is_none_or(|cur| date < *cur) {
                        earliest_transcript = Some(date);
                    }
                }
            }
        }
    }

    // history.jsonl survives cleanup and lists every session ever run.
    let mut in_history: HashSet<String> = HashSet::new();
    let mut earliest_ms: Option<i64> = None;
    if let Ok(file) = std::fs::File::open(claude.join("history.jsonl")) {
        for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if let Some(id) = value.get("sessionId").and_then(serde_json::Value::as_str) {
                in_history.insert(id.to_string());
            }
            if let Some(ts) = value.get("timestamp").and_then(serde_json::Value::as_i64) {
                if earliest_ms.is_none_or(|cur| ts < cur) {
                    earliest_ms = Some(ts);
                }
            }
        }
    }

    let earliest_session = earliest_ms.and_then(|ms| {
        chrono::DateTime::from_timestamp_millis(ms).map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
    });

    let pruned = in_history.difference(&on_disk).count();

    Ok(ClaudeRetentionStatus {
        cleanup_period_days,
        effective_days: cleanup_period_days.unwrap_or(DEFAULT_CLEANUP_DAYS),
        using_default: cleanup_period_days.is_none(),
        sessions_in_history: in_history.len(),
        transcripts_on_disk: on_disk.len(),
        sessions_pruned: pruned,
        earliest_session,
        earliest_transcript,
    })
}
