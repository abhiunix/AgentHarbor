//! Cost estimation engine for Claude Code analytics.
//! Matches CodexBar's pricing logic: per-model rates with tiered pricing for Sonnet.

use serde::{Deserialize, Serialize};

/// Per-model pricing (USD per token, NOT per million).
/// Stored as per-token to match CodexBar's precision.
#[derive(Debug, Clone)]
struct RawPricing {
    input: f64,
    output: f64,
    cache_create: f64,
    cache_read: f64,
    /// Optional tiered pricing above a token threshold
    threshold: Option<u64>,
    above_input: Option<f64>,
    above_output: Option<f64>,
    above_cache_create: Option<f64>,
    above_cache_read: Option<f64>,
}

/// Token counts for cost estimation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokensForCost {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

// ── Pricing table (matches CodexBar CostUsagePricing.swift) ─────────────

fn get_raw_pricing(model: &str) -> RawPricing {
    let normalized = normalize_model(model);
    match normalized.as_str() {
        // Haiku 4.5
        "claude-haiku-4-5" => RawPricing {
            input: 1e-6, output: 5e-6, cache_create: 1.25e-6, cache_read: 1e-7,
            threshold: None, above_input: None, above_output: None,
            above_cache_create: None, above_cache_read: None,
        },
        // Opus 4.5 through Opus 5 ($5/$25 per Mtok; no long-context premium)
        "claude-opus-4-5" | "claude-opus-4-6" | "claude-opus-4-7" | "claude-opus-4-8"
        | "claude-opus-5" => RawPricing {
            input: 5e-6, output: 2.5e-5, cache_create: 6.25e-6, cache_read: 5e-7,
            threshold: None, above_input: None, above_output: None,
            above_cache_create: None, above_cache_read: None,
        },
        // Fable 5 / Mythos 5 ($10/$50 per Mtok)
        "claude-fable-5" | "claude-mythos-5" => RawPricing {
            input: 1e-5, output: 5e-5, cache_create: 1.25e-5, cache_read: 1e-6,
            threshold: None, above_input: None, above_output: None,
            above_cache_create: None, above_cache_read: None,
        },
        // Sonnet 5 ($3/$15 sticker; no documented long-context premium)
        "claude-sonnet-5" => RawPricing {
            input: 3e-6, output: 1.5e-5, cache_create: 3.75e-6, cache_read: 3e-7,
            threshold: None, above_input: None, above_output: None,
            above_cache_create: None, above_cache_read: None,
        },
        // Opus 4 / 4.1 (older, more expensive)
        "claude-opus-4" | "claude-opus-4-1" => RawPricing {
            input: 1.5e-5, output: 7.5e-5, cache_create: 1.875e-5, cache_read: 1.5e-6,
            threshold: None, above_input: None, above_output: None,
            above_cache_create: None, above_cache_read: None,
        },
        // Sonnet 4 / 4.5 (tiered: above 200K tokens rates double)
        "claude-sonnet-4" | "claude-sonnet-4-5" => RawPricing {
            input: 3e-6, output: 1.5e-5, cache_create: 3.75e-6, cache_read: 3e-7,
            threshold: Some(200_000),
            above_input: Some(6e-6),
            above_output: Some(2.25e-5),
            above_cache_create: Some(7.5e-6),
            above_cache_read: Some(6e-7),
        },
        // Default: Sonnet pricing (most common in Claude Code)
        _ => RawPricing {
            input: 3e-6, output: 1.5e-5, cache_create: 3.75e-6, cache_read: 3e-7,
            threshold: Some(200_000),
            above_input: Some(6e-6),
            above_output: Some(2.25e-5),
            above_cache_create: Some(7.5e-6),
            above_cache_read: Some(6e-7),
        },
    }
}

/// Normalize model name: strip date suffix, "anthropic." prefix, version tags.
/// Matches CodexBar's normalizeClaudeModel().
fn normalize_model(raw: &str) -> String {
    let mut s = raw.trim().to_string();

    // Strip "anthropic." prefix
    if let Some(rest) = s.strip_prefix("anthropic.") {
        s = rest.to_string();
    }

    // Handle dot-separated namespaces: extract the "claude-..." part after last dot
    if s.contains("claude-") {
        if let Some(dot_pos) = s.rfind('.') {
            let tail = &s[dot_pos + 1..];
            if tail.starts_with("claude-") {
                s = tail.to_string();
            }
        }
    }

    // Strip Claude Code thinking-variant suffixes so the base model matches
    for suffix in ["-max-thinking-fast", "-max-thinking", "-high-thinking"] {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest.to_string();
            break;
        }
    }

    // Remove version suffix like "-v1:0"
    let version_re = regex_lite_find(&s, r"-v\d+:\d+$");
    if let Some(end) = version_re {
        s = s[..end].to_string();
    }

    // Remove date suffix like "-20250514" (8 digits at end)
    let date_re = regex_lite_find(&s, r"-\d{8}$");
    if let Some(end) = date_re {
        s[..end].to_string()
    } else {
        s
    }
}

/// Simple regex-like suffix matcher. Returns the start index of the match, or None.
fn regex_lite_find(s: &str, pattern: &str) -> Option<usize> {
    match pattern {
        r"-v\d+:\d+$" => {
            // Match -v<digits>:<digits> at end
            let bytes = s.as_bytes();
            let len = bytes.len();
            if len < 5 { return None; }
            // Walk backwards to find the pattern
            let mut i = len;
            // Find trailing digits after ':'
            while i > 0 && bytes[i - 1].is_ascii_digit() { i -= 1; }
            if i == 0 || bytes[i - 1] != b':' { return None; }
            i -= 1;
            // Find digits before ':'
            while i > 0 && bytes[i - 1].is_ascii_digit() { i -= 1; }
            if i < 2 || bytes[i - 1] != b'v' || bytes[i - 2] != b'-' { return None; }
            Some(i - 2)
        }
        r"-\d{8}$" => {
            // Match -YYYYMMDD at end (exactly 8 digits)
            if s.len() < 9 { return None; }
            let suffix = &s[s.len() - 9..];
            if suffix.starts_with('-') && suffix[1..].chars().all(|c| c.is_ascii_digit()) {
                Some(s.len() - 9)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Tiered cost calculation matching CodexBar's logic.
/// If no threshold, all tokens use base rate.
/// If threshold exists: tokens up to threshold use base, tokens above use above_rate.
fn tiered_cost(tokens: u64, base_rate: f64, above_rate: Option<f64>, threshold: Option<u64>) -> f64 {
    match (threshold, above_rate) {
        (Some(thr), Some(above)) => {
            let below = tokens.min(thr);
            let over = tokens.saturating_sub(thr);
            (below as f64) * base_rate + (over as f64) * above
        }
        _ => (tokens as f64) * base_rate,
    }
}

/// Estimate cost in USD from token counts and model name.
/// Uses tiered pricing matching CodexBar's logic.
/// Per-component cost split. `estimate_cost` is the sum of these, so any
/// UI that shows components alongside a total stays internally consistent.
#[derive(Debug, Clone, Copy, Default)]
pub struct CostComponents {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

impl CostComponents {
    pub fn total(&self) -> f64 {
        self.input + self.output + self.cache_read + self.cache_write
    }
}

pub fn estimate_cost_components(model: Option<&str>, tokens: &TokensForCost) -> CostComponents {
    let raw = get_raw_pricing(model.unwrap_or("sonnet"));
    CostComponents {
        input: tiered_cost(tokens.input, raw.input, raw.above_input, raw.threshold),
        output: tiered_cost(tokens.output, raw.output, raw.above_output, raw.threshold),
        cache_read: tiered_cost(tokens.cache_read, raw.cache_read, raw.above_cache_read, raw.threshold),
        cache_write: tiered_cost(tokens.cache_write, raw.cache_create, raw.above_cache_create, raw.threshold),
    }
}

pub fn estimate_cost(model: Option<&str>, tokens: &TokensForCost) -> f64 {
    estimate_cost_components(model, tokens).total()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_model() {
        assert_eq!(normalize_model("claude-sonnet-4-5-20250929"), "claude-sonnet-4-5");
        assert_eq!(normalize_model("claude-opus-4-6-20260205"), "claude-opus-4-6");
        assert_eq!(normalize_model("claude-haiku-4-5-20251001"), "claude-haiku-4-5");
        assert_eq!(normalize_model("anthropic.claude-sonnet-4-5"), "claude-sonnet-4-5");
        assert_eq!(normalize_model("claude-sonnet-4-5"), "claude-sonnet-4-5");
    }

    #[test]
    fn current_generation_models_are_priced() {
        // Regression: these fell through to the Sonnet default, making every
        // cost surface wrong in a different way.
        let tokens = TokensForCost { input: 1_000_000, output: 1_000_000, cache_read: 0, cache_write: 0 };
        assert!((estimate_cost(Some("claude-opus-4-8"), &tokens) - 30.0).abs() < 1e-6);
        assert!((estimate_cost(Some("claude-opus-5"), &tokens) - 30.0).abs() < 1e-6);
        assert!((estimate_cost(Some("claude-fable-5"), &tokens) - 60.0).abs() < 1e-6);
        assert!((estimate_cost(Some("claude-sonnet-5"), &tokens) - 18.0).abs() < 1e-6);
    }

    #[test]
    fn thinking_variant_suffixes_normalize() {
        assert_eq!(normalize_model("claude-opus-4-8-high-thinking"), "claude-opus-4-8");
        assert_eq!(normalize_model("claude-fable-5-max-thinking-fast"), "claude-fable-5");
        assert_eq!(normalize_model("claude-opus-5-max-thinking"), "claude-opus-5");
    }

    #[test]
    fn components_sum_to_total() {
        // Tiled breakdowns must equal the headline by construction.
        let tokens = TokensForCost { input: 350_000, output: 12_000, cache_read: 900_000, cache_write: 40_000 };
        for model in ["claude-sonnet-4-5", "claude-opus-5", "claude-fable-5", "unknown-model"] {
            let c = estimate_cost_components(Some(model), &tokens);
            assert!((c.total() - estimate_cost(Some(model), &tokens)).abs() < 1e-9);
        }
    }

    #[test]
    fn test_tiered_cost_no_threshold() {
        // Opus: no tiering
        assert!((tiered_cost(1_000_000, 5e-6, None, None) - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_tiered_cost_below_threshold() {
        // Sonnet: 100K tokens, all below 200K threshold
        let cost = tiered_cost(100_000, 3e-6, Some(6e-6), Some(200_000));
        assert!((cost - 0.30).abs() < 0.001);
    }

    #[test]
    fn test_tiered_cost_above_threshold() {
        // Sonnet: 300K tokens, 200K below + 100K above
        let cost = tiered_cost(300_000, 3e-6, Some(6e-6), Some(200_000));
        // 200K * 3e-6 = 0.60, 100K * 6e-6 = 0.60 → total 1.20
        assert!((cost - 1.20).abs() < 0.001);
    }

    #[test]
    fn test_opus_pricing() {
        let mtok = TokensForCost { input: 1_000_000, output: 1_000_000, cache_read: 0, cache_write: 0 };
        assert!((estimate_cost(Some("claude-opus-4-6"), &mtok) - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_sonnet_pricing() {
        let inp = TokensForCost { input: 100_000, output: 0, cache_read: 0, cache_write: 0 };
        assert!((estimate_cost(Some("claude-sonnet-4-5"), &inp) - 0.30).abs() < 0.001);
    }

    #[test]
    fn test_haiku_pricing() {
        let inp = TokensForCost { input: 1_000_000, output: 0, cache_read: 0, cache_write: 0 };
        assert!((estimate_cost(Some("claude-haiku-4-5"), &inp) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_with_tiering() {
        // Sonnet with tokens above 200K threshold
        let tokens = TokensForCost {
            input: 300_000,
            output: 100_000,
            cache_read: 10_000_000,
            cache_write: 500_000,
        };
        let cost = estimate_cost(Some("claude-sonnet-4-5"), &tokens);
        // input: 200K*3e-6 + 100K*6e-6 = 0.60 + 0.60 = 1.20
        // output: 100K*1.5e-5 = 1.50 (all below threshold)
        // cache_read: 200K*3e-7 + 9.8M*6e-7 = 0.06 + 5.88 = 5.94
        // cache_write: 200K*3.75e-6 + 300K*7.5e-6 = 0.75 + 2.25 = 3.00
        let expected = 1.20 + 1.50 + 5.94 + 3.00;
        assert!((cost - expected).abs() < 0.01);
    }

    #[test]
    fn test_estimate_zero_tokens() {
        let tokens = TokensForCost::default();
        assert_eq!(estimate_cost(None, &tokens), 0.0);
    }
}
