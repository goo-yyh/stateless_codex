mod common;

use stateless_codex::AgentBuilder;
use stateless_codex::AgentConfig;
use stateless_codex::ChatEvent;
use stateless_codex::FinishReason;
use stateless_codex::MessageStatus;
use stateless_codex::SessionStorage;
use stateless_codex::domain::event::AgentEvent;
use stateless_codex::domain::ledger::SessionEventPayload;
use stateless_codex::support::in_memory::InMemorySessionStorage;
use common::PatchArgsPlugin;
use common::RecordingTool;
use common::TestProvider;
use common::text_message;
use tokio_stream::StreamExt;

fn base_config() -> AgentConfig {
    AgentConfig {
        default_model: "gpt-test".to_string(),
        memory_namespace: "workspace".to_string(),
        ..AgentConfig::default()
    }
}

#[tokio::test]
async fn tool_call_arguments_use_effective_arguments_and_events_are_sequenced() {
    let provider = TestProvider::new(
        "provider",
        "gpt-test",
        [
            stateless_codex::ProviderCapability::ToolUse,
            stateless_codex::ProviderCapability::Streaming,
        ],
        vec![
            vec![
                Ok(ChatEvent::ToolCall {
                    call_id: "call-1".to_string(),
                    tool_name: "echo".to_string(),
                    arguments: serde_json::json!({ "value": "raw" }),
                }),
                Ok(ChatEvent::Done {
                    finish_reason: FinishReason::ToolCalls,
                    usage: Default::default(),
                }),
            ],
            vec![
                Ok(ChatEvent::TextDelta {
                    text: "done".to_string(),
                }),
                Ok(ChatEvent::Done {
                    finish_reason: FinishReason::Stop,
                    usage: Default::default(),
                }),
            ],
        ],
    );
    let storage = InMemorySessionStorage::default();
    let tool = RecordingTool::new("echo", false);
    let tool_probe = tool.clone();
    let agent = AgentBuilder::new(base_config())
        .register_model_provider(provider)
        .register_tool_handler(tool)
        .register_plugin(
            PatchArgsPlugin::new(serde_json::json!({ "value": "patched" })),
            serde_json::Value::Null,
        )
        .register_session_storage(storage.clone())
        .build()
        .expect("agent should build");

    let session = agent
        .new_session(Default::default())
        .await
        .expect("session should be created");
    let mut turn = session
        .send_message(text_message("run the tool"))
        .await
        .expect("turn should start");

    let mut events = Vec::new();
    while let Some(event) = turn.events_mut().next().await {
        events.push(event);
    }
    let outcome = turn.join().await;

    assert_eq!(events[0].sequence, 1);
    assert!(matches!(events[0].event, AgentEvent::TurnStarted { .. }));
    let tool_start = events
        .iter()
        .find_map(|event| match &event.event {
            AgentEvent::ToolCallStart { arguments, .. } => Some(arguments.clone()),
            _ => None,
        })
        .expect("tool start event should be emitted");
    assert_eq!(tool_start, serde_json::json!({ "value": "patched" }));
    assert_eq!(outcome, stateless_codex::TurnOutcome::Completed);

    let tool_calls = tool_probe.calls().await;
    assert_eq!(tool_calls, vec![serde_json::json!({ "value": "patched" })]);

    let persisted = storage
        .load_events(session.session_id())
        .await
        .expect("events should be persisted");
    let tool_call = persisted
        .iter()
        .find_map(|event| match &event.payload {
            SessionEventPayload::ToolCall { call } => Some(call.clone()),
            _ => None,
        })
        .expect("tool call should be recorded");
    assert_eq!(
        tool_call.requested_arguments,
        serde_json::json!({ "value": "raw" })
    );
    assert_eq!(
        tool_call.effective_arguments,
        serde_json::json!({ "value": "patched" })
    );
    assert!(persisted.iter().any(|event| matches!(
        &event.payload,
        SessionEventPayload::AssistantMessage { status, .. } if *status == MessageStatus::Complete
    )));

    session.close().await.expect("close should succeed");
}
