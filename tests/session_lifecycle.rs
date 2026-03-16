mod common;

use codex_codex::AgentBuilder;
use codex_codex::AgentConfig;
use codex_codex::SessionStorage;
use codex_codex::domain::ledger::SESSION_PROFILE_KEY;
use codex_codex::domain::ledger::SessionEventPayload;
use codex_codex::support::in_memory::InMemorySessionStorage;
use common::TestProvider;

fn base_config() -> AgentConfig {
    AgentConfig {
        default_model: "gpt-test".to_string(),
        memory_namespace: "workspace".to_string(),
        ..AgentConfig::default()
    }
}

#[tokio::test]
async fn new_session_writes_session_profile_first() {
    let provider = TestProvider::new("provider", "gpt-test", [], vec![]);
    let storage = InMemorySessionStorage::default();
    let agent = AgentBuilder::new(base_config())
        .register_model_provider(provider)
        .register_session_storage(storage.clone())
        .build()
        .expect("agent should build");

    let session = agent
        .new_session(Default::default())
        .await
        .expect("session should be created");
    let events = storage
        .load_events(session.session_id())
        .await
        .expect("events should be persisted");

    assert!(matches!(
        &events[0].payload,
        SessionEventPayload::Metadata { key, .. } if key == SESSION_PROFILE_KEY
    ));
    session.close().await.expect("close should succeed");
}

#[tokio::test]
async fn session_busy_when_active_session_exists() {
    let provider = TestProvider::new("provider", "gpt-test", [], vec![]);
    let agent = AgentBuilder::new(base_config())
        .register_model_provider(provider)
        .build()
        .expect("agent should build");

    let session = agent
        .new_session(Default::default())
        .await
        .expect("first session should be created");
    let second = agent.new_session(Default::default()).await;

    assert!(second.is_err(), "second session should be rejected");
    session.close().await.expect("close should succeed");
}

#[tokio::test]
async fn resume_reuses_pinned_session_profile() {
    let provider = TestProvider::new("provider", "gpt-test", [], vec![]);
    let storage = InMemorySessionStorage::default();
    let agent = AgentBuilder::new(base_config())
        .register_model_provider(provider)
        .register_session_storage(storage.clone())
        .build()
        .expect("agent should build");

    let session = agent
        .new_session(Default::default())
        .await
        .expect("session should be created");
    let session_id = session.session_id().to_string();
    session.close().await.expect("close should succeed");

    let resumed = agent
        .resume_session(session_id.clone())
        .await
        .expect("resume should succeed");
    let events = storage
        .load_events(&session_id)
        .await
        .expect("events should still be persisted");

    assert!(matches!(
        &events[0].payload,
        SessionEventPayload::Metadata { key, .. } if key == SESSION_PROFILE_KEY
    ));
    assert_eq!(resumed.session_id(), session_id);
    resumed.close().await.expect("close should succeed");
}
