use crate::application::prompt_builder::RenderedPrompt;
use crate::domain::content::Message;
use crate::domain::content::MessageRole;
use crate::domain::error::AgentError;
use crate::domain::error::AgentErrorCode;
use crate::domain::model::ChatRequest;
use crate::domain::model::ModelInfo;
use crate::domain::model::ModelMessage;
use crate::domain::model::ModelMessageRole;
use crate::domain::model::ProviderCapability;
use crate::domain::tool::ToolDescriptor;
use crate::domain::turn::RequestContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestBuildOptions {
    pub allow_tools: bool,
}

pub struct ChatRequestBuilder;

impl ChatRequestBuilder {
    pub fn build(
        rendered_prompt: &RenderedPrompt,
        request_context: &RequestContext,
        tool_descriptors: &[ToolDescriptor],
        options: RequestBuildOptions,
        model_info: &ModelInfo,
    ) -> Result<ChatRequest, AgentError> {
        let mut ordered_messages = request_context.pre_anchor_messages.clone();
        ordered_messages.push(request_context.anchor_message.clone());
        ordered_messages.extend(request_context.post_anchor_augmentations.clone());
        ordered_messages.extend(request_context.post_anchor_messages.clone());

        // 能力预检必须在 provider I/O 之前完成，这样模型不支持时可以直接返回结构化错误。
        if ordered_messages.iter().any(Self::message_uses_vision)
            && !model_info
                .capabilities
                .contains(&ProviderCapability::Vision)
        {
            return Err(AgentError::new(
                AgentErrorCode::ModelNotSupported,
                format!(
                    "model `{}` does not support vision input",
                    model_info.model_id
                ),
            ));
        }

        let tools = if options.allow_tools {
            if !tool_descriptors.is_empty()
                && !model_info
                    .capabilities
                    .contains(&ProviderCapability::ToolUse)
            {
                return Err(AgentError::new(
                    AgentErrorCode::ModelNotSupported,
                    format!("model `{}` does not support tool use", model_info.model_id),
                ));
            }
            tool_descriptors.to_vec()
        } else {
            Vec::new()
        };

        Ok(ChatRequest {
            model_id: model_info.model_id.clone(),
            system_prompt: rendered_prompt.system_prompt.clone(),
            messages: ordered_messages
                .into_iter()
                .map(Self::serialize_message)
                .collect(),
            tools,
            temperature: None,
            max_tokens: None,
            reasoning_effort: None,
        })
    }

    fn serialize_message(message: Message) -> ModelMessage {
        let role = match message.role {
            MessageRole::System => ModelMessageRole::System,
            MessageRole::User => ModelMessageRole::User,
            MessageRole::Assistant => ModelMessageRole::Assistant,
            MessageRole::Tool => ModelMessageRole::Tool,
        };
        ModelMessage {
            role,
            content: message.content,
            tool_call_id: message.tool_call_id,
        }
    }

    fn message_uses_vision(message: &Message) -> bool {
        message.content.iter().any(|block| block.is_image())
    }
}
