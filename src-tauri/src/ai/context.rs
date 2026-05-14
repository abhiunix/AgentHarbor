use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::adapters::AdapterRegistry;
use crate::models::UniversalCapability;
use crate::registry::{
    get_bundled_registry_path, get_community_registry_path, load_capabilities,
};

/// Compact snapshot fed to the LLM. We deliberately keep this small so the
/// model can reason over it without burning tokens.
#[derive(Debug, Clone, Serialize)]
pub struct RecommenderContext {
    pub adapters: Vec<AdapterSummary>,
    pub projects: Vec<ProjectSummary>,
    pub deployed_capability_ids: Vec<String>,
    pub available_capabilities: Vec<CapabilitySummary>,
    pub usage_signals: Vec<UsageSignal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdapterSummary {
    pub id: String,
    pub name: String,
    /// Whether at least one project has this adapter configured on disk.
    pub detected_anywhere: bool,
    pub supports_mcp: bool,
    pub supports_rules: bool,
    pub supports_skills: bool,
    pub supports_hooks: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    pub name: String,
    pub detected_adapters: Vec<String>,
    pub deployed_capability_ids: Vec<String>,
    pub deployed_agent_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilitySummary {
    pub id: String,
    pub name: String,
    /// "mcp" | "rule" | "skill" | "hook" | "plugin" | "custom"
    pub capability_type: String,
    pub description: String,
    pub tags: Vec<String>,
    pub compatible_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageSignal {
    pub provider_id: String,
    pub provider_name: String,
    pub connected: bool,
    pub headline: String,
}

fn registry_paths() -> Vec<std::path::PathBuf> {
    let mut dirs = vec![get_bundled_registry_path()];
    let community = get_community_registry_path();
    if community.exists() {
        dirs.push(community);
    }
    let custom = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.agentharbor.app")
        .join("registry")
        .join("custom");
    if custom.exists() {
        dirs.push(custom);
    }
    dirs
}

fn summarize_capability(cap: &UniversalCapability) -> CapabilitySummary {
    let (capability_type, description, tags, compatible) = match cap {
        UniversalCapability::Mcp(m) => (
            "mcp",
            m.description.clone(),
            m.tags.clone(),
            m.compatible_agents.clone(),
        ),
        UniversalCapability::Rule(r) => (
            "rule",
            r.description.clone(),
            r.tags.clone(),
            r.compatible_agents.clone(),
        ),
        UniversalCapability::Skill(s) => (
            "skill",
            s.description.clone(),
            s.tags.clone(),
            s.compatible_agents.clone(),
        ),
        UniversalCapability::Hook(h) => (
            "hook",
            h.description.clone(),
            h.tags.clone(),
            h.compatible_agents.clone(),
        ),
        UniversalCapability::Plugin(p) => (
            "plugin",
            p.description.clone(),
            p.tags.clone(),
            p.compatible_agents.clone(),
        ),
        UniversalCapability::Custom(c) => (
            "custom",
            c.description.clone(),
            c.tags.clone(),
            c.compatible_agents.clone(),
        ),
    };

    CapabilitySummary {
        id: cap.id().to_string(),
        name: cap.name().to_string(),
        capability_type: capability_type.to_string(),
        description: truncate(&description, 200),
        tags,
        compatible_agents: compatible,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

fn build_adapter_summaries(adapters_seen: &HashSet<String>) -> Vec<AdapterSummary> {
    let registry = AdapterRegistry::new();
    registry
        .all()
        .iter()
        .map(|a| {
            let caps = a.capabilities();
            AdapterSummary {
                id: a.id().to_string(),
                name: a.name().to_string(),
                detected_anywhere: adapters_seen.contains(a.id()),
                supports_mcp: caps.mcp,
                supports_rules: caps.rules,
                supports_skills: caps.skills,
                supports_hooks: caps.hooks,
            }
        })
        .collect()
}

fn build_project_summaries() -> (Vec<ProjectSummary>, HashSet<String>, HashSet<String>) {
    let projects = crate::commands::projects::get_all_projects();
    let mut summaries = Vec::with_capacity(projects.len());
    let mut adapters_seen = HashSet::new();
    let mut deployed_ids = HashSet::new();

    for p in projects.iter() {
        let detail = crate::commands::projects::get_project_detail(p.path.clone());
        let (deployed_caps, deployed_agents) = match &detail {
            Some(d) => (d.deployed_capabilities.clone(), d.deployed_agents.clone()),
            None => (vec![], vec![]),
        };

        for a in &p.detected_adapters {
            adapters_seen.insert(a.clone());
        }
        for c in &deployed_caps {
            deployed_ids.insert(c.clone());
        }

        summaries.push(ProjectSummary {
            name: p.name.clone(),
            detected_adapters: p.detected_adapters.clone(),
            deployed_capability_ids: deployed_caps,
            deployed_agent_ids: deployed_agents,
        });
    }

    (summaries, adapters_seen, deployed_ids)
}

fn build_usage_signals() -> Vec<UsageSignal> {
    let statuses = crate::analytics::commands::get_all_provider_status();
    statuses
        .into_iter()
        .map(|s| {
            let headline = if !s.connected {
                "not connected".to_string()
            } else {
                match (&s.plan_name, &s.account_email) {
                    (Some(plan), Some(email)) => format!("{} • {}", plan, email),
                    (Some(plan), None) => plan.clone(),
                    (None, Some(email)) => email.clone(),
                    _ => "connected".to_string(),
                }
            };
            UsageSignal {
                provider_id: s.provider_id,
                provider_name: s.provider_name,
                connected: s.connected,
                headline,
            }
        })
        .collect()
}

/// Trim the registry to a manageable shortlist for the LLM:
/// - Always include items compatible with at least one detected adapter
/// - Cap by `max_per_type` and `max_total`
fn shortlist_capabilities(
    caps: Vec<UniversalCapability>,
    adapters_seen: &HashSet<String>,
    deployed: &HashSet<String>,
    max_per_type: usize,
    max_total: usize,
) -> Vec<CapabilitySummary> {
    let mut by_type: HashMap<&'static str, Vec<CapabilitySummary>> = HashMap::new();

    for cap in caps {
        if deployed.contains(&cap.id().to_string()) {
            continue;
        }
        let summary = summarize_capability(&cap);
        let relevant = adapters_seen.is_empty()
            || summary
                .compatible_agents
                .iter()
                .any(|a| adapters_seen.contains(a));
        if !relevant {
            continue;
        }
        let key: &'static str = match cap {
            UniversalCapability::Mcp(_) => "mcp",
            UniversalCapability::Rule(_) => "rule",
            UniversalCapability::Skill(_) => "skill",
            UniversalCapability::Hook(_) => "hook",
            UniversalCapability::Plugin(_) => "plugin",
            UniversalCapability::Custom(_) => "custom",
        };
        let bucket = by_type.entry(key).or_default();
        if bucket.len() < max_per_type {
            bucket.push(summary);
        }
    }

    let mut out: Vec<CapabilitySummary> = by_type.into_values().flatten().collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.truncate(max_total);
    out
}

pub fn build_context() -> RecommenderContext {
    let (projects, adapters_seen, deployed_ids) = build_project_summaries();
    let adapters = build_adapter_summaries(&adapters_seen);

    let dirs = registry_paths();
    let load = load_capabilities(&dirs);
    let shortlist = shortlist_capabilities(load.items, &adapters_seen, &deployed_ids, 20, 60);

    let usage_signals = build_usage_signals();

    RecommenderContext {
        adapters,
        projects,
        deployed_capability_ids: deployed_ids.into_iter().collect(),
        available_capabilities: shortlist,
        usage_signals,
    }
}
