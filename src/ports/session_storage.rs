use crate::domain::error::AgentError;
use crate::domain::ledger::LedgerEvent;
use crate::domain::session::SessionListQuery;
use crate::domain::session::SessionPage;
use crate::domain::session::SessionSearchQuery;
use crate::domain::session::SessionSummary;
use async_trait::async_trait;

#[async_trait]
pub trait SessionStorage: Send + Sync {
    async fn append_event(
        &self,
        session_id: &str,
        event: &LedgerEvent,
        summary: &SessionSummary,
    ) -> Result<(), AgentError>;

    async fn load_events(&self, session_id: &str) -> Result<Vec<LedgerEvent>, AgentError>;

    async fn list_sessions(&self, query: SessionListQuery) -> Result<SessionPage, AgentError>;

    async fn find_sessions(
        &self,
        query: SessionSearchQuery,
        limit: usize,
    ) -> Result<Vec<SessionSummary>, AgentError>;

    async fn delete_session(&self, session_id: &str) -> Result<(), AgentError>;
}
