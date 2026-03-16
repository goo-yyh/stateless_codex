use crate::domain::error::AgentError;
use crate::domain::tool::ToolDescriptor;
use crate::domain::tool::ToolInput;
use crate::domain::tool::ToolOutput;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn descriptor(&self) -> &ToolDescriptor;
    async fn call(
        &self,
        input: ToolInput,
        cancel: CancellationToken,
    ) -> Result<ToolOutput, AgentError>;
}
