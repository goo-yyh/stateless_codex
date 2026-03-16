use crate::domain::error::AgentError;
use crate::domain::memory::Memory;
use async_trait::async_trait;

#[async_trait]
pub trait MemoryStorage: Send + Sync {
    async fn search(
        &self,
        namespace: &str,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<Memory>, AgentError>;

    async fn list_recent(&self, namespace: &str, limit: usize) -> Result<Vec<Memory>, AgentError>;

    async fn list_all(&self, namespace: &str) -> Result<Vec<Memory>, AgentError>;

    async fn upsert(&self, memory: Memory) -> Result<(), AgentError>;

    async fn delete(&self, namespace: &str, id: &str) -> Result<(), AgentError>;
}
