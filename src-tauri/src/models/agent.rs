use serde::{Deserialize, Serialize};

use super::capability::Visibility;
use super::composite_id::CompositeId;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentModel {
    Haiku,
    Sonnet,
    Opus,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentColor {
    Red,
    Blue,
    Green,
    Yellow,
    Purple,
    Orange,
    Pink,
    Cyan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MemoryScope {
    Project,
    User,
    None,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ToolAccess {
    All,
    ReadOnly,
    Edit,
    Execution,
    Mcp,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentExample {
    pub user: String,
    pub agent: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentDefinition {
    pub id: CompositeId,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub visibility: Visibility,
    pub tags: Vec<String>,
    pub model: AgentModel,
    pub color: AgentColor,
    pub memory: MemoryScope,
    pub tools: Vec<ToolAccess>,
    pub required_capabilities: Vec<CompositeId>,
    pub prompt: String,
    #[serde(default)]
    pub examples: Vec<AgentExample>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_agent_model_serialization() {
        assert_eq!(
            serde_json::to_string(&AgentModel::Haiku).unwrap(),
            "\"haiku\""
        );
        assert_eq!(
            serde_json::to_string(&AgentModel::Sonnet).unwrap(),
            "\"sonnet\""
        );
        assert_eq!(
            serde_json::to_string(&AgentModel::Opus).unwrap(),
            "\"opus\""
        );
    }
    
    #[test]
    fn test_agent_color_serialization() {
        assert_eq!(
            serde_json::to_string(&AgentColor::Red).unwrap(),
            "\"red\""
        );
        assert_eq!(
            serde_json::to_string(&AgentColor::Blue).unwrap(),
            "\"blue\""
        );
        assert_eq!(
            serde_json::to_string(&AgentColor::Green).unwrap(),
            "\"green\""
        );
        assert_eq!(
            serde_json::to_string(&AgentColor::Yellow).unwrap(),
            "\"yellow\""
        );
        assert_eq!(
            serde_json::to_string(&AgentColor::Purple).unwrap(),
            "\"purple\""
        );
        assert_eq!(
            serde_json::to_string(&AgentColor::Orange).unwrap(),
            "\"orange\""
        );
        assert_eq!(
            serde_json::to_string(&AgentColor::Pink).unwrap(),
            "\"pink\""
        );
        assert_eq!(
            serde_json::to_string(&AgentColor::Cyan).unwrap(),
            "\"cyan\""
        );
    }
    
    #[test]
    fn test_memory_scope_serialization() {
        assert_eq!(
            serde_json::to_string(&MemoryScope::Project).unwrap(),
            "\"project\""
        );
        assert_eq!(
            serde_json::to_string(&MemoryScope::User).unwrap(),
            "\"user\""
        );
        assert_eq!(
            serde_json::to_string(&MemoryScope::None).unwrap(),
            "\"none\""
        );
    }
    
    #[test]
    fn test_tool_access_serialization() {
        assert_eq!(
            serde_json::to_string(&ToolAccess::All).unwrap(),
            "\"all\""
        );
        assert_eq!(
            serde_json::to_string(&ToolAccess::ReadOnly).unwrap(),
            "\"read-only\""
        );
        assert_eq!(
            serde_json::to_string(&ToolAccess::Edit).unwrap(),
            "\"edit\""
        );
        assert_eq!(
            serde_json::to_string(&ToolAccess::Execution).unwrap(),
            "\"execution\""
        );
        assert_eq!(
            serde_json::to_string(&ToolAccess::Mcp).unwrap(),
            "\"mcp\""
        );
        assert_eq!(
            serde_json::to_string(&ToolAccess::Other).unwrap(),
            "\"other\""
        );
    }
    
    #[test]
    fn test_agent_definition_serialization() {
        let json = r#"{
            "type": "agent",
            "id": "community/api-test-runner",
            "name": "API Test Runner",
            "description": "Use this agent when new API endpoints have been created or modified and need to be tested.",
            "version": "1.0.0",
            "author": "community",
            "visibility": "public",
            "tags": ["testing", "api", "quality"],
            "model": "haiku",
            "color": "green",
            "memory": "project",
            "tools": ["all"],
            "required_capabilities": ["community/github-mcp"],
            "prompt": "You are an elite API testing engineer with deep expertise in RESTful API design.",
            "examples": [
                {
                    "user": "Add a new POST /api/users endpoint",
                    "agent": "Let me launch the api-test-runner agent to test the newly created endpoint."
                }
            ]
        }"#;
        
        #[derive(Deserialize)]
        struct AgentWithType {
            #[serde(rename = "type")]
            _type: String,
            #[serde(flatten)]
            agent: AgentDefinition,
        }
        
        let parsed: AgentWithType = serde_json::from_str(json).unwrap();
        let agent = parsed.agent;
        
        assert_eq!(agent.id.to_string(), "community/api-test-runner");
        assert_eq!(agent.name, "API Test Runner");
        assert_eq!(agent.model, AgentModel::Haiku);
        assert_eq!(agent.color, AgentColor::Green);
        assert_eq!(agent.memory, MemoryScope::Project);
        assert_eq!(agent.tools.len(), 1);
        assert_eq!(agent.tools[0], ToolAccess::All);
        assert_eq!(agent.required_capabilities.len(), 1);
        assert_eq!(
            agent.required_capabilities[0].to_string(),
            "community/github-mcp"
        );
        assert_eq!(agent.examples.len(), 1);
        assert_eq!(agent.examples[0].user, "Add a new POST /api/users endpoint");
    }
    
    #[test]
    fn test_agent_definition_roundtrip() {
        let agent = AgentDefinition {
            id: "community/test-agent".parse().unwrap(),
            name: "Test Agent".to_string(),
            description: "A test agent".to_string(),
            version: "1.0.0".to_string(),
            author: "community".to_string(),
            visibility: Visibility::Public,
            tags: vec!["test".to_string()],
            model: AgentModel::Sonnet,
            color: AgentColor::Blue,
            memory: MemoryScope::User,
            tools: vec![ToolAccess::ReadOnly, ToolAccess::Mcp],
            required_capabilities: vec![],
            prompt: "You are a test agent.".to_string(),
            examples: vec![AgentExample {
                user: "Test question".to_string(),
                agent: "Test response".to_string(),
            }],
        };
        
        let serialized = serde_json::to_string(&agent).unwrap();
        let deserialized: AgentDefinition = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(agent.id.to_string(), deserialized.id.to_string());
        assert_eq!(agent.name, deserialized.name);
        assert_eq!(agent.model, deserialized.model);
        assert_eq!(agent.color, deserialized.color);
        assert_eq!(agent.memory, deserialized.memory);
        assert_eq!(agent.tools, deserialized.tools);
    }
    
    #[test]
    fn test_agent_example_serialization() {
        let example = AgentExample {
            user: "How do I test this?".to_string(),
            agent: "Let me help you with that.".to_string(),
        };
        
        let json = serde_json::to_string(&example).unwrap();
        let parsed: AgentExample = serde_json::from_str(&json).unwrap();
        
        assert_eq!(example.user, parsed.user);
        assert_eq!(example.agent, parsed.agent);
    }
}
