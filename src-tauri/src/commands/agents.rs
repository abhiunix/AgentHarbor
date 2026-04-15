use crate::models::AgentDefinition;
use std::fs;
use std::path::PathBuf;

fn get_custom_agents_dir() -> PathBuf {
    crate::utils::paths::app_data_dir().join("agents")
}

#[tauri::command]
pub fn save_agent(agent: AgentDefinition) -> Result<AgentDefinition, String> {
    let agents_dir = get_custom_agents_dir();
    
    if !agents_dir.exists() {
        fs::create_dir_all(&agents_dir)
            .map_err(|e| format!("Failed to create agents directory: {}", e))?;
    }

    let filename = format!("{}.json", agent.id.name);
    let filepath = agents_dir.join(&filename);
    let temp_filepath = agents_dir.join(format!("{}.tmp", agent.id.name));

    let json = serde_json::to_string_pretty(&agent)
        .map_err(|e| format!("Failed to serialize agent: {}", e))?;

    fs::write(&temp_filepath, &json)
        .map_err(|e| format!("Failed to write agent file: {}", e))?;

    fs::rename(&temp_filepath, &filepath)
        .map_err(|e| format!("Failed to save agent file: {}", e))?;

    Ok(agent)
}

#[tauri::command]
pub fn delete_agent(id: String) -> Result<(), String> {
    let agents_dir = get_custom_agents_dir();
    
    let parts: Vec<&str> = id.splitn(2, '/').collect();
    if parts.len() != 2 {
        return Err("Invalid agent ID format".to_string());
    }
    let name = parts[1];

    let filepath = agents_dir.join(format!("{}.json", name));

    if !filepath.exists() {
        return Err("Agent not found".to_string());
    }

    fs::remove_file(&filepath)
        .map_err(|e| format!("Failed to delete agent: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentModel, AgentColor, MemoryScope, ToolAccess, CompositeId};
    use crate::models::Visibility;

    fn create_test_agent() -> AgentDefinition {
        AgentDefinition {
            id: CompositeId::new("test", "my-agent").unwrap(),
            name: "My Agent".to_string(),
            description: "Test agent".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Private,
            tags: vec!["test".to_string()],
            model: AgentModel::Sonnet,
            color: AgentColor::Blue,
            memory: MemoryScope::None,
            tools: vec![ToolAccess::All],
            required_capabilities: vec![],
            prompt: "You are a test agent.".to_string(),
            examples: vec![],
        }
    }

    #[test]
    fn test_get_custom_agents_dir() {
        let dir = get_custom_agents_dir();
        assert!(dir.to_string_lossy().contains("com.agentharbor.app"));
        assert!(dir.to_string_lossy().ends_with("agents"));
    }
}
