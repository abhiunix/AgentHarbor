use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use zip::write::FileOptions;

use crate::analytics::http::build_client;
use crate::analytics::token_store;
use crate::utils::paths::{app_data_dir, atomic_write, atomic_write_str, read_with_sharing};

/// Base URLs for every benchmark-eligible provider.
///
/// Production callers use `ProviderEndpoints::default()`, which returns the
/// real provider hostnames. Tests construct one pointing at a wiremock
/// server's `uri()` to avoid live HTTP calls.
#[derive(Debug, Clone)]
pub struct ProviderEndpoints {
    pub openai: String,
    pub anthropic: String,
    pub gemini: String,
    pub openrouter: String,
}

impl ProviderEndpoints {
    pub fn production() -> Self {
        Self {
            openai: "https://api.openai.com".into(),
            anthropic: "https://api.anthropic.com".into(),
            gemini: "https://generativelanguage.googleapis.com".into(),
            openrouter: "https://openrouter.ai".into(),
        }
    }
}

impl Default for ProviderEndpoints {
    fn default() -> Self {
        Self::production()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkModality {
    Text,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkAssertionKind {
    Contains,
    Regex,
    ExactMatch,
    JsonParse,
    JsonKeysPresent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkAssertion {
    pub kind: BenchmarkAssertionKind,
    pub value: Option<String>,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkCase {
    pub id: String,
    pub name: String,
    pub modality: BenchmarkModality,
    pub input: String,
    pub reference_output: Option<String>,
    #[serde(default)]
    pub assertions: Vec<BenchmarkAssertion>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkVariant {
    pub id: String,
    pub name: String,
    pub system_prompt: Option<String>,
    pub prompt_prefix: Option<String>,
    pub prompt_suffix: Option<String>,
    pub capability_context: Option<String>,
    #[serde(default)]
    pub capability_labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkTarget {
    pub provider_id: String,
    pub model_id: String,
    pub modality: BenchmarkModality,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<u32>,
    pub image_size: Option<String>,
    pub image_quality: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkJudgeConfig {
    pub enabled: bool,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub rubric: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BenchmarkTokenCounts {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub kind: String,
    pub label: String,
    pub path: String,
    pub mime_type: String,
    pub preview_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterministicScore {
    pub kind: String,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeScore {
    pub score: Option<f64>,
    pub rationale: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManualReview {
    pub rating: Option<u8>,
    pub preferred: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkItemStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRunItem {
    pub item_id: String,
    pub case_id: String,
    pub case_name: String,
    pub variant_id: String,
    pub variant_name: String,
    pub provider_id: String,
    pub model_id: String,
    pub modality: BenchmarkModality,
    pub status: BenchmarkItemStatus,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub latency_ms: Option<u128>,
    pub token_counts: BenchmarkTokenCounts,
    pub estimated_cost_usd: Option<f64>,
    pub context_window: Option<u64>,
    pub context_used_percent: Option<f64>,
    pub output_text: Option<String>,
    #[serde(default)]
    pub artifact_refs: Vec<ArtifactRef>,
    #[serde(default)]
    pub deterministic_scores: Vec<DeterministicScore>,
    pub judge_score: Option<JudgeScore>,
    pub manual_review: ManualReview,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkRunStatus {
    Running,
    Completed,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRun {
    pub id: String,
    pub name: String,
    pub modality: BenchmarkModality,
    pub dataset_name: Option<String>,
    pub status: BenchmarkRunStatus,
    pub created_at: String,
    pub completed_at: Option<String>,
    #[serde(default)]
    pub cases: Vec<BenchmarkCase>,
    #[serde(default)]
    pub variants: Vec<BenchmarkVariant>,
    #[serde(default)]
    pub targets: Vec<BenchmarkTarget>,
    #[serde(default)]
    pub items: Vec<BenchmarkRunItem>,
    pub judge_config: Option<BenchmarkJudgeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRunSummary {
    pub id: String,
    pub name: String,
    pub modality: BenchmarkModality,
    pub status: BenchmarkRunStatus,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub item_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkDataset {
    pub id: String,
    pub name: String,
    pub modality: BenchmarkModality,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub cases: Vec<BenchmarkCase>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkProvider {
    pub id: String,
    pub name: String,
    pub auth_type: String,
    pub key_label: String,
    pub key_placeholder: String,
    #[serde(default)]
    pub supported_modalities: Vec<BenchmarkModality>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkModel {
    pub provider_id: String,
    pub id: String,
    pub display_name: String,
    pub modality: BenchmarkModality,
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub supports_judge: bool,
    pub input_cost_per_million: Option<f64>,
    pub output_cost_per_million: Option<f64>,
    pub image_price_low_1024: Option<f64>,
    pub image_price_medium_1024: Option<f64>,
    pub image_price_high_1024: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceBenchmark {
    pub id: String,
    pub name: String,
    pub category: String,
    pub summary: String,
    pub source_url: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRunRequest {
    pub name: String,
    pub modality: BenchmarkModality,
    pub dataset_name: Option<String>,
    #[serde(default)]
    pub cases: Vec<BenchmarkCase>,
    #[serde(default)]
    pub variants: Vec<BenchmarkVariant>,
    #[serde(default)]
    pub targets: Vec<BenchmarkTarget>,
    pub judge: Option<BenchmarkJudgeConfig>,
}

fn benchmarks_dir() -> PathBuf {
    app_data_dir().join("benchmarks")
}

fn datasets_dir() -> PathBuf {
    benchmarks_dir().join("datasets")
}

fn runs_dir() -> PathBuf {
    benchmarks_dir().join("runs")
}

fn reference_dir() -> PathBuf {
    benchmarks_dir().join("reference")
}

fn run_dir(run_id: &str) -> PathBuf {
    runs_dir().join(run_id)
}

fn run_manifest_path(run_id: &str) -> PathBuf {
    run_dir(run_id).join("manifest.json")
}

fn run_items_dir(run_id: &str) -> PathBuf {
    run_dir(run_id).join("items")
}

fn run_artifacts_dir(run_id: &str) -> PathBuf {
    run_dir(run_id).join("artifacts")
}

fn dataset_path(dataset_id: &str) -> PathBuf {
    datasets_dir().join(format!("{}.json", dataset_id))
}

fn ensure_benchmark_dirs() -> Result<(), String> {
    fs::create_dir_all(datasets_dir()).map_err(|e| e.to_string())?;
    fs::create_dir_all(runs_dir()).map_err(|e| e.to_string())?;
    fs::create_dir_all(reference_dir()).map_err(|e| e.to_string())?;
    Ok(())
}

fn seeded_providers() -> Vec<BenchmarkProvider> {
    vec![
        BenchmarkProvider {
            id: "mock".into(),
            name: "Mock".into(),
            auth_type: "none".into(),
            key_label: String::new(),
            key_placeholder: String::new(),
            supported_modalities: vec![BenchmarkModality::Text, BenchmarkModality::Image],
        },
        BenchmarkProvider {
            id: "openai".into(),
            name: "OpenAI".into(),
            auth_type: "api-key".into(),
            key_label: "OpenAI API Key".into(),
            key_placeholder: "sk-...".into(),
            supported_modalities: vec![BenchmarkModality::Text, BenchmarkModality::Image],
        },
        BenchmarkProvider {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            auth_type: "api-key".into(),
            key_label: "Anthropic API Key".into(),
            key_placeholder: "sk-ant-...".into(),
            supported_modalities: vec![BenchmarkModality::Text],
        },
        BenchmarkProvider {
            id: "gemini".into(),
            name: "Gemini".into(),
            auth_type: "api-key".into(),
            key_label: "Gemini API Key".into(),
            key_placeholder: "AIza...".into(),
            supported_modalities: vec![BenchmarkModality::Text, BenchmarkModality::Image],
        },
        BenchmarkProvider {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            auth_type: "api-key".into(),
            key_label: "OpenRouter API Key".into(),
            key_placeholder: "sk-or-v1-...".into(),
            supported_modalities: vec![BenchmarkModality::Text],
        },
    ]
}

fn seeded_models() -> Vec<BenchmarkModel> {
    vec![
        BenchmarkModel {
            provider_id: "mock".into(),
            id: "mock-fast".into(),
            display_name: "Mock Fast".into(),
            modality: BenchmarkModality::Text,
            context_window: Some(128_000),
            max_output_tokens: Some(8_192),
            supports_judge: true,
            input_cost_per_million: Some(0.0),
            output_cost_per_million: Some(0.0),
            image_price_low_1024: None,
            image_price_medium_1024: None,
            image_price_high_1024: None,
        },
        BenchmarkModel {
            provider_id: "mock".into(),
            id: "mock-image".into(),
            display_name: "Mock Image".into(),
            modality: BenchmarkModality::Image,
            context_window: None,
            max_output_tokens: None,
            supports_judge: false,
            input_cost_per_million: None,
            output_cost_per_million: None,
            image_price_low_1024: Some(0.0),
            image_price_medium_1024: Some(0.0),
            image_price_high_1024: Some(0.0),
        },
        BenchmarkModel {
            provider_id: "openai".into(),
            id: "gpt-5".into(),
            display_name: "GPT-5".into(),
            modality: BenchmarkModality::Text,
            context_window: Some(400_000),
            max_output_tokens: Some(128_000),
            supports_judge: true,
            input_cost_per_million: Some(1.25),
            output_cost_per_million: Some(10.0),
            image_price_low_1024: None,
            image_price_medium_1024: None,
            image_price_high_1024: None,
        },
        BenchmarkModel {
            provider_id: "openai".into(),
            id: "gpt-5-mini".into(),
            display_name: "GPT-5 mini".into(),
            modality: BenchmarkModality::Text,
            context_window: Some(400_000),
            max_output_tokens: Some(128_000),
            supports_judge: true,
            input_cost_per_million: Some(0.25),
            output_cost_per_million: Some(2.0),
            image_price_low_1024: None,
            image_price_medium_1024: None,
            image_price_high_1024: None,
        },
        BenchmarkModel {
            provider_id: "openai".into(),
            id: "gpt-image-1".into(),
            display_name: "GPT Image 1".into(),
            modality: BenchmarkModality::Image,
            context_window: None,
            max_output_tokens: None,
            supports_judge: false,
            input_cost_per_million: None,
            output_cost_per_million: None,
            image_price_low_1024: Some(0.011),
            image_price_medium_1024: Some(0.042),
            image_price_high_1024: Some(0.167),
        },
        BenchmarkModel {
            provider_id: "anthropic".into(),
            id: "claude-sonnet-4-6".into(),
            display_name: "Claude Sonnet 4.6".into(),
            modality: BenchmarkModality::Text,
            context_window: Some(200_000),
            max_output_tokens: Some(64_000),
            supports_judge: true,
            input_cost_per_million: Some(3.0),
            output_cost_per_million: Some(15.0),
            image_price_low_1024: None,
            image_price_medium_1024: None,
            image_price_high_1024: None,
        },
        BenchmarkModel {
            provider_id: "anthropic".into(),
            id: "claude-opus-4-7".into(),
            display_name: "Claude Opus 4.7".into(),
            modality: BenchmarkModality::Text,
            context_window: Some(200_000),
            max_output_tokens: Some(64_000),
            supports_judge: true,
            input_cost_per_million: Some(5.0),
            output_cost_per_million: Some(25.0),
            image_price_low_1024: None,
            image_price_medium_1024: None,
            image_price_high_1024: None,
        },
        BenchmarkModel {
            provider_id: "gemini".into(),
            id: "gemini-2.5-flash".into(),
            display_name: "Gemini 2.5 Flash".into(),
            modality: BenchmarkModality::Text,
            context_window: Some(1_000_000),
            max_output_tokens: Some(65_536),
            supports_judge: true,
            input_cost_per_million: None,
            output_cost_per_million: None,
            image_price_low_1024: None,
            image_price_medium_1024: None,
            image_price_high_1024: None,
        },
        BenchmarkModel {
            provider_id: "gemini".into(),
            id: "gemini-3.1-flash-image-preview".into(),
            display_name: "Gemini 3.1 Flash Image Preview".into(),
            modality: BenchmarkModality::Image,
            context_window: None,
            max_output_tokens: None,
            supports_judge: false,
            input_cost_per_million: None,
            output_cost_per_million: None,
            image_price_low_1024: None,
            image_price_medium_1024: None,
            image_price_high_1024: None,
        },
        BenchmarkModel {
            provider_id: "openrouter".into(),
            id: "openai/gpt-5-mini".into(),
            display_name: "OpenRouter -> GPT-5 mini".into(),
            modality: BenchmarkModality::Text,
            context_window: Some(400_000),
            max_output_tokens: Some(128_000),
            supports_judge: true,
            input_cost_per_million: None,
            output_cost_per_million: None,
            image_price_low_1024: None,
            image_price_medium_1024: None,
            image_price_high_1024: None,
        },
    ]
}

fn seeded_reference_benchmarks() -> Vec<ReferenceBenchmark> {
    vec![
        ReferenceBenchmark {
            id: "swe-bench".into(),
            name: "SWE-bench".into(),
            category: "coding".into(),
            summary: "Reference family for software issue resolution and code-change correctness.".into(),
            source_url: "https://www.swebench.com/".into(),
            notes: "Use as category context for coding evaluations, not as a single universal score.".into(),
        },
        ReferenceBenchmark {
            id: "gdpval".into(),
            name: "GDPval".into(),
            category: "real-world".into(),
            summary: "OpenAI benchmark focused on economically valuable real-world tasks.".into(),
            source_url: "https://openai.com/index/gdpval/".into(),
            notes: "Useful reference for task realism and business impact framing.".into(),
        },
        ReferenceBenchmark {
            id: "browsecomp".into(),
            name: "BrowseComp".into(),
            category: "research".into(),
            summary: "OpenAI browsing benchmark for difficult web research and retrieval tasks.".into(),
            source_url: "https://openai.com/index/browsecomp/".into(),
            notes: "Helpful for benchmarking synthesis and web research flows.".into(),
        },
        ReferenceBenchmark {
            id: "helm".into(),
            name: "HELM".into(),
            category: "taxonomy".into(),
            summary: "Living benchmark framework spanning multiple evaluation dimensions.".into(),
            source_url: "https://crfm.stanford.edu/helm/latest/".into(),
            notes: "Use to guide benchmark diversity and reporting dimensions.".into(),
        },
        ReferenceBenchmark {
            id: "t2i-compbench-plus-plus".into(),
            name: "T2I-CompBench++".into(),
            category: "image".into(),
            summary: "Text-to-image compositional fidelity benchmark.".into(),
            source_url: "https://arxiv.org/abs/2307.06350".into(),
            notes: "Useful reference for prompt adherence and composition quality in image tasks.".into(),
        },
    ]
}

fn seeded_datasets() -> Vec<BenchmarkDataset> {
    let now = Utc::now().to_rfc3339();
    vec![
        BenchmarkDataset {
            id: "seeded/coding-quick-check".into(),
            name: "Coding Quick Check".into(),
            modality: BenchmarkModality::Text,
            description: "Short coding-oriented prompts for comparing code quality, tests, and refactors.".into(),
            cases: vec![
                BenchmarkCase {
                    id: "coding-bugfix".into(),
                    name: "Bugfix explanation".into(),
                    modality: BenchmarkModality::Text,
                    input: "Explain how you would fix a pagination bug where page 2 duplicates items from page 1. Return a concise plan and one pseudocode patch.".into(),
                    reference_output: None,
                    assertions: vec![BenchmarkAssertion {
                        kind: BenchmarkAssertionKind::Contains,
                        value: Some("pagination".into()),
                        values: vec![],
                    }],
                    tags: vec!["coding".into(), "bugfix".into()],
                },
                BenchmarkCase {
                    id: "coding-tests".into(),
                    name: "Test plan".into(),
                    modality: BenchmarkModality::Text,
                    input: "Given a function that parses user JSON into a settings object, propose edge-case tests and return them as valid JSON with a top-level `tests` array.".into(),
                    reference_output: None,
                    assertions: vec![
                        BenchmarkAssertion {
                            kind: BenchmarkAssertionKind::JsonParse,
                            value: None,
                            values: vec![],
                        },
                        BenchmarkAssertion {
                            kind: BenchmarkAssertionKind::JsonKeysPresent,
                            value: None,
                            values: vec!["tests".into()],
                        },
                    ],
                    tags: vec!["coding".into(), "tests".into()],
                },
            ],
            tags: vec!["seeded".into(), "coding".into()],
            created_at: now.clone(),
            updated_at: now.clone(),
        },
        BenchmarkDataset {
            id: "seeded/image-prompt-fidelity".into(),
            name: "Image Prompt Fidelity".into(),
            modality: BenchmarkModality::Image,
            description: "Simple prompt-fidelity checks for image generations.".into(),
            cases: vec![
                BenchmarkCase {
                    id: "image-poster".into(),
                    name: "Editorial poster".into(),
                    modality: BenchmarkModality::Image,
                    input: "Create an editorial poster for a robotics conference with a clean grid, warm orange accents, and no visible logos.".into(),
                    reference_output: None,
                    assertions: vec![],
                    tags: vec!["image".into(), "design".into()],
                },
                BenchmarkCase {
                    id: "image-product".into(),
                    name: "Product render".into(),
                    modality: BenchmarkModality::Image,
                    input: "Generate a studio product image of a matte black mechanical keyboard on a pale stone surface with soft daylight.".into(),
                    reference_output: None,
                    assertions: vec![],
                    tags: vec!["image".into(), "product".into()],
                },
            ],
            tags: vec!["seeded".into(), "image".into()],
            created_at: now.clone(),
            updated_at: now,
        },
    ]
}

fn provider_token(provider_id: &str) -> Result<Option<String>, String> {
    let token_provider_id = format!("benchmark-{}", provider_id);
    token_store::get_provider_token(&token_provider_id, "api-key")
}

fn save_run(run: &BenchmarkRun) -> Result<(), String> {
    let run_dir = run_dir(&run.id);
    fs::create_dir_all(run_items_dir(&run.id)).map_err(|e| e.to_string())?;
    fs::create_dir_all(run_artifacts_dir(&run.id)).map_err(|e| e.to_string())?;
    let manifest = serde_json::to_string_pretty(run).map_err(|e| e.to_string())?;
    atomic_write_str(&run_manifest_path(&run.id), &manifest)?;
    for item in &run.items {
        let item_json = serde_json::to_string_pretty(item).map_err(|e| e.to_string())?;
        let path = run_items_dir(&run.id).join(format!("{}.json", item.item_id));
        atomic_write_str(&path, &item_json)?;
    }
    let _ = fs::create_dir_all(run_dir);
    Ok(())
}

fn load_run(run_id: &str) -> Result<BenchmarkRun, String> {
    let path = run_manifest_path(run_id);
    let content = read_with_sharing(&path)?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

fn list_saved_run_ids() -> Vec<String> {
    let root = runs_dir();
    let mut ids = vec![];
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.push(name.to_string());
                }
            }
        }
    }
    ids
}

fn list_saved_datasets() -> Vec<BenchmarkDataset> {
    let mut datasets = vec![];
    if let Ok(entries) = fs::read_dir(datasets_dir()) {
        for entry in entries.flatten() {
            if let Ok(content) = read_with_sharing(&entry.path()) {
                if let Ok(dataset) = serde_json::from_str::<BenchmarkDataset>(&content) {
                    datasets.push(dataset);
                }
            }
        }
    }
    datasets
}

fn get_model(provider_id: &str, model_id: &str) -> Option<BenchmarkModel> {
    seeded_models()
        .into_iter()
        .find(|model| model.provider_id == provider_id && model.id == model_id)
}

fn escape_regex_char(c: char) -> String {
    match c {
        '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' => {
            format!("\\{}", c)
        }
        _ => c.to_string(),
    }
}

fn regex_like_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split(".*").collect();
    if parts.len() == 1 {
        return text.contains(pattern);
    }
    let mut offset = 0usize;
    for part in parts {
        if part.is_empty() {
            continue;
        }
        if let Some(index) = text[offset..].find(part) {
            offset += index + part.len();
        } else {
            return false;
        }
    }
    true
}

fn deterministic_scores(case: &BenchmarkCase, output_text: Option<&str>) -> Vec<DeterministicScore> {
    let mut scores = vec![];
    let text = output_text.unwrap_or_default();
    for assertion in &case.assertions {
        let score = match assertion.kind {
            BenchmarkAssertionKind::Contains => {
                let needle = assertion.value.clone().unwrap_or_default();
                DeterministicScore {
                    kind: "contains".into(),
                    passed: text.to_lowercase().contains(&needle.to_lowercase()),
                    details: format!("must contain '{}'", needle),
                }
            }
            BenchmarkAssertionKind::ExactMatch => {
                let expected = assertion.value.clone().unwrap_or_default();
                DeterministicScore {
                    kind: "exact_match".into(),
                    passed: text.trim() == expected.trim(),
                    details: "must match exactly".into(),
                }
            }
            BenchmarkAssertionKind::Regex => {
                let pattern = assertion.value.clone().unwrap_or_default();
                let normalized = pattern
                    .chars()
                    .map(escape_regex_char)
                    .collect::<Vec<_>>()
                    .join("");
                DeterministicScore {
                    kind: "regex".into(),
                    passed: regex_like_match(&normalized.replace("\\.\\*", ".*"), text),
                    details: format!("must match regex-like pattern '{}'", pattern),
                }
            }
            BenchmarkAssertionKind::JsonParse => DeterministicScore {
                kind: "json_parse".into(),
                passed: serde_json::from_str::<Value>(text).is_ok(),
                details: "must be valid JSON".into(),
            },
            BenchmarkAssertionKind::JsonKeysPresent => {
                let parsed = serde_json::from_str::<Value>(text).ok();
                let keys_present = parsed
                    .as_ref()
                    .and_then(|value| value.as_object())
                    .map(|object| assertion.values.iter().all(|key| object.contains_key(key)))
                    .unwrap_or(false);
                DeterministicScore {
                    kind: "json_keys_present".into(),
                    passed: keys_present,
                    details: format!("must include keys {:?}", assertion.values),
                }
            }
        };
        scores.push(score);
    }
    scores
}

fn compute_context_used_percent(tokens: &BenchmarkTokenCounts, context_window: Option<u64>) -> Option<f64> {
    context_window.map(|window| {
        if window == 0 {
            0.0
        } else {
            let total = tokens.input_tokens + tokens.output_tokens + tokens.cache_read_tokens + tokens.cache_write_tokens;
            ((total as f64 / window as f64) * 100.0 * 100.0).round() / 100.0
        }
    })
}

fn compute_text_cost(model: &BenchmarkModel, tokens: &BenchmarkTokenCounts) -> Option<f64> {
    let input = model.input_cost_per_million?;
    let output = model.output_cost_per_million?;
    let input_cost = (tokens.input_tokens as f64 / 1_000_000.0) * input;
    let output_cost = (tokens.output_tokens as f64 / 1_000_000.0) * output;
    Some(((input_cost + output_cost) * 100_000.0).round() / 100_000.0)
}

fn compute_image_cost(model: &BenchmarkModel, quality: Option<&str>) -> Option<f64> {
    match quality.unwrap_or("medium") {
        "low" => model.image_price_low_1024,
        "high" => model.image_price_high_1024,
        _ => model.image_price_medium_1024.or(model.image_price_low_1024),
    }
}

#[derive(Debug, Clone)]
struct TextResponse {
    output: String,
    tokens: BenchmarkTokenCounts,
}

#[derive(Debug, Clone)]
struct ImageResponse {
    bytes: Vec<u8>,
    mime_type: String,
    preview_data_url: String,
}

fn build_benchmark_prompt(case: &BenchmarkCase, variant: &BenchmarkVariant) -> String {
    let mut prompt = String::new();
    if let Some(prefix) = &variant.prompt_prefix {
        prompt.push_str(prefix.trim());
        prompt.push_str("\n\n");
    }
    if let Some(context) = &variant.capability_context {
        if !context.trim().is_empty() {
            prompt.push_str("Additional instructions and capability context:\n");
            prompt.push_str(context.trim());
            prompt.push_str("\n\n");
        }
    }
    prompt.push_str(case.input.trim());
    if let Some(suffix) = &variant.prompt_suffix {
        prompt.push_str("\n\n");
        prompt.push_str(suffix.trim());
    }
    prompt
}

fn build_client_json_headers(bearer: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    let auth = HeaderValue::from_str(&format!("Bearer {}", bearer)).map_err(|e| e.to_string())?;
    headers.insert(AUTHORIZATION, auth);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(headers)
}

fn run_mock_text(case: &BenchmarkCase, variant: &BenchmarkVariant, target: &BenchmarkTarget) -> TextResponse {
    let output = format!(
        "[mock:{}:{}] {}",
        target.provider_id,
        target.model_id,
        build_benchmark_prompt(case, variant)
    );
    TextResponse {
        output,
        tokens: BenchmarkTokenCounts {
            input_tokens: 120,
            output_tokens: 240,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        },
    }
}

fn run_openai_text(endpoints: &ProviderEndpoints, api_key: &str, case: &BenchmarkCase, variant: &BenchmarkVariant, target: &BenchmarkTarget) -> Result<TextResponse, String> {
    let client = build_client(45).map_err(String::from)?;
    let body = json!({
        "model": target.model_id,
        "messages": [
            {"role": "system", "content": variant.system_prompt.clone().unwrap_or_else(|| "You are a precise benchmark assistant.".into())},
            {"role": "user", "content": build_benchmark_prompt(case, variant)}
        ],
        "temperature": target.temperature.unwrap_or(0.2),
        "max_tokens": target.max_output_tokens.unwrap_or(1200),
    });
    let response: Value = client
        .post(format!("{}/v1/chat/completions", endpoints.openai))
        .headers(build_client_json_headers(api_key)?)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let output = response["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let tokens = BenchmarkTokenCounts {
        input_tokens: response["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        output_tokens: response["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        cache_read_tokens: response["usage"]["prompt_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0),
        cache_write_tokens: 0,
    };
    Ok(TextResponse { output, tokens })
}

fn run_openrouter_text(endpoints: &ProviderEndpoints, api_key: &str, case: &BenchmarkCase, variant: &BenchmarkVariant, target: &BenchmarkTarget) -> Result<TextResponse, String> {
    let client = build_client(45).map_err(String::from)?;
    let mut headers = build_client_json_headers(api_key)?;
    headers.insert("HTTP-Referer", HeaderValue::from_static("https://agentharbor.local"));
    headers.insert("X-Title", HeaderValue::from_static("AgentHarbor Benchmark Lab"));
    let body = json!({
        "model": target.model_id,
        "messages": [
            {"role": "system", "content": variant.system_prompt.clone().unwrap_or_else(|| "You are a precise benchmark assistant.".into())},
            {"role": "user", "content": build_benchmark_prompt(case, variant)}
        ],
        "temperature": target.temperature.unwrap_or(0.2),
        "max_tokens": target.max_output_tokens.unwrap_or(1200),
    });
    let response: Value = client
        .post(format!("{}/api/v1/chat/completions", endpoints.openrouter))
        .headers(headers)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let output = response["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let tokens = BenchmarkTokenCounts {
        input_tokens: response["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        output_tokens: response["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        cache_read_tokens: 0,
        cache_write_tokens: 0,
    };
    Ok(TextResponse { output, tokens })
}

fn run_anthropic_text(endpoints: &ProviderEndpoints, api_key: &str, case: &BenchmarkCase, variant: &BenchmarkVariant, target: &BenchmarkTarget) -> Result<TextResponse, String> {
    let client = build_client(45).map_err(String::from)?;
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_str(api_key).map_err(|e| e.to_string())?);
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let body = json!({
        "model": target.model_id,
        "system": variant.system_prompt.clone().unwrap_or_else(|| "You are a precise benchmark assistant.".into()),
        "messages": [
            {"role": "user", "content": [{"type":"text","text": build_benchmark_prompt(case, variant)}]}
        ],
        "temperature": target.temperature.unwrap_or(0.2),
        "max_tokens": target.max_output_tokens.unwrap_or(1200),
    });
    let response: Value = client
        .post(format!("{}/v1/messages", endpoints.anthropic))
        .headers(headers)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let mut output = String::new();
    if let Some(parts) = response["content"].as_array() {
        for part in parts {
            if let Some(text) = part["text"].as_str() {
                output.push_str(text);
            }
        }
    }
    let tokens = BenchmarkTokenCounts {
        input_tokens: response["usage"]["input_tokens"].as_u64().unwrap_or(0),
        output_tokens: response["usage"]["output_tokens"].as_u64().unwrap_or(0),
        cache_read_tokens: response["usage"]["cache_read_input_tokens"].as_u64().unwrap_or(0),
        cache_write_tokens: response["usage"]["cache_creation_input_tokens"].as_u64().unwrap_or(0),
    };
    Ok(TextResponse { output, tokens })
}

fn run_gemini_text(endpoints: &ProviderEndpoints, api_key: &str, case: &BenchmarkCase, variant: &BenchmarkVariant, target: &BenchmarkTarget) -> Result<TextResponse, String> {
    let client = build_client(45).map_err(String::from)?;
    let body = json!({
        "systemInstruction": {
            "parts": [{"text": variant.system_prompt.clone().unwrap_or_else(|| "You are a precise benchmark assistant.".into())}]
        },
        "contents": [{
            "parts": [{"text": build_benchmark_prompt(case, variant)}]
        }],
        "generationConfig": {
            "temperature": target.temperature.unwrap_or(0.2),
            "maxOutputTokens": target.max_output_tokens.unwrap_or(1200),
        }
    });
    let response: Value = client
        .post(format!("{}/v1beta/models/{}:generateContent?key={}", endpoints.gemini, target.model_id, api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let output = response["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let tokens = BenchmarkTokenCounts {
        input_tokens: response["usageMetadata"]["promptTokenCount"].as_u64().unwrap_or(0),
        output_tokens: response["usageMetadata"]["candidatesTokenCount"].as_u64().unwrap_or(0),
        cache_read_tokens: response["usageMetadata"]["cachedContentTokenCount"].as_u64().unwrap_or(0),
        cache_write_tokens: 0,
    };
    Ok(TextResponse { output, tokens })
}

fn run_openai_image(endpoints: &ProviderEndpoints, api_key: &str, case: &BenchmarkCase, variant: &BenchmarkVariant, target: &BenchmarkTarget) -> Result<ImageResponse, String> {
    let client = build_client(90).map_err(String::from)?;
    let body = json!({
        "model": target.model_id,
        "prompt": build_benchmark_prompt(case, variant),
        "size": target.image_size.clone().unwrap_or_else(|| "1024x1024".into()),
        "quality": target.image_quality.clone().unwrap_or_else(|| "medium".into()),
        "response_format": "b64_json",
    });
    let response: Value = client
        .post(format!("{}/v1/images/generations", endpoints.openai))
        .headers(build_client_json_headers(api_key)?)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let b64 = response["data"][0]["b64_json"].as_str().unwrap_or_default();
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| e.to_string())?;
    Ok(ImageResponse {
        preview_data_url: format!("data:image/png;base64,{}", b64),
        bytes,
        mime_type: "image/png".into(),
    })
}

fn run_gemini_image(endpoints: &ProviderEndpoints, api_key: &str, case: &BenchmarkCase, variant: &BenchmarkVariant, target: &BenchmarkTarget) -> Result<ImageResponse, String> {
    let client = build_client(90).map_err(String::from)?;
    let response_format = match target.model_id.as_str() {
        "gemini-2.5-flash-image" => json!({
            "image": {
                "aspectRatio": target.image_size.clone().unwrap_or_else(|| "1:1".into())
            }
        }),
        _ => json!({
            "image": {
                "aspectRatio": target.image_size.clone().unwrap_or_else(|| "1:1".into()),
                "imageSize": "1K"
            }
        }),
    };
    let body = json!({
        "contents": [{
            "parts": [{"text": build_benchmark_prompt(case, variant)}]
        }],
        "generationConfig": {
            "responseModalities": ["IMAGE"],
            "responseFormat": response_format
        }
    });
    let response: Value = client
        .post(format!("{}/v1beta/models/{}:generateContent?key={}", endpoints.gemini, target.model_id, api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let inline_data = response["candidates"][0]["content"]["parts"]
        .as_array()
        .and_then(|parts| parts.iter().find(|part| part.get("inlineData").is_some() || part.get("inline_data").is_some()))
        .cloned()
        .ok_or_else(|| "Gemini image response did not include inline image data".to_string())?;
    let image_b64 = inline_data["inlineData"]["data"]
        .as_str()
        .or_else(|| inline_data["inline_data"]["data"].as_str())
        .unwrap_or_default();
    let mime_type = inline_data["inlineData"]["mimeType"]
        .as_str()
        .or_else(|| inline_data["inline_data"]["mime_type"].as_str())
        .unwrap_or("image/png")
        .to_string();
    let bytes = base64::engine::general_purpose::STANDARD.decode(image_b64).map_err(|e| e.to_string())?;
    Ok(ImageResponse {
        preview_data_url: format!("data:{};base64,{}", mime_type, image_b64),
        bytes,
        mime_type,
    })
}

fn run_mock_image(case: &BenchmarkCase, variant: &BenchmarkVariant) -> ImageResponse {
    let content = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1024\" height=\"1024\"><rect width=\"100%\" height=\"100%\" fill=\"#111827\"/><text x=\"64\" y=\"180\" fill=\"#f9fafb\" font-size=\"46\" font-family=\"Arial\">Mock Benchmark Image</text><text x=\"64\" y=\"260\" fill=\"#93c5fd\" font-size=\"28\" font-family=\"Arial\">{}</text><text x=\"64\" y=\"320\" fill=\"#f9fafb\" font-size=\"22\" font-family=\"Arial\">{}</text></svg>",
        xml_escape(&variant.name),
        xml_escape(&case.input),
    );
    let bytes = content.into_bytes();
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    ImageResponse {
        bytes,
        mime_type: "image/svg+xml".into(),
        preview_data_url: format!("data:image/svg+xml;base64,{}", b64),
    }
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn run_text_provider(
    endpoints: &ProviderEndpoints,
    provider_id: &str,
    case: &BenchmarkCase,
    variant: &BenchmarkVariant,
    target: &BenchmarkTarget,
) -> Result<TextResponse, String> {
    match provider_id {
        "mock" => Ok(run_mock_text(case, variant, target)),
        "openai" => {
            let key = provider_token("openai")?.ok_or_else(|| "Missing OpenAI benchmark API key".to_string())?;
            run_openai_text(endpoints, &key, case, variant, target)
        }
        "anthropic" => {
            let key = provider_token("anthropic")?.ok_or_else(|| "Missing Anthropic benchmark API key".to_string())?;
            run_anthropic_text(endpoints, &key, case, variant, target)
        }
        "gemini" => {
            let key = provider_token("gemini")?.ok_or_else(|| "Missing Gemini benchmark API key".to_string())?;
            run_gemini_text(endpoints, &key, case, variant, target)
        }
        "openrouter" => {
            let key = provider_token("openrouter")?.ok_or_else(|| "Missing OpenRouter benchmark API key".to_string())?;
            run_openrouter_text(endpoints, &key, case, variant, target)
        }
        _ => Err(format!("Unsupported benchmark provider '{}'", provider_id)),
    }
}

fn run_image_provider(
    endpoints: &ProviderEndpoints,
    provider_id: &str,
    case: &BenchmarkCase,
    variant: &BenchmarkVariant,
    target: &BenchmarkTarget,
) -> Result<ImageResponse, String> {
    match provider_id {
        "mock" => Ok(run_mock_image(case, variant)),
        "openai" => {
            let key = provider_token("openai")?.ok_or_else(|| "Missing OpenAI benchmark API key".to_string())?;
            run_openai_image(endpoints, &key, case, variant, target)
        }
        "gemini" => {
            let key = provider_token("gemini")?.ok_or_else(|| "Missing Gemini benchmark API key".to_string())?;
            run_gemini_image(endpoints, &key, case, variant, target)
        }
        _ => Err(format!("Image benchmarking is not supported for '{}'", provider_id)),
    }
}

fn judge_output(
    endpoints: &ProviderEndpoints,
    judge: &BenchmarkJudgeConfig,
    case: &BenchmarkCase,
    variant: &BenchmarkVariant,
    output_text: &str,
) -> Option<JudgeScore> {
    if !judge.enabled {
        return None;
    }
    let provider_id = judge.provider_id.clone()?;
    let model_id = judge.model_id.clone()?;
    let rubric = judge
        .rubric
        .clone()
        .unwrap_or_else(|| "Score the response from 0 to 100 for instruction-following, usefulness, and correctness. Return strict JSON with keys score and rationale.".into());
    let judge_case = BenchmarkCase {
        id: "judge".into(),
        name: "Judge".into(),
        modality: BenchmarkModality::Text,
        input: format!(
            "Rubric:\n{}\n\nTask:\n{}\n\nVariant:\n{}\n\nCandidate response:\n{}",
            rubric, case.input, variant.name, output_text
        ),
        reference_output: None,
        assertions: vec![],
        tags: vec![],
    };
    let judge_variant = BenchmarkVariant {
        id: "judge".into(),
        name: "Judge".into(),
        system_prompt: Some("You are an evaluation model. Return only valid JSON with fields score and rationale.".into()),
        prompt_prefix: None,
        prompt_suffix: None,
        capability_context: None,
        capability_labels: vec![],
    };
    let judge_target = BenchmarkTarget {
        provider_id: provider_id.clone(),
        model_id: model_id.clone(),
        modality: BenchmarkModality::Text,
        temperature: Some(0.0),
        max_output_tokens: Some(300),
        image_size: None,
        image_quality: None,
    };
    match run_text_provider(endpoints, &provider_id, &judge_case, &judge_variant, &judge_target) {
        Ok(resp) => {
            let parsed = serde_json::from_str::<Value>(&resp.output).ok();
            let score = parsed.as_ref().and_then(|value| value.get("score")).and_then(|value| value.as_f64());
            let rationale = parsed
                .as_ref()
                .and_then(|value| value.get("rationale"))
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
                .or(Some(resp.output));
            Some(JudgeScore {
                score,
                rationale,
                provider_id: Some(provider_id),
                model_id: Some(model_id),
                error: None,
            })
        }
        Err(error) => Some(JudgeScore {
            score: None,
            rationale: None,
            provider_id: Some(provider_id),
            model_id: Some(model_id),
            error: Some(error),
        }),
    }
}

fn default_variants_if_missing(variants: &[BenchmarkVariant]) -> Vec<BenchmarkVariant> {
    if variants.is_empty() {
        vec![BenchmarkVariant {
            id: "default".into(),
            name: "Default".into(),
            system_prompt: Some("You are a precise benchmark assistant.".into()),
            prompt_prefix: None,
            prompt_suffix: None,
            capability_context: None,
            capability_labels: vec![],
        }]
    } else {
        variants.to_vec()
    }
}

fn write_image_artifact(run_id: &str, item_id: &str, response: &ImageResponse) -> Result<ArtifactRef, String> {
    let extension = if response.mime_type.contains("svg") {
        "svg"
    } else if response.mime_type.contains("jpeg") || response.mime_type.contains("jpg") {
        "jpg"
    } else {
        "png"
    };
    let path = run_artifacts_dir(run_id).join(format!("{}.{}", item_id, extension));
    atomic_write(&path, &response.bytes)?;
    Ok(ArtifactRef {
        kind: "image".into(),
        label: "Generated image".into(),
        path: path.to_string_lossy().to_string(),
        mime_type: response.mime_type.clone(),
        preview_data_url: Some(response.preview_data_url.clone()),
    })
}

#[tauri::command]
pub fn list_benchmark_providers() -> Vec<BenchmarkProvider> {
    seeded_providers()
}

#[tauri::command]
pub fn list_benchmark_models() -> Vec<BenchmarkModel> {
    seeded_models()
}

#[tauri::command]
pub fn list_reference_benchmarks() -> Vec<ReferenceBenchmark> {
    seeded_reference_benchmarks()
}

#[tauri::command]
pub fn list_benchmark_datasets() -> Result<Vec<BenchmarkDataset>, String> {
    ensure_benchmark_dirs()?;
    let mut datasets = seeded_datasets();
    datasets.extend(list_saved_datasets());
    datasets.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(datasets)
}

#[tauri::command]
pub fn save_benchmark_dataset(mut dataset: BenchmarkDataset) -> Result<BenchmarkDataset, String> {
    ensure_benchmark_dirs()?;
    if dataset.id.trim().is_empty() {
        dataset.id = format!("user/{}", Uuid::new_v4());
    }
    let now = Utc::now().to_rfc3339();
    if dataset.created_at.trim().is_empty() {
        dataset.created_at = now.clone();
    }
    dataset.updated_at = now;
    let content = serde_json::to_string_pretty(&dataset).map_err(|e| e.to_string())?;
    atomic_write_str(&dataset_path(&dataset.id), &content)?;
    Ok(dataset)
}

#[tauri::command]
pub fn delete_benchmark_dataset(dataset_id: String) -> Result<(), String> {
    let path = dataset_path(&dataset_id);
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub fn import_benchmark_dataset(json_string: String) -> Result<BenchmarkDataset, String> {
    let dataset = serde_json::from_str::<BenchmarkDataset>(&json_string).map_err(|e| e.to_string())?;
    save_benchmark_dataset(dataset)
}

#[tauri::command]
pub fn export_benchmark_dataset(dataset_id: String, output_path: String) -> Result<(), String> {
    ensure_benchmark_dirs()?;
    let datasets = list_benchmark_datasets()?;
    let dataset = datasets
        .into_iter()
        .find(|candidate| candidate.id == dataset_id)
        .ok_or_else(|| format!("Dataset '{}' not found", dataset_id))?;
    let json = serde_json::to_string_pretty(&dataset).map_err(|e| e.to_string())?;
    atomic_write_str(Path::new(&output_path), &json)
}

#[tauri::command]
pub fn list_benchmark_runs() -> Result<Vec<BenchmarkRunSummary>, String> {
    ensure_benchmark_dirs()?;
    let mut runs = vec![];
    for run_id in list_saved_run_ids() {
        if let Ok(run) = load_run(&run_id) {
            runs.push(BenchmarkRunSummary {
                id: run.id,
                name: run.name,
                modality: run.modality,
                status: run.status,
                created_at: run.created_at,
                completed_at: run.completed_at,
                item_count: run.items.len(),
            });
        }
    }
    runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(runs)
}

#[tauri::command]
pub fn get_benchmark_run(run_id: String) -> Result<BenchmarkRun, String> {
    ensure_benchmark_dirs()?;
    load_run(&run_id)
}

#[tauri::command]
pub fn update_benchmark_manual_review(run_id: String, item_id: String, manual_review: ManualReview) -> Result<BenchmarkRun, String> {
    let mut run = load_run(&run_id)?;
    let item = run
        .items
        .iter_mut()
        .find(|item| item.item_id == item_id)
        .ok_or_else(|| format!("Run item '{}' not found", item_id))?;
    item.manual_review = manual_review;
    save_run(&run)?;
    Ok(run)
}

#[tauri::command]
pub fn export_benchmark_run(run_id: String, output_path: String) -> Result<(), String> {
    ensure_benchmark_dirs()?;
    let run = load_run(&run_id)?;
    let file = File::create(&output_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let manifest = serde_json::to_string_pretty(&run).map_err(|e| e.to_string())?;
    zip.start_file("manifest.json", options).map_err(|e| e.to_string())?;
    zip.write_all(manifest.as_bytes()).map_err(|e| e.to_string())?;
    for item in &run.items {
        let item_json = serde_json::to_string_pretty(item).map_err(|e| e.to_string())?;
        zip.start_file(format!("items/{}.json", item.item_id), options).map_err(|e| e.to_string())?;
        zip.write_all(item_json.as_bytes()).map_err(|e| e.to_string())?;
    }
    for artifact in &run.items {
        for artifact_ref in &artifact.artifact_refs {
            let path = PathBuf::from(&artifact_ref.path);
            if path.exists() {
                let bytes = fs::read(&path).map_err(|e| e.to_string())?;
                let filename = path.file_name().and_then(|value| value.to_str()).unwrap_or("artifact.bin");
                zip.start_file(format!("artifacts/{}", filename), options).map_err(|e| e.to_string())?;
                zip.write_all(&bytes).map_err(|e| e.to_string())?;
            }
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn run_benchmark_suite(request: BenchmarkRunRequest) -> Result<BenchmarkRun, String> {
    ensure_benchmark_dirs()?;
    let endpoints = ProviderEndpoints::default();
    let variants = default_variants_if_missing(&request.variants);
    let run_id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    let mut run = BenchmarkRun {
        id: run_id.clone(),
        name: request.name,
        modality: request.modality.clone(),
        dataset_name: request.dataset_name,
        status: BenchmarkRunStatus::Running,
        created_at,
        completed_at: None,
        cases: request.cases.clone(),
        variants: variants.clone(),
        targets: request.targets.clone(),
        items: vec![],
        judge_config: request.judge.clone(),
    };
    save_run(&run)?;

    for case in &request.cases {
        for variant in &variants {
            for target in &request.targets {
                if case.modality != target.modality || case.modality != request.modality {
                    continue;
                }

                let start = std::time::Instant::now();
                let started_at = Utc::now().to_rfc3339();
                let mut item = BenchmarkRunItem {
                    item_id: Uuid::new_v4().to_string(),
                    case_id: case.id.clone(),
                    case_name: case.name.clone(),
                    variant_id: variant.id.clone(),
                    variant_name: variant.name.clone(),
                    provider_id: target.provider_id.clone(),
                    model_id: target.model_id.clone(),
                    modality: target.modality.clone(),
                    status: BenchmarkItemStatus::Pending,
                    started_at,
                    completed_at: None,
                    latency_ms: None,
                    token_counts: BenchmarkTokenCounts::default(),
                    estimated_cost_usd: None,
                    context_window: None,
                    context_used_percent: None,
                    output_text: None,
                    artifact_refs: vec![],
                    deterministic_scores: vec![],
                    judge_score: None,
                    manual_review: ManualReview::default(),
                    error: None,
                };

                let model = get_model(&target.provider_id, &target.model_id);
                item.context_window = model.as_ref().and_then(|candidate| candidate.context_window);

                let result = match target.modality {
                    BenchmarkModality::Text => run_text_provider(&endpoints, &target.provider_id, case, variant, target).map(|response| {
                        item.output_text = Some(response.output.clone());
                        item.token_counts = response.tokens.clone();
                        if let Some(model) = &model {
                            item.estimated_cost_usd = compute_text_cost(model, &response.tokens);
                        }
                        item.deterministic_scores = deterministic_scores(case, item.output_text.as_deref());
                        item.judge_score = item
                            .output_text
                            .as_deref()
                            .and_then(|output| request.judge.as_ref().and_then(|judge| judge_output(&endpoints, judge, case, variant, output)));
                    }),
                    BenchmarkModality::Image => run_image_provider(&endpoints, &target.provider_id, case, variant, target).and_then(|response| {
                        let artifact = write_image_artifact(&run.id, &item.item_id, &response)?;
                        item.artifact_refs.push(artifact);
                        if let Some(model) = &model {
                            item.estimated_cost_usd = compute_image_cost(model, target.image_quality.as_deref());
                        }
                        Ok(())
                    }),
                };

                item.latency_ms = Some(start.elapsed().as_millis());
                item.context_used_percent = compute_context_used_percent(&item.token_counts, item.context_window);
                item.completed_at = Some(Utc::now().to_rfc3339());

                match result {
                    Ok(_) => item.status = BenchmarkItemStatus::Completed,
                    Err(error) => {
                        item.status = BenchmarkItemStatus::Failed;
                        item.error = Some(error);
                    }
                }

                run.items.push(item);
                save_run(&run)?;
            }
        }
    }

    let failures = run
        .items
        .iter()
        .filter(|item| item.status == BenchmarkItemStatus::Failed)
        .count();
    run.status = if run.items.is_empty() || failures == run.items.len() {
        BenchmarkRunStatus::Failed
    } else if failures > 0 {
        BenchmarkRunStatus::Partial
    } else {
        BenchmarkRunStatus::Completed
    };
    run.completed_at = Some(Utc::now().to_rfc3339());
    save_run(&run)?;
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_json_keys_score_passes() {
        let case = BenchmarkCase {
            id: "case".into(),
            name: "case".into(),
            modality: BenchmarkModality::Text,
            input: "input".into(),
            reference_output: None,
            assertions: vec![BenchmarkAssertion {
                kind: BenchmarkAssertionKind::JsonKeysPresent,
                value: None,
                values: vec!["tests".into()],
            }],
            tags: vec![],
        };
        let scores = deterministic_scores(&case, Some(r#"{"tests": []}"#));
        assert_eq!(scores.len(), 1);
        assert!(scores[0].passed);
    }

    #[test]
    fn default_variant_is_created() {
        let variants = default_variants_if_missing(&[]);
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].id, "default");
    }

    #[test]
    fn context_usage_is_computed() {
        let tokens = BenchmarkTokenCounts {
            input_tokens: 500,
            output_tokens: 500,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        };
        let pct = compute_context_used_percent(&tokens, Some(10_000)).unwrap();
        assert!(pct > 0.0);
    }

    mod provider_mocks {
        use super::*;
        use wiremock::matchers::{header, header_exists, method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        fn sample_case() -> BenchmarkCase {
            BenchmarkCase {
                id: "case-1".into(),
                name: "case-1".into(),
                modality: BenchmarkModality::Text,
                input: "Hello".into(),
                reference_output: None,
                assertions: vec![],
                tags: vec![],
            }
        }

        fn sample_variant() -> BenchmarkVariant {
            BenchmarkVariant {
                id: "v1".into(),
                name: "v1".into(),
                system_prompt: None,
                prompt_prefix: None,
                prompt_suffix: None,
                capability_context: None,
                capability_labels: vec![],
            }
        }

        fn sample_target(provider: &str, model: &str) -> BenchmarkTarget {
            BenchmarkTarget {
                provider_id: provider.into(),
                model_id: model.into(),
                modality: BenchmarkModality::Text,
                temperature: Some(0.2),
                max_output_tokens: Some(256),
                image_size: None,
                image_quality: None,
            }
        }

        fn endpoints_with_uri(server_uri: &str) -> ProviderEndpoints {
            ProviderEndpoints {
                openai: server_uri.into(),
                anthropic: server_uri.into(),
                gemini: server_uri.into(),
                openrouter: server_uri.into(),
            }
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn openai_text_happy_path_parses_tokens_and_output() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
                .and(header("authorization", "Bearer test-key"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "choices": [{"message": {"content": "hi there"}}],
                    "usage": {
                        "prompt_tokens": 17,
                        "completion_tokens": 9,
                        "prompt_tokens_details": {"cached_tokens": 4}
                    }
                })))
                .mount(&server)
                .await;

            let endpoints = endpoints_with_uri(&server.uri());
            let case = sample_case();
            let variant = sample_variant();
            let target = sample_target("openai", "gpt-4o-mini");
            let resp = tokio::task::spawn_blocking(move || {
                run_openai_text(&endpoints, "test-key", &case, &variant, &target)
            })
            .await
            .unwrap()
            .expect("openai mock should succeed");

            assert_eq!(resp.output, "hi there");
            assert_eq!(resp.tokens.input_tokens, 17);
            assert_eq!(resp.tokens.output_tokens, 9);
            assert_eq!(resp.tokens.cache_read_tokens, 4);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn anthropic_text_happy_path_concatenates_content_parts() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/messages"))
                .and(header("x-api-key", "ak-test"))
                .and(header("anthropic-version", "2023-06-01"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "content": [
                        {"type": "text", "text": "part-a "},
                        {"type": "text", "text": "part-b"}
                    ],
                    "usage": {
                        "input_tokens": 31,
                        "output_tokens": 5,
                        "cache_read_input_tokens": 8,
                        "cache_creation_input_tokens": 2
                    }
                })))
                .mount(&server)
                .await;

            let endpoints = endpoints_with_uri(&server.uri());
            let case = sample_case();
            let variant = sample_variant();
            let target = sample_target("anthropic", "claude-sonnet-4-6");
            let resp = tokio::task::spawn_blocking(move || {
                run_anthropic_text(&endpoints, "ak-test", &case, &variant, &target)
            })
            .await
            .unwrap()
            .expect("anthropic mock should succeed");

            assert_eq!(resp.output, "part-a part-b");
            assert_eq!(resp.tokens.input_tokens, 31);
            assert_eq!(resp.tokens.output_tokens, 5);
            assert_eq!(resp.tokens.cache_read_tokens, 8);
            assert_eq!(resp.tokens.cache_write_tokens, 2);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn gemini_text_happy_path_uses_api_key_in_query() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
                .and(query_param("key", "gk-test"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "candidates": [{
                        "content": {"parts": [{"text": "gemini-output"}]}
                    }],
                    "usageMetadata": {
                        "promptTokenCount": 12,
                        "candidatesTokenCount": 7,
                        "cachedContentTokenCount": 3
                    }
                })))
                .mount(&server)
                .await;

            let endpoints = endpoints_with_uri(&server.uri());
            let case = sample_case();
            let variant = sample_variant();
            let target = sample_target("gemini", "gemini-2.5-flash");
            let resp = tokio::task::spawn_blocking(move || {
                run_gemini_text(&endpoints, "gk-test", &case, &variant, &target)
            })
            .await
            .unwrap()
            .expect("gemini mock should succeed");

            assert_eq!(resp.output, "gemini-output");
            assert_eq!(resp.tokens.input_tokens, 12);
            assert_eq!(resp.tokens.output_tokens, 7);
            assert_eq!(resp.tokens.cache_read_tokens, 3);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn openrouter_text_happy_path_sends_attribution_headers() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/v1/chat/completions"))
                .and(header("authorization", "Bearer or-test"))
                .and(header_exists("http-referer"))
                .and(header_exists("x-title"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "choices": [{"message": {"content": "router-out"}}],
                    "usage": {"prompt_tokens": 4, "completion_tokens": 2}
                })))
                .mount(&server)
                .await;

            let endpoints = endpoints_with_uri(&server.uri());
            let case = sample_case();
            let variant = sample_variant();
            let target = sample_target("openrouter", "anthropic/claude-opus-4-7");
            let resp = tokio::task::spawn_blocking(move || {
                run_openrouter_text(&endpoints, "or-test", &case, &variant, &target)
            })
            .await
            .unwrap()
            .expect("openrouter mock should succeed");

            assert_eq!(resp.output, "router-out");
            assert_eq!(resp.tokens.input_tokens, 4);
            assert_eq!(resp.tokens.output_tokens, 2);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn openai_text_surfaces_error_on_invalid_json() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(500).set_body_string("upstream exploded"),
                )
                .mount(&server)
                .await;

            let endpoints = endpoints_with_uri(&server.uri());
            let case = sample_case();
            let variant = sample_variant();
            let target = sample_target("openai", "gpt-4o-mini");
            let err = tokio::task::spawn_blocking(move || {
                run_openai_text(&endpoints, "test-key", &case, &variant, &target)
            })
            .await
            .unwrap()
            .expect_err("non-JSON body should fail to parse and surface an error");

            assert!(!err.is_empty());
        }
    }
}
