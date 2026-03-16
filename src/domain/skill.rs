use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDefinition {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub prompt: String,
    pub tool_dependencies: Vec<String>,
    pub allow_implicit_invocation: bool,
}
