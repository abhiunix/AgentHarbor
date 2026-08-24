use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CompositeId {
    pub author: String,
    pub name: String,
}

#[derive(Debug, PartialEq)]
pub enum CompositeIdError {
    EmptyAuthor,
    EmptyName,
    InvalidFormat,
    InvalidCharacter(String),
    SlashInPart,
}

impl fmt::Display for CompositeIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompositeIdError::EmptyAuthor => write!(f, "author cannot be empty"),
            CompositeIdError::EmptyName => write!(f, "name cannot be empty"),
            CompositeIdError::InvalidFormat => {
                write!(f, "invalid format, expected 'author/name'")
            }
            CompositeIdError::InvalidCharacter(part) => {
                write!(f, "invalid character in '{}', must be kebab-case", part)
            }
            CompositeIdError::SlashInPart => write!(f, "slash not allowed within author or name"),
        }
    }
}

impl std::error::Error for CompositeIdError {}

fn is_valid_kebab_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    
    let mut prev_was_hyphen = true;
    
    for c in s.chars() {
        if c == '-' {
            if prev_was_hyphen {
                return false;
            }
            prev_was_hyphen = true;
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
            prev_was_hyphen = false;
        } else {
            return false;
        }
    }
    
    !prev_was_hyphen
}

impl CompositeId {
    pub fn new(author: &str, name: &str) -> Result<Self, CompositeIdError> {
        if author.is_empty() {
            return Err(CompositeIdError::EmptyAuthor);
        }
        if name.is_empty() {
            return Err(CompositeIdError::EmptyName);
        }
        if author.contains('/') || name.contains('/') {
            return Err(CompositeIdError::SlashInPart);
        }
        if !is_valid_kebab_case(author) {
            return Err(CompositeIdError::InvalidCharacter(author.to_string()));
        }
        if !is_valid_kebab_case(name) {
            return Err(CompositeIdError::InvalidCharacter(name.to_string()));
        }
        
        Ok(Self {
            author: author.to_string(),
            name: name.to_string(),
        })
    }
    
    pub fn is_public(&self) -> bool {
        self.author == "community"
    }
    
    pub fn is_private(&self) -> bool {
        !self.is_public()
    }

    /// Deterministic, readable artifact key for filenames/scripts. For community items
    /// returns `id.name`; for private items returns a kebab slug of display_name plus
    /// a short hash of the id so opaque IDs still produce human-usable filenames.
    pub fn artifact_name(&self, display_name: &str) -> String {
        if self.author == "community" {
            return self.name.clone();
        }
        let slug = to_kebab_slug(display_name);
        let short = short_hash(self);
        if slug.is_empty() {
            short
        } else {
            format!("{}-{}", slug, short)
        }
    }
}

pub fn to_kebab_slug(s: &str) -> String {
    let s = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let s = s.trim_matches('-');
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

fn short_hash(id: &CompositeId) -> String {
    let mut hasher = DefaultHasher::new();
    id.to_string().hash(&mut hasher);
    format!("{:016x}", hasher.finish())[..8].to_string()
}

impl fmt::Display for CompositeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.author, self.name)
    }
}

impl FromStr for CompositeId {
    type Err = CompositeIdError;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        
        if parts.len() != 2 {
            return Err(CompositeIdError::InvalidFormat);
        }
        
        CompositeId::new(parts[0], parts[1])
    }
}

impl Serialize for CompositeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CompositeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        CompositeId::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_valid_composite_id() {
        let id = CompositeId::from_str("community/github-mcp").unwrap();
        assert_eq!(id.author, "community");
        assert_eq!(id.name, "github-mcp");
    }
    
    #[test]
    fn test_display_composite_id() {
        let id = CompositeId::new("community", "github-mcp").unwrap();
        assert_eq!(id.to_string(), "community/github-mcp");
    }
    
    #[test]
    fn test_roundtrip_composite_id() {
        let original = "community/github-mcp";
        let id = CompositeId::from_str(original).unwrap();
        let rendered = id.to_string();
        let reparsed = CompositeId::from_str(&rendered).unwrap();
        
        assert_eq!(id, reparsed);
        assert_eq!(original, rendered);
    }
    
    #[test]
    fn test_is_public() {
        let public_id = CompositeId::from_str("community/github-mcp").unwrap();
        assert!(public_id.is_public());
        assert!(!public_id.is_private());
        
        let private_id = CompositeId::from_str("abhi/my-mcp").unwrap();
        assert!(!private_id.is_public());
        assert!(private_id.is_private());
    }
    
    #[test]
    fn test_reject_empty_author() {
        let result = CompositeId::from_str("/github-mcp");
        assert!(matches!(result, Err(CompositeIdError::EmptyAuthor)));
    }
    
    #[test]
    fn test_reject_empty_name() {
        let result = CompositeId::from_str("community/");
        assert!(matches!(result, Err(CompositeIdError::EmptyName)));
    }
    
    #[test]
    fn test_reject_no_slash() {
        let result = CompositeId::from_str("communitygithub-mcp");
        assert!(matches!(result, Err(CompositeIdError::InvalidFormat)));
    }
    
    #[test]
    fn test_reject_invalid_chars() {
        let result = CompositeId::from_str("Community/github-mcp");
        assert!(matches!(result, Err(CompositeIdError::InvalidCharacter(_))));
        
        let result = CompositeId::from_str("community/GitHub_mcp");
        assert!(matches!(result, Err(CompositeIdError::InvalidCharacter(_))));
        
        let result = CompositeId::from_str("community/github mcp");
        assert!(matches!(result, Err(CompositeIdError::InvalidCharacter(_))));
    }
    
    #[test]
    fn test_reject_consecutive_hyphens() {
        let result = CompositeId::from_str("community/github--mcp");
        assert!(matches!(result, Err(CompositeIdError::InvalidCharacter(_))));
    }
    
    #[test]
    fn test_artifact_name_community() {
        let id = CompositeId::from_str("community/github-mcp").unwrap();
        assert_eq!(id.artifact_name("GitHub MCP"), "github-mcp");
    }

    #[test]
    fn test_artifact_name_private_deterministic() {
        let id = CompositeId::from_str("user/a1b2c3d4").unwrap();
        let a = id.artifact_name("My Skill");
        assert!(a.starts_with("my-skill-"));
        assert_eq!(a.len(), "my-skill-".len() + 8);
        assert_eq!(id.artifact_name("My Skill"), a);
    }

    #[test]
    fn test_reject_leading_trailing_hyphens() {
        let result = CompositeId::from_str("community/-github-mcp");
        assert!(matches!(result, Err(CompositeIdError::InvalidCharacter(_))));
        
        let result = CompositeId::from_str("community/github-mcp-");
        assert!(matches!(result, Err(CompositeIdError::InvalidCharacter(_))));
    }
    
    #[test]
    fn test_serde_roundtrip() {
        let id = CompositeId::from_str("community/github-mcp").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"community/github-mcp\"");
        
        let deserialized: CompositeId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }
    
    #[test]
    fn test_valid_kebab_case_names() {
        assert!(CompositeId::from_str("community/github-mcp").is_ok());
        assert!(CompositeId::from_str("abhi/my-tool-v2").is_ok());
        assert!(CompositeId::from_str("user123/test-tool").is_ok());
    }
}
