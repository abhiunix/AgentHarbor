pub mod traits;
pub mod claude_code;
pub mod cursor;
pub mod windsurf;
pub mod gemini;
pub mod copilot;
pub mod antigravity;
pub mod vscode;
pub mod codex;

pub use traits::{
    AdapterCapabilities, AgentAdapter, AgentConfig, ChangeType, ConfigDiffEntry,
    DeployResult, DeployStrategy, RemoveResult,
};
pub use claude_code::ClaudeCodeAdapter;
pub use cursor::CursorAdapter;
pub use windsurf::WindsurfAdapter;
pub use gemini::GeminiAdapter;
pub use copilot::CopilotAdapter;
pub use antigravity::AntigravityAdapter;
pub use vscode::VsCodeAdapter;
pub use codex::CodexAdapter;

use std::sync::Arc;

pub struct AdapterRegistry {
    adapters: Vec<Arc<dyn AgentAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: vec![
                Arc::new(ClaudeCodeAdapter::new()),
                Arc::new(CursorAdapter::new()),
                Arc::new(WindsurfAdapter::new()),
                Arc::new(GeminiAdapter::new()),
                Arc::new(CopilotAdapter::new()),
                Arc::new(AntigravityAdapter::new()),
                Arc::new(VsCodeAdapter::new()),
                Arc::new(CodexAdapter::new()),
            ],
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn AgentAdapter>> {
        self.adapters.iter().find(|a| a.id() == id).cloned()
    }

    pub fn all(&self) -> &[Arc<dyn AgentAdapter>] {
        &self.adapters
    }

    pub fn detect_adapters(&self, project_path: &std::path::Path) -> Vec<String> {
        self.adapters
            .iter()
            .filter(|a| a.detect(project_path))
            .map(|a| a.id().to_string())
            .collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_registry_creation() {
        let registry = AdapterRegistry::new();
        assert!(!registry.all().is_empty());
    }

    #[test]
    fn test_get_claude_code_adapter() {
        let registry = AdapterRegistry::new();
        let adapter = registry.get("claude-code");
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().name(), "Claude Code");
    }

    #[test]
    fn test_get_unknown_adapter() {
        let registry = AdapterRegistry::new();
        let adapter = registry.get("unknown");
        assert!(adapter.is_none());
    }
}
