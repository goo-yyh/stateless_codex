use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HookKind {
    SessionStart,
    SessionEnd,
    TurnStart,
    TurnEnd,
    BeforeToolUse,
    AfterToolUse,
    BeforeCompact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TurnStartPatch {
    pub append_dynamic_sections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BeforeToolUsePatch {
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HookOutcome<P> {
    Continue,
    ContinueWith(P),
    Abort { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionStartPayload {
    pub session_id: String,
    pub model_id: String,
    pub plugin_config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionEndPayload {
    pub session_id: String,
    pub message_count: u64,
    pub plugin_config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TurnStartPayload {
    pub session_id: String,
    pub turn_id: String,
    pub user_input: Vec<crate::domain::content::ContentBlock>,
    pub dynamic_sections: Vec<String>,
    pub plugin_config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TurnEndPayload {
    pub session_id: String,
    pub turn_id: String,
    pub assistant_output: String,
    pub tool_calls_count: usize,
    pub cancelled: bool,
    pub plugin_config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BeforeToolUsePayload {
    pub session_id: String,
    pub turn_id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub plugin_config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AfterToolUsePayload {
    pub session_id: String,
    pub turn_id: String,
    pub tool_name: String,
    pub output: crate::domain::tool::ToolOutput,
    pub duration_ms: u128,
    pub success: bool,
    pub plugin_config: Value,
}
