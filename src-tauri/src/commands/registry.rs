use crate::models::{AgentDefinition, UniversalCapability};
use crate::registry::{get_bundled_registry_path, get_community_registry_path, load_agents, load_capabilities};

fn get_all_registry_paths() -> Vec<std::path::PathBuf> {
    let mut dirs = vec![get_bundled_registry_path()];
    
    let community_path = get_community_registry_path();
    if community_path.exists() {
        dirs.push(community_path);
    }
    
    let custom_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.agentharbor.app")
        .join("registry")
        .join("custom");
    if custom_path.exists() {
        dirs.push(custom_path);
    }
    
    dirs
}

#[tauri::command]
pub fn get_all_capabilities() -> Vec<UniversalCapability> {
    let dirs = get_all_registry_paths();
    
    let result = load_capabilities(&dirs);
    
    for error in &result.errors {
        eprintln!(
            "Warning: Failed to load capability from {:?}: {}",
            error.path, error.message
        );
    }
    
    result.items
}

#[tauri::command]
pub fn get_all_agents() -> Vec<AgentDefinition> {
    let dirs = get_all_registry_paths();
    
    let result = load_agents(&dirs);
    
    for error in &result.errors {
        eprintln!(
            "Warning: Failed to load agent from {:?}: {}",
            error.path, error.message
        );
    }
    
    result.items
}

#[tauri::command]
pub fn get_capabilities_by_type(capability_type: String) -> Vec<UniversalCapability> {
    let all = get_all_capabilities();
    
    all.into_iter()
        .filter(|cap| {
            let type_str = match cap {
                UniversalCapability::Mcp(_) => "mcp",
                UniversalCapability::Rule(_) => "rule",
                UniversalCapability::Skill(_) => "skill",
                UniversalCapability::Hook(_) => "hook",
                UniversalCapability::Plugin(_) => "plugin",
                UniversalCapability::Custom(_) => "custom",
            };
            type_str == capability_type
        })
        .collect()
}

#[tauri::command]
pub fn search_capabilities(query: String) -> Vec<UniversalCapability> {
    let all = get_all_capabilities();
    let query_lower = query.to_lowercase();
    
    all.into_iter()
        .filter(|cap| {
            let name = cap.name().to_lowercase();
            let id = cap.id().to_string().to_lowercase();
            name.contains(&query_lower) || id.contains(&query_lower)
        })
        .collect()
}

#[tauri::command]
pub fn search_agents(query: String) -> Vec<AgentDefinition> {
    let all = get_all_agents();
    let query_lower = query.to_lowercase();
    
    all.into_iter()
        .filter(|agent| {
            let name = agent.name.to_lowercase();
            let id = agent.id.to_string().to_lowercase();
            let desc = agent.description.to_lowercase();
            name.contains(&query_lower) || id.contains(&query_lower) || desc.contains(&query_lower)
        })
        .collect()
}
