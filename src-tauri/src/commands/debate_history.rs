//! Persistence + querying for completed debates.
//!
//! Storage layout under `<app_data>/debates/`:
//!   - `<debate_id>.json`   — full `DebateRecord` (one per debate)
//!   - `index.json`         — array of `DebateSummary`, newest-first
//!
//! The per-debate file is the source of truth. The index is a small,
//! denormalized cache rebuilt by each save/delete; if it gets out of sync
//! with the on-disk files we simply re-read it lazily — readers tolerate
//! missing/corrupt data by returning `[]`.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::utils::paths;

// ── Types ───────────────────────────────────────────────────────────────────

/// One tool invocation made by an author/reviewer during a round. Persisted
/// inline on the round so the history view can replay what the model looked
/// at. `output` is truncated at 2 KiB during save to avoid bloating the file
/// — full results are never persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateToolCallRecord {
    pub tool: String,
    /// Serialized JSON of the model's tool input.
    pub input: String,
    /// Truncated tool result text (≤ 2 KiB).
    pub output: String,
    pub is_error: bool,
}

/// One model turn within a debate. Verdict is only meaningful for reviewer rounds.
///
/// LEGACY type used by the old round-pair debate engine (one author turn +
/// one reviewer turn per "round"). Kept around for back-compat: pre-v2
/// debates on disk still deserialize via this. New debates use
/// [`DebateTurn`] instead and leave `rounds` empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateRoundRecord {
    pub round: u32,
    pub role: String,               // "author" | "reviewer"
    pub model: String,
    pub full_text: String,
    #[serde(default)]
    pub verdict: Option<String>,    // "APPROVED" | "REVISE"
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub tool_calls: Vec<DebateToolCallRecord>,
    /// `system` prompt sent to the model for this round. Stored verbatim for
    /// the history "Inspect" view; empty on records written before this field
    /// existed.
    #[serde(default)]
    pub system_prompt: String,
    /// First-turn `user` content. Tool follow-up turns are not stored verbatim
    /// — those are captured in `tool_calls` instead.
    #[serde(default)]
    pub user_prompt: String,
}

/// One model turn in the v2 turn-based debate engine. Speaker + kind tag the
/// position in the flow (opening/critique/response/finalize). `parsed` is the
/// tagged-by-kind structured payload extracted from `raw_text`; `parse_error`
/// is set when both the initial parse and the corrective retry failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateTurn {
    pub index: u32,
    pub speaker: String, // "author" | "reviewer"
    pub kind: String,    // "opening" | "critique" | "response" | "finalize"
    pub model: String,
    pub raw_text: String,
    #[serde(default)]
    pub parsed: Option<serde_json::Value>,
    #[serde(default)]
    pub parse_error: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub user_prompt: String,
    #[serde(default)]
    pub tool_calls: Vec<DebateToolCallRecord>,
}

/// Full debate record persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateRecord {
    pub id: String,
    pub created_at: String, // RFC3339
    pub plan_path: String,
    pub plan_name: String,
    #[serde(default)]
    pub project_dir: String,
    /// Where the refined plan was auto-saved on disk
    /// (`<plan_dir>/<basename>_debate_<N>.md`). Empty when no save was
    /// possible (e.g. plan came from a non-writable location).
    #[serde(default)]
    pub refined_plan_path: String,
    pub author_provider: String,
    pub author_model: String,
    pub reviewer_provider: String,
    pub reviewer_model: String,
    pub max_rounds: u32,
    pub rounds_used: u32,
    pub approved: bool,
    pub original_plan: String,
    pub final_plan: String,
    /// LEGACY: only populated for pre-v2 debates. New debates leave this
    /// empty and populate [`turns`] instead.
    #[serde(default)]
    pub rounds: Vec<DebateRoundRecord>,
    /// v2 turn-based transcript. New debates populate this; older debates
    /// have `rounds` instead.
    #[serde(default)]
    pub turns: Vec<DebateTurn>,
    /// Reviewer's caveats from the finalize turn. Empty for legacy records
    /// and for v2 debates where the reviewer had nothing to flag.
    #[serde(default)]
    pub caveats: Vec<String>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub author_input_tokens: u64,
    pub author_output_tokens: u64,
    pub reviewer_input_tokens: u64,
    pub reviewer_output_tokens: u64,
    pub cost_author_usd: f64,
    pub cost_reviewer_usd: f64,
    pub cost_total_usd: f64,
}

/// Slim view used to render the history index. Same fields as `DebateRecord`
/// minus the bulky markdown bodies (`original_plan`, `final_plan`, `rounds`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateSummary {
    pub id: String,
    pub created_at: String,
    pub plan_path: String,
    pub plan_name: String,
    #[serde(default)]
    pub project_dir: String,
    #[serde(default)]
    pub refined_plan_path: String,
    pub author_provider: String,
    pub author_model: String,
    pub reviewer_provider: String,
    pub reviewer_model: String,
    pub max_rounds: u32,
    pub rounds_used: u32,
    /// Total turns executed in the v2 engine (opening + critiques +
    /// responses + finalize). Zero for legacy records that used `rounds_used`
    /// as their primary count.
    #[serde(default)]
    pub turns_used: u32,
    pub approved: bool,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub author_input_tokens: u64,
    pub author_output_tokens: u64,
    pub reviewer_input_tokens: u64,
    pub reviewer_output_tokens: u64,
    pub cost_author_usd: f64,
    pub cost_reviewer_usd: f64,
    pub cost_total_usd: f64,
}

impl From<&DebateRecord> for DebateSummary {
    fn from(r: &DebateRecord) -> Self {
        DebateSummary {
            id: r.id.clone(),
            created_at: r.created_at.clone(),
            plan_path: r.plan_path.clone(),
            plan_name: r.plan_name.clone(),
            project_dir: r.project_dir.clone(),
            refined_plan_path: r.refined_plan_path.clone(),
            author_provider: r.author_provider.clone(),
            author_model: r.author_model.clone(),
            reviewer_provider: r.reviewer_provider.clone(),
            reviewer_model: r.reviewer_model.clone(),
            max_rounds: r.max_rounds,
            rounds_used: r.rounds_used,
            turns_used: r.turns.len() as u32,
            approved: r.approved,
            total_input_tokens: r.total_input_tokens,
            total_output_tokens: r.total_output_tokens,
            author_input_tokens: r.author_input_tokens,
            author_output_tokens: r.author_output_tokens,
            reviewer_input_tokens: r.reviewer_input_tokens,
            reviewer_output_tokens: r.reviewer_output_tokens,
            cost_author_usd: r.cost_author_usd,
            cost_reviewer_usd: r.cost_reviewer_usd,
            cost_total_usd: r.cost_total_usd,
        }
    }
}

// ── Paths ───────────────────────────────────────────────────────────────────

fn debates_dir() -> PathBuf {
    paths::app_data_dir().join("debates")
}

fn record_path(id: &str) -> PathBuf {
    debates_dir().join(format!("{id}.json"))
}

fn index_path() -> PathBuf {
    debates_dir().join("index.json")
}

// ── Read helpers (tolerant) ─────────────────────────────────────────────────

fn read_index() -> Vec<DebateSummary> {
    let path = index_path();
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str::<Vec<DebateSummary>>(&raw).unwrap_or_default()
}

fn write_index(summaries: &[DebateSummary]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(summaries)
        .map_err(|e| format!("Failed to serialize debate index: {}", e))?;
    paths::atomic_write_str(&index_path(), &json)
}

// ── Public save API (called from `run_debate`) ──────────────────────────────

/// Persist a completed debate. Writes the per-debate JSON, then prepends a
/// summary entry to `index.json`. Both writes are atomic. Caller should
/// `let _ = save_debate(...)` — failures here MUST NOT block the
/// `debate:complete` event.
pub fn save_debate(record: DebateRecord) -> Result<(), String> {
    // Make sure the parent directory exists.
    let dir = debates_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        return Err(format!(
            "Failed to create debates dir {}: {}",
            dir.display(),
            e
        ));
    }

    // Per-debate file
    let body = serde_json::to_string_pretty(&record)
        .map_err(|e| format!("Failed to serialize debate record: {}", e))?;
    paths::atomic_write_str(&record_path(&record.id), &body)?;

    // Index — prepend (newest-first) and dedupe by id in case of a collision.
    let summary = DebateSummary::from(&record);
    let mut existing = read_index();
    existing.retain(|s| s.id != summary.id);
    existing.insert(0, summary);
    write_index(&existing)?;

    Ok(())
}

// ── Tauri commands ──────────────────────────────────────────────────────────

/// List debates newest-first. Tolerant of a missing or corrupt index file —
/// returns `[]` rather than erroring.
#[tauri::command]
pub fn list_debates() -> Vec<DebateSummary> {
    read_index()
}

/// Fetch the full record for one debate.
#[tauri::command]
pub fn get_debate(id: String) -> Result<DebateRecord, String> {
    let path = record_path(&id);
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read debate {}: {}", id, e))?;
    serde_json::from_str::<DebateRecord>(&raw)
        .map_err(|e| format!("Failed to parse debate {}: {}", id, e))
}

/// Delete one debate. Removes the per-debate JSON if present and rewrites
/// the index without the entry. Missing files are tolerated — the goal is
/// "the entry is gone", not "the file was here".
#[tauri::command]
pub fn delete_debate(id: String) -> Result<(), String> {
    let path = record_path(&id);
    if path.exists() {
        if let Err(e) = fs::remove_file(&path) {
            return Err(format!("Failed to remove debate file {}: {}", id, e));
        }
    }
    let mut existing = read_index();
    let before = existing.len();
    existing.retain(|s| s.id != id);
    if existing.len() != before {
        write_index(&existing)?;
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record(id: &str) -> DebateRecord {
        DebateRecord {
            id: id.to_string(),
            created_at: "2026-05-14T00:00:00Z".to_string(),
            plan_path: "/tmp/plan.md".to_string(),
            plan_name: "plan.md".to_string(),
            project_dir: "/tmp".to_string(),
            refined_plan_path: String::new(),
            author_provider: "anthropic".to_string(),
            author_model: "claude-opus-4-7".to_string(),
            reviewer_provider: "openai".to_string(),
            reviewer_model: "gpt-5".to_string(),
            max_rounds: 3,
            rounds_used: 1,
            approved: true,
            original_plan: "x".to_string(),
            final_plan: "x".to_string(),
            rounds: vec![],
            turns: vec![],
            caveats: vec![],
            total_input_tokens: 0,
            total_output_tokens: 0,
            author_input_tokens: 0,
            author_output_tokens: 0,
            reviewer_input_tokens: 0,
            reviewer_output_tokens: 0,
            cost_author_usd: 0.0,
            cost_reviewer_usd: 0.0,
            cost_total_usd: 0.0,
        }
    }

    #[test]
    fn summary_from_record_strips_bodies() {
        let r = sample_record("abc");
        let s = DebateSummary::from(&r);
        assert_eq!(s.id, "abc");
        assert_eq!(s.plan_name, "plan.md");
        assert!(s.approved);
    }

    #[test]
    fn record_roundtrips_json() {
        let r = sample_record("abc");
        let json = serde_json::to_string(&r).unwrap();
        let parsed: DebateRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "abc");
        assert_eq!(parsed.author_model, "claude-opus-4-7");
    }
}
