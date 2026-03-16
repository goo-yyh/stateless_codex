use crate::domain::error::AgentError;
use crate::domain::error::AgentErrorCode;
use crate::domain::hook::AfterToolUsePayload;
use crate::domain::hook::BeforeToolUsePatch;
use crate::domain::hook::BeforeToolUsePayload;
use crate::domain::hook::HookKind;
use crate::domain::hook::HookOutcome;
use crate::domain::hook::SessionEndPayload;
use crate::domain::hook::SessionStartPayload;
use crate::domain::hook::TurnEndPayload;
use crate::domain::hook::TurnStartPatch;
use crate::domain::hook::TurnStartPayload;
use crate::domain::plugin::PluginDescriptor;
use crate::ports::plugin::Plugin;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct PluginEntry {
    pub descriptor: PluginDescriptor,
    pub config: Value,
    pub plugin: Arc<dyn Plugin>,
}

#[derive(Clone, Default)]
pub struct PluginManager {
    entries: Vec<PluginEntry>,
}

impl PluginManager {
    pub fn new(entries: Vec<PluginEntry>) -> Self {
        Self { entries }
    }

    pub fn descriptors(&self) -> Vec<PluginDescriptor> {
        self.entries
            .iter()
            .map(|entry| entry.descriptor.clone())
            .collect()
    }

    pub async fn on_session_start(
        &self,
        session_id: &str,
        model_id: &str,
    ) -> Result<(), AgentError> {
        for entry in &self.entries {
            if !entry
                .descriptor
                .tapped_hooks
                .contains(&HookKind::SessionStart)
            {
                continue;
            }
            let payload = SessionStartPayload {
                session_id: session_id.to_string(),
                model_id: model_id.to_string(),
                plugin_config: entry.config.clone(),
            };
            Self::expect_continue(entry.plugin.on_session_start(payload).await?)?;
        }
        Ok(())
    }

    pub async fn on_session_end(
        &self,
        session_id: &str,
        message_count: u64,
    ) -> Result<(), AgentError> {
        for entry in &self.entries {
            if !entry
                .descriptor
                .tapped_hooks
                .contains(&HookKind::SessionEnd)
            {
                continue;
            }
            let payload = SessionEndPayload {
                session_id: session_id.to_string(),
                message_count,
                plugin_config: entry.config.clone(),
            };
            Self::expect_continue(entry.plugin.on_session_end(payload).await?)?;
        }
        Ok(())
    }

    pub async fn on_turn_start(
        &self,
        session_id: &str,
        turn_id: &str,
        user_input: &[crate::domain::content::ContentBlock],
    ) -> Result<Vec<String>, AgentError> {
        let mut dynamic_sections = Vec::new();
        for entry in &self.entries {
            if !entry.descriptor.tapped_hooks.contains(&HookKind::TurnStart) {
                continue;
            }
            let payload = TurnStartPayload {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                user_input: user_input.to_vec(),
                dynamic_sections: dynamic_sections.clone(),
                plugin_config: entry.config.clone(),
            };
            match entry.plugin.on_turn_start(payload).await? {
                HookOutcome::Continue => {}
                HookOutcome::ContinueWith(TurnStartPatch {
                    append_dynamic_sections,
                }) => {
                    dynamic_sections.extend(append_dynamic_sections);
                }
                HookOutcome::Abort { reason } => {
                    return Err(AgentError::new(AgentErrorCode::HookAborted, reason));
                }
            }
        }
        Ok(dynamic_sections)
    }

    pub async fn on_turn_end(&self, payload: TurnEndPayload) -> Result<(), AgentError> {
        for entry in &self.entries {
            if !entry.descriptor.tapped_hooks.contains(&HookKind::TurnEnd) {
                continue;
            }
            let mut payload = payload.clone();
            payload.plugin_config = entry.config.clone();
            Self::expect_continue(entry.plugin.on_turn_end(payload).await?)?;
        }
        Ok(())
    }

    pub async fn on_before_tool_use(
        &self,
        session_id: &str,
        turn_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, AgentError> {
        let mut effective_arguments = arguments;
        for entry in &self.entries {
            if !entry
                .descriptor
                .tapped_hooks
                .contains(&HookKind::BeforeToolUse)
            {
                continue;
            }
            let payload = BeforeToolUsePayload {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                tool_name: tool_name.to_string(),
                arguments: effective_arguments.clone(),
                plugin_config: entry.config.clone(),
            };
            match entry.plugin.on_before_tool_use(payload).await? {
                HookOutcome::Continue => {}
                HookOutcome::ContinueWith(BeforeToolUsePatch { arguments }) => {
                    effective_arguments = arguments;
                }
                HookOutcome::Abort { reason } => {
                    return Err(AgentError::new(AgentErrorCode::HookAborted, reason));
                }
            }
        }
        Ok(effective_arguments)
    }

    pub async fn on_after_tool_use(&self, payload: AfterToolUsePayload) -> Result<(), AgentError> {
        for entry in &self.entries {
            if !entry
                .descriptor
                .tapped_hooks
                .contains(&HookKind::AfterToolUse)
            {
                continue;
            }
            let mut payload = payload.clone();
            payload.plugin_config = entry.config.clone();
            Self::expect_continue(entry.plugin.on_after_tool_use(payload).await?)?;
        }
        Ok(())
    }

    pub fn shutdown_all(&self) -> Result<(), AgentError> {
        for entry in self.entries.iter().rev() {
            entry.plugin.shutdown()?;
        }
        Ok(())
    }

    fn expect_continue(result: HookOutcome<()>) -> Result<(), AgentError> {
        match result {
            HookOutcome::Continue | HookOutcome::ContinueWith(()) => Ok(()),
            HookOutcome::Abort { reason } => {
                Err(AgentError::new(AgentErrorCode::HookAborted, reason))
            }
        }
    }
}
