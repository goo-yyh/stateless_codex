use crate::api::agent::AgentCore;
use crate::api::running_turn::RunningTurn;
use crate::application::session_service::SessionRuntime;
use crate::application::turn_engine::TurnEngine;
use crate::domain::content::ContentBlock;
use crate::domain::error::AgentError;
use crate::domain::error::AgentErrorCode;
use crate::domain::session::SessionListQuery;
use crate::domain::session::SessionSearchQuery;
use crate::domain::session::SessionSummary;
use std::sync::Arc;

#[derive(Clone)]
pub struct SessionHandle {
    core: Arc<AgentCore>,
    runtime: Arc<SessionRuntime>,
}

impl SessionHandle {
    pub(crate) fn new(core: Arc<AgentCore>, runtime: Arc<SessionRuntime>) -> Self {
        Self { core, runtime }
    }

    pub fn session_id(&self) -> &str {
        &self.runtime.session_id
    }

    pub async fn send_message(
        &self,
        content: Vec<ContentBlock>,
    ) -> Result<RunningTurn, AgentError> {
        TurnEngine::spawn(self.core.clone(), self.runtime.clone(), content).await
    }

    pub async fn close(&self) -> Result<(), AgentError> {
        self.core
            .session_service
            .close_session(self.core.clone(), self.runtime.clone())
            .await
    }

    pub async fn summary(&self) -> SessionSummary {
        self.runtime.summary().await
    }
}

#[derive(Clone)]
pub struct SessionCatalog {
    core: Arc<AgentCore>,
}

impl SessionCatalog {
    pub(crate) fn new(core: Arc<AgentCore>) -> Self {
        Self { core }
    }

    pub async fn list(
        &self,
        query: SessionListQuery,
    ) -> Result<crate::domain::session::SessionPage, AgentError> {
        let storage = self.core.session_storage.as_ref().ok_or_else(|| {
            AgentError::new(
                AgentErrorCode::SessionNotFound,
                "session storage is not registered",
            )
        })?;
        storage.list_sessions(query).await
    }

    pub async fn find(
        &self,
        query: SessionSearchQuery,
        limit: usize,
    ) -> Result<Vec<SessionSummary>, AgentError> {
        let storage = self.core.session_storage.as_ref().ok_or_else(|| {
            AgentError::new(
                AgentErrorCode::SessionNotFound,
                "session storage is not registered",
            )
        })?;
        storage.find_sessions(query, limit).await
    }

    pub async fn delete(&self, session_id: &str) -> Result<(), AgentError> {
        if self
            .core
            .session_service
            .active_session_id()
            .is_some_and(|active_id| active_id == session_id)
        {
            return Err(AgentError::new(
                AgentErrorCode::SessionBusy,
                "cannot delete the active session",
            ));
        }
        let storage = self.core.session_storage.as_ref().ok_or_else(|| {
            AgentError::new(
                AgentErrorCode::SessionNotFound,
                "session storage is not registered",
            )
        })?;
        storage.delete_session(session_id).await
    }
}
