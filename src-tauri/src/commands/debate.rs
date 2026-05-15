//! Tauri commands for the "Debate" feature (v2 turn-based engine).
//!
//! A debate is a turn-based conversation between an "author" model and a
//! "reviewer" model with a shared transcript and structured tagged output.
//!
//! Phases:
//!   1. **Opening** — Author surfaces the plan to debate (we synthesize the
//!      structured output directly from the supplied plan content; the model
//!      narration is captured for the inspector but not used downstream).
//!   2. **Critique / Response** — up to `max_rounds` reviewer critiques, each
//!      followed by an author response (skipped on the very last cycle). Each
//!      turn emits structured tags; the next speaker sees parsed pieces of
//!      prior turns rather than raw narration.
//!   3. **Finalize** — the reviewer always gets the last word: they emit a
//!      polished canonical plan plus a caveats list, regardless of whether
//!      the critique loop ended in APPROVE or budget exhaustion.
//!
//! Tokens are streamed live to the frontend via Tauri events. Each turn is
//! parsed by [`parse_turn_output`]; if parsing fails we send ONE corrective
//! follow-up to the same conversation and reparse. After the retry the debate
//! always continues — malformed turns degrade gracefully (verdict defaults to
//! REQUEST_CHANGES, response no-ops, finalize falls back to the last refined
//! plan).

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Wry};

use crate::commands::debate_history::{
    self, DebateRecord, DebateToolCallRecord, DebateTurn,
};
use crate::commands::debate_tools::{
    anthropic_tool_specs, execute_tool, openai_tool_specs, preview_input, preview_output,
    truncate_for_record,
};
use crate::commands::plans;
use crate::utils::keychain;
use crate::utils::paths;

/// Hard cap on the agentic tool-use loop iterations per turn. After this many
/// passes we force a final text-only request to wrap the turn.
const MAX_TOOL_LOOP_ITERATIONS: u32 = 6;
/// `input_preview` clamp on the wire (`debate:tool_call` event).
const TOOL_EVENT_INPUT_PREVIEW: usize = 200;
/// `output_preview` clamp on the wire. Matches the persisted-record size so
/// the live "Inspect" view and the saved history show the same content.
const TOOL_EVENT_OUTPUT_PREVIEW: usize = 2048;
/// Truncation length for tool outputs as persisted in the saved debate record.
const TOOL_RECORD_OUTPUT_BYTES: usize = 2048;

// ── Constants ───────────────────────────────────────────────────────────────

const ANTHROPIC_MODEL: &str = "claude-sonnet-4-5-20250929";
const OPENAI_MODEL: &str = "gpt-5";

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";

const ANTHROPIC_API_KEY: &str = "ANTHROPIC_API_KEY";
const OPENAI_API_KEY: &str = "OPENAI_API_KEY";

const ANTHROPIC_MAX_TOKENS: u32 = 8192;

// ── Cost engine ─────────────────────────────────────────────────────────────
//
// Rough public list prices in USD per **1,000,000** tokens, as of the time
// this feature shipped. These are estimates so we can show a cost on the
// debate history — tweak as providers change pricing. Returning (0, 0) for
// unknown ids keeps the UI sane (it just shows $0.0000) instead of erroring.

/// `(input_per_1m_usd, output_per_1m_usd)` for a model id; (0, 0) if unknown.
fn model_prices_per_1m(model: &str) -> (f64, f64) {
    match model {
        "claude-opus-4-7" => (15.00, 75.00),
        "claude-sonnet-4-6" => (3.00, 15.00),
        "claude-haiku-4-5-20251001" => (0.80, 4.00),
        "gpt-5" => (1.25, 10.00),
        "gpt-5-mini" => (0.25, 2.00),
        "gpt-4o" => (2.50, 10.00),
        "gpt-4o-mini" => (0.15, 0.60),
        _ => (0.0, 0.0),
    }
}

/// Compute the USD cost for one side of a debate.
pub fn cost_for(model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    let (in_per_1m, out_per_1m) = model_prices_per_1m(model);
    let in_cost = (input_tokens as f64) * in_per_1m / 1_000_000.0;
    let out_cost = (output_tokens as f64) * out_per_1m / 1_000_000.0;
    in_cost + out_cost
}

// ── State ───────────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref DEBATE_CANCEL_FLAGS: Mutex<HashMap<String, Arc<AtomicBool>>> = Default::default();
    static ref DEBATE_ALLOWED_PATHS: Mutex<HashMap<String, String>> = Default::default();
}

fn register_debate(debate_id: &str, plan_path: Option<&str>) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    if let Ok(mut g) = DEBATE_CANCEL_FLAGS.lock() {
        g.insert(debate_id.to_string(), flag.clone());
    }
    if let (Some(p), Ok(mut g)) = (plan_path, DEBATE_ALLOWED_PATHS.lock()) {
        g.insert(debate_id.to_string(), p.to_string());
    }
    flag
}

fn unregister_debate(debate_id: &str) {
    if let Ok(mut g) = DEBATE_CANCEL_FLAGS.lock() {
        g.remove(debate_id);
    }
    if let Ok(mut g) = DEBATE_ALLOWED_PATHS.lock() {
        g.remove(debate_id);
    }
}

fn lookup_allowed_path(debate_id: &str) -> Option<String> {
    DEBATE_ALLOWED_PATHS
        .lock()
        .ok()
        .and_then(|g| g.get(debate_id).cloned())
}

// ── Args / payloads ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct StartDebateArgs {
    pub plan_content: String,
    #[serde(default)]
    pub plan_path: Option<String>,
    /// Project root that the plan belongs to. When provided, the models see
    /// it in their context and gain sandboxed read-only tools that operate
    /// within this directory.
    #[serde(default)]
    pub project_dir: Option<String>,
    pub author_provider: String,
    pub author_model: String,
    pub reviewer_provider: String,
    pub reviewer_model: String,
    /// Max reviewer critique turns. The finalize turn is extra.
    pub max_rounds: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReplacePlanFileArgs {
    pub debate_id: String,
    pub file_path: String,
    pub new_content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveRefinedPlanArgs {
    pub target_path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DebateCredentialsStatus {
    pub anthropic: bool,
    pub openai: bool,
}

// ── Event payloads ──────────────────────────────────────────────────────────
//
// snake_case keys verbatim so the JS side gets the contract documented at
// the top of this file.

#[derive(Debug, Clone, Serialize)]
struct TurnStartPayload<'a> {
    debate_id: &'a str,
    index: u32,
    speaker: &'a str,
    kind: &'a str,
    model: &'a str,
    system_prompt: &'a str,
    user_prompt: &'a str,
}

#[derive(Debug, Clone, Serialize)]
struct TokenPayload<'a> {
    debate_id: &'a str,
    index: u32,
    text: &'a str,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
struct Usage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug, Clone, Serialize)]
struct TurnCompletePayload<'a> {
    debate_id: &'a str,
    index: u32,
    speaker: &'a str,
    kind: &'a str,
    raw_text: &'a str,
    parsed: Option<&'a serde_json::Value>,
    parse_error: Option<&'a str>,
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug, Clone, Serialize)]
struct CompletePayload<'a> {
    debate_id: &'a str,
    refined_plan_path: &'a str,
    final_plan: &'a str,
    caveats: &'a [String],
    turns_used: u32,
    approved: bool,
    total_input_tokens: u64,
    total_output_tokens: u64,
    author_input_tokens: u64,
    author_output_tokens: u64,
    reviewer_input_tokens: u64,
    reviewer_output_tokens: u64,
    cost_author_usd: f64,
    cost_reviewer_usd: f64,
    cost_total_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
struct ErrorPayload<'a> {
    debate_id: &'a str,
    message: &'a str,
}

#[derive(Debug, Clone, Serialize)]
struct ToolCallPayload<'a> {
    debate_id: &'a str,
    index: u32,
    role: &'a str,
    tool: &'a str,
    input_preview: &'a str,
    output_preview: &'a str,
    is_error: bool,
}

/// In-memory record of one tool call so the worker can emit the event AFTER
/// the tool runs and also stash a copy on the persisted turn record.
#[derive(Debug, Clone)]
struct ToolCallTrace {
    tool: String,
    /// The raw JSON input the model sent (compact serialization).
    input_json: String,
    /// Full result text (truncated to `TOOL_RECORD_OUTPUT_BYTES` before persist).
    output: String,
    is_error: bool,
}

// ── Commands ────────────────────────────────────────────────────────────────

/// Quick existence check for the two credentials the debate feature needs.
/// Never returns the secret values themselves.
#[tauri::command]
pub fn check_debate_credentials() -> DebateCredentialsStatus {
    DebateCredentialsStatus {
        anthropic: keychain::is_known(ANTHROPIC_API_KEY),
        openai: keychain::is_known(OPENAI_API_KEY),
    }
}

/// Kick off a debate. Returns a `debate_id` immediately; all output streams
/// back to the frontend via `debate:*` events scoped by that ID.
///
/// The worker thread is responsible for emitting `debate:complete` or
/// `debate:error` exactly once.
#[tauri::command]
pub fn start_debate(args: StartDebateArgs, app: AppHandle<Wry>) -> Result<String, String> {
    let author_provider = parse_provider_strict(&args.author_provider)
        .ok_or_else(|| format!("invalid_provider: author={}", args.author_provider))?;
    let reviewer_provider = parse_provider_strict(&args.reviewer_provider)
        .ok_or_else(|| format!("invalid_provider: reviewer={}", args.reviewer_provider))?;

    let author_model = args.author_model.trim().to_string();
    let reviewer_model = args.reviewer_model.trim().to_string();
    if author_model.is_empty() {
        return Err("invalid_model: author".to_string());
    }
    if reviewer_model.is_empty() {
        return Err("invalid_model: reviewer".to_string());
    }

    let needs_anthropic = author_provider == Provider::Anthropic
        || reviewer_provider == Provider::Anthropic;
    let needs_openai = author_provider == Provider::OpenAI
        || reviewer_provider == Provider::OpenAI;

    let anthropic_key = if needs_anthropic {
        keychain::get_secret(ANTHROPIC_API_KEY)
            .map_err(|e| format!("keychain_error: {}", e))?
            .filter(|s| !s.trim().is_empty())
    } else {
        None
    };
    let openai_key = if needs_openai {
        keychain::get_secret(OPENAI_API_KEY)
            .map_err(|e| format!("keychain_error: {}", e))?
            .filter(|s| !s.trim().is_empty())
    } else {
        None
    };

    let mut missing: Vec<&str> = Vec::new();
    if needs_anthropic && anthropic_key.is_none() {
        missing.push(ANTHROPIC_API_KEY);
    }
    if needs_openai && openai_key.is_none() {
        missing.push(OPENAI_API_KEY);
    }
    if !missing.is_empty() {
        return Err(format!("missing_credentials: {}", missing.join("|")));
    }

    let max_rounds = args.max_rounds.max(1);

    let debate_id = uuid::Uuid::new_v4().to_string();
    let cancel_flag = register_debate(&debate_id, args.plan_path.as_deref());

    let debate_id_thread = debate_id.clone();
    let plan_content = args.plan_content;
    let plan_path_thread = args.plan_path.clone();
    let project_dir_thread = args.project_dir.clone();
    let app_clone = app.clone();

    thread::spawn(move || {
        // Brief pause so the frontend has time to capture the returned
        // debate_id (in its matcher ref) before our first emit.
        thread::sleep(std::time::Duration::from_millis(150));

        let result = run_debate(
            &app_clone,
            &debate_id_thread,
            &cancel_flag,
            author_provider,
            &author_model,
            reviewer_provider,
            &reviewer_model,
            max_rounds,
            &plan_content,
            plan_path_thread.as_deref(),
            project_dir_thread.as_deref(),
            anthropic_key.as_deref(),
            openai_key.as_deref(),
        );
        match result {
            Ok(()) => {}
            Err(msg) => {
                let _ = app_clone.emit(
                    "debate:error",
                    ErrorPayload {
                        debate_id: &debate_id_thread,
                        message: &msg,
                    },
                );
            }
        }
        unregister_debate(&debate_id_thread);
    });

    Ok(debate_id)
}

/// Flip the per-debate cancel flag. The worker thread polls between SSE lines
/// and exits cleanly with `debate:error { message: "cancelled" }`.
#[tauri::command]
pub fn cancel_debate(debate_id: String) {
    if let Ok(g) = DEBATE_CANCEL_FLAGS.lock() {
        if let Some(flag) = g.get(&debate_id) {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

/// Overwrite the on-disk plan file with the refined content. The caller MUST
/// pass the `debate_id` that produced the refined plan so we can validate
/// the destination against (a) the per-debate allow-list seeded at start time
/// and (b) the existing `plans::is_safe_plan_path` check. We write a single
/// `.bak.<unix_ts>` sibling first, then `atomic_write` the new content.
#[tauri::command]
pub fn replace_plan_file(args: ReplacePlanFileArgs) -> Result<(), String> {
    let allow_listed = lookup_allowed_path(&args.debate_id);
    let allow_listed_match = allow_listed
        .as_deref()
        .map(|p| p == args.file_path)
        .unwrap_or(false);
    if !allow_listed_match && !plans::is_safe_plan_path(&args.file_path) {
        return Err("Invalid plan path".to_string());
    }

    let target = std::path::PathBuf::from(&args.file_path);

    if target.exists() {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup_name = format!(
            "{}.bak.{}",
            target
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("plan.md"),
            ts
        );
        let backup_path = target
            .parent()
            .map(|p| p.join(&backup_name))
            .unwrap_or_else(|| std::path::PathBuf::from(&backup_name));

        match std::fs::read(&target) {
            Ok(existing) => {
                paths::atomic_write(&backup_path, &existing)?;
            }
            Err(e) => {
                return Err(format!("Failed to read existing plan for backup: {}", e));
            }
        }
    }

    paths::atomic_write_str(&target, &args.new_content)?;
    Ok(())
}

/// Save the refined plan to a user-picked path (no allow-list — the OS save
/// dialog already captured explicit consent).
#[tauri::command]
pub fn save_refined_plan(args: SaveRefinedPlanArgs) -> Result<(), String> {
    let target = std::path::PathBuf::from(&args.target_path);
    paths::atomic_write_str(&target, &args.content)?;
    Ok(())
}

// ── Project plan discovery ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredPlan {
    pub name: String,
    pub source: String, // "claude" | "cursor"
    pub file_path: String,
    pub modified_at: String,
}

/// Scan a user-picked project folder for plans living inside `.claude/plans/`
/// and `.cursor/plans/`. Used by the Debate page's "Scan project folder"
/// button to sidestep macOS Finder hiding dotfolders in the file picker.
#[tauri::command]
pub fn discover_project_plans(project_dir: String) -> Result<Vec<DiscoveredPlan>, String> {
    let base = std::path::Path::new(&project_dir);
    if !base.is_dir() {
        return Err(format!("not_a_directory: {}", project_dir));
    }

    let candidates: &[(&str, &str)] = &[
        (".claude/plans", "claude"),
        (".cursor/plans", "cursor"),
    ];

    let mut out: Vec<DiscoveredPlan> = Vec::new();
    for (subdir, source) in candidates {
        let dir = base.join(subdir);
        if !dir.is_dir() {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            let modified_at = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| {
                    let secs = d.as_secs() as i64;
                    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            out.push(DiscoveredPlan {
                name,
                source: (*source).to_string(),
                file_path: path.to_string_lossy().to_string(),
                modified_at,
            });
        }
    }
    out.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(out)
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Provider {
    Anthropic,
    OpenAI,
}

impl Provider {
    fn label(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::OpenAI => "openai",
        }
    }

    #[allow(dead_code)]
    fn model(self) -> &'static str {
        match self {
            Provider::Anthropic => ANTHROPIC_MODEL,
            Provider::OpenAI => OPENAI_MODEL,
        }
    }
}

fn parse_provider_strict(s: &str) -> Option<Provider> {
    match s.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Some(Provider::Anthropic),
        "openai" => Some(Provider::OpenAI),
        _ => None,
    }
}

// ── Turn kinds + parsed payloads ────────────────────────────────────────────

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum TurnKind {
    Opening,
    Critique,
    Response,
    Finalize,
}

impl TurnKind {
    fn label(self) -> &'static str {
        match self {
            TurnKind::Opening => "opening",
            TurnKind::Critique => "critique",
            TurnKind::Response => "response",
            TurnKind::Finalize => "finalize",
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Speaker {
    Author,
    Reviewer,
}

impl Speaker {
    fn label(self) -> &'static str {
        match self {
            Speaker::Author => "author",
            Speaker::Reviewer => "reviewer",
        }
    }
}

/// Tagged-by-kind parsed payload. The serialized form (via `to_json`) is what
/// goes on the wire as `parsed` in `debate:turn_complete` and on disk as the
/// `parsed` field of [`DebateTurn`].
#[derive(Debug, Clone)]
enum ParsedTurn {
    Opening {
        plan: String,
    },
    Critique {
        issues: Vec<String>,
        verdict: String, // "APPROVE" | "REQUEST_CHANGES"
    },
    Response {
        accepted: Vec<String>,
        rebutted: Vec<String>,
        refined_plan: Option<String>,
    },
    Finalize {
        plan: String,
        caveats: Vec<String>,
    },
}

impl ParsedTurn {
    fn to_json(&self) -> serde_json::Value {
        match self {
            ParsedTurn::Opening { plan } => serde_json::json!({
                "kind": "opening",
                "plan": plan,
            }),
            ParsedTurn::Critique { issues, verdict } => serde_json::json!({
                "kind": "critique",
                "issues": issues,
                "verdict": verdict,
            }),
            ParsedTurn::Response {
                accepted,
                rebutted,
                refined_plan,
            } => serde_json::json!({
                "kind": "response",
                "accepted": accepted,
                "rebutted": rebutted,
                "refined_plan": refined_plan,
            }),
            ParsedTurn::Finalize { plan, caveats } => serde_json::json!({
                "kind": "finalize",
                "plan": plan,
                "caveats": caveats,
            }),
        }
    }
}

/// Parser error. `missing` lists tag names (no angle brackets) that were
/// absent or invalid.
#[derive(Debug, Clone)]
struct ParseError {
    missing: Vec<String>,
}

impl ParseError {
    fn message(&self) -> String {
        format!("missing or invalid tags: {}", self.missing.join(", "))
    }
}

/// Pull the inner content of `<tag>...</tag>` from `text`. Greedy from the
/// FIRST opening to the LAST closing, so the inner content can include
/// fenced code blocks containing `<` without breaking. Trims leading and
/// trailing whitespace from the result.
fn extract_tag(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = text.find(&open)?;
    let after_open = start + open.len();
    let close_rel = text[after_open..].rfind(&close)?;
    let inner = &text[after_open..after_open + close_rel];
    Some(inner.trim().to_string())
}

/// Split a tag body into numbered items (`1.`, `2.`, …). Trim each.
fn split_numbered(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        // Look for `\d+.` prefix.
        let mut idx = 0;
        let bytes = trimmed.as_bytes();
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        let is_numbered = idx > 0 && bytes.get(idx) == Some(&b'.');
        if is_numbered {
            if let Some(prev) = current.take() {
                let s = prev.trim().to_string();
                if !s.is_empty() {
                    out.push(s);
                }
            }
            let rest = trimmed[idx + 1..].trim_start();
            current = Some(rest.to_string());
        } else if let Some(buf) = current.as_mut() {
            // Continuation of the current item.
            if !buf.is_empty() {
                buf.push(' ');
            }
            buf.push_str(line.trim());
        }
    }
    if let Some(prev) = current.take() {
        let s = prev.trim().to_string();
        if !s.is_empty() {
            out.push(s);
        }
    }
    out
}

/// Split a tag body into bullet items (`-` lines). Trim each.
fn split_bulleted(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('-') {
            if let Some(prev) = current.take() {
                let s = prev.trim().to_string();
                if !s.is_empty() {
                    out.push(s);
                }
            }
            current = Some(rest.trim_start().to_string());
        } else if let Some(buf) = current.as_mut() {
            if !buf.is_empty() {
                buf.push(' ');
            }
            buf.push_str(line.trim());
        }
    }
    if let Some(prev) = current.take() {
        let s = prev.trim().to_string();
        if !s.is_empty() {
            out.push(s);
        }
    }
    out
}

/// Parse one turn's raw model output into a [`ParsedTurn`]. Pure function.
///
/// Required tags per kind:
///   - Opening: not called (the caller synthesizes the opening payload directly
///     from the supplied plan content).
///   - Critique: `<issues>`, `<verdict>`
///   - Response: `<accepted>`, `<rebutted>`, `<refined_plan>`
///   - Finalize: `<final_plan>`, `<caveats>`
fn parse_turn_output(text: &str, kind: TurnKind) -> Result<ParsedTurn, ParseError> {
    match kind {
        TurnKind::Opening => {
            // Not parsed from text; caller should never call this.
            Err(ParseError {
                missing: vec!["opening_not_parseable".to_string()],
            })
        }
        TurnKind::Critique => {
            let mut missing: Vec<String> = Vec::new();
            let issues_body = extract_tag(text, "issues");
            let verdict_body = extract_tag(text, "verdict");
            if issues_body.is_none() {
                missing.push("issues".to_string());
            }
            // Verdict required AND must be a recognized literal.
            let verdict_value = verdict_body.as_ref().map(|s| s.trim().to_ascii_uppercase());
            let verdict_ok = matches!(
                verdict_value.as_deref(),
                Some("APPROVE") | Some("REQUEST_CHANGES")
            );
            if !verdict_ok {
                missing.push("verdict".to_string());
            }
            if !missing.is_empty() {
                return Err(ParseError { missing });
            }
            let issues = split_numbered(issues_body.as_deref().unwrap_or(""));
            Ok(ParsedTurn::Critique {
                issues,
                verdict: verdict_value.unwrap(),
            })
        }
        TurnKind::Response => {
            let mut missing: Vec<String> = Vec::new();
            let accepted_body = extract_tag(text, "accepted");
            let rebutted_body = extract_tag(text, "rebutted");
            let refined_body = extract_tag(text, "refined_plan");
            if accepted_body.is_none() {
                missing.push("accepted".to_string());
            }
            if rebutted_body.is_none() {
                missing.push("rebutted".to_string());
            }
            if refined_body.is_none() {
                missing.push("refined_plan".to_string());
            }
            if !missing.is_empty() {
                return Err(ParseError { missing });
            }
            let accepted = split_bulleted(accepted_body.as_deref().unwrap_or(""));
            let rebutted = split_bulleted(rebutted_body.as_deref().unwrap_or(""));
            let refined_plan_raw = refined_body.unwrap();
            let refined_plan = if refined_plan_raw.trim().is_empty() {
                None
            } else {
                Some(refined_plan_raw)
            };
            Ok(ParsedTurn::Response {
                accepted,
                rebutted,
                refined_plan,
            })
        }
        TurnKind::Finalize => {
            let mut missing: Vec<String> = Vec::new();
            let plan_body = extract_tag(text, "final_plan");
            let caveats_body = extract_tag(text, "caveats");
            if plan_body.is_none() {
                missing.push("final_plan".to_string());
            }
            if caveats_body.is_none() {
                missing.push("caveats".to_string());
            }
            if !missing.is_empty() {
                return Err(ParseError { missing });
            }
            let caveats = split_bulleted(caveats_body.as_deref().unwrap_or(""));
            Ok(ParsedTurn::Finalize {
                plan: plan_body.unwrap(),
                caveats,
            })
        }
    }
}

// ── Prompts ────────────────────────────────────────────────────────────────

fn project_blurb(project_dir: Option<&str>) -> String {
    match project_dir {
        Some(dir) if !dir.is_empty() => format!(
            "The plan belongs to a real project at `{}`. Ground every claim in \
what would actually be true in that repo. Tools available: read_file, \
list_directory, grep — use them to verify rather than guess. Tool calls and \
narration between them are for your own thinking; they will NOT appear in \
your final answer to the other side.\n\n",
            dir
        ),
        _ => String::new(),
    }
}

fn final_answer_rule() -> &'static str {
    "Your narration and tool reasoning are for your own use. Your FINAL ANSWER \
must contain ONLY the tagged sections specified below, in that exact order, \
with no extra prose before, between, or after them."
}

fn author_opening_system(project_dir: Option<&str>) -> String {
    format!(
        "You are the AUTHOR of a software/product plan that is about to be \
debated with a sharp reviewer. This is your OPENING turn: you are presenting \
the plan to the reviewer.\n\n\
{blurb}\
The plan content is supplied in the user message — present it as your opening \
position. You may briefly note anchor points or assumptions you want the \
reviewer to focus on, but do not rewrite the plan here.\n\n\
{rule}\n\n\
Output schema (exact tags, in this order):\n\
<opening_notes>\nA short paragraph (2–4 sentences) framing the plan for the reviewer.\n</opening_notes>",
        blurb = project_blurb(project_dir),
        rule = final_answer_rule(),
    )
}

fn author_opening_user(plan_content: &str, project_dir: Option<&str>) -> String {
    let proj_line = match project_dir {
        Some(dir) if !dir.is_empty() => format!("Project root: {}\n\n", dir),
        _ => String::new(),
    };
    format!(
        "{proj}Here is the plan you are putting forward for debate:\n\n\
─── PLAN ───\n{plan}\n─── END PLAN ───\n\n\
Present this plan as your opening. Use tools first if you need to verify any \
factual anchors. Then produce ONLY your <opening_notes> tag.",
        proj = proj_line,
        plan = plan_content,
    )
}

fn reviewer_critique_system(
    project_dir: Option<&str>,
    round: u32,
    max_rounds: u32,
) -> String {
    format!(
        "You are a SHARP, SKEPTICAL REVIEWER auditing a software/product plan. \
Your job is to find concrete, actionable problems, then either approve the \
plan or request changes.\n\n\
{blurb}\
You are on critique round {round} of {max_rounds}. After this you may get an \
author response, and then the cycle continues until APPROVE or budget runs \
out.\n\n\
Rules:\n\
- Each issue must be a SINGLE sentence citing what is missing, wrong, or \
unverified, tied to a specific section or claim from the plan.\n\
- Prefer issues you can verify with the tools.\n\
- Do NOT rewrite the plan. Critique only.\n\
- Use APPROVE only when there are no remaining blocking issues. Otherwise \
REQUEST_CHANGES. An APPROVE may carry zero or a few minor non-blocking issues.\n\n\
{rule}\n\n\
Output schema (exact tags, in this order):\n\
<issues>\n\
1. <issue text>\n\
2. <issue text>\n\
…\n\
</issues>\n\
<verdict>REQUEST_CHANGES</verdict>\n\
or\n\
<verdict>APPROVE</verdict>",
        blurb = project_blurb(project_dir),
        rule = final_answer_rule(),
        round = round,
        max_rounds = max_rounds,
    )
}

fn reviewer_critique_user(transcript: &str, round: u32, max_rounds: u32) -> String {
    format!(
        "Shared debate transcript so far (parsed structured pieces from each \
turn — this is what each side gets to see):\n\n{transcript}\n\n\
Produce your critique for round {round} of {max_rounds} now. Use tools to \
verify what the author claims. Then output ONLY the <issues> and <verdict> tags."
    )
}

fn author_response_system(
    project_dir: Option<&str>,
    round: u32,
    max_rounds: u32,
) -> String {
    let remaining = max_rounds.saturating_sub(round);
    format!(
        "You are the AUTHOR responding to the reviewer's critique. For each \
issue, EITHER accept it (and refine the plan to address it) or rebut it \
(with a concrete reason).\n\n\
{blurb}\
You are responding to critique round {round} of {max_rounds} (critique rounds \
remaining after this one: {remaining}). Budget is tight — prioritize.\n\n\
Rules:\n\
- Reference issues by their critique number, e.g. `#1`, `#3`.\n\
- <accepted> and <rebutted> may EACH be empty, but BOTH tags MUST be present.\n\
- <refined_plan> MUST be present — re-emit the full updated markdown plan. \
If you changed nothing, re-emit the prior plan verbatim.\n\
- The plan inside <refined_plan> must stand alone — no diff syntax, no \
change-log preamble.\n\n\
{rule}\n\n\
Output schema (exact tags, in this order):\n\
<accepted>\n\
- #N: <how you addressed issue N>\n\
- #M: <how you addressed issue M>\n\
</accepted>\n\
<rebutted>\n\
- #N: <concrete reason for rejecting issue N>\n\
</rebutted>\n\
<refined_plan>\n\
<full updated markdown plan>\n\
</refined_plan>",
        blurb = project_blurb(project_dir),
        rule = final_answer_rule(),
        round = round,
        max_rounds = max_rounds,
        remaining = remaining,
    )
}

fn author_response_user(transcript: &str, round: u32, max_rounds: u32) -> String {
    format!(
        "Shared debate transcript so far (parsed structured pieces from each \
turn):\n\n{transcript}\n\n\
Produce your response to critique round {round} of {max_rounds} now. Use tools \
if you need to verify a specific claim. Then output ONLY the <accepted>, \
<rebutted>, and <refined_plan> tags."
    )
}

fn reviewer_finalize_system(project_dir: Option<&str>) -> String {
    format!(
        "You are the REVIEWER and this is the FINALIZE turn — your role here \
is to produce the canonical, ready-to-save markdown plan from the debate so \
far, plus any caveats the user should know about.\n\n\
{blurb}\
You are NOT critiquing anymore. Take the most recent author plan, fold in any \
small refinements you would have requested in a hypothetical next critique, \
and emit a polished version. Do NOT change the substantive direction of the \
plan — the author already accepted or rebutted your critiques.\n\n\
Rules:\n\
- <final_plan> is the polished plan, in clean markdown, ready for direct save.\n\
- <caveats> is a bullet list of open questions, known-imperfect items, or \
follow-ups for the human. Empty is fine; the tag MUST still be present.\n\n\
{rule}\n\n\
Output schema (exact tags, in this order):\n\
<final_plan>\n\
<the polished, canonical markdown plan>\n\
</final_plan>\n\
<caveats>\n\
- <open question or known-imperfect item>\n\
- …\n\
</caveats>",
        blurb = project_blurb(project_dir),
        rule = final_answer_rule(),
    )
}

fn reviewer_finalize_user(transcript: &str, last_refined_plan: &str) -> String {
    format!(
        "Shared debate transcript so far (parsed structured pieces from each \
turn):\n\n{transcript}\n\n\
The author's most recent plan (this is your starting point for the polished \
version):\n\n─── LAST AUTHOR PLAN ───\n{plan}\n─── END LAST AUTHOR PLAN ───\n\n\
Produce the finalize output now. Output ONLY the <final_plan> and <caveats> tags.",
        plan = last_refined_plan,
    )
}

// ── Transcript ─────────────────────────────────────────────────────────────

/// Build a transcript view of prior turns to feed the next prompt. Uses
/// parsed structured pieces when available; falls back to raw_text marked
/// `(unstructured)` when parsing previously failed.
fn transcript_excerpt(turns: &[DebateTurn]) -> String {
    let mut buf = String::new();
    for t in turns {
        let header = format!(
            "─── Turn {} · {} ({}) ───\n",
            t.index,
            cap_label(&t.speaker),
            t.kind
        );
        buf.push_str(&header);

        match (t.kind.as_str(), &t.parsed) {
            ("opening", Some(p)) => {
                if let Some(plan) = p.get("plan").and_then(|v| v.as_str()) {
                    buf.push_str(plan);
                    buf.push('\n');
                }
            }
            ("critique", Some(p)) => {
                if let Some(issues) = p.get("issues").and_then(|v| v.as_array()) {
                    if issues.is_empty() {
                        buf.push_str("(no issues)\n");
                    } else {
                        for (i, item) in issues.iter().enumerate() {
                            let s = item.as_str().unwrap_or("");
                            buf.push_str(&format!("{}. {}\n", i + 1, s));
                        }
                    }
                }
                let verdict = p.get("verdict").and_then(|v| v.as_str()).unwrap_or("");
                buf.push_str(&format!("Verdict: {}\n", verdict));
            }
            ("response", Some(p)) => {
                buf.push_str("Accepted:\n");
                if let Some(arr) = p.get("accepted").and_then(|v| v.as_array()) {
                    if arr.is_empty() {
                        buf.push_str("- (none)\n");
                    } else {
                        for item in arr {
                            buf.push_str(&format!("- {}\n", item.as_str().unwrap_or("")));
                        }
                    }
                }
                buf.push_str("Rebutted:\n");
                if let Some(arr) = p.get("rebutted").and_then(|v| v.as_array()) {
                    if arr.is_empty() {
                        buf.push_str("- (none)\n");
                    } else {
                        for item in arr {
                            buf.push_str(&format!("- {}\n", item.as_str().unwrap_or("")));
                        }
                    }
                }
                buf.push_str("Refined plan:\n");
                if let Some(plan) = p.get("refined_plan").and_then(|v| v.as_str()) {
                    buf.push_str(plan);
                    buf.push('\n');
                } else {
                    buf.push_str("(unchanged from previous turn)\n");
                }
            }
            ("finalize", Some(p)) => {
                if let Some(plan) = p.get("plan").and_then(|v| v.as_str()) {
                    buf.push_str("Final plan:\n");
                    buf.push_str(plan);
                    buf.push('\n');
                }
                if let Some(arr) = p.get("caveats").and_then(|v| v.as_array()) {
                    buf.push_str("Caveats:\n");
                    if arr.is_empty() {
                        buf.push_str("- (none)\n");
                    } else {
                        for item in arr {
                            buf.push_str(&format!("- {}\n", item.as_str().unwrap_or("")));
                        }
                    }
                }
            }
            _ => {
                // Parsing failed (or unknown kind) — fall back to raw_text so
                // the next speaker still has something to react to.
                buf.push_str("(unstructured)\n");
                buf.push_str(&t.raw_text);
                buf.push('\n');
            }
        }
        buf.push('\n');
    }
    buf
}

fn cap_label(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
    }
}

// ── Run loop ───────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn run_debate(
    app: &AppHandle<Wry>,
    debate_id: &str,
    cancel: &Arc<AtomicBool>,
    author_provider: Provider,
    author_model: &str,
    reviewer_provider: Provider,
    reviewer_model: &str,
    max_rounds: u32,
    plan_content: &str,
    plan_path: Option<&str>,
    project_dir: Option<&str>,
    anthropic_key: Option<&str>,
    openai_key: Option<&str>,
) -> Result<(), String> {
    let key_for = |p: Provider| -> Result<String, String> {
        match p {
            Provider::Anthropic => anthropic_key
                .map(|s| s.to_string())
                .ok_or_else(|| format!("missing_credentials: {}", ANTHROPIC_API_KEY)),
            Provider::OpenAI => openai_key
                .map(|s| s.to_string())
                .ok_or_else(|| format!("missing_credentials: {}", OPENAI_API_KEY)),
        }
    };

    let mut turns: Vec<DebateTurn> = Vec::new();
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut author_input_tokens: u64 = 0;
    let mut author_output_tokens: u64 = 0;
    let mut reviewer_input_tokens: u64 = 0;
    let mut reviewer_output_tokens: u64 = 0;

    let mut last_refined_plan = plan_content.to_string();
    let mut approved = false;

    // ── PHASE 1: Author opens ───────────────────────────────────────────────
    {
        let index: u32 = (turns.len() as u32) + 1;
        let sys = author_opening_system(project_dir);
        let usr = author_opening_user(plan_content, project_dir);

        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }
        let api_key = key_for(author_provider)?;
        let turn = run_turn(
            app,
            debate_id,
            index,
            Speaker::Author,
            TurnKind::Opening,
            author_provider,
            author_model,
            &api_key,
            &sys,
            &usr,
            project_dir,
            cancel,
            // For opening we synthesize the parsed value (the plan is the input),
            // so the model's text is captured but not required.
            Some(plan_content.to_string()),
        )?;
        total_input_tokens += turn.input_tokens;
        total_output_tokens += turn.output_tokens;
        author_input_tokens += turn.input_tokens;
        author_output_tokens += turn.output_tokens;
        turns.push(turn);
    }

    // ── PHASE 2: Critique / Response cycles ─────────────────────────────────
    for k in 1..=max_rounds {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }

        // Reviewer critique
        let index: u32 = (turns.len() as u32) + 1;
        let sys = reviewer_critique_system(project_dir, k, max_rounds);
        let transcript = transcript_excerpt(&turns);
        let usr = reviewer_critique_user(&transcript, k, max_rounds);
        let api_key = key_for(reviewer_provider)?;
        let critique = run_turn(
            app,
            debate_id,
            index,
            Speaker::Reviewer,
            TurnKind::Critique,
            reviewer_provider,
            reviewer_model,
            &api_key,
            &sys,
            &usr,
            project_dir,
            cancel,
            None,
        )?;
        total_input_tokens += critique.input_tokens;
        total_output_tokens += critique.output_tokens;
        reviewer_input_tokens += critique.input_tokens;
        reviewer_output_tokens += critique.output_tokens;

        let verdict = critique
            .parsed
            .as_ref()
            .and_then(|v| v.get("verdict").and_then(|x| x.as_str()).map(|s| s.to_string()));
        turns.push(critique);

        if verdict.as_deref() == Some("APPROVE") {
            approved = true;
            break;
        }

        // Skip author response on the last critique round — no point arguing
        // when no further critique can land.
        if k == max_rounds {
            break;
        }

        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }

        // Author response
        let index: u32 = (turns.len() as u32) + 1;
        let sys = author_response_system(project_dir, k, max_rounds);
        let transcript = transcript_excerpt(&turns);
        let usr = author_response_user(&transcript, k, max_rounds);
        let api_key = key_for(author_provider)?;
        let response = run_turn(
            app,
            debate_id,
            index,
            Speaker::Author,
            TurnKind::Response,
            author_provider,
            author_model,
            &api_key,
            &sys,
            &usr,
            project_dir,
            cancel,
            None,
        )?;
        total_input_tokens += response.input_tokens;
        total_output_tokens += response.output_tokens;
        author_input_tokens += response.input_tokens;
        author_output_tokens += response.output_tokens;

        if let Some(p) = response
            .parsed
            .as_ref()
            .and_then(|v| v.get("refined_plan").and_then(|x| x.as_str()))
        {
            if !p.trim().is_empty() {
                last_refined_plan = p.to_string();
            }
        }
        turns.push(response);
    }

    // ── PHASE 3: Reviewer finalize (always runs) ────────────────────────────
    if cancel.load(Ordering::SeqCst) {
        return Err("cancelled".to_string());
    }
    let index: u32 = (turns.len() as u32) + 1;
    let sys = reviewer_finalize_system(project_dir);
    let transcript = transcript_excerpt(&turns);
    let usr = reviewer_finalize_user(&transcript, &last_refined_plan);
    let api_key = key_for(reviewer_provider)?;
    let finalize = run_turn(
        app,
        debate_id,
        index,
        Speaker::Reviewer,
        TurnKind::Finalize,
        reviewer_provider,
        reviewer_model,
        &api_key,
        &sys,
        &usr,
        project_dir,
        cancel,
        None,
    )?;
    total_input_tokens += finalize.input_tokens;
    total_output_tokens += finalize.output_tokens;
    reviewer_input_tokens += finalize.input_tokens;
    reviewer_output_tokens += finalize.output_tokens;

    let (final_plan, caveats) = match finalize.parsed.as_ref() {
        Some(p) => {
            let plan = p
                .get("plan")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| last_refined_plan.clone());
            let caveats: Vec<String> = p
                .get("caveats")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            (plan, caveats)
        }
        None => (last_refined_plan.clone(), Vec::new()),
    };

    turns.push(finalize);
    let turns_used: u32 = turns.len() as u32;

    // Per-side USD cost using the model that actually ran each side.
    let cost_author_usd = cost_for(author_model, author_input_tokens, author_output_tokens);
    let cost_reviewer_usd = cost_for(
        reviewer_model,
        reviewer_input_tokens,
        reviewer_output_tokens,
    );
    let cost_total_usd = cost_author_usd + cost_reviewer_usd;

    // Persist file + record.
    let plan_path_str = plan_path.unwrap_or("").to_string();
    let plan_name = if plan_path_str.is_empty() {
        "Unsaved plan".to_string()
    } else {
        std::path::Path::new(&plan_path_str)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Unsaved plan".to_string())
    };

    let saved_body = format_saved_file(
        &final_plan,
        &caveats,
        author_model,
        reviewer_model,
        turns_used,
        cost_total_usd,
    );
    let refined_plan_path = if !plan_path_str.is_empty() {
        match auto_save_versioned_plan(&plan_path_str, &saved_body) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("debate auto-save failed: {}", e);
                String::new()
            }
        }
    } else {
        String::new()
    };

    let record = DebateRecord {
        id: debate_id.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        plan_path: plan_path_str,
        plan_name,
        project_dir: project_dir.unwrap_or("").to_string(),
        refined_plan_path: refined_plan_path.clone(),
        author_provider: author_provider.label().to_string(),
        author_model: author_model.to_string(),
        reviewer_provider: reviewer_provider.label().to_string(),
        reviewer_model: reviewer_model.to_string(),
        max_rounds,
        // Map "rounds_used" to the number of reviewer critique turns actually
        // run, so legacy UI bits that still read `rounds_used` keep showing a
        // sensible count. New UI should prefer `turns_used`.
        rounds_used: turns
            .iter()
            .filter(|t| t.kind == "critique")
            .count() as u32,
        approved,
        original_plan: plan_content.to_string(),
        final_plan: final_plan.clone(),
        rounds: Vec::new(),
        turns: turns.clone(),
        caveats: caveats.clone(),
        total_input_tokens,
        total_output_tokens,
        author_input_tokens,
        author_output_tokens,
        reviewer_input_tokens,
        reviewer_output_tokens,
        cost_author_usd,
        cost_reviewer_usd,
        cost_total_usd,
    };
    if let Err(e) = debate_history::save_debate(record) {
        eprintln!("debate_history: save_debate failed: {}", e);
    }

    let _ = app.emit(
        "debate:complete",
        CompletePayload {
            debate_id,
            refined_plan_path: &refined_plan_path,
            final_plan: &final_plan,
            caveats: &caveats,
            turns_used,
            approved,
            total_input_tokens,
            total_output_tokens,
            author_input_tokens,
            author_output_tokens,
            reviewer_input_tokens,
            reviewer_output_tokens,
            cost_author_usd,
            cost_reviewer_usd,
            cost_total_usd,
        },
    );

    Ok(())
}

/// Compose the saved-file body: `final_plan` + (when caveats present) a
/// reviewer-caveats footer.
fn format_saved_file(
    final_plan: &str,
    caveats: &[String],
    author_model: &str,
    reviewer_model: &str,
    turns_used: u32,
    cost_total_usd: f64,
) -> String {
    if caveats.is_empty() {
        return final_plan.to_string();
    }
    let mut buf = String::new();
    buf.push_str(final_plan);
    if !final_plan.ends_with('\n') {
        buf.push('\n');
    }
    buf.push_str(&format!(
        "\n---\n\n> **Reviewer caveats from debate** ({} vs {}, {} turns, ${:.4})\n",
        author_model, reviewer_model, turns_used, cost_total_usd
    ));
    for c in caveats {
        buf.push_str(&format!("> - {}\n", c));
    }
    buf
}

/// Auto-save the refined plan next to the original as `<stem>_v<N>.md`. The
/// stem is the source filename with `.md` stripped and any trailing
/// `_v<digits>` removed; N is one greater than the largest sibling already
/// matching `^<stem>_v(\d+)\.md$` (defaults to 2 when none exists).
fn auto_save_versioned_plan(plan_path: &str, content: &str) -> Result<String, String> {
    let original = std::path::Path::new(plan_path);
    let parent = original
        .parent()
        .ok_or_else(|| format!("plan has no parent dir: {}", plan_path))?;
    let file_name = original
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("plan has no filename: {}", plan_path))?;
    let no_md = file_name.strip_suffix(".md").unwrap_or(file_name);
    let stem = strip_v_suffix(no_md);

    // Scan the parent directory for siblings matching `^<stem>_v(\d+)\.md$`.
    let prefix = format!("{}_v", stem);
    let mut max_n: u32 = 1;
    let mut found = false;
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = match entry.file_name().into_string() {
                Ok(s) => s,
                Err(_) => continue,
            };
            let after = match name.strip_prefix(&prefix) {
                Some(s) => s,
                None => continue,
            };
            let digits = match after.strip_suffix(".md") {
                Some(s) => s,
                None => continue,
            };
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            if let Ok(n) = digits.parse::<u32>() {
                if !found || n > max_n {
                    max_n = n;
                    found = true;
                }
            }
        }
    }
    let next = if found { max_n + 1 } else { 2 };
    let candidate = parent.join(format!("{}_v{}.md", stem, next));
    paths::atomic_write_str(&candidate, content)?;
    Ok(candidate.to_string_lossy().to_string())
}

/// Strip a trailing `_v<digits>` from a stem. Pure string op so it's cheap
/// to unit-test.
fn strip_v_suffix(stem: &str) -> &str {
    if let Some(idx) = stem.rfind("_v") {
        let tail = &stem[idx + "_v".len()..];
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            return &stem[..idx];
        }
    }
    stem
}

// ── Run one turn (stream + parse + retry) ──────────────────────────────────

/// Run a single turn: stream tokens, accumulate tool traces, parse the result,
/// and on parse failure send ONE corrective follow-up to the same conversation
/// before giving up and degrading.
///
/// `opening_plan` is `Some` only when called for the opening turn — in that
/// case parsing is bypassed and we synthesize the parsed payload from the
/// supplied plan content. The model's narration is captured in `raw_text` but
/// otherwise unused.
#[allow(clippy::too_many_arguments)]
fn run_turn(
    app: &AppHandle<Wry>,
    debate_id: &str,
    index: u32,
    speaker: Speaker,
    kind: TurnKind,
    provider: Provider,
    model: &str,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    project_dir: Option<&str>,
    cancel: &Arc<AtomicBool>,
    opening_plan: Option<String>,
) -> Result<DebateTurn, String> {
    let _ = app.emit(
        "debate:turn_start",
        TurnStartPayload {
            debate_id,
            index,
            speaker: speaker.label(),
            kind: kind.label(),
            model,
            system_prompt,
            user_prompt,
        },
    );

    let (raw1, usage1, tools1, conv1) = stream_completion(
        app,
        debate_id,
        index,
        speaker.label(),
        cancel,
        provider,
        model,
        api_key,
        system_prompt,
        user_prompt,
        project_dir,
    )?;

    let mut total_input = usage1.input_tokens;
    let mut total_output = usage1.output_tokens;
    let mut all_tools = tools1;
    let mut combined_raw = raw1.clone();

    // Opening: synthesize the parsed payload directly from the plan content.
    let (parsed_json, parse_error) = if kind == TurnKind::Opening {
        let plan = opening_plan.unwrap_or_default();
        let p = ParsedTurn::Opening { plan };
        (Some(p.to_json()), None)
    } else {
        match parse_turn_output(&raw1, kind) {
            Ok(p) => (Some(p.to_json()), None),
            Err(err) => {
                // ONE corrective retry on the SAME conversation.
                let missing_tags = err
                    .missing
                    .iter()
                    .map(|t| format!("<{}>", t))
                    .collect::<Vec<_>>()
                    .join(", ");
                let corrective = format!(
                    "Your previous response was missing required tags: {}. Produce \
them now exactly as specified. Do not call more tools. Output ONLY the tagged sections.",
                    missing_tags
                );

                let (raw2, usage2, tools2, _conv2) = stream_completion_continue(
                    app,
                    debate_id,
                    index,
                    speaker.label(),
                    cancel,
                    provider,
                    model,
                    api_key,
                    system_prompt,
                    conv1,
                    &raw1,
                    &corrective,
                    project_dir,
                )?;
                total_input += usage2.input_tokens;
                total_output += usage2.output_tokens;
                all_tools.extend(tools2);
                if !raw2.is_empty() {
                    combined_raw.push_str("\n\n[retry]\n\n");
                    combined_raw.push_str(&raw2);
                }
                match parse_turn_output(&raw2, kind) {
                    Ok(p) => (Some(p.to_json()), None),
                    Err(err2) => {
                        let msg = format!("missing tags after retry: {}", err2.message());
                        (None, Some(msg))
                    }
                }
            }
        }
    };

    let tool_records: Vec<DebateToolCallRecord> = all_tools
        .iter()
        .map(|t| DebateToolCallRecord {
            tool: t.tool.clone(),
            input: t.input_json.clone(),
            output: truncate_for_record(&t.output, TOOL_RECORD_OUTPUT_BYTES),
            is_error: t.is_error,
        })
        .collect();

    let turn = DebateTurn {
        index,
        speaker: speaker.label().to_string(),
        kind: kind.label().to_string(),
        model: model.to_string(),
        raw_text: combined_raw,
        parsed: parsed_json,
        parse_error,
        input_tokens: total_input,
        output_tokens: total_output,
        system_prompt: system_prompt.to_string(),
        user_prompt: user_prompt.to_string(),
        tool_calls: tool_records,
    };

    let _ = app.emit(
        "debate:turn_complete",
        TurnCompletePayload {
            debate_id,
            index: turn.index,
            speaker: &turn.speaker,
            kind: &turn.kind,
            raw_text: &turn.raw_text,
            parsed: turn.parsed.as_ref(),
            parse_error: turn.parse_error.as_deref(),
            input_tokens: turn.input_tokens,
            output_tokens: turn.output_tokens,
        },
    );

    Ok(turn)
}

// ── HTTP streaming + tool-use agentic loop ──────────────────────────────────

/// Opaque conversation state carried between the initial turn and a corrective
/// retry so the retry continues the SAME chat rather than rebuilding from
/// scratch.
#[derive(Debug, Clone)]
enum Conversation {
    Anthropic(Vec<serde_json::Value>),
    OpenAI(Vec<serde_json::Value>),
}

#[allow(clippy::too_many_arguments)]
fn stream_completion(
    app: &AppHandle<Wry>,
    debate_id: &str,
    index: u32,
    role: &str,
    cancel: &Arc<AtomicBool>,
    provider: Provider,
    model: &str,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    project_dir: Option<&str>,
) -> Result<(String, Usage, Vec<ToolCallTrace>, Conversation), String> {
    let project_dir_for_tools = project_dir.filter(|s| !s.is_empty());
    match provider {
        Provider::Anthropic => {
            let messages = vec![serde_json::json!({
                "role": "user",
                "content": user_prompt,
            })];
            let (text, usage, traces, msgs) = stream_anthropic(
                app,
                debate_id,
                index,
                role,
                cancel,
                api_key,
                model,
                system_prompt,
                messages,
                project_dir_for_tools,
            )?;
            Ok((text, usage, traces, Conversation::Anthropic(msgs)))
        }
        Provider::OpenAI => {
            let messages = vec![
                serde_json::json!({ "role": "system", "content": system_prompt }),
                serde_json::json!({ "role": "user", "content": user_prompt }),
            ];
            let (text, usage, traces, msgs) = stream_openai(
                app,
                debate_id,
                index,
                role,
                cancel,
                api_key,
                model,
                messages,
                project_dir_for_tools,
            )?;
            Ok((text, usage, traces, Conversation::OpenAI(msgs)))
        }
    }
}

/// Continue an existing conversation with an additional user message — used
/// by the parser's one-shot retry. Appends (a) the prior assistant text as a
/// fresh assistant turn, and (b) the corrective user message.
#[allow(clippy::too_many_arguments)]
fn stream_completion_continue(
    app: &AppHandle<Wry>,
    debate_id: &str,
    index: u32,
    role: &str,
    cancel: &Arc<AtomicBool>,
    provider: Provider,
    model: &str,
    api_key: &str,
    system_prompt: &str,
    prior: Conversation,
    prior_assistant_text: &str,
    corrective_user: &str,
    project_dir: Option<&str>,
) -> Result<(String, Usage, Vec<ToolCallTrace>, Conversation), String> {
    let project_dir_for_tools = project_dir.filter(|s| !s.is_empty());
    match (provider, prior) {
        (Provider::Anthropic, Conversation::Anthropic(mut messages)) => {
            // Append assistant text + corrective user.
            if !prior_assistant_text.is_empty() {
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": prior_assistant_text,
                }));
            }
            messages.push(serde_json::json!({
                "role": "user",
                "content": corrective_user,
            }));
            let (text, usage, traces, msgs) = stream_anthropic(
                app,
                debate_id,
                index,
                role,
                cancel,
                api_key,
                model,
                system_prompt,
                messages,
                project_dir_for_tools,
            )?;
            Ok((text, usage, traces, Conversation::Anthropic(msgs)))
        }
        (Provider::OpenAI, Conversation::OpenAI(mut messages)) => {
            if !prior_assistant_text.is_empty() {
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": prior_assistant_text,
                }));
            }
            messages.push(serde_json::json!({
                "role": "user",
                "content": corrective_user,
            }));
            let (text, usage, traces, msgs) = stream_openai(
                app,
                debate_id,
                index,
                role,
                cancel,
                api_key,
                model,
                messages,
                project_dir_for_tools,
            )?;
            Ok((text, usage, traces, Conversation::OpenAI(msgs)))
        }
        // Provider/conversation mismatch — shouldn't happen, but treat as fresh.
        (Provider::Anthropic, _) => stream_completion(
            app,
            debate_id,
            index,
            role,
            cancel,
            provider,
            model,
            api_key,
            system_prompt,
            corrective_user,
            project_dir,
        ),
        (Provider::OpenAI, _) => stream_completion(
            app,
            debate_id,
            index,
            role,
            cancel,
            provider,
            model,
            api_key,
            system_prompt,
            corrective_user,
            project_dir,
        ),
    }
}

fn emit_token(app: &AppHandle<Wry>, debate_id: &str, index: u32, text: &str) {
    if text.is_empty() {
        return;
    }
    let _ = app.emit(
        "debate:token",
        TokenPayload {
            debate_id,
            index,
            text,
        },
    );
}

/// Emit one `debate:tool_call` event AFTER the tool runs (so we know the result).
fn emit_tool_call(
    app: &AppHandle<Wry>,
    debate_id: &str,
    index: u32,
    role: &str,
    tool: &str,
    input_preview: &str,
    output_preview: &str,
    is_error: bool,
) {
    let _ = app.emit(
        "debate:tool_call",
        ToolCallPayload {
            debate_id,
            index,
            role,
            tool,
            input_preview,
            output_preview,
            is_error,
        },
    );
}

// ── Anthropic agentic loop ──────────────────────────────────────────────────

/// One `tool_use` block accumulated from the stream. `partial_input` is the
/// concatenation of `input_json_delta.partial_json` fragments; we parse it
/// at `content_block_stop` time.
#[derive(Debug, Clone)]
struct AnthropicToolUse {
    id: String,
    name: String,
    partial_input: String,
}

/// What one streaming pass produced. Used to drive the agentic loop.
struct AnthropicStreamPass {
    text: String,
    tool_uses: Vec<AnthropicToolUse>,
    usage: Usage,
    stop_reason: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn stream_anthropic(
    app: &AppHandle<Wry>,
    debate_id: &str,
    index: u32,
    role: &str,
    cancel: &Arc<AtomicBool>,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    mut messages: Vec<serde_json::Value>,
    project_dir: Option<&str>,
) -> Result<(String, Usage, Vec<ToolCallTrace>, Vec<serde_json::Value>), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .build()
        .map_err(|e| format!("http_client_error: {}", e))?;

    let tools = if project_dir.is_some() {
        Some(anthropic_tool_specs())
    } else {
        None
    };

    let mut total_usage = Usage::default();
    let mut total_text = String::new();
    let mut tool_traces: Vec<ToolCallTrace> = Vec::new();

    for iteration in 0..MAX_TOOL_LOOP_ITERATIONS {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }

        // Last iteration is forced text-only so the model can't keep
        // tool-calling forever.
        let allow_tools = tools.is_some() && iteration + 1 < MAX_TOOL_LOOP_ITERATIONS;

        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": ANTHROPIC_MAX_TOKENS,
            "stream": true,
            "system": system_prompt,
            "messages": messages,
        });
        if allow_tools {
            if let Some(t) = &tools {
                body["tools"] = serde_json::Value::Array(t.clone());
            }
        }

        let pass = anthropic_stream_pass(
            app,
            debate_id,
            index,
            cancel,
            &client,
            api_key,
            &body,
        )?;

        total_usage.input_tokens += pass.usage.input_tokens;
        total_usage.output_tokens += pass.usage.output_tokens;
        if !pass.text.is_empty() {
            total_text.push_str(&pass.text);
        }

        let wants_tool_use = pass.stop_reason.as_deref() == Some("tool_use")
            && !pass.tool_uses.is_empty()
            && allow_tools;

        if !wants_tool_use {
            break;
        }

        // Build the assistant turn that the API expects to see verbatim on the
        // next request. Anthropic requires the full ordered content blocks.
        let mut assistant_blocks: Vec<serde_json::Value> = Vec::new();
        if !pass.text.is_empty() {
            assistant_blocks.push(serde_json::json!({
                "type": "text",
                "text": pass.text,
            }));
        }
        for tu in &pass.tool_uses {
            let parsed_input: serde_json::Value = serde_json::from_str(&tu.partial_input)
                .unwrap_or_else(|_| serde_json::json!({}));
            assistant_blocks.push(serde_json::json!({
                "type": "tool_use",
                "id": tu.id,
                "name": tu.name,
                "input": parsed_input,
            }));
        }
        messages.push(serde_json::json!({
            "role": "assistant",
            "content": assistant_blocks,
        }));

        // Execute each tool sequentially and build a user `tool_result` turn.
        let mut tool_results: Vec<serde_json::Value> = Vec::new();
        for tu in &pass.tool_uses {
            if cancel.load(Ordering::SeqCst) {
                return Err("cancelled".to_string());
            }
            let input_value: serde_json::Value = serde_json::from_str(&tu.partial_input)
                .unwrap_or_else(|_| serde_json::json!({}));
            let (output, is_error) = match project_dir {
                Some(dir) => match execute_tool(&tu.name, &input_value, dir) {
                    Ok(s) => (s, false),
                    Err(e) => (e, true),
                },
                None => (
                    "tools_unavailable: no project_dir was provided".to_string(),
                    true,
                ),
            };
            let input_preview = preview_input(&input_value, TOOL_EVENT_INPUT_PREVIEW);
            let output_preview = preview_output(&output, TOOL_EVENT_OUTPUT_PREVIEW);
            emit_tool_call(
                app,
                debate_id,
                index,
                role,
                &tu.name,
                &input_preview,
                &output_preview,
                is_error,
            );
            tool_traces.push(ToolCallTrace {
                tool: tu.name.clone(),
                input_json: serde_json::to_string(&input_value).unwrap_or_default(),
                output: output.clone(),
                is_error,
            });
            tool_results.push(serde_json::json!({
                "type": "tool_result",
                "tool_use_id": tu.id,
                "content": output,
                "is_error": is_error,
            }));
        }
        messages.push(serde_json::json!({
            "role": "user",
            "content": tool_results,
        }));
    }

    Ok((total_text, total_usage, tool_traces, messages))
}

#[allow(clippy::too_many_arguments)]
fn anthropic_stream_pass(
    app: &AppHandle<Wry>,
    debate_id: &str,
    index: u32,
    cancel: &Arc<AtomicBool>,
    client: &reqwest::blocking::Client,
    api_key: &str,
    body: &serde_json::Value,
) -> Result<AnthropicStreamPass, String> {
    let resp = client
        .post(ANTHROPIC_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(body)
        .send()
        .map_err(|e| format!("anthropic_request_error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("anthropic_http_{}: {}", status.as_u16(), text));
    }

    let reader = BufReader::new(resp);
    let mut accumulated = String::new();
    let mut usage = Usage::default();
    let mut stop_reason: Option<String> = None;

    // index → either an in-progress tool_use block or None for text blocks.
    let mut current_tool_uses: HashMap<u32, AnthropicToolUse> = HashMap::new();
    let mut final_tool_uses: Vec<AnthropicToolUse> = Vec::new();

    for line_result in reader.lines() {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }
        let line = match line_result {
            Ok(l) => l,
            Err(e) => return Err(format!("anthropic_read_error: {}", e)),
        };
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        let json: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let event_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match event_type {
            "message_start" => {
                if let Some(u) = json.get("message").and_then(|m| m.get("usage")) {
                    if let Some(v) = u.get("input_tokens").and_then(|x| x.as_u64()) {
                        usage.input_tokens = v;
                    }
                    if let Some(v) = u.get("output_tokens").and_then(|x| x.as_u64()) {
                        usage.output_tokens = v;
                    }
                }
            }
            "content_block_start" => {
                let idx = json.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let block = json.get("content_block");
                let btype = block.and_then(|b| b.get("type")).and_then(|t| t.as_str()).unwrap_or("");
                if btype == "tool_use" {
                    let id = block
                        .and_then(|b| b.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let name = block
                        .and_then(|b| b.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    current_tool_uses.insert(
                        idx,
                        AnthropicToolUse {
                            id,
                            name,
                            partial_input: String::new(),
                        },
                    );
                }
            }
            "content_block_delta" => {
                let idx = json.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let delta = json.get("delta");
                let dtype = delta.and_then(|d| d.get("type")).and_then(|t| t.as_str()).unwrap_or("");
                match dtype {
                    "text_delta" => {
                        if let Some(text) = delta
                            .and_then(|d| d.get("text"))
                            .and_then(|t| t.as_str())
                        {
                            accumulated.push_str(text);
                            emit_token(app, debate_id, index, text);
                        }
                    }
                    "input_json_delta" => {
                        if let Some(frag) = delta
                            .and_then(|d| d.get("partial_json"))
                            .and_then(|t| t.as_str())
                        {
                            if let Some(tu) = current_tool_uses.get_mut(&idx) {
                                tu.partial_input.push_str(frag);
                            }
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let idx = json.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                if let Some(tu) = current_tool_uses.remove(&idx) {
                    final_tool_uses.push(tu);
                }
            }
            "message_delta" => {
                if let Some(u) = json.get("usage") {
                    if let Some(v) = u.get("output_tokens").and_then(|x| x.as_u64()) {
                        usage.output_tokens = v;
                    }
                }
                if let Some(reason) = json
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(|s| s.as_str())
                {
                    stop_reason = Some(reason.to_string());
                }
            }
            "message_stop" => break,
            "error" => {
                let msg = json
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("anthropic stream error");
                return Err(format!("anthropic_stream_error: {}", msg));
            }
            _ => {}
        }
    }

    // Any tool_use blocks left open (e.g. stream cut early) — empty input.
    for (_idx, tu) in current_tool_uses.drain() {
        final_tool_uses.push(tu);
    }

    Ok(AnthropicStreamPass {
        text: accumulated,
        tool_uses: final_tool_uses,
        usage,
        stop_reason,
    })
}

// ── OpenAI agentic loop ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct OpenAIToolCall {
    id: String,
    name: String,
    arguments: String,
}

struct OpenAIStreamPass {
    text: String,
    tool_calls: Vec<OpenAIToolCall>,
    usage: Usage,
    finish_reason: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn stream_openai(
    app: &AppHandle<Wry>,
    debate_id: &str,
    index: u32,
    role: &str,
    cancel: &Arc<AtomicBool>,
    api_key: &str,
    model: &str,
    mut messages: Vec<serde_json::Value>,
    project_dir: Option<&str>,
) -> Result<(String, Usage, Vec<ToolCallTrace>, Vec<serde_json::Value>), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .build()
        .map_err(|e| format!("http_client_error: {}", e))?;

    let tools = if project_dir.is_some() {
        Some(openai_tool_specs())
    } else {
        None
    };

    let mut total_usage = Usage::default();
    let mut total_text = String::new();
    let mut tool_traces: Vec<ToolCallTrace> = Vec::new();

    for iteration in 0..MAX_TOOL_LOOP_ITERATIONS {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }

        let allow_tools = tools.is_some() && iteration + 1 < MAX_TOOL_LOOP_ITERATIONS;

        let mut body = serde_json::json!({
            "model": model,
            "stream": true,
            "stream_options": { "include_usage": true },
            "messages": messages,
        });
        if allow_tools {
            if let Some(t) = &tools {
                body["tools"] = serde_json::Value::Array(t.clone());
                body["tool_choice"] = serde_json::Value::String("auto".to_string());
            }
        }

        let pass = openai_stream_pass(
            app,
            debate_id,
            index,
            cancel,
            &client,
            api_key,
            &body,
        )?;

        total_usage.input_tokens += pass.usage.input_tokens;
        total_usage.output_tokens += pass.usage.output_tokens;
        if !pass.text.is_empty() {
            total_text.push_str(&pass.text);
        }

        let wants_tool_calls = pass.finish_reason.as_deref() == Some("tool_calls")
            && !pass.tool_calls.is_empty()
            && allow_tools;

        if !wants_tool_calls {
            break;
        }

        let tool_calls_json: Vec<serde_json::Value> = pass
            .tool_calls
            .iter()
            .map(|tc| {
                serde_json::json!({
                    "id": tc.id,
                    "type": "function",
                    "function": { "name": tc.name, "arguments": tc.arguments },
                })
            })
            .collect();
        messages.push(serde_json::json!({
            "role": "assistant",
            "content": if pass.text.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(pass.text.clone())
            },
            "tool_calls": tool_calls_json,
        }));

        for tc in &pass.tool_calls {
            if cancel.load(Ordering::SeqCst) {
                return Err("cancelled".to_string());
            }
            let input_value: serde_json::Value = serde_json::from_str(&tc.arguments)
                .unwrap_or_else(|_| serde_json::json!({}));
            let (output, is_error) = match project_dir {
                Some(dir) => match execute_tool(&tc.name, &input_value, dir) {
                    Ok(s) => (s, false),
                    Err(e) => (e, true),
                },
                None => (
                    "tools_unavailable: no project_dir was provided".to_string(),
                    true,
                ),
            };
            let input_preview = preview_input(&input_value, TOOL_EVENT_INPUT_PREVIEW);
            let output_preview = preview_output(&output, TOOL_EVENT_OUTPUT_PREVIEW);
            emit_tool_call(
                app,
                debate_id,
                index,
                role,
                &tc.name,
                &input_preview,
                &output_preview,
                is_error,
            );
            tool_traces.push(ToolCallTrace {
                tool: tc.name.clone(),
                input_json: serde_json::to_string(&input_value).unwrap_or_default(),
                output: output.clone(),
                is_error,
            });
            messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": output,
            }));
        }
    }

    Ok((total_text, total_usage, tool_traces, messages))
}

#[allow(clippy::too_many_arguments)]
fn openai_stream_pass(
    app: &AppHandle<Wry>,
    debate_id: &str,
    index: u32,
    cancel: &Arc<AtomicBool>,
    client: &reqwest::blocking::Client,
    api_key: &str,
    body: &serde_json::Value,
) -> Result<OpenAIStreamPass, String> {
    let resp = client
        .post(OPENAI_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("content-type", "application/json")
        .json(body)
        .send()
        .map_err(|e| format!("openai_request_error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("openai_http_{}: {}", status.as_u16(), text));
    }

    let reader = BufReader::new(resp);
    let mut accumulated = String::new();
    let mut usage = Usage::default();
    let mut finish_reason: Option<String> = None;

    let mut partial_calls: HashMap<u32, OpenAIToolCall> = HashMap::new();

    for line_result in reader.lines() {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }
        let line = match line_result {
            Ok(l) => l,
            Err(e) => return Err(format!("openai_read_error: {}", e)),
        };
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            break;
        }
        let json: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                if let Some(text) = choice
                    .get("delta")
                    .and_then(|d| d.get("content"))
                    .and_then(|t| t.as_str())
                {
                    accumulated.push_str(text);
                    emit_token(app, debate_id, index, text);
                }
                if let Some(tool_calls) = choice
                    .get("delta")
                    .and_then(|d| d.get("tool_calls"))
                    .and_then(|t| t.as_array())
                {
                    for tc in tool_calls {
                        let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let entry =
                            partial_calls.entry(idx).or_insert_with(|| OpenAIToolCall {
                                id: String::new(),
                                name: String::new(),
                                arguments: String::new(),
                            });
                        if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                            if !id.is_empty() {
                                entry.id = id.to_string();
                            }
                        }
                        if let Some(function) = tc.get("function") {
                            if let Some(name) = function.get("name").and_then(|v| v.as_str()) {
                                if !name.is_empty() {
                                    entry.name = name.to_string();
                                }
                            }
                            if let Some(arg_frag) =
                                function.get("arguments").and_then(|v| v.as_str())
                            {
                                entry.arguments.push_str(arg_frag);
                            }
                        }
                    }
                }
                if let Some(reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                    finish_reason = Some(reason.to_string());
                }
            }
        }
        if let Some(u) = json.get("usage") {
            if let Some(v) = u.get("prompt_tokens").and_then(|x| x.as_u64()) {
                usage.input_tokens = v;
            }
            if let Some(v) = u.get("completion_tokens").and_then(|x| x.as_u64()) {
                usage.output_tokens = v;
            }
        }
        if let Some(err) = json.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("openai stream error");
            return Err(format!("openai_stream_error: {}", msg));
        }
    }

    let mut indices: Vec<u32> = partial_calls.keys().copied().collect();
    indices.sort_unstable();
    let mut tool_calls: Vec<OpenAIToolCall> = Vec::with_capacity(indices.len());
    for idx in indices {
        if let Some(tc) = partial_calls.remove(&idx) {
            tool_calls.push(tc);
        }
    }

    Ok(OpenAIStreamPass {
        text: accumulated,
        tool_calls,
        usage,
        finish_reason,
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── cost_for ────────────────────────────────────────────────────────

    #[test]
    fn cost_for_known_anthropic_model() {
        let c = cost_for("claude-opus-4-7", 1_000_000, 1_000_000);
        assert!((c - 90.0).abs() < 1e-9, "got {}", c);
    }

    #[test]
    fn cost_for_known_openai_model() {
        let c = cost_for("gpt-5", 1_000_000, 1_000_000);
        assert!((c - 11.25).abs() < 1e-9, "got {}", c);
    }

    #[test]
    fn cost_for_unknown_model_is_zero() {
        let c = cost_for("not-a-real-model", 1_000_000, 1_000_000);
        assert_eq!(c, 0.0);
    }

    #[test]
    fn cost_scales_linearly() {
        let c = cost_for("claude-haiku-4-5-20251001", 1000, 0);
        assert!((c - 0.0008).abs() < 1e-9, "got {}", c);
    }

    // ── strip_v_suffix ──────────────────────────────────────────────────

    #[test]
    fn strip_v_no_suffix() {
        assert_eq!(strip_v_suffix("foo.plan"), "foo.plan");
        assert_eq!(strip_v_suffix("foo"), "foo");
    }

    #[test]
    fn strip_v_one_suffix() {
        assert_eq!(strip_v_suffix("foo_v1"), "foo");
        assert_eq!(strip_v_suffix("foo_v42"), "foo");
        assert_eq!(strip_v_suffix("foo.plan_v2"), "foo.plan");
    }

    #[test]
    fn strip_v_requires_terminal_digits() {
        assert_eq!(strip_v_suffix("foo_v1_extra"), "foo_v1_extra");
        assert_eq!(strip_v_suffix("foo_vabc"), "foo_vabc");
        assert_eq!(strip_v_suffix("foo_v"), "foo_v");
    }

    // ── parse_turn_output ───────────────────────────────────────────────

    #[test]
    fn parse_critique_valid_request_changes() {
        let text = "Thinking out loud...\n<issues>\n1. Missing X.\n2. Wrong Y.\n</issues>\n<verdict>REQUEST_CHANGES</verdict>";
        let parsed = parse_turn_output(text, TurnKind::Critique).unwrap();
        match parsed {
            ParsedTurn::Critique { issues, verdict } => {
                assert_eq!(issues, vec!["Missing X.".to_string(), "Wrong Y.".to_string()]);
                assert_eq!(verdict, "REQUEST_CHANGES");
            }
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn parse_critique_approve_with_empty_issues() {
        let text = "<issues>\n</issues>\n<verdict>APPROVE</verdict>";
        let parsed = parse_turn_output(text, TurnKind::Critique).unwrap();
        match parsed {
            ParsedTurn::Critique { issues, verdict } => {
                assert!(issues.is_empty());
                assert_eq!(verdict, "APPROVE");
            }
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn parse_critique_missing_verdict() {
        let text = "<issues>\n1. foo\n</issues>";
        let err = parse_turn_output(text, TurnKind::Critique).unwrap_err();
        assert!(err.missing.contains(&"verdict".to_string()));
    }

    #[test]
    fn parse_critique_missing_issues() {
        let text = "<verdict>APPROVE</verdict>";
        let err = parse_turn_output(text, TurnKind::Critique).unwrap_err();
        assert!(err.missing.contains(&"issues".to_string()));
    }

    #[test]
    fn parse_critique_invalid_verdict_literal() {
        let text = "<issues>\n1. foo\n</issues>\n<verdict>MAYBE</verdict>";
        let err = parse_turn_output(text, TurnKind::Critique).unwrap_err();
        assert!(err.missing.contains(&"verdict".to_string()));
    }

    #[test]
    fn parse_response_valid() {
        let text = "<accepted>\n- #1: rewrote section X\n- #3: added test\n</accepted>\n<rebutted>\n- #2: invariant already covered\n</rebutted>\n<refined_plan>\n# New plan\n\nbody\n</refined_plan>";
        let parsed = parse_turn_output(text, TurnKind::Response).unwrap();
        match parsed {
            ParsedTurn::Response { accepted, rebutted, refined_plan } => {
                assert_eq!(accepted.len(), 2);
                assert_eq!(rebutted.len(), 1);
                assert!(refined_plan.unwrap().contains("# New plan"));
            }
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn parse_response_empty_accepted_and_rebutted() {
        let text = "<accepted>\n</accepted>\n<rebutted>\n</rebutted>\n<refined_plan>\nsame plan\n</refined_plan>";
        let parsed = parse_turn_output(text, TurnKind::Response).unwrap();
        match parsed {
            ParsedTurn::Response { accepted, rebutted, refined_plan } => {
                assert!(accepted.is_empty());
                assert!(rebutted.is_empty());
                assert_eq!(refined_plan.as_deref(), Some("same plan"));
            }
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn parse_response_missing_refined_plan() {
        let text = "<accepted>\n- #1: x\n</accepted>\n<rebutted>\n</rebutted>";
        let err = parse_turn_output(text, TurnKind::Response).unwrap_err();
        assert!(err.missing.contains(&"refined_plan".to_string()));
    }

    #[test]
    fn parse_response_missing_accepted() {
        let text = "<rebutted>\n</rebutted>\n<refined_plan>\nx\n</refined_plan>";
        let err = parse_turn_output(text, TurnKind::Response).unwrap_err();
        assert!(err.missing.contains(&"accepted".to_string()));
    }

    #[test]
    fn parse_finalize_valid() {
        let text = "<final_plan>\n# Plan\n\nbody\n</final_plan>\n<caveats>\n- Open question on Z\n- Edge case w\n</caveats>";
        let parsed = parse_turn_output(text, TurnKind::Finalize).unwrap();
        match parsed {
            ParsedTurn::Finalize { plan, caveats } => {
                assert!(plan.starts_with("# Plan"));
                assert_eq!(caveats.len(), 2);
            }
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn parse_finalize_empty_caveats() {
        let text = "<final_plan>\nfoo\n</final_plan>\n<caveats>\n</caveats>";
        let parsed = parse_turn_output(text, TurnKind::Finalize).unwrap();
        match parsed {
            ParsedTurn::Finalize { plan, caveats } => {
                assert_eq!(plan, "foo");
                assert!(caveats.is_empty());
            }
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn parse_finalize_missing_caveats() {
        let text = "<final_plan>\nfoo\n</final_plan>";
        let err = parse_turn_output(text, TurnKind::Finalize).unwrap_err();
        assert!(err.missing.contains(&"caveats".to_string()));
    }

    #[test]
    fn parse_tolerates_nested_angle_brackets_in_content() {
        // The greedy-to-last-closing rule lets inner content include `<` /
        // `</` snippets that aren't a true closing tag.
        let text = "<final_plan>\nuse the <Foo /> component, then close it with </Foo>.\n</final_plan>\n<caveats>\n</caveats>";
        let parsed = parse_turn_output(text, TurnKind::Finalize).unwrap();
        match parsed {
            ParsedTurn::Finalize { plan, .. } => {
                assert!(plan.contains("<Foo />"));
                assert!(plan.contains("</Foo>"));
            }
            _ => panic!("wrong kind"),
        }
    }

    // ── transcript_excerpt ─────────────────────────────────────────────

    #[test]
    fn transcript_excerpt_includes_delimiters_and_parsed() {
        let opening = DebateTurn {
            index: 1,
            speaker: "author".to_string(),
            kind: "opening".to_string(),
            model: "m".to_string(),
            raw_text: "narration".to_string(),
            parsed: Some(ParsedTurn::Opening { plan: "PLAN_CONTENT".to_string() }.to_json()),
            parse_error: None,
            input_tokens: 0,
            output_tokens: 0,
            system_prompt: String::new(),
            user_prompt: String::new(),
            tool_calls: vec![],
        };
        let critique = DebateTurn {
            index: 2,
            speaker: "reviewer".to_string(),
            kind: "critique".to_string(),
            model: "m".to_string(),
            raw_text: "raw".to_string(),
            parsed: Some(
                ParsedTurn::Critique {
                    issues: vec!["A".to_string(), "B".to_string()],
                    verdict: "REQUEST_CHANGES".to_string(),
                }
                .to_json(),
            ),
            parse_error: None,
            input_tokens: 0,
            output_tokens: 0,
            system_prompt: String::new(),
            user_prompt: String::new(),
            tool_calls: vec![],
        };
        let s = transcript_excerpt(&[opening, critique]);
        assert!(s.contains("─── Turn 1 · Author (opening) ───"));
        assert!(s.contains("PLAN_CONTENT"));
        // narration text from opening must NOT appear — only parsed plan.
        assert!(!s.contains("narration"));
        assert!(s.contains("─── Turn 2 · Reviewer (critique) ───"));
        assert!(s.contains("1. A"));
        assert!(s.contains("2. B"));
        assert!(s.contains("Verdict: REQUEST_CHANGES"));
    }

    #[test]
    fn transcript_excerpt_falls_back_to_raw_on_parse_error() {
        let bad = DebateTurn {
            index: 1,
            speaker: "reviewer".to_string(),
            kind: "critique".to_string(),
            model: "m".to_string(),
            raw_text: "this was malformed".to_string(),
            parsed: None,
            parse_error: Some("missing tags after retry".to_string()),
            input_tokens: 0,
            output_tokens: 0,
            system_prompt: String::new(),
            user_prompt: String::new(),
            tool_calls: vec![],
        };
        let s = transcript_excerpt(&[bad]);
        assert!(s.contains("(unstructured)"));
        assert!(s.contains("this was malformed"));
    }

    // ── auto_save_versioned_plan + format_saved_file ───────────────────

    #[test]
    fn auto_save_versioned_plan_starts_at_v2_for_plain_md() {
        let tmp = tempfile::tempdir().unwrap();
        let plan_path = tmp.path().join("foo.md");
        std::fs::write(&plan_path, "original").unwrap();
        let out =
            auto_save_versioned_plan(plan_path.to_str().unwrap(), "refined").unwrap();
        assert!(out.ends_with("foo_v2.md"), "got {}", out);
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "refined");
    }

    #[test]
    fn auto_save_versioned_plan_extends_existing_v_sequence() {
        let tmp = tempfile::tempdir().unwrap();
        let plan_path = tmp.path().join("foo.md");
        std::fs::write(&plan_path, "original").unwrap();
        std::fs::write(tmp.path().join("foo_v2.md"), "x").unwrap();
        std::fs::write(tmp.path().join("foo_v5.md"), "x").unwrap();
        let out =
            auto_save_versioned_plan(plan_path.to_str().unwrap(), "refined").unwrap();
        assert!(out.ends_with("foo_v6.md"), "got {}", out);
    }

    #[test]
    fn auto_save_versioned_plan_strips_input_v_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        let plan_path = tmp.path().join("foo_v2.md");
        std::fs::write(&plan_path, "v2 plan").unwrap();
        let out =
            auto_save_versioned_plan(plan_path.to_str().unwrap(), "refined").unwrap();
        // Stem becomes "foo"; foo_v2.md exists so next is foo_v3.md.
        assert!(out.ends_with("foo_v3.md"), "got {}", out);
    }

    #[test]
    fn auto_save_versioned_plan_preserves_intermediate_dots() {
        let tmp = tempfile::tempdir().unwrap();
        let plan_path = tmp.path().join("foo.plan.md");
        std::fs::write(&plan_path, "original").unwrap();
        let out =
            auto_save_versioned_plan(plan_path.to_str().unwrap(), "refined").unwrap();
        assert!(out.ends_with("foo.plan_v2.md"), "got {}", out);
    }

    #[test]
    fn format_saved_file_no_caveats_returns_plan_only() {
        let s = format_saved_file("# plan", &[], "a", "b", 6, 0.1234);
        assert_eq!(s, "# plan");
    }

    #[test]
    fn format_saved_file_appends_caveats_block() {
        let caveats = vec!["one".to_string(), "two".to_string()];
        let s = format_saved_file("# plan\n", &caveats, "auth-m", "rev-m", 6, 0.0987);
        assert!(s.contains("Reviewer caveats from debate"));
        assert!(s.contains("auth-m vs rev-m"));
        assert!(s.contains("6 turns"));
        assert!(s.contains("$0.0987"));
        assert!(s.contains("> - one"));
        assert!(s.contains("> - two"));
    }

    // ── Numbered / bulleted splitters ──────────────────────────────────

    #[test]
    fn split_numbered_basic() {
        let body = "1. first\n2. second\n3. third";
        let v = split_numbered(body);
        assert_eq!(v, vec!["first", "second", "third"]);
    }

    #[test]
    fn split_numbered_multiline_items() {
        let body = "1. first\n   continued\n2. second";
        let v = split_numbered(body);
        assert_eq!(v.len(), 2);
        assert!(v[0].contains("continued"));
    }

    #[test]
    fn split_bulleted_basic() {
        let body = "- one\n- two";
        let v = split_bulleted(body);
        assert_eq!(v, vec!["one", "two"]);
    }
}
