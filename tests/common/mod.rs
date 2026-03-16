#![allow(dead_code)]

use async_trait::async_trait;
use codex_codex::BeforeToolUsePatch;
use codex_codex::ChatEvent;
use codex_codex::ContentBlock;
use codex_codex::FinishReason;
use codex_codex::HookKind;
use codex_codex::HookOutcome;
use codex_codex::ModelInfo;
use codex_codex::Plugin;
use codex_codex::PluginDescriptor;
use codex_codex::ProviderCapability;
use codex_codex::ToolDescriptor;
use codex_codex::ToolHandler;
use codex_codex::ToolInput;
use codex_codex::ToolOutput;
use codex_codex::ports::model::ChatEventStream;
use codex_codex::ports::model::ModelProvider;
use futures::stream;
use serde_json::Value;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct TestProvider {
    provider_id: String,
    models: Vec<ModelInfo>,
    responses:
        Arc<Mutex<VecDeque<Vec<Result<ChatEvent, codex_codex::domain::model::ProviderError>>>>>,
    requests: Arc<Mutex<Vec<codex_codex::ChatRequest>>>,
}

impl TestProvider {
    pub fn new(
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        capabilities: impl IntoIterator<Item = ProviderCapability>,
        responses: Vec<Vec<Result<ChatEvent, codex_codex::domain::model::ProviderError>>>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            models: vec![ModelInfo {
                model_id: model_id.into(),
                display_name: "Test Model".to_string(),
                context_window: 32_000,
                capabilities: capabilities.into_iter().collect::<BTreeSet<_>>(),
            }],
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl ModelProvider for TestProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn models(&self) -> &[ModelInfo] {
        &self.models
    }

    async fn chat(
        &self,
        request: codex_codex::ChatRequest,
        _cancel: CancellationToken,
    ) -> Result<ChatEventStream, codex_codex::domain::model::ProviderError> {
        self.requests.lock().await.push(request);
        let events = self.responses.lock().await.pop_front().unwrap_or_else(|| {
            vec![Ok(ChatEvent::Done {
                finish_reason: FinishReason::Stop,
                usage: Default::default(),
            })]
        });
        Ok(Box::pin(stream::iter(events)))
    }
}

#[derive(Clone)]
pub struct RecordingTool {
    descriptor: ToolDescriptor,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl RecordingTool {
    pub fn new(name: &str, mutating: bool) -> Self {
        Self {
            descriptor: ToolDescriptor {
                name: name.to_string(),
                description: "records incoming arguments".to_string(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "value": { "type": "string" }
                    }
                }),
                mutating,
            },
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn calls(&self) -> Vec<Value> {
        self.calls.lock().await.clone()
    }
}

#[async_trait]
impl ToolHandler for RecordingTool {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    async fn call(
        &self,
        input: ToolInput,
        _cancel: CancellationToken,
    ) -> Result<ToolOutput, codex_codex::AgentError> {
        self.calls.lock().await.push(input.arguments.clone());
        Ok(ToolOutput {
            content: format!("tool:{}", input.arguments),
            is_error: false,
            metadata: None,
        })
    }
}

pub struct PatchArgsPlugin {
    descriptor: PluginDescriptor,
    replacement: Value,
}

impl PatchArgsPlugin {
    pub fn new(replacement: Value) -> Self {
        Self {
            descriptor: PluginDescriptor {
                id: "patch-args".to_string(),
                display_name: "Patch Args".to_string(),
                description: "rewrites tool arguments before execution".to_string(),
                tapped_hooks: vec![HookKind::BeforeToolUse],
            },
            replacement,
        }
    }
}

#[async_trait]
impl Plugin for PatchArgsPlugin {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    async fn on_before_tool_use(
        &self,
        _payload: codex_codex::domain::hook::BeforeToolUsePayload,
    ) -> Result<HookOutcome<BeforeToolUsePatch>, codex_codex::AgentError> {
        Ok(HookOutcome::ContinueWith(BeforeToolUsePatch {
            arguments: self.replacement.clone(),
        }))
    }
}

pub fn text_message(text: &str) -> Vec<ContentBlock> {
    vec![ContentBlock::text(text)]
}
