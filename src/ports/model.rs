use crate::domain::model::ChatEvent;
use crate::domain::model::ChatRequest;
use crate::domain::model::ModelInfo;
use crate::domain::model::ProviderError;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

pub type ChatEventStream = Pin<Box<dyn Stream<Item = Result<ChatEvent, ProviderError>> + Send>>;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn models(&self) -> &[ModelInfo];
    async fn chat(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
    ) -> Result<ChatEventStream, ProviderError>;
}
