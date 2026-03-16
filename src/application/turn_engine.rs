use crate::api::agent::AgentCore;
use crate::api::running_turn::RunningTurn;
use crate::api::running_turn::TurnController;
use crate::application::context_manager::ContextManager;
use crate::application::memory_manager::MemoryManager;
use crate::application::prompt_builder::PromptBuilder;
use crate::application::request_builder::ChatRequestBuilder;
use crate::application::request_builder::RequestBuildOptions;
use crate::application::session_service::SessionRuntime;
use crate::application::skill_resolver::SkillResolver;
use crate::application::tool_dispatcher::ToolDispatcher;
use crate::domain::content::ContentBlock;
use crate::domain::error::AgentError;
use crate::domain::error::AgentErrorCode;
use crate::domain::event::AgentEvent;
use crate::domain::event::AgentEventEnvelope;
use crate::domain::hook::TurnEndPayload;
use crate::domain::ledger::SKILL_INVOCATIONS_KEY;
use crate::domain::ledger::SessionEventPayload;
use crate::domain::ledger::ToolCallRecord;
use crate::domain::ledger::ToolResultRecord;
use crate::domain::model::ChatEvent;
use crate::domain::model::FinishReason;
use crate::domain::tool::RequestedToolBatch;
use crate::domain::turn::TurnOutcome;
use futures::FutureExt;
use futures::StreamExt;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::sync::CancellationToken;

struct EventEmitter {
    sender: mpsc::UnboundedSender<AgentEventEnvelope>,
    next_sequence: u64,
}

impl EventEmitter {
    fn new(sender: mpsc::UnboundedSender<AgentEventEnvelope>) -> Self {
        Self {
            sender,
            next_sequence: 1,
        }
    }

    fn emit(&mut self, event: AgentEvent) {
        let _ = self.sender.send(AgentEventEnvelope {
            sequence: self.next_sequence,
            event,
        });
        self.next_sequence += 1;
    }
}

pub struct TurnEngine;

impl TurnEngine {
    pub(crate) async fn spawn(
        core: Arc<AgentCore>,
        runtime: Arc<SessionRuntime>,
        content: Vec<ContentBlock>,
    ) -> Result<RunningTurn, AgentError> {
        Self::validate_input(&content)?;
        let turn_index = runtime.begin_turn().await?;
        let turn_id = format!("turn-{turn_index}");
        let (sender, receiver) = mpsc::unbounded_channel();
        let (outcome_tx, outcome_rx) = oneshot::channel();
        let controller = TurnController::new(CancellationToken::new());
        let task_controller = controller.clone();
        let task_runtime = runtime.clone();
        tokio::spawn(async move {
            let outcome = match AssertUnwindSafe(Self::run(
                core,
                runtime,
                content,
                turn_id,
                task_controller.token(),
                EventEmitter::new(sender),
            ))
            .catch_unwind()
            .await
            {
                Ok(outcome) => outcome,
                Err(_) => TurnOutcome::Panicked,
            };
            task_runtime.finish_turn().await;
            let _ = outcome_tx.send(outcome);
        });

        Ok(RunningTurn::new(
            UnboundedReceiverStream::new(receiver),
            controller,
            outcome_rx,
        ))
    }

    async fn run(
        core: Arc<AgentCore>,
        runtime: Arc<SessionRuntime>,
        content: Vec<ContentBlock>,
        turn_id: String,
        turn_cancel: CancellationToken,
        mut emitter: EventEmitter,
    ) -> TurnOutcome {
        emitter.emit(AgentEvent::TurnStarted {
            turn_id: turn_id.clone(),
        });

        let user_event = match core
            .session_service
            .append_event(
                core.clone(),
                runtime.clone(),
                SessionEventPayload::UserMessage {
                    content: content.clone(),
                },
            )
            .await
        {
            Ok(event) => event,
            Err(error) => return TurnOutcome::Failed(error),
        };

        let resolved_turn = match SkillResolver::resolve(&content, &core.skills, user_event.seq) {
            Ok(resolved) => resolved,
            Err(error) => return TurnOutcome::Failed(error),
        };
        if !resolved_turn.invoked_skill_names.is_empty() {
            let _ = core
                .session_service
                .append_event(
                    core.clone(),
                    runtime.clone(),
                    SessionEventPayload::Metadata {
                        key: SKILL_INVOCATIONS_KEY.to_string(),
                        value: serde_json::json!(
                            resolved_turn
                                .invoked_skill_names
                                .iter()
                                .map(|skill_name| serde_json::json!({ "skill_name": skill_name }))
                                .collect::<Vec<_>>()
                        ),
                    },
                )
                .await;
        }

        let dynamic_sections = match core
            .plugin_manager
            .on_turn_start(&runtime.session_id, &turn_id, &content)
            .await
        {
            Ok(sections) => sections,
            Err(error) => return TurnOutcome::Failed(error),
        };

        let turn_memories = match MemoryManager::prepare_turn_memories(
            core.memory_storage.as_ref(),
            &runtime.resolved_config.pinned.memory_namespace,
            &content,
            runtime.resolved_config.runtime_policy.memory_max_items,
        )
        .await
        {
            Ok(memories) => memories,
            Err(error) => return TurnOutcome::Failed(error),
        };
        runtime.set_last_turn_memories(turn_memories).await;

        let mut allow_tools = true;
        let mut total_tool_calls = 0usize;
        let mut assistant_output = String::new();

        loop {
            // tool loop 的每一轮都从 ledger 当前投影重建 request，确保 skill augmentation 和 tool result
            // 走同一条上下文构建链路，而不是拼接局部缓存。
            let context_state = runtime.context_state().await;
            let request_context = match ContextManager::build_request_context(
                &context_state,
                &resolved_turn.augmentation,
            ) {
                Ok(context) => context,
                Err(error) => return TurnOutcome::Failed(error),
            };
            let memory_state = runtime.memory_state().await;
            let prompt_memories = MemoryManager::merged_prompt_memories(
                &memory_state,
                runtime.resolved_config.runtime_policy.memory_max_items,
            );
            let rendered_prompt = PromptBuilder::render(
                &core.config,
                &runtime.resolved_config,
                &core.skills,
                &core.plugin_manager.descriptors(),
                &prompt_memories,
                &dynamic_sections,
            );
            let request = match ChatRequestBuilder::build(
                &rendered_prompt,
                &request_context,
                &core.tool_descriptors(),
                RequestBuildOptions { allow_tools },
                &runtime.model_info,
            ) {
                Ok(request) => request,
                Err(error) => return TurnOutcome::Failed(error),
            };

            let mut stream = match runtime.provider.chat(request, turn_cancel.clone()).await {
                Ok(stream) => stream,
                Err(error) => return TurnOutcome::Failed(error.into()),
            };

            let mut text_delta_buffer = String::new();
            let mut requested_tool_calls = Vec::new();
            let mut finish_reason = FinishReason::Error;
            while let Some(event) = stream.next().await {
                match event {
                    Ok(ChatEvent::TextDelta { text }) => {
                        assistant_output.push_str(&text);
                        text_delta_buffer.push_str(&text);
                        emitter.emit(AgentEvent::TextDelta { text });
                    }
                    Ok(ChatEvent::ReasoningDelta { text }) => {
                        emitter.emit(AgentEvent::ReasoningDelta { text });
                    }
                    Ok(ChatEvent::ToolCall {
                        call_id,
                        tool_name,
                        arguments,
                    }) => {
                        // 这里只收集一个完整批次，等 provider 用 `Done(ToolCalls)` 封口后再统一调度。
                        requested_tool_calls.push(crate::domain::tool::RequestedToolCall {
                            call_id,
                            tool_name,
                            requested_arguments: arguments,
                        });
                    }
                    Ok(ChatEvent::Done {
                        finish_reason: reason,
                        ..
                    }) => {
                        finish_reason = reason;
                        break;
                    }
                    Ok(ChatEvent::Error(error)) => {
                        return TurnOutcome::Failed(error.into());
                    }
                    Err(error) => {
                        if turn_cancel.is_cancelled() {
                            finish_reason = FinishReason::Cancelled;
                            break;
                        }
                        return TurnOutcome::Failed(error.into());
                    }
                }
            }

            if !text_delta_buffer.is_empty() {
                let status = if finish_reason == FinishReason::Cancelled {
                    crate::domain::ledger::MessageStatus::Incomplete
                } else {
                    crate::domain::ledger::MessageStatus::Complete
                };
                if let Err(error) = core
                    .session_service
                    .append_event(
                        core.clone(),
                        runtime.clone(),
                        SessionEventPayload::AssistantMessage {
                            content: vec![ContentBlock::text(text_delta_buffer)],
                            status,
                        },
                    )
                    .await
                {
                    return TurnOutcome::Failed(error);
                }
            }

            match finish_reason {
                FinishReason::Stop | FinishReason::Length => {
                    let outcome = TurnOutcome::Completed;
                    let _ = core
                        .plugin_manager
                        .on_turn_end(TurnEndPayload {
                            session_id: runtime.session_id.clone(),
                            turn_id: turn_id.clone(),
                            assistant_output: assistant_output.clone(),
                            tool_calls_count: total_tool_calls,
                            cancelled: false,
                            plugin_config: serde_json::Value::Null,
                        })
                        .await;
                    emitter.emit(AgentEvent::TurnFinished {
                        outcome: outcome.clone(),
                    });
                    return outcome;
                }
                FinishReason::Cancelled => {
                    let outcome = TurnOutcome::Cancelled;
                    let _ = core
                        .plugin_manager
                        .on_turn_end(TurnEndPayload {
                            session_id: runtime.session_id.clone(),
                            turn_id: turn_id.clone(),
                            assistant_output: assistant_output.clone(),
                            tool_calls_count: total_tool_calls,
                            cancelled: true,
                            plugin_config: serde_json::Value::Null,
                        })
                        .await;
                    emitter.emit(AgentEvent::TurnFinished {
                        outcome: outcome.clone(),
                    });
                    return outcome;
                }
                FinishReason::ToolCalls => {
                    if requested_tool_calls.is_empty() {
                        return TurnOutcome::Failed(AgentError::new(
                            AgentErrorCode::ProviderError,
                            "provider returned ToolCalls finish reason without tool calls",
                        ));
                    }
                    total_tool_calls += requested_tool_calls.len();
                    let dispatch_result = ToolDispatcher::dispatch_batch(
                        core.clone(),
                        runtime.clone(),
                        &turn_id,
                        RequestedToolBatch {
                            calls: requested_tool_calls,
                        },
                        turn_cancel.clone(),
                    )
                    .await;
                    for record in dispatch_result.records {
                        emitter.emit(AgentEvent::ToolCallStart {
                            call_id: record.call.call_id.clone(),
                            tool_name: record.call.tool_name.clone(),
                            arguments: record.call.effective_arguments.clone(),
                        });
                        if let Err(error) = core
                            .session_service
                            .append_event(
                                core.clone(),
                                runtime.clone(),
                                SessionEventPayload::ToolCall {
                                    call: ToolCallRecord {
                                        call_id: record.call_record.call_id.clone(),
                                        tool_name: record.call_record.tool_name.clone(),
                                        requested_arguments: record
                                            .call_record
                                            .requested_arguments
                                            .clone(),
                                        effective_arguments: record
                                            .call_record
                                            .effective_arguments
                                            .clone(),
                                    },
                                },
                            )
                            .await
                        {
                            return TurnOutcome::Failed(error);
                        }
                        if let Err(error) = core
                            .session_service
                            .append_event(
                                core.clone(),
                                runtime.clone(),
                                SessionEventPayload::ToolResult {
                                    result: ToolResultRecord {
                                        call_id: record.result_record.call_id.clone(),
                                        tool_name: record.result_record.tool_name.clone(),
                                        output: record.result_record.output.clone(),
                                    },
                                },
                            )
                            .await
                        {
                            return TurnOutcome::Failed(error);
                        }
                        emitter.emit(AgentEvent::ToolCallEnd {
                            call_id: record.call.call_id,
                            tool_name: record.call.tool_name,
                            output: record.result_record.output,
                        });
                        if let Some(reason) = record.abort_reason {
                            return TurnOutcome::Failed(AgentError::new(
                                AgentErrorCode::HookAborted,
                                reason,
                            ));
                        }
                    }

                    if dispatch_result.aborted_reason.is_some() {
                        return TurnOutcome::Failed(AgentError::new(
                            AgentErrorCode::HookAborted,
                            dispatch_result
                                .aborted_reason
                                .unwrap_or_else(|| "tool dispatch aborted".to_string()),
                        ));
                    }

                    if total_tool_calls
                        >= runtime
                            .resolved_config
                            .runtime_policy
                            .max_tool_calls_per_turn
                    {
                        // 达到上限后发一次禁用 tools 的收尾请求，让模型给出最终自然语言答复。
                        allow_tools = false;
                    }
                }
                FinishReason::Error => {
                    return TurnOutcome::Failed(AgentError::new(
                        AgentErrorCode::ProviderError,
                        "provider finished with error",
                    ));
                }
            }
        }
    }

    fn validate_input(content: &[ContentBlock]) -> Result<(), AgentError> {
        if content.is_empty() {
            return Err(AgentError::new(
                AgentErrorCode::InvalidInput,
                "turn input must contain at least one content block",
            ));
        }
        for block in content {
            if let ContentBlock::Image {
                mime_type,
                data_base64,
            } = block
            {
                if mime_type.trim().is_empty() || data_base64.trim().is_empty() {
                    return Err(AgentError::new(
                        AgentErrorCode::InvalidInput,
                        "image blocks require mime_type and data_base64",
                    ));
                }
            }
        }
        Ok(())
    }
}
