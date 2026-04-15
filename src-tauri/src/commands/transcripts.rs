use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSession {
    pub session_id: String,
    pub project_name: String,
    pub source: String,
    pub modified_at: String,
    pub file_size_bytes: u64,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptMessage {
    pub role: String,
    pub content: String,
}

fn decode_project_path(encoded_dir_name: &str) -> String {
    let clean = encoded_dir_name.trim_start_matches('-');
    if clean.is_empty() {
        return "/".to_string();
    }

    let raw_parts: Vec<&str> = clean.split('-').collect();
    let mut segments: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw_parts.len() {
        if raw_parts[i].is_empty() {
            let mut empties: usize = 0;
            while i < raw_parts.len() && raw_parts[i].is_empty() {
                empties += 1;
                i += 1;
            }
            if i < raw_parts.len() {
                let prefix = format!(".{}", "-".repeat(empties.saturating_sub(1)));
                segments.push(format!("{}{}", prefix, raw_parts[i]));
                i += 1;
            }
        } else {
            segments.push(raw_parts[i].to_string());
            i += 1;
        }
    }

    let mut result = Vec::new();
    let sep = std::path::MAIN_SEPARATOR;
    if fs_recombine(&segments, 0, &mut result) {
        format!("{}{}", sep, result.join(&sep.to_string()))
    } else {
        format!("{}{}", sep, segments.join(&sep.to_string()))
    }
}

fn fs_recombine(segments: &[String], start: usize, result: &mut Vec<String>) -> bool {
    if start >= segments.len() {
        return true;
    }

    let base = if result.is_empty() {
        String::new()
    } else {
        format!("/{}", result.join("/"))
    };

    let mut candidate = segments[start].clone();
    for end in start..segments.len() {
        if end > start {
            candidate = format!("{}-{}", candidate, &segments[end]);
        }

        let test_path = format!("{}/{}", base, candidate);
        let is_last = end + 1 >= segments.len();

        if is_last {
            if std::path::Path::new(&test_path).exists() {
                result.push(candidate);
                return true;
            }
        } else if std::path::Path::new(&test_path).is_dir() {
            result.push(candidate.clone());
            if fs_recombine(segments, end + 1, result) {
                return true;
            }
            result.pop();
        }
    }
    false
}

fn scan_claude_transcripts() -> Vec<TranscriptSession> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return vec![],
    };
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.exists() {
        return vec![];
    }

    let mut sessions = Vec::new();
    let entries = match std::fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    for project_entry in entries.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let dir_name = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if dir_name.starts_with('.') || dir_name.is_empty() {
            continue;
        }

        let decoded_path = decode_project_path(&dir_name);

        let files = match std::fs::read_dir(&project_path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        for file_entry in files.flatten() {
            let file_path = file_entry.path();
            if !file_path.is_file() {
                continue;
            }
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "jsonl" {
                continue;
            }

            let session_id = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let meta = match std::fs::metadata(&file_path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let modified_at = meta
                .modified()
                .map(|t| {
                    let dt: chrono::DateTime<chrono::Utc> = t.into();
                    dt.to_rfc3339()
                })
                .unwrap_or_default();

            sessions.push(TranscriptSession {
                session_id,
                project_name: decoded_path.clone(),
                source: "claude".to_string(),
                modified_at,
                file_size_bytes: meta.len(),
                file_path: file_path.to_string_lossy().to_string(),
            });
        }
    }
    sessions
}

fn scan_cursor_transcripts() -> Vec<TranscriptSession> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return vec![],
    };
    let projects_dir = home.join(".cursor").join("projects");
    if !projects_dir.exists() {
        return vec![];
    }

    let mut sessions = Vec::new();
    let entries = match std::fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    for project_entry in entries.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let dir_name = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let decoded_path = decode_project_path(&dir_name);

        let transcripts_dir = project_path.join("agent-transcripts");
        if !transcripts_dir.exists() {
            continue;
        }

        let transcript_entries = match std::fs::read_dir(&transcripts_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for t_entry in transcript_entries.flatten() {
            let t_path = t_entry.path();
            if !t_path.is_dir() {
                continue;
            }
            let uuid_name = t_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if uuid_name == "subagents" {
                continue;
            }

            let jsonl_path = t_path.join(format!("{}.jsonl", uuid_name));
            if !jsonl_path.exists() {
                continue;
            }

            let meta = match std::fs::metadata(&jsonl_path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let modified_at = meta
                .modified()
                .map(|t| {
                    let dt: chrono::DateTime<chrono::Utc> = t.into();
                    dt.to_rfc3339()
                })
                .unwrap_or_default();

            sessions.push(TranscriptSession {
                session_id: uuid_name,
                project_name: decoded_path.clone(),
                source: "cursor".to_string(),
                modified_at,
                file_size_bytes: meta.len(),
                file_path: jsonl_path.to_string_lossy().to_string(),
            });
        }
    }
    sessions
}

fn is_safe_transcript_path(file_path: &str) -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };
    let path = PathBuf::from(file_path);
    let canonical = dunce::canonicalize(&path).unwrap_or(path.clone());
    let claude_dir = home.join(".claude");
    let cursor_dir = home.join(".cursor");
    canonical.starts_with(&claude_dir) || canonical.starts_with(&cursor_dir)
}

#[tauri::command]
pub fn list_transcript_sessions() -> Result<Vec<TranscriptSession>, String> {
    let mut sessions = scan_claude_transcripts();
    sessions.extend(scan_cursor_transcripts());
    sessions.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(sessions)
}

#[tauri::command]
pub fn search_transcripts(query: String) -> Result<Vec<TranscriptSession>, String> {
    if query.trim().is_empty() {
        return list_transcript_sessions();
    }

    let query_lower = query.to_lowercase();
    let all_sessions = list_transcript_sessions()?;

    let mut matching = Vec::new();
    for session in &all_sessions {
        if !is_safe_transcript_path(&session.file_path) {
            continue;
        }
        if has_matching_user_prompt(&session.file_path, &query_lower) {
            matching.push(session.clone());
        }
    }

    Ok(matching)
}

fn has_matching_user_prompt(file_path: &str, query: &str) -> bool {
    let path = PathBuf::from(file_path);
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let json: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let role_str = json.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let is_user = if role_str == "user" {
            true
        } else {
            json.get("type").and_then(|v| v.as_str()).unwrap_or("") == "user"
        };

        if !is_user {
            continue;
        }

        let content = extract_text_content(&json);
        if content.to_lowercase().contains(query) {
            return true;
        }
    }
    false
}

#[tauri::command]
pub fn read_transcript(
    file_path: String,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<TranscriptMessage>, String> {
    if !is_safe_transcript_path(&file_path) {
        return Err("Invalid transcript path".to_string());
    }

    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Err("Transcript file not found".to_string());
    }

    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let off = offset.unwrap_or(0);
    let lim = limit.unwrap_or(50);

    let mut messages = Vec::new();
    let mut count = 0;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let json: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let role_str = json.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let role = if role_str == "user" || role_str == "assistant" {
            role_str
        } else {
            json.get("type").and_then(|v| v.as_str()).unwrap_or("")
        };
        if role != "user" && role != "assistant" {
            continue;
        }

        if count < off {
            count += 1;
            continue;
        }
        if messages.len() >= lim {
            break;
        }

        let content = extract_text_content(&json);
        if content.is_empty() {
            continue;
        }

        messages.push(TranscriptMessage {
            role: role.to_string(),
            content,
        });
        count += 1;
    }

    Ok(messages)
}

fn extract_text_content(json: &serde_json::Value) -> String {
    if let Some(content) = json.get("message").and_then(|m| m.get("content")) {
        return extract_from_content_value(content);
    }
    if let Some(content) = json.get("content") {
        return extract_from_content_value(content);
    }
    String::new()
}

fn extract_from_content_value(content: &serde_json::Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        let texts: Vec<String> = arr
            .iter()
            .filter_map(|item| {
                let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if item_type == "text" {
                    item.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect();
        return texts.join("\n");
    }
    String::new()
}
