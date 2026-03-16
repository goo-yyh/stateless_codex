use crate::domain::error::AgentError;
use crate::domain::error::AgentErrorCode;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EnvironmentContext {
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub current_date: Option<String>,
    pub timezone: Option<String>,
}

impl EnvironmentContext {
    pub fn serialize_to_xml(&self) -> String {
        let mut lines = vec!["<environment_context>".to_string()];
        if let Some(cwd) = &self.cwd {
            lines.push(format!("  <cwd>{cwd}</cwd>"));
        }
        if let Some(shell) = &self.shell {
            lines.push(format!("  <shell>{shell}</shell>"));
        }
        if let Some(current_date) = &self.current_date {
            lines.push(format!("  <current_date>{current_date}</current_date>"));
        }
        if let Some(timezone) = &self.timezone {
            lines.push(format!("  <timezone>{timezone}</timezone>"));
        }
        lines.push("</environment_context>".to_string());
        lines.join("\n")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    pub default_model: String,
    pub system_instructions: Vec<String>,
    pub personality: Option<String>,
    pub environment_context: Option<EnvironmentContext>,
    pub tool_timeout_ms: u64,
    pub compact_threshold: f32,
    pub compact_model: Option<String>,
    pub compact_prompt: Option<String>,
    pub max_tool_calls_per_turn: usize,
    pub memory_model: Option<String>,
    pub memory_checkpoint_interval: u32,
    pub memory_max_items: usize,
    pub memory_namespace: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            default_model: String::new(),
            system_instructions: Vec::new(),
            personality: None,
            environment_context: None,
            tool_timeout_ms: 120_000,
            compact_threshold: 0.8,
            compact_model: None,
            compact_prompt: None,
            max_tool_calls_per_turn: 50,
            memory_model: None,
            memory_checkpoint_interval: 10,
            memory_max_items: 20,
            memory_namespace: "default".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SessionConfig {
    pub model_id: Option<String>,
    pub system_prompt_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionPinnedConfig {
    pub model_id: String,
    pub system_prompt_override: Option<String>,
    pub memory_namespace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionRuntimePolicy {
    pub tool_timeout_ms: u64,
    pub compact_threshold: f32,
    pub compact_model_id: String,
    pub compact_prompt: String,
    pub max_tool_calls_per_turn: usize,
    pub memory_model_id: String,
    pub memory_checkpoint_interval: u32,
    pub memory_max_items: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedSessionConfig {
    pub pinned: SessionPinnedConfig,
    pub runtime_policy: SessionRuntimePolicy,
}

pub const DEFAULT_COMPACT_PROMPT: &str = "Summarize the older conversation faithfully and preserve unresolved constraints, tool results, and open questions.";

pub fn resolve_session_pinned_config(
    agent_config: &AgentConfig,
    session_config: SessionConfig,
) -> Result<SessionPinnedConfig, AgentError> {
    let model_id = session_config
        .model_id
        .unwrap_or_else(|| agent_config.default_model.clone());
    let memory_namespace = agent_config.memory_namespace.trim().to_string();
    if memory_namespace.is_empty() {
        return Err(AgentError::new(
            AgentErrorCode::InvalidConfig,
            "memory_namespace must not be blank",
        ));
    }

    Ok(SessionPinnedConfig {
        model_id,
        system_prompt_override: normalize_optional_text(session_config.system_prompt_override),
        memory_namespace,
    })
}

pub fn resolve_session_runtime_policy(
    agent_config: &AgentConfig,
    pinned: &SessionPinnedConfig,
) -> SessionRuntimePolicy {
    SessionRuntimePolicy {
        tool_timeout_ms: agent_config.tool_timeout_ms,
        compact_threshold: agent_config.compact_threshold,
        compact_model_id: agent_config
            .compact_model
            .clone()
            .unwrap_or_else(|| pinned.model_id.clone()),
        compact_prompt: normalize_optional_text(agent_config.compact_prompt.clone())
            .unwrap_or_else(|| DEFAULT_COMPACT_PROMPT.to_string()),
        max_tool_calls_per_turn: agent_config.max_tool_calls_per_turn,
        memory_model_id: agent_config
            .memory_model
            .clone()
            .unwrap_or_else(|| pinned.model_id.clone()),
        memory_checkpoint_interval: agent_config.memory_checkpoint_interval,
        memory_max_items: agent_config.memory_max_items,
    }
}

pub fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}
