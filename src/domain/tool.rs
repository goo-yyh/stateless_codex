use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

pub const MAX_TOOL_OUTPUT_BYTES: usize = 1024 * 1024;
pub const TOOL_OUTPUT_TRUNCATION_SUFFIX: &str = "\n[tool output truncated]";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
    pub mutating: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolInput {
    pub call_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
    pub metadata: Option<Value>,
}

impl ToolOutput {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: message.into(),
            is_error: true,
            metadata: None,
        }
    }

    pub fn truncate(self) -> Self {
        if self.content.len() <= MAX_TOOL_OUTPUT_BYTES {
            return self;
        }

        let allowed = MAX_TOOL_OUTPUT_BYTES.saturating_sub(TOOL_OUTPUT_TRUNCATION_SUFFIX.len());
        let mut content = self.content;
        content.truncate(allowed);
        content.push_str(TOOL_OUTPUT_TRUNCATION_SUFFIX);
        Self { content, ..self }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestedToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub requested_arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestedToolBatch {
    pub calls: Vec<RequestedToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub requested_arguments: Value,
    pub effective_arguments: Value,
    pub mutating: bool,
}
