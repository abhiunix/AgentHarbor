use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Adapters a skill installed under a universal `.agents/skills` directory applies to.
/// Mirrors the default `compatible_adapters` fan-out in registry/loader.rs.
const UNIVERSAL_ADAPTERS: [&str; 6] = [
    "claude-code",
    "cursor",
    "windsurf",
    "gemini",
    "codex",
    "copilot",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    pub name: String,
    pub description: String,
    /// Canonical path of the skill directory.
    pub source: String,
    pub adapter_ids: Vec<String>,
    /// True when the skill looks like one AgentHarbor deployed itself.
    pub managed: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPlugin {
    pub name: String,
    pub marketplace: String,
    pub description: String,
    pub version: String,
    pub enabled: bool,
    pub scope: String,
    pub author: String,
    pub homepage: String,
    pub skill_count: usize,
    /// Install path of the plugin cache directory.
    pub source: String,
}

// ---------------------------------------------------------------------------
// SKILL.md frontmatter
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    metadata: Option<SkillFrontmatterMetadata>,
}

#[derive(Debug, Default, Deserialize)]
struct SkillFrontmatterMetadata {
    tags: Option<serde_yaml::Value>,
}

/// Split a SKILL.md into (frontmatter yaml, body). Frontmatter is the block
/// between a leading `---` fence and the next closing fence.
fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let trimmed = content.strip_prefix('\u{feff}').unwrap_or(content);
    let rest = match trimmed.strip_prefix("---\n") {
        Some(r) => r,
        None => match trimmed.strip_prefix("---\r\n") {
            Some(r) => r,
            None => return (None, trimmed),
        },
    };
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        let marker = line.trim_end_matches(['\n', '\r']).trim_end();
        if marker == "---" || marker == "..." {
            return (Some(&rest[..offset]), &rest[offset + line.len()..]);
        }
        offset += line.len();
    }
    (None, trimmed)
}

/// First markdown heading text, used when a skill has no frontmatter description.
fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .find(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .filter(|s| !s.is_empty())
}

/// `metadata.tags` is a comma-joined string in the wild, but accept a list too.
fn tags_from_yaml(value: &serde_yaml::Value) -> Vec<String> {
    let raw: Vec<String> = match value {
        serde_yaml::Value::String(s) => s.split(',').map(|t| t.to_string()).collect(),
        serde_yaml::Value::Sequence(seq) => seq
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    };
    raw.into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

struct ParsedSkill {
    name: String,
    description: String,
    tags: Vec<String>,
    /// Frontmatter carries the invocation-flag pair AgentHarbor's deployer always emits.
    has_deploy_fingerprint: bool,
}

fn parse_skill_md(content: &str, dir_name: &str) -> ParsedSkill {
    let (frontmatter, body) = split_frontmatter(content);
    let mapping = frontmatter
        .and_then(|fm| serde_yaml::from_str::<serde_yaml::Mapping>(fm).ok())
        .unwrap_or_default();

    let has_deploy_fingerprint = mapping.contains_key(serde_yaml::Value::from("user-invocable"))
        && mapping.contains_key(serde_yaml::Value::from("disable-model-invocation"));

    let parsed: SkillFrontmatter =
        serde_yaml::from_value(serde_yaml::Value::Mapping(mapping)).unwrap_or_default();

    let name = parsed
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| dir_name.to_string());

    let description = parsed
        .description
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .or_else(|| first_heading(body))
        .unwrap_or_default();

    let tags = parsed
        .metadata
        .and_then(|m| m.tags)
        .map(|t| tags_from_yaml(&t))
        .unwrap_or_default();

    ParsedSkill {
        name,
        description,
        tags,
        has_deploy_fingerprint,
    }
}

/// AgentHarbor deploys skills into `<slug>-<8 hex>` directories.
fn has_hash_suffix(dir_name: &str) -> bool {
    let bytes = dir_name.as_bytes();
    if bytes.len() < 10 {
        return false;
    }
    let split = bytes.len() - 9;
    bytes[split] == b'-'
        && dir_name[split + 1..]
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

// ---------------------------------------------------------------------------
// Skill collection
// ---------------------------------------------------------------------------

#[derive(Default)]
struct SkillCollector {
    out: Vec<DiscoveredSkill>,
    /// Primary dedup: case-folded canonical path (collapses symlinks and the
    /// case-variant project paths macOS's case-insensitive FS allows).
    by_path: HashMap<String, usize>,
    /// Secondary dedup: byte-identical copies of the same skill in two roots.
    by_content: HashMap<(String, String), usize>,
    excluded_roots: Vec<PathBuf>,
}

impl SkillCollector {
    fn new(excluded_roots: Vec<PathBuf>) -> Self {
        Self {
            excluded_roots,
            ..Default::default()
        }
    }

    fn is_excluded(&self, path: &Path) -> bool {
        if path
            .components()
            .any(|c| c.as_os_str().to_string_lossy() == ".system")
        {
            return true;
        }
        self.excluded_roots.iter().any(|root| path.starts_with(root))
    }

    fn merge_adapters(&mut self, idx: usize, adapter_ids: &[&str]) {
        for adapter in adapter_ids {
            if !self.out[idx].adapter_ids.iter().any(|a| a == adapter) {
                self.out[idx].adapter_ids.push((*adapter).to_string());
            }
        }
    }

    fn add(&mut self, skill_dir: &Path, adapter_ids: &[&str]) {
        let canonical = match fs::canonicalize(skill_dir) {
            Ok(c) => c,
            Err(_) => return,
        };
        if self.is_excluded(&canonical) {
            return;
        }
        let path_key = canonical.to_string_lossy().to_lowercase();
        if let Some(&idx) = self.by_path.get(&path_key) {
            self.merge_adapters(idx, adapter_ids);
            return;
        }

        let skill_md = canonical.join("SKILL.md");
        let bytes = match fs::read(&skill_md) {
            Ok(b) => b,
            Err(_) => return,
        };
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let content = String::from_utf8_lossy(&bytes);
        let dir_name = canonical
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parsed = parse_skill_md(&content, &dir_name);

        let content_key = (parsed.name.to_lowercase(), hash);
        if let Some(&idx) = self.by_content.get(&content_key) {
            self.by_path.insert(path_key, idx);
            self.merge_adapters(idx, adapter_ids);
            return;
        }

        let idx = self.out.len();
        self.out.push(DiscoveredSkill {
            name: parsed.name,
            description: parsed.description,
            source: canonical.to_string_lossy().into_owned(),
            adapter_ids: adapter_ids.iter().map(|a| (*a).to_string()).collect(),
            managed: parsed.has_deploy_fingerprint && has_hash_suffix(&dir_name),
            tags: parsed.tags,
        });
        self.by_path.insert(path_key, idx);
        self.by_content.insert(content_key, idx);
    }

    /// A skill is a *direct* child directory holding a SKILL.md. Deliberately
    /// non-recursive: some installers drop sibling skill trees inside a skill.
    fn scan_dir(&mut self, skills_dir: &Path, adapter_ids: &[&str]) {
        let entries = match fs::read_dir(skills_dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            // is_dir() follows symlinks; file_type().is_dir() would not, and
            // symlinked skill farms are common.
            if !path.is_dir() {
                continue;
            }
            self.add(&path, adapter_ids);
        }
    }
}

fn excluded_skill_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots = vec![
        // AgentHarbor's own synced registry clone is not a "local install".
        crate::utils::paths::app_data_dir().join("registry"),
        // Cursor vendor-managed skills.
        home.join(".cursor").join("skills-cursor"),
    ];
    roots = roots
        .into_iter()
        .map(|r| fs::canonicalize(&r).unwrap_or(r))
        .collect();
    roots
}

fn global_skill_dirs(home: &Path) -> Vec<(PathBuf, Vec<&'static str>)> {
    [
        (".claude", "claude-code"),
        (".cursor", "cursor"),
        (".codex", "codex"),
        (".gemini", "gemini"),
        (".copilot", "copilot"),
        (".windsurf", "windsurf"),
    ]
    .iter()
    .map(|(dir, adapter)| (home.join(dir).join("skills"), vec![*adapter]))
    .collect()
}

fn project_skill_dirs(project: &Path) -> Vec<(PathBuf, Vec<&'static str>)> {
    let mut dirs: Vec<(PathBuf, Vec<&'static str>)> = vec![
        (project.join(".agents").join("skills"), UNIVERSAL_ADAPTERS.to_vec()),
        (
            project.join(".github").join("copilot").join("skills"),
            vec!["copilot"],
        ),
    ];
    for (dir, adapter) in [
        (".claude", "claude-code"),
        (".cursor", "cursor"),
        (".windsurf", "windsurf"),
        (".gemini", "gemini"),
        (".vscode", "vscode"),
        (".antigravity", "antigravity"),
    ] {
        dirs.push((project.join(dir).join("skills"), vec![adapter]));
    }
    dirs
}

/// Project roots to scan: every project Claude Code knows about plus the ones
/// AgentHarbor tracks, deduped by case-folded canonical path.
fn discoverable_project_roots(home: &Path) -> Vec<PathBuf> {
    let mut raw: Vec<String> = Vec::new();
    let claude_json = home.join(".claude.json");
    if let Ok(content) = fs::read_to_string(&claude_json) {
        if let Ok(json) = serde_json::from_str::<Value>(&content) {
            if let Some(projects) = json.get("projects").and_then(|p| p.as_object()) {
                raw.extend(projects.keys().cloned());
            }
        }
    }
    raw.extend(crate::commands::projects::get_tracked_project_paths());

    let mut seen: HashSet<String> = HashSet::new();
    let mut roots = Vec::new();
    for path in raw {
        // Foreign-OS paths (e.g. "D:\Projects\x") simply fail to canonicalize.
        let canonical = match fs::canonicalize(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !canonical.is_dir() {
            continue;
        }
        if seen.insert(canonical.to_string_lossy().to_lowercase()) {
            roots.push(canonical);
        }
    }
    roots
}

#[tauri::command]
pub fn discover_skills() -> Result<Vec<DiscoveredSkill>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let mut collector = SkillCollector::new(excluded_skill_roots(&home));

    for (dir, adapters) in global_skill_dirs(&home) {
        collector.scan_dir(&dir, &adapters);
    }
    for project in discoverable_project_roots(&home) {
        for (dir, adapters) in project_skill_dirs(&project) {
            collector.scan_dir(&dir, &adapters);
        }
    }

    let mut out = collector.out;
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Plugins (Claude plugin system v2)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct InstalledPluginRecord {
    #[serde(default)]
    scope: String,
    #[serde(default, rename = "installPath")]
    install_path: String,
    #[serde(default)]
    version: String,
    #[serde(default, rename = "lastUpdated")]
    last_updated: String,
}

#[derive(Debug, Deserialize)]
struct InstalledPluginsFile {
    #[serde(default)]
    plugins: HashMap<String, Vec<InstalledPluginRecord>>,
}

#[derive(Debug, Default)]
struct PluginMeta {
    description: String,
    version: String,
    author: String,
    homepage: String,
}

fn author_name(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(o)) => o
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

fn plugin_meta_from_manifest(install_path: &Path) -> Option<PluginMeta> {
    let manifest = install_path.join(".claude-plugin").join("plugin.json");
    let content = fs::read_to_string(&manifest).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;
    Some(PluginMeta {
        description: json
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        version: json
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        author: author_name(json.get("author")),
        homepage: json
            .get("homepage")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

/// Some cached plugins ship no `.claude-plugin/plugin.json`; the marketplace
/// manifest that installed them is the only place their metadata exists.
fn plugin_meta_from_marketplace(
    marketplaces_dir: &Path,
    marketplace: &str,
    name: &str,
) -> Option<PluginMeta> {
    let manifest = marketplaces_dir
        .join(marketplace)
        .join(".claude-plugin")
        .join("marketplace.json");
    let content = fs::read_to_string(&manifest).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;
    let entry = json
        .get("plugins")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))?;
    Some(PluginMeta {
        description: entry
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        version: entry
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        author: author_name(entry.get("author")),
        homepage: entry
            .get("homepage")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

fn read_enabled_plugins(settings_path: &Path) -> HashMap<String, bool> {
    let content = match fs::read_to_string(settings_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let json: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };
    json.get("enabledPlugins")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), b)))
                .collect()
        })
        .unwrap_or_default()
}

/// Bundled plugin skills are reported as a count only, never as standalone
/// discovered skills.
fn count_bundled_skills(install_path: &Path) -> usize {
    let skills_dir = install_path.join("skills");
    let entries = match fs::read_dir(&skills_dir) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    entries
        .flatten()
        .filter(|e| {
            let path = e.path();
            path.is_dir() && path.join("SKILL.md").is_file()
        })
        .count()
}

fn collect_plugins(plugins_dir: &Path, settings_path: &Path) -> Vec<DiscoveredPlugin> {
    let installed_path = plugins_dir.join("installed_plugins.json");
    let content = match fs::read_to_string(&installed_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let file: InstalledPluginsFile = match serde_json::from_str(&content) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let enabled = read_enabled_plugins(settings_path);
    let marketplaces_dir = plugins_dir.join("marketplaces");
    let mut out = Vec::new();

    for (key, records) in &file.plugins {
        let record = match records.iter().max_by(|a, b| a.last_updated.cmp(&b.last_updated)) {
            Some(r) => r,
            None => continue,
        };
        let install_path = PathBuf::from(&record.install_path);
        if install_path.join(".orphaned_at").exists() {
            continue;
        }
        let (name, marketplace) = match key.rsplit_once('@') {
            Some((n, m)) => (n.to_string(), m.to_string()),
            None => (key.clone(), String::new()),
        };
        let meta = plugin_meta_from_manifest(&install_path)
            .or_else(|| plugin_meta_from_marketplace(&marketplaces_dir, &marketplace, &name))
            .unwrap_or_default();

        out.push(DiscoveredPlugin {
            name,
            marketplace,
            description: meta.description,
            version: if meta.version.is_empty() {
                record.version.clone()
            } else {
                meta.version
            },
            enabled: enabled.get(key).copied().unwrap_or(false),
            scope: record.scope.clone(),
            author: meta.author,
            homepage: meta.homepage,
            skill_count: count_bundled_skills(&install_path),
            source: record.install_path.clone(),
        });
    }

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

#[tauri::command]
pub fn discover_plugins() -> Result<Vec<DiscoveredPlugin>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    Ok(collect_plugins(
        &home.join(".claude").join("plugins"),
        &home.join(".claude").join("settings.json"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_skill(root: &Path, name: &str, content: &str) -> PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), content).unwrap();
        dir
    }

    #[test]
    fn parses_block_scalar_description() {
        let content = "---\nname: block\ndescription: >-\n  First line of the description\n  continues on the second line.\n---\n\n# Body\n";
        let parsed = parse_skill_md(content, "block-dir");
        assert_eq!(parsed.name, "block");
        assert_eq!(
            parsed.description,
            "First line of the description continues on the second line."
        );
    }

    #[test]
    fn parses_quoted_scalar_with_colon_and_quotes() {
        let content =
            "---\nname: quoted\ndescription: 'Use when the user says \"go\": run it now'\n---\n";
        let parsed = parse_skill_md(content, "quoted-dir");
        assert_eq!(parsed.description, "Use when the user says \"go\": run it now");
    }

    #[test]
    fn parses_metadata_tags_comma_string() {
        let content = "---\nname: tagged\ndescription: d\nmetadata:\n  tags: security, review , audit\n---\n";
        let parsed = parse_skill_md(content, "tagged-dir");
        assert_eq!(parsed.tags, vec!["security", "review", "audit"]);
    }

    #[test]
    fn parses_metadata_tags_sequence() {
        let content = "---\nname: tagged\ndescription: d\nmetadata:\n  tags:\n    - a\n    - b\n---\n";
        let parsed = parse_skill_md(content, "tagged-dir");
        assert_eq!(parsed.tags, vec!["a", "b"]);
    }

    #[test]
    fn falls_back_to_dir_name_without_frontmatter() {
        let parsed = parse_skill_md("# My Heading\n\nbody text\n", "my-skill-dir");
        assert_eq!(parsed.name, "my-skill-dir");
        assert_eq!(parsed.description, "My Heading");
        assert!(parsed.tags.is_empty());
        assert!(!parsed.has_deploy_fingerprint);
    }

    #[test]
    fn empty_skill_md_falls_back_cleanly() {
        let parsed = parse_skill_md("", "bare");
        assert_eq!(parsed.name, "bare");
        assert_eq!(parsed.description, "");
    }

    #[test]
    fn symlinked_and_real_skill_dedup_and_merge_adapters() {
        let dir = tempdir().unwrap();
        let agents = dir.path().join(".agents").join("skills");
        fs::create_dir_all(&agents).unwrap();
        let real = write_skill(&agents, "shared", "---\nname: shared\ndescription: d\n---\n");

        let claude = dir.path().join(".claude").join("skills");
        fs::create_dir_all(&claude).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, claude.join("shared")).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&real, claude.join("shared")).is_err() {
            // Creating symlinks on Windows needs Developer Mode or admin.
            eprintln!("skipping: symlink creation not permitted on this host");
            return;
        }

        let mut collector = SkillCollector::new(vec![]);
        collector.scan_dir(&agents, &["cursor"]);
        collector.scan_dir(&claude, &["claude-code"]);

        assert_eq!(collector.out.len(), 1);
        assert_eq!(
            collector.out[0].adapter_ids,
            vec!["cursor".to_string(), "claude-code".to_string()]
        );
    }

    #[test]
    fn byte_identical_copies_dedup() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let content = "---\nname: twin\ndescription: same bytes\n---\n";
        write_skill(&a, "twin", content);
        write_skill(&b, "twin", content);

        let mut collector = SkillCollector::new(vec![]);
        collector.scan_dir(&a, &["claude-code"]);
        collector.scan_dir(&b, &["cursor"]);

        assert_eq!(collector.out.len(), 1);
        assert_eq!(
            collector.out[0].adapter_ids,
            vec!["claude-code".to_string(), "cursor".to_string()]
        );
    }

    #[test]
    fn skips_dirs_without_skill_md_and_nested_children() {
        let dir = tempdir().unwrap();
        let skills = dir.path().join("skills");
        fs::create_dir_all(skills.join("no-manifest")).unwrap();
        let outer = write_skill(&skills, "outer", "---\nname: outer\ndescription: d\n---\n");
        write_skill(&outer, "stowaway", "---\nname: stowaway\ndescription: d\n---\n");

        let mut collector = SkillCollector::new(vec![]);
        collector.scan_dir(&skills, &["claude-code"]);

        assert_eq!(collector.out.len(), 1);
        assert_eq!(collector.out[0].name, "outer");
    }

    #[test]
    fn skips_hidden_and_system_dirs() {
        let dir = tempdir().unwrap();
        let skills = dir.path().join("skills");
        fs::create_dir_all(&skills).unwrap();
        write_skill(&skills, ".system", "---\nname: builtin\ndescription: d\n---\n");
        write_skill(&skills, "visible", "---\nname: visible\ndescription: d\n---\n");

        let mut collector = SkillCollector::new(vec![]);
        collector.scan_dir(&skills, &["codex"]);

        assert_eq!(collector.out.len(), 1);
        assert_eq!(collector.out[0].name, "visible");

        // Also excluded when reached directly, not just via the hidden-dir skip.
        let mut direct = SkillCollector::new(vec![]);
        direct.add(&skills.join(".system"), &["codex"]);
        assert!(direct.out.is_empty());
    }

    #[test]
    fn excluded_roots_are_skipped() {
        let dir = tempdir().unwrap();
        let registry = dir.path().join("registry");
        let skills = registry.join("skills");
        fs::create_dir_all(&skills).unwrap();
        write_skill(&skills, "bundled", "---\nname: bundled\ndescription: d\n---\n");

        let mut collector =
            SkillCollector::new(vec![fs::canonicalize(&registry).unwrap()]);
        collector.scan_dir(&skills, &["claude-code"]);
        assert!(collector.out.is_empty());
    }

    #[test]
    fn detects_agentharbor_managed_skills() {
        let dir = tempdir().unwrap();
        let skills = dir.path().join("skills");
        fs::create_dir_all(&skills).unwrap();
        write_skill(
            &skills,
            "code-review-1a2b3c4d",
            "---\nname: code-review\ndescription: d\nuser-invocable: true\ndisable-model-invocation: false\n---\n",
        );
        write_skill(
            &skills,
            "third-party",
            "---\nname: third-party\ndescription: d\nallowed-tools: Read Grep\n---\n",
        );
        // Hash suffix alone is not enough.
        write_skill(
            &skills,
            "coincidence-deadbeef",
            "---\nname: coincidence\ndescription: d\n---\n",
        );

        let mut collector = SkillCollector::new(vec![]);
        collector.scan_dir(&skills, &["claude-code"]);

        let managed: Vec<&str> = collector
            .out
            .iter()
            .filter(|s| s.managed)
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(managed, vec!["code-review"]);
    }

    #[test]
    fn hash_suffix_matcher() {
        assert!(has_hash_suffix("skill-1a2b3c4d"));
        assert!(has_hash_suffix("a-00000000"));
        assert!(!has_hash_suffix("-1a2b3c4d"));
        assert!(!has_hash_suffix("skill-1A2B3C4D"));
        assert!(!has_hash_suffix("skill-1a2b3c4"));
        assert!(!has_hash_suffix("skill"));
    }

    // -- plugins -----------------------------------------------------------

    fn write_json(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    /// Embed a path in a JSON string literal (Windows backslashes must be escaped).
    fn json_path(p: &Path) -> String {
        p.to_string_lossy().replace('\\', "\\\\")
    }

    #[test]
    fn picks_newest_record_and_reads_plugin_manifest() {
        let dir = tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        let old = plugins.join("cache/mkt/demo/1.0.0");
        let new = plugins.join("cache/mkt/demo/2.0.0");
        write_json(
            &new.join(".claude-plugin/plugin.json"),
            r#"{"name":"demo","description":"newest","version":"2.0.0","author":{"name":"Acme"},"homepage":"https://acme.test"}"#,
        );
        fs::create_dir_all(new.join("skills/one")).unwrap();
        fs::write(new.join("skills/one/SKILL.md"), "---\nname: one\n---\n").unwrap();
        fs::create_dir_all(&old).unwrap();

        write_json(
            &plugins.join("installed_plugins.json"),
            &format!(
                r#"{{"version":2,"plugins":{{"demo@mkt":[
                  {{"scope":"project","installPath":"{}","version":"1.0.0","lastUpdated":"2026-01-01T00:00:00.000Z"}},
                  {{"scope":"user","installPath":"{}","version":"2.0.0","lastUpdated":"2026-05-01T00:00:00.000Z"}}
                ]}}}}"#,
                json_path(&old),
                json_path(&new)
            ),
        );

        let settings = dir.path().join("settings.json");
        write_json(&settings, r#"{"enabledPlugins":{"demo@mkt":true}}"#);

        let out = collect_plugins(&plugins, &settings);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].version, "2.0.0");
        assert_eq!(out[0].scope, "user");
        assert_eq!(out[0].description, "newest");
        assert_eq!(out[0].author, "Acme");
        assert_eq!(out[0].marketplace, "mkt");
        assert_eq!(out[0].skill_count, 1);
        assert!(out[0].enabled);
    }

    #[test]
    fn falls_back_to_marketplace_manifest_and_defaults_disabled() {
        let dir = tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        let install = plugins.join("cache/mkt/lsp/1.0.0");
        fs::create_dir_all(&install).unwrap();
        write_json(
            &plugins.join("marketplaces/mkt/.claude-plugin/marketplace.json"),
            r#"{"plugins":[{"name":"other","description":"nope"},{"name":"lsp","description":"from marketplace","version":"1.0.0","author":{"name":"Anthropic"},"homepage":"https://a.test"}]}"#,
        );
        write_json(
            &plugins.join("installed_plugins.json"),
            &format!(
                r#"{{"version":2,"plugins":{{"lsp@mkt":[{{"scope":"user","installPath":"{}","version":"1.0.0","lastUpdated":"2026-05-01T00:00:00.000Z"}}]}}}}"#,
                json_path(&install)
            ),
        );

        let settings = dir.path().join("settings.json");
        let out = collect_plugins(&plugins, &settings);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].description, "from marketplace");
        assert_eq!(out[0].author, "Anthropic");
        assert_eq!(out[0].homepage, "https://a.test");
        assert!(!out[0].enabled);
        assert_eq!(out[0].skill_count, 0);
    }

    #[test]
    fn skips_orphaned_cache_dirs() {
        let dir = tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        let install = plugins.join("cache/mkt/gone/1.0.0");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join(".orphaned_at"), "2026-01-01").unwrap();
        write_json(
            &plugins.join("installed_plugins.json"),
            &format!(
                r#"{{"version":2,"plugins":{{"gone@mkt":[{{"scope":"user","installPath":"{}","version":"1.0.0","lastUpdated":"2026-05-01T00:00:00.000Z"}}]}}}}"#,
                json_path(&install)
            ),
        );
        let settings = dir.path().join("settings.json");
        assert!(collect_plugins(&plugins, &settings).is_empty());
    }
}
