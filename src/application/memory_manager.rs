use crate::domain::content::ContentBlock;
use crate::domain::memory::Memory;
use crate::domain::memory::SessionMemoryState;
use crate::ports::memory_storage::MemoryStorage;
use std::collections::HashSet;
use std::sync::Arc;

pub struct MemoryManager;

impl MemoryManager {
    pub async fn load_bootstrap_memories(
        storage: Option<&Arc<dyn MemoryStorage>>,
        namespace: &str,
        limit: usize,
    ) -> Result<Vec<Memory>, crate::domain::error::AgentError> {
        let Some(storage) = storage else {
            return Ok(Vec::new());
        };
        storage.list_recent(namespace, limit).await
    }

    pub async fn prepare_turn_memories(
        storage: Option<&Arc<dyn MemoryStorage>>,
        namespace: &str,
        content: &[ContentBlock],
        limit: usize,
    ) -> Result<Vec<Memory>, crate::domain::error::AgentError> {
        let Some(storage) = storage else {
            return Ok(Vec::new());
        };
        let query = content
            .iter()
            .filter_map(ContentBlock::explicit_text)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if query.is_empty() {
            return Ok(Vec::new());
        }
        storage.search(namespace, &query, limit).await
    }

    pub fn merged_prompt_memories(state: &SessionMemoryState, limit: usize) -> Vec<Memory> {
        let mut seen = HashSet::new();
        let mut merged = Vec::new();
        for memory in state
            .bootstrap_memories
            .iter()
            .chain(state.last_turn_memories.iter())
        {
            if seen.insert(memory.id.clone()) {
                merged.push(memory.clone());
            }
            if merged.len() >= limit {
                break;
            }
        }
        merged
    }
}
