use crate::domain::model::ProviderError;
use crate::domain::model::ProviderErrorKind;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentErrorCode {
    NoModelProvider,
    NameConflict,
    InvalidConfig,
    SkillDependencyNotMet,
    StorageDuplicate,
    InvalidDefaultModel,
    AgentShutdown,
    SessionBusy,
    SessionNotFound,
    TurnBusy,
    SkillNotFound,
    InvalidInput,
    ProviderError,
    ModelNotSupported,
    ToolNotFound,
    ToolExecutionError,
    ToolTimeout,
    HookAborted,
    StorageError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentError {
    pub code: AgentErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl AgentError {
    pub fn new(code: AgentErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
        }
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for AgentError {}

impl From<ProviderError> for AgentError {
    fn from(value: ProviderError) -> Self {
        let code = match value.kind {
            ProviderErrorKind::UnsupportedCapability { .. } => AgentErrorCode::ModelNotSupported,
            ProviderErrorKind::ContextLengthExceeded => AgentErrorCode::ProviderError,
            ProviderErrorKind::Transport | ProviderErrorKind::Provider => {
                AgentErrorCode::ProviderError
            }
        };
        AgentError {
            code,
            message: value.message,
            retryable: value.retryable,
        }
    }
}
