use serde::{Deserialize, Serialize};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::PathBuf;

fn db_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let p = home.join(".cursor").join("ai-tracking").join("ai-code-tracking.db");
    if p.exists() { Some(p) } else { None }
}

fn open_db() -> Result<Connection, String> {
    let path = db_path().ok_or("Cursor AI tracking database not found")?;
    Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Failed to open AI tracking database: {}", e))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredCommit {
    pub commit_hash: String,
    pub branch_name: String,
    pub scored_at: i64,
    pub lines_added: Option<i64>,
    pub lines_deleted: Option<i64>,
    pub tab_lines_added: Option<i64>,
    pub tab_lines_deleted: Option<i64>,
    pub composer_lines_added: Option<i64>,
    pub composer_lines_deleted: Option<i64>,
    pub human_lines_added: Option<i64>,
    pub human_lines_deleted: Option<i64>,
    pub blank_lines_added: Option<i64>,
    pub blank_lines_deleted: Option<i64>,
    pub commit_message: Option<String>,
    pub commit_date: Option<String>,
    pub ai_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCodeSummary {
    pub total_commits: u64,
    pub avg_ai_percentage: f64,
    pub total_composer_lines: i64,
    pub total_human_lines: i64,
    pub total_lines_added: i64,
    pub total_lines_deleted: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub conversation_id: String,
    pub title: Option<String>,
    pub tldr: Option<String>,
    pub overview: Option<String>,
    pub summary_bullets: Option<String>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTypeBreakdown {
    pub file_extension: String,
    pub source: String,
    pub count: u64,
}

fn parse_ai_percentage(v2: Option<String>, v1: Option<String>) -> f64 {
    v2.and_then(|s| s.parse::<f64>().ok())
        .or_else(|| v1.and_then(|s| s.parse::<f64>().ok()))
        .unwrap_or(0.0)
}

/// Shared row-mapping so the real command and tests (in-memory fixture DB)
/// exercise the exact same query + column layout.
fn query_commit_scores(conn: &Connection, lim: i64, off: i64) -> Result<Vec<ScoredCommit>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT commitHash, branchName, scoredAt, linesAdded, linesDeleted,
                    tabLinesAdded, tabLinesDeleted,
                    composerLinesAdded, composerLinesDeleted, humanLinesAdded, humanLinesDeleted,
                    blankLinesAdded, blankLinesDeleted, commitMessage, commitDate,
                    v1AiPercentage, v2AiPercentage
             FROM scored_commits ORDER BY scoredAt DESC LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| format!("Query prepare failed: {}", e))?;

    let rows = stmt
        .query_map([lim, off], |row| {
            let v1: Option<String> = row.get(15)?;
            let v2: Option<String> = row.get(16)?;
            Ok(ScoredCommit {
                commit_hash: row.get(0)?,
                branch_name: row.get(1)?,
                scored_at: row.get(2)?,
                lines_added: row.get(3)?,
                lines_deleted: row.get(4)?,
                tab_lines_added: row.get(5)?,
                tab_lines_deleted: row.get(6)?,
                composer_lines_added: row.get(7)?,
                composer_lines_deleted: row.get(8)?,
                human_lines_added: row.get(9)?,
                human_lines_deleted: row.get(10)?,
                blank_lines_added: row.get(11)?,
                blank_lines_deleted: row.get(12)?,
                commit_message: row.get(13)?,
                commit_date: row.get(14)?,
                ai_percentage: parse_ai_percentage(v2, v1),
            })
        })
        .map_err(|e| format!("Query failed: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        if let Ok(commit) = row {
            results.push(commit);
        }
    }
    Ok(results)
}

#[tauri::command]
pub fn get_ai_commit_scores(limit: Option<usize>, offset: Option<usize>) -> Result<Vec<ScoredCommit>, String> {
    let conn = match open_db() {
        Ok(c) => c,
        Err(_) => return Ok(vec![]),
    };

    let lim = limit.unwrap_or(200) as i64;
    let off = offset.unwrap_or(0) as i64;
    query_commit_scores(&conn, lim, off)
}

#[tauri::command]
pub fn get_ai_code_summary() -> Result<AiCodeSummary, String> {
    let conn = match open_db() {
        Ok(c) => c,
        Err(_) => {
            return Ok(AiCodeSummary {
                total_commits: 0,
                avg_ai_percentage: 0.0,
                total_composer_lines: 0,
                total_human_lines: 0,
                total_lines_added: 0,
                total_lines_deleted: 0,
            });
        }
    };

    let mut stmt = conn
        .prepare(
            "SELECT COUNT(*),
                    AVG(CAST(COALESCE(v2AiPercentage, v1AiPercentage, '0') AS REAL)),
                    COALESCE(SUM(composerLinesAdded), 0),
                    COALESCE(SUM(humanLinesAdded), 0),
                    COALESCE(SUM(linesAdded), 0),
                    COALESCE(SUM(linesDeleted), 0)
             FROM scored_commits",
        )
        .map_err(|e| format!("Query prepare failed: {}", e))?;

    let summary = stmt
        .query_row([], |row| {
            Ok(AiCodeSummary {
                total_commits: row.get::<_, i64>(0).unwrap_or(0) as u64,
                avg_ai_percentage: row.get::<_, f64>(1).unwrap_or(0.0),
                total_composer_lines: row.get(2).unwrap_or(0),
                total_human_lines: row.get(3).unwrap_or(0),
                total_lines_added: row.get(4).unwrap_or(0),
                total_lines_deleted: row.get(5).unwrap_or(0),
            })
        })
        .map_err(|e| format!("Query failed: {}", e))?;

    Ok(summary)
}

#[tauri::command]
pub fn get_conversation_summaries(limit: Option<usize>, offset: Option<usize>) -> Result<Vec<ConversationSummary>, String> {
    let conn = match open_db() {
        Ok(c) => c,
        Err(_) => return Ok(vec![]),
    };

    let lim = limit.unwrap_or(100) as i64;
    let off = offset.unwrap_or(0) as i64;

    let mut stmt = conn
        .prepare(
            "SELECT conversationId, title, tldr, overview, summaryBullets, model, mode, updatedAt
             FROM conversation_summaries ORDER BY updatedAt DESC LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| format!("Query prepare failed: {}", e))?;

    let rows = stmt
        .query_map([lim, off], |row| {
            Ok(ConversationSummary {
                conversation_id: row.get(0)?,
                title: row.get(1)?,
                tldr: row.get(2)?,
                overview: row.get(3)?,
                summary_bullets: row.get(4)?,
                model: row.get(5)?,
                mode: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|e| format!("Query failed: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        if let Ok(summary) = row {
            results.push(summary);
        }
    }
    Ok(results)
}

#[tauri::command]
pub fn get_ai_file_type_breakdown() -> Result<Vec<FileTypeBreakdown>, String> {
    let conn = match open_db() {
        Ok(c) => c,
        Err(_) => return Ok(vec![]),
    };

    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(fileExtension, 'unknown'), source, COUNT(*)
             FROM ai_code_hashes
             GROUP BY fileExtension, source
             ORDER BY COUNT(*) DESC",
        )
        .map_err(|e| format!("Query prepare failed: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(FileTypeBreakdown {
                file_extension: row.get(0)?,
                source: row.get(1)?,
                count: row.get::<_, i64>(2).unwrap_or(0) as u64,
            })
        })
        .map_err(|e| format!("Query failed: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        if let Ok(entry) = row {
            results.push(entry);
        }
    }
    Ok(results)
}

#[tauri::command]
pub fn get_ai_tracking_model_breakdown() -> Result<HashMap<String, u64>, String> {
    let conn = match open_db() {
        Ok(c) => c,
        Err(_) => return Ok(HashMap::new()),
    };

    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(model, 'unknown'), COUNT(*)
             FROM ai_code_hashes
             WHERE source != 'human'
             GROUP BY model
             ORDER BY COUNT(*) DESC",
        )
        .map_err(|e| format!("Query prepare failed: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            let model: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((model, count as u64))
        })
        .map_err(|e| format!("Query failed: {}", e))?;

    let mut map = HashMap::new();
    for row in rows {
        if let Ok((model, count)) = row {
            map.insert(model, count);
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE scored_commits (
                commitHash TEXT NOT NULL,
                branchName TEXT NOT NULL,
                scoredAt INTEGER NOT NULL, linesAdded INTEGER, linesDeleted INTEGER,
                tabLinesAdded INTEGER, tabLinesDeleted INTEGER,
                composerLinesAdded INTEGER, composerLinesDeleted INTEGER,
                humanLinesAdded INTEGER, humanLinesDeleted INTEGER,
                blankLinesAdded INTEGER, blankLinesDeleted INTEGER,
                commitMessage TEXT, commitDate TEXT, v1AiPercentage TEXT, v2AiPercentage TEXT,
                PRIMARY KEY (commitHash, branchName)
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO scored_commits
             (commitHash, branchName, scoredAt, linesAdded, linesDeleted, tabLinesAdded, tabLinesDeleted,
              composerLinesAdded, composerLinesDeleted, humanLinesAdded, humanLinesDeleted,
              blankLinesAdded, blankLinesDeleted, commitMessage, commitDate, v1AiPercentage, v2AiPercentage)
             VALUES ('abc123', 'main', 1700000000000, 40, 10, 25, 3, 30, 5, 10, 5, 0, 0,
                     'feat: thing', '2026-01-01T00:00:00Z', '0', '62.5')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn query_commit_scores_reads_tab_lines_columns() {
        let conn = fixture_conn();
        let rows = query_commit_scores(&conn, 200, 0).unwrap();
        assert_eq!(rows.len(), 1);
        let c = &rows[0];
        assert_eq!(c.commit_hash, "abc123");
        assert_eq!(c.tab_lines_added, Some(25));
        assert_eq!(c.tab_lines_deleted, Some(3));
        assert_eq!(c.lines_added, Some(40));
        assert_eq!(c.lines_deleted, Some(10));
        assert!((c.ai_percentage - 62.5).abs() < 1e-9);
    }
}
