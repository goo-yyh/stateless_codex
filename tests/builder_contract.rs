mod common;

use codex_codex::AgentBuilder;
use codex_codex::AgentConfig;
use codex_codex::AgentErrorCode;
use codex_codex::SkillDefinition;
use common::RecordingTool;
use common::TestProvider;

fn base_config() -> AgentConfig {
    AgentConfig {
        default_model: "gpt-test".to_string(),
        memory_namespace: "workspace".to_string(),
        ..AgentConfig::default()
    }
}

#[test]
fn rejects_invalid_default_model() {
    let provider = TestProvider::new("provider", "other-model", [], vec![]);
    let result = AgentBuilder::new(base_config())
        .register_model_provider(provider)
        .build();

    let error = result
        .err()
        .expect("builder should reject an unknown default model");
    assert_eq!(error.code, AgentErrorCode::InvalidDefaultModel);
}

#[test]
fn rejects_skill_missing_tool_dependency() {
    let provider = TestProvider::new("provider", "gpt-test", [], vec![]);
    let skill = SkillDefinition {
        name: "commit".to_string(),
        display_name: "Commit".to_string(),
        description: "create a commit".to_string(),
        prompt: "make a commit".to_string(),
        tool_dependencies: vec!["git_commit".to_string()],
        allow_implicit_invocation: true,
    };

    let result = AgentBuilder::new(base_config())
        .register_model_provider(provider)
        .register_skill(skill)
        .build();

    let error = result
        .err()
        .expect("builder should reject missing skill dependencies");
    assert_eq!(error.code, AgentErrorCode::SkillDependencyNotMet);
}

#[test]
fn rejects_duplicate_tool_name() {
    let provider = TestProvider::new("provider", "gpt-test", [], vec![]);
    let result = AgentBuilder::new(base_config())
        .register_model_provider(provider)
        .register_tool_handler(RecordingTool::new("echo", false))
        .register_tool_handler(RecordingTool::new("echo", false))
        .build();

    let error = result
        .err()
        .expect("builder should reject duplicate tool names");
    assert_eq!(error.code, AgentErrorCode::NameConflict);
}
