use crate::domain::content::ContentBlock;
use crate::domain::memory::MemoryCheckpointRecord;
use crate::domain::session::CompactionRecord;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageStatus {
    Complete,
    Incomplete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResultRecord {
    pub call_id: String,
    pub tool_name: String,
    pub output: crate::domain::tool::ToolOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallRecord {
    pub call_id: String,
    pub tool_name: String,
    pub requested_arguments: Value,
    pub effective_arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionEventPayload {
    UserMessage {
        content: Vec<ContentBlock>,
    },
    AssistantMessage {
        content: Vec<ContentBlock>,
        status: MessageStatus,
    },
    ToolCall {
        call: ToolCallRecord,
    },
    ToolResult {
        result: ToolResultRecord,
    },
    SystemMessage {
        content: String,
    },
    Metadata {
        key: String,
        value: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LedgerEvent {
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub payload: SessionEventPayload,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionLedger {
    events: Vec<LedgerEvent>,
}

pub const SESSION_PROFILE_KEY: &str = "session_profile";
pub const SKILL_INVOCATIONS_KEY: &str = "skill_invocations";
pub const CONTEXT_COMPACTION_KEY: &str = "context_compaction";
pub const MEMORY_CHECKPOINT_KEY: &str = "memory_checkpoint";

impl SessionLedger {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn with_events(events: Vec<LedgerEvent>) -> Self {
        Self { events }
    }

    pub fn events(&self) -> &[LedgerEvent] {
        &self.events
    }

    pub fn append_payload(&mut self, payload: SessionEventPayload) -> LedgerEvent {
        let event = LedgerEvent {
            seq: self.events.len() as u64 + 1,
            timestamp: Utc::now(),
            payload,
        };
        self.events.push(event.clone());
        event
    }

    pub fn message_count(&self) -> u64 {
        self.events
            .iter()
            .filter(|event| {
                matches!(
                    event.payload,
                    SessionEventPayload::UserMessage { .. }
                        | SessionEventPayload::AssistantMessage { .. }
                        | SessionEventPayload::SystemMessage { .. }
                )
            })
            .count() as u64
    }

    pub fn latest_compaction(&self) -> Option<CompactionRecord> {
        self.events
            .iter()
            .rev()
            .find_map(|event| match &event.payload {
                SessionEventPayload::Metadata { key, value } if key == CONTEXT_COMPACTION_KEY => {
                    serde_json::from_value(value.clone()).ok()
                }
                _ => None,
            })
    }

    pub fn latest_memory_checkpoint(&self) -> Option<MemoryCheckpointRecord> {
        self.events
            .iter()
            .rev()
            .find_map(|event| match &event.payload {
                SessionEventPayload::Metadata { key, value } if key == MEMORY_CHECKPOINT_KEY => {
                    serde_json::from_value(value.clone()).ok()
                }
                _ => None,
            })
    }
}
