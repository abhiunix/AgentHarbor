use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawPromptLine {
    display: Option<String>,
    timestamp: Option<u64>,
    project: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptEntry {
    pub display: String,
    pub timestamp: String,
    pub timestamp_ms: u64,
    pub project: Option<String>,
    pub project_name: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptStats {
    pub total: usize,
    pub projects: Vec<String>,
}

fn history_path() -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    let p = home.join(".claude").join("history.jsonl");
    if p.exists() { Some(p) } else { None }
}

fn read_all_prompts() -> Result<Vec<PromptEntry>, String> {
    let path = match history_path() {
        Some(p) => p,
        None => return Ok(vec![]),
    };
    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        let raw: RawPromptLine = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let display = raw.display.unwrap_or_default();
        if display.trim().is_empty() { continue; }

        let ts_ms = raw.timestamp.unwrap_or(0);
        let timestamp = chrono::DateTime::from_timestamp_millis(ts_ms as i64)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        let project_name = raw.project.as_ref().and_then(|p| {
            std::path::Path::new(p).file_name().and_then(|n| n.to_str()).map(String::from)
        });

        entries.push(PromptEntry {
            display,
            timestamp,
            timestamp_ms: ts_ms,
            project: raw.project,
            project_name,
            session_id: raw.session_id,
        });
    }

    entries.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
    Ok(entries)
}

#[tauri::command]
pub fn get_prompt_history(limit: Option<usize>, offset: Option<usize>) -> Result<Vec<PromptEntry>, String> {
    let all = read_all_prompts()?;
    let off = offset.unwrap_or(0);
    let lim = limit.unwrap_or(100);
    Ok(all.into_iter().skip(off).take(lim).collect())
}

#[tauri::command]
pub fn search_prompt_history(query: String) -> Result<Vec<PromptEntry>, String> {
    let all = read_all_prompts()?;
    let q = query.to_lowercase();
    let results: Vec<PromptEntry> = all
        .into_iter()
        .filter(|e| e.display.to_lowercase().contains(&q))
        .take(200)
        .collect();
    Ok(results)
}

/// Build the `claude --resume <session-id>` command for a session.
fn resume_command(session_id: &str, project: Option<&str>) -> String {
    let resume = format!("claude --resume {}", session_id);
    match project {
        Some(p) if !p.is_empty() => format!("cd {} && {}", shell_quote(p), resume),
        _ => resume,
    }
}

/// Single-quote a path for POSIX shells, escaping embedded single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[tauri::command]
pub fn build_resume_command(session_id: String, project: Option<String>) -> String {
    resume_command(&session_id, project.as_deref())
}

#[tauri::command]
pub fn start_claude_session(session_id: String, project: Option<String>) -> Result<(), String> {
    let command = resume_command(&session_id, project.as_deref());
    crate::utils::platform::launch_in_terminal(&command)
}

#[tauri::command]
pub fn get_prompt_stats() -> Result<PromptStats, String> {
    let all = read_all_prompts()?;
    let mut project_set: HashMap<String, bool> = HashMap::new();
    for entry in &all {
        if let Some(ref p) = entry.project_name {
            project_set.insert(p.clone(), true);
        }
    }
    let mut projects: Vec<String> = project_set.into_keys().collect();
    projects.sort();
    Ok(PromptStats {
        total: all.len(),
        projects,
    })
}
