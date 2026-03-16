use crate::domain::tool::ToolOutput;
use crate::domain::turn::TurnOutcome;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentEvent {
    TurnStarted {
        turn_id: String,
    },
    TextDelta {
        text: String,
    },
    ReasoningDelta {
        text: String,
    },
    ToolCallStart {
        call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    ToolCallEnd {
        call_id: String,
        tool_name: String,
        output: ToolOutput,
    },
    TurnFinished {
        outcome: TurnOutcome,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentEventEnvelope {
    pub sequence: u64,
    pub event: AgentEvent,
}
