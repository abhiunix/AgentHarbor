use crate::models::{AgentDefinition, UniversalCapability, CompositeId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterCapabilities {
    pub mcp: bool,
    pub rules: bool,
    pub skills: bool,
    pub hooks: bool,
    pub plugins: bool,
    pub agents: bool,
    pub custom: bool,
}

impl AdapterCapabilities {
    pub fn all() -> Self {
        Self {
            mcp: true,
            rules: true,
            skills: true,
            hooks: true,
            plugins: true,
            agents: true,
            custom: true,
        }
    }

    pub fn mcp_only() -> Self {
        Self {
            mcp: true,
            rules: false,
            skills: false,
            hooks: false,
            plugins: false,
            agents: false,
            custom: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeType {
    Add,
    Modify,
    Remove,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDiffEntry {
    pub file_path: PathBuf,
    pub change_type: ChangeType,
    pub current_content: Option<String>,
    pub proposed_content: String,
    pub merged_content: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum DeployStrategy {
    Merge,
    Overwrite,
    Skip,
}

impl Default for DeployStrategy {
    fn default() -> Self {
        Self::Merge
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResult {
    pub success: bool,
    pub files_written: Vec<PathBuf>,
    pub errors: Vec<String>,
}

impl DeployResult {
    pub fn success(files: Vec<PathBuf>) -> Self {
        Self {
            success: true,
            files_written: files,
            errors: vec![],
        }
    }

    pub fn failure(errors: Vec<String>) -> Self {
        Self {
            success: false,
            files_written: vec![],
            errors,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveResult {
    pub success: bool,
    pub files_removed: Vec<PathBuf>,
    pub errors: Vec<String>,
}

impl RemoveResult {
    pub fn success(files: Vec<PathBuf>) -> Self {
        Self {
            success: true,
            files_removed: files,
            errors: vec![],
        }
    }

    pub fn failure(errors: Vec<String>) -> Self {
        Self {
            success: false,
            files_removed: vec![],
            errors,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    pub mcp_servers: Vec<String>,
    pub rules: Vec<String>,
    pub skills: Vec<String>,
    pub hooks: Vec<String>,
    pub plugins: Vec<String>,
    pub agents: Vec<String>,
}

pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn capabilities(&self) -> AdapterCapabilities;
    
    fn detect(&self, project_path: &Path) -> bool;
    fn read_config(&self, project_path: &Path) -> Result<AgentConfig, String>;
    
    fn diff(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        agents: &[AgentDefinition],
        options: Option<&serde_json::Value>,
    ) -> Result<Vec<ConfigDiffEntry>, String>;
    
    fn deploy(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        agents: &[AgentDefinition],
        strategy: DeployStrategy,
        options: Option<&serde_json::Value>,
    ) -> Result<DeployResult, String>;
    
    fn remove(
        &self,
        project_path: &Path,
        capability_ids: &[CompositeId],
        agent_ids: &[CompositeId],
    ) -> Result<RemoveResult, String>;
    
    fn managed_paths(&self, project_path: &Path) -> Vec<PathBuf>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_capabilities_all() {
        let caps = AdapterCapabilities::all();
        assert!(caps.mcp);
        assert!(caps.rules);
        assert!(caps.skills);
        assert!(caps.hooks);
        assert!(caps.plugins);
        assert!(caps.agents);
        assert!(caps.custom);
    }

    #[test]
    fn test_adapter_capabilities_mcp_only() {
        let caps = AdapterCapabilities::mcp_only();
        assert!(caps.mcp);
        assert!(!caps.rules);
        assert!(!caps.skills);
        assert!(!caps.hooks);
        assert!(!caps.plugins);
        assert!(!caps.agents);
        assert!(!caps.custom);
    }

    #[test]
    fn test_deploy_strategy_default() {
        assert_eq!(DeployStrategy::default(), DeployStrategy::Merge);
    }

    #[test]
    fn test_deploy_result_success() {
        let result = DeployResult::success(vec![PathBuf::from("test.json")]);
        assert!(result.success);
        assert_eq!(result.files_written.len(), 1);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_deploy_result_failure() {
        let result = DeployResult::failure(vec!["Error".to_string()]);
        assert!(!result.success);
        assert!(result.files_written.is_empty());
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_change_type_serialization() {
        let add = ChangeType::Add;
        let json = serde_json::to_string(&add).unwrap();
        assert!(json.contains("Add"));
    }
}
