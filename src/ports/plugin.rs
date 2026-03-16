use crate::domain::error::AgentError;
use crate::domain::hook::AfterToolUsePayload;
use crate::domain::hook::BeforeToolUsePatch;
use crate::domain::hook::BeforeToolUsePayload;
use crate::domain::hook::HookOutcome;
use crate::domain::hook::SessionEndPayload;
use crate::domain::hook::SessionStartPayload;
use crate::domain::hook::TurnEndPayload;
use crate::domain::hook::TurnStartPatch;
use crate::domain::hook::TurnStartPayload;
use crate::domain::plugin::PluginDescriptor;
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait Plugin: Send + Sync {
    fn descriptor(&self) -> &PluginDescriptor;

    fn initialize(&self, _config: &Value) -> Result<(), AgentError> {
        Ok(())
    }

    async fn on_session_start(
        &self,
        _payload: SessionStartPayload,
    ) -> Result<HookOutcome<()>, AgentError> {
        Ok(HookOutcome::Continue)
    }

    async fn on_session_end(
        &self,
        _payload: SessionEndPayload,
    ) -> Result<HookOutcome<()>, AgentError> {
        Ok(HookOutcome::Continue)
    }

    async fn on_turn_start(
        &self,
        _payload: TurnStartPayload,
    ) -> Result<HookOutcome<TurnStartPatch>, AgentError> {
        Ok(HookOutcome::Continue)
    }

    async fn on_turn_end(&self, _payload: TurnEndPayload) -> Result<HookOutcome<()>, AgentError> {
        Ok(HookOutcome::Continue)
    }

    async fn on_before_tool_use(
        &self,
        _payload: BeforeToolUsePayload,
    ) -> Result<HookOutcome<BeforeToolUsePatch>, AgentError> {
        Ok(HookOutcome::Continue)
    }

    async fn on_after_tool_use(
        &self,
        _payload: AfterToolUsePayload,
    ) -> Result<HookOutcome<()>, AgentError> {
        Ok(HookOutcome::Continue)
    }

    fn shutdown(&self) -> Result<(), AgentError> {
        Ok(())
    }
}
