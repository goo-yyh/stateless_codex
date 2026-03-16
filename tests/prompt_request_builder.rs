use stateless_codex::AgentConfig;
use stateless_codex::ContentBlock;
use stateless_codex::RequestContext;
use stateless_codex::ResolvedSessionConfig;
use stateless_codex::SessionPinnedConfig;
use stateless_codex::SkillDefinition;
use stateless_codex::application::prompt_builder::PromptBuilder;
use stateless_codex::application::request_builder::ChatRequestBuilder;
use stateless_codex::application::request_builder::RequestBuildOptions;
use stateless_codex::domain::config::SessionRuntimePolicy;
use stateless_codex::domain::content::Message;
use stateless_codex::domain::error::AgentErrorCode;
use stateless_codex::domain::model::ModelInfo;
use stateless_codex::domain::plugin::PluginDescriptor;
use stateless_codex::domain::tool::ToolDescriptor;
use std::collections::BTreeSet;
use std::collections::HashMap;

fn resolved_session_config() -> ResolvedSessionConfig {
    ResolvedSessionConfig {
        pinned: SessionPinnedConfig {
            model_id: "gpt-test".to_string(),
            system_prompt_override: None,
            memory_namespace: "workspace".to_string(),
        },
        runtime_policy: SessionRuntimePolicy {
            tool_timeout_ms: 120_000,
            compact_threshold: 0.8,
            compact_model_id: "gpt-test".to_string(),
            compact_prompt: "compact".to_string(),
            max_tool_calls_per_turn: 50,
            memory_model_id: "gpt-test".to_string(),
            memory_checkpoint_interval: 10,
            memory_max_items: 20,
        },
    }
}

fn request_context_with(block: ContentBlock) -> RequestContext {
    RequestContext {
        pre_anchor_messages: Vec::new(),
        anchor_message: Message::user(vec![block]),
        post_anchor_augmentations: Vec::new(),
        post_anchor_messages: Vec::new(),
    }
}

#[test]
fn skill_list_only_contains_allow_implicit_invocation() {
    let mut skills = HashMap::new();
    skills.insert(
        "commit".to_string(),
        SkillDefinition {
            name: "commit".to_string(),
            display_name: "Commit".to_string(),
            description: "create a commit".to_string(),
            prompt: "make a commit".to_string(),
            tool_dependencies: Vec::new(),
            allow_implicit_invocation: true,
        },
    );
    skills.insert(
        "internal".to_string(),
        SkillDefinition {
            name: "internal".to_string(),
            display_name: "Internal".to_string(),
            description: "hidden".to_string(),
            prompt: "hidden".to_string(),
            tool_dependencies: Vec::new(),
            allow_implicit_invocation: false,
        },
    );

    let prompt = PromptBuilder::render(
        &AgentConfig {
            default_model: "gpt-test".to_string(),
            system_instructions: vec!["follow the rules".to_string()],
            memory_namespace: "workspace".to_string(),
            ..AgentConfig::default()
        },
        &resolved_session_config(),
        &skills,
        &[PluginDescriptor {
            id: "plugin".to_string(),
            display_name: "Plugin".to_string(),
            description: "visible".to_string(),
            tapped_hooks: Vec::new(),
        }],
        &[],
        &[],
    );

    assert!(prompt.system_prompt.contains("/commit"));
    assert!(!prompt.system_prompt.contains("/internal"));
}

#[test]
fn tool_definitions_only_exist_in_request_tools_and_file_content_stays_text_only() {
    let request = ChatRequestBuilder::build(
        &stateless_codex::application::prompt_builder::RenderedPrompt {
            system_prompt: "base prompt".to_string(),
        },
        &request_context_with(ContentBlock::FileContent {
            file_name: Some("notes.md".to_string()),
            media_type: Some("text/markdown".to_string()),
            text: "hello".to_string(),
        }),
        &[ToolDescriptor {
            name: "echo".to_string(),
            description: "echo".to_string(),
            parameters_schema: serde_json::json!({"type":"object"}),
            mutating: false,
        }],
        RequestBuildOptions { allow_tools: true },
        &ModelInfo {
            model_id: "gpt-test".to_string(),
            display_name: "Test".to_string(),
            context_window: 32_000,
            capabilities: [stateless_codex::ProviderCapability::ToolUse]
                .into_iter()
                .collect::<BTreeSet<_>>(),
        },
    )
    .expect("file content should not require vision support");

    assert_eq!(request.tools.len(), 1);
    assert!(!request.system_prompt.contains("echo"));
    assert_eq!(request.messages.len(), 1);
}

#[test]
fn image_requires_vision_capability() {
    let error = ChatRequestBuilder::build(
        &stateless_codex::application::prompt_builder::RenderedPrompt {
            system_prompt: "base prompt".to_string(),
        },
        &request_context_with(ContentBlock::Image {
            mime_type: "image/png".to_string(),
            data_base64: "abcd".to_string(),
        }),
        &[],
        RequestBuildOptions { allow_tools: false },
        &ModelInfo {
            model_id: "gpt-test".to_string(),
            display_name: "Test".to_string(),
            context_window: 32_000,
            capabilities: BTreeSet::new(),
        },
    )
    .expect_err("vision should be required for image input");

    assert_eq!(error.code, AgentErrorCode::ModelNotSupported);
}
