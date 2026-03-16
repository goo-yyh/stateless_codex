use crate::domain::error::AgentError;
use crate::domain::error::AgentErrorCode;
use crate::domain::ledger::LedgerEvent;
use crate::domain::memory::Memory;
use crate::domain::session::SessionListQuery;
use crate::domain::session::SessionPage;
use crate::domain::session::SessionSearchQuery;
use crate::domain::session::SessionSummary;
use crate::ports::memory_storage::MemoryStorage;
use crate::ports::session_storage::SessionStorage;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Default)]
pub struct InMemorySessionStorage {
    inner: Arc<Mutex<BTreeMap<String, StoredSession>>>,
}

#[derive(Debug, Clone)]
struct StoredSession {
    events: Vec<LedgerEvent>,
    summary: SessionSummary,
}

#[async_trait]
impl SessionStorage for InMemorySessionStorage {
    async fn append_event(
        &self,
        session_id: &str,
        event: &LedgerEvent,
        summary: &SessionSummary,
    ) -> Result<(), AgentError> {
        let mut guard = self.inner.lock().await;
        let entry = guard
            .entry(session_id.to_string())
            .or_insert_with(|| StoredSession {
                events: Vec::new(),
                summary: summary.clone(),
            });
        entry.events.push(event.clone());
        entry.summary = summary.clone();
        Ok(())
    }

    async fn load_events(&self, session_id: &str) -> Result<Vec<LedgerEvent>, AgentError> {
        let guard = self.inner.lock().await;
        guard
            .get(session_id)
            .map(|stored| stored.events.clone())
            .ok_or_else(|| AgentError::new(AgentErrorCode::SessionNotFound, "session not found"))
    }

    async fn list_sessions(&self, query: SessionListQuery) -> Result<SessionPage, AgentError> {
        let guard = self.inner.lock().await;
        let mut items: Vec<SessionSummary> = guard
            .values()
            .map(|stored| stored.summary.clone())
            .collect();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        let start = query
            .cursor
            .as_ref()
            .and_then(|cursor| items.iter().position(|item| item.session_id == *cursor))
            .map(|index| index + 1)
            .unwrap_or(0);
        let slice = items
            .into_iter()
            .skip(start)
            .take(query.limit)
            .collect::<Vec<_>>();
        let next_cursor = if slice.len() == query.limit {
            slice.last().map(|item| item.session_id.clone())
        } else {
            None
        };
        Ok(SessionPage {
            items: slice,
            next_cursor,
        })
    }

    async fn find_sessions(
        &self,
        query: SessionSearchQuery,
        limit: usize,
    ) -> Result<Vec<SessionSummary>, AgentError> {
        let guard = self.inner.lock().await;
        let needle = match &query {
            SessionSearchQuery::IdPrefix(value)
            | SessionSearchQuery::TitleContains(value)
            | SessionSearchQuery::IdPrefixOrTitle(value) => value.to_lowercase(),
        };
        let mut items = guard
            .values()
            .filter_map(|stored| {
                let summary = &stored.summary;
                let matches = match &query {
                    SessionSearchQuery::IdPrefix(_) => {
                        summary.session_id.to_lowercase().starts_with(&needle)
                    }
                    SessionSearchQuery::TitleContains(_) => {
                        summary.title.to_lowercase().contains(&needle)
                    }
                    SessionSearchQuery::IdPrefixOrTitle(_) => {
                        summary.session_id.to_lowercase().starts_with(&needle)
                            || summary.title.to_lowercase().contains(&needle)
                    }
                };
                matches.then(|| summary.clone())
            })
            .take(limit)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(items)
    }

    async fn delete_session(&self, session_id: &str) -> Result<(), AgentError> {
        self.inner.lock().await.remove(session_id);
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryMemoryStorage {
    inner: Arc<Mutex<BTreeMap<String, BTreeMap<String, Memory>>>>,
}

#[async_trait]
impl MemoryStorage for InMemoryMemoryStorage {
    async fn search(
        &self,
        namespace: &str,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<Memory>, AgentError> {
        let lowered = query.to_lowercase();
        let guard = self.inner.lock().await;
        let mut items = guard
            .get(namespace)
            .into_iter()
            .flat_map(|memories| memories.values())
            .filter(|memory| memory.content.to_lowercase().contains(&lowered))
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items.truncate(top_k);
        Ok(items)
    }

    async fn list_recent(&self, namespace: &str, limit: usize) -> Result<Vec<Memory>, AgentError> {
        let guard = self.inner.lock().await;
        let mut items = guard
            .get(namespace)
            .into_iter()
            .flat_map(|memories| memories.values())
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items.truncate(limit);
        Ok(items)
    }

    async fn list_all(&self, namespace: &str) -> Result<Vec<Memory>, AgentError> {
        let guard = self.inner.lock().await;
        Ok(guard
            .get(namespace)
            .into_iter()
            .flat_map(|memories| memories.values())
            .cloned()
            .collect())
    }

    async fn upsert(&self, memory: Memory) -> Result<(), AgentError> {
        let mut guard = self.inner.lock().await;
        guard
            .entry(memory.namespace.clone())
            .or_default()
            .insert(memory.id.clone(), memory);
        Ok(())
    }

    async fn delete(&self, namespace: &str, id: &str) -> Result<(), AgentError> {
        if let Some(namespace_memories) = self.inner.lock().await.get_mut(namespace) {
            namespace_memories.remove(id);
        }
        Ok(())
    }
}
