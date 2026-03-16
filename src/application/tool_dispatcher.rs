use crate::api::agent::AgentCore;
use crate::application::session_service::SessionRuntime;
use crate::domain::error::AgentErrorCode;
use crate::domain::hook::AfterToolUsePayload;
use crate::domain::ledger::ToolCallRecord;
use crate::domain::ledger::ToolResultRecord;
use crate::domain::tool::RequestedToolBatch;
use crate::domain::tool::ResolvedToolCall;
use crate::domain::tool::ToolInput;
use crate::domain::tool::ToolOutput;
use futures::future::join_all;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolExecutionRecord {
    pub call: ResolvedToolCall,
    pub call_record: ToolCallRecord,
    pub result_record: ToolResultRecord,
    pub abort_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolDispatchResult {
    pub records: Vec<ToolExecutionRecord>,
    pub aborted_reason: Option<String>,
}

pub struct ToolDispatcher;

impl ToolDispatcher {
    pub(crate) async fn dispatch_batch(
        core: Arc<AgentCore>,
        runtime: Arc<SessionRuntime>,
        turn_id: &str,
        batch: RequestedToolBatch,
        turn_cancel: CancellationToken,
    ) -> ToolDispatchResult {
        let mut immediate_results = Vec::new();
        let mut pending = Vec::<(
            usize,
            ResolvedToolCall,
            Arc<dyn crate::ports::tool::ToolHandler>,
        )>::new();

        for (index, requested) in batch.calls.into_iter().enumerate() {
            let Some(tool_registration) = core.tools.get(&requested.tool_name) else {
                let output =
                    ToolOutput::error(format!("tool `{}` is not registered", requested.tool_name));
                let call = ResolvedToolCall {
                    call_id: requested.call_id.clone(),
                    tool_name: requested.tool_name.clone(),
                    requested_arguments: requested.requested_arguments.clone(),
                    effective_arguments: requested.requested_arguments.clone(),
                    mutating: false,
                };
                immediate_results.push((
                    index,
                    ToolExecutionRecord {
                        call: call.clone(),
                        call_record: ToolCallRecord {
                            call_id: call.call_id.clone(),
                            tool_name: call.tool_name.clone(),
                            requested_arguments: call.requested_arguments.clone(),
                            effective_arguments: call.effective_arguments.clone(),
                        },
                        result_record: ToolResultRecord {
                            call_id: call.call_id.clone(),
                            tool_name: call.tool_name.clone(),
                            output,
                        },
                        abort_reason: Some("tool not found".to_string()),
                    },
                ));
                continue;
            };

            let effective_arguments = match core
                .plugin_manager
                .on_before_tool_use(
                    &runtime.session_id,
                    turn_id,
                    &requested.tool_name,
                    requested.requested_arguments.clone(),
                )
                .await
            {
                Ok(arguments) => arguments,
                Err(error) => {
                    // hook abort 不直接丢掉这次调用，而是落一个 synthetic error output，保证账本可审计。
                    let call = ResolvedToolCall {
                        call_id: requested.call_id.clone(),
                        tool_name: requested.tool_name.clone(),
                        requested_arguments: requested.requested_arguments.clone(),
                        effective_arguments: requested.requested_arguments.clone(),
                        mutating: tool_registration.descriptor.mutating,
                    };
                    immediate_results.push((
                        index,
                        ToolExecutionRecord {
                            call: call.clone(),
                            call_record: ToolCallRecord {
                                call_id: call.call_id.clone(),
                                tool_name: call.tool_name.clone(),
                                requested_arguments: call.requested_arguments.clone(),
                                effective_arguments: call.effective_arguments.clone(),
                            },
                            result_record: ToolResultRecord {
                                call_id: call.call_id.clone(),
                                tool_name: call.tool_name.clone(),
                                output: ToolOutput::error(error.message),
                            },
                            abort_reason: Some("before_tool_use aborted".to_string()),
                        },
                    ));
                    continue;
                }
            };

            let call = ResolvedToolCall {
                call_id: requested.call_id,
                tool_name: requested.tool_name,
                requested_arguments: requested.requested_arguments,
                effective_arguments,
                mutating: tool_registration.descriptor.mutating,
            };
            pending.push((index, call, tool_registration.handler.clone()));
        }

        let mut ordered_records = immediate_results;
        let mut cursor = 0;
        let mut batch_aborted_reason = None;
        while cursor < pending.len() {
            let is_mutating = pending[cursor].1.mutating;
            if is_mutating {
                // 有副作用的 tool 必须严格按模型给出的顺序串行执行，避免改写工作区时发生竞态。
                let record = Self::execute_call(
                    core.clone(),
                    runtime.clone(),
                    turn_id,
                    pending[cursor].1.clone(),
                    pending[cursor].2.clone(),
                    turn_cancel.clone(),
                )
                .await;
                if batch_aborted_reason.is_none() {
                    batch_aborted_reason = record.abort_reason.clone();
                }
                ordered_records.push((pending[cursor].0, record));
                cursor += 1;
                continue;
            }

            let start = cursor;
            while cursor < pending.len() && !pending[cursor].1.mutating {
                cursor += 1;
            }
            // 只读 tool 可以并发跑，但最终回放顺序仍然要恢复成模型原始顺序。
            let futures = pending[start..cursor]
                .iter()
                .map(|(_, call, handler)| {
                    Self::execute_call(
                        core.clone(),
                        runtime.clone(),
                        turn_id,
                        call.clone(),
                        handler.clone(),
                        turn_cancel.clone(),
                    )
                })
                .collect::<Vec<_>>();
            let results = join_all(futures).await;
            for (offset, record) in results.into_iter().enumerate() {
                if batch_aborted_reason.is_none() {
                    batch_aborted_reason = record.abort_reason.clone();
                }
                ordered_records.push((pending[start + offset].0, record));
            }
        }

        ordered_records.sort_by_key(|(index, _)| *index);
        ToolDispatchResult {
            records: ordered_records
                .into_iter()
                .map(|(_, record)| record)
                .collect(),
            aborted_reason: batch_aborted_reason,
        }
    }

    async fn execute_call(
        core: Arc<AgentCore>,
        runtime: Arc<SessionRuntime>,
        turn_id: &str,
        call: ResolvedToolCall,
        handler: Arc<dyn crate::ports::tool::ToolHandler>,
        turn_cancel: CancellationToken,
    ) -> ToolExecutionRecord {
        let output = if turn_cancel.is_cancelled() {
            ToolOutput {
                content: "[tool skipped: cancelled before execution]".to_string(),
                is_error: true,
                metadata: Some(serde_json::json!({
                    "synthetic": true,
                    "skipped": true,
                    "reason": "cancelled_before_execution",
                })),
            }
        } else {
            let timeout_cancel = CancellationToken::new();
            let started_at = Instant::now();
            // turn cancel 只阻止“尚未开始”的调用；真正执行中的 handler 只看独立的 timeout token。
            let result = timeout(
                std::time::Duration::from_millis(
                    runtime.resolved_config.runtime_policy.tool_timeout_ms,
                ),
                handler.call(
                    ToolInput {
                        call_id: call.call_id.clone(),
                        tool_name: call.tool_name.clone(),
                        arguments: call.effective_arguments.clone(),
                    },
                    timeout_cancel.clone(),
                ),
            )
            .await;
            let output = match result {
                Ok(Ok(output)) => output.truncate(),
                Ok(Err(error)) => ToolOutput::error(error.message),
                Err(_) => {
                    timeout_cancel.cancel();
                    ToolOutput::error("tool timed out")
                }
            };

            let after_payload = AfterToolUsePayload {
                session_id: runtime.session_id.clone(),
                turn_id: turn_id.to_string(),
                tool_name: call.tool_name.clone(),
                output: output.clone(),
                duration_ms: started_at.elapsed().as_millis(),
                success: !output.is_error,
                plugin_config: serde_json::Value::Null,
            };
            let abort_reason = core
                .plugin_manager
                .on_after_tool_use(after_payload)
                .await
                .err()
                .map(|error| error.message);
            if let Some(reason) = abort_reason {
                return ToolExecutionRecord {
                    call: call.clone(),
                    call_record: ToolCallRecord {
                        call_id: call.call_id.clone(),
                        tool_name: call.tool_name.clone(),
                        requested_arguments: call.requested_arguments.clone(),
                        effective_arguments: call.effective_arguments.clone(),
                    },
                    result_record: ToolResultRecord {
                        call_id: call.call_id.clone(),
                        tool_name: call.tool_name.clone(),
                        output,
                    },
                    abort_reason: Some(reason),
                };
            }
            output
        };

        let is_timeout = output.content == "tool timed out";
        let error_output = if is_timeout {
            ToolOutput {
                content: output.content,
                is_error: true,
                metadata: Some(serde_json::json!({
                    "reason": "timeout",
                    "code": AgentErrorCode::ToolTimeout,
                })),
            }
        } else {
            output
        };

        ToolExecutionRecord {
            call: call.clone(),
            call_record: ToolCallRecord {
                call_id: call.call_id.clone(),
                tool_name: call.tool_name.clone(),
                requested_arguments: call.requested_arguments.clone(),
                effective_arguments: call.effective_arguments.clone(),
            },
            result_record: ToolResultRecord {
                call_id: call.call_id.clone(),
                tool_name: call.tool_name.clone(),
                output: error_output,
            },
            abort_reason: None,
        }
    }
}
