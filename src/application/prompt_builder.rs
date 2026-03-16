use crate::domain::config::AgentConfig;
use crate::domain::config::ResolvedSessionConfig;
use crate::domain::memory::Memory;
use crate::domain::plugin::PluginDescriptor;
use crate::domain::skill::SkillDefinition;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPrompt {
    pub system_prompt: String,
}

pub struct PromptBuilder;

impl PromptBuilder {
    pub fn render(
        agent_config: &AgentConfig,
        resolved_config: &ResolvedSessionConfig,
        skills: &HashMap<String, SkillDefinition>,
        plugins: &[PluginDescriptor],
        memories: &[Memory],
        dynamic_sections: &[String],
    ) -> RenderedPrompt {
        let mut sections = Vec::new();

        let instructions = agent_config
            .system_instructions
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        if !instructions.is_empty() {
            sections.push(format!(
                "<system_instructions>\n{}\n</system_instructions>",
                instructions.join("\n\n")
            ));
        }

        if let Some(override_prompt) = &resolved_config.pinned.system_prompt_override {
            sections.push(format!(
                "<system_prompt_override>\n{}\n</system_prompt_override>",
                override_prompt
            ));
        }

        if let Some(personality) = agent_config.personality.as_ref().map(|value| value.trim()) {
            if !personality.is_empty() {
                sections.push(format!(
                    "<personality_spec>\n{}\n</personality_spec>",
                    personality
                ));
            }
        }

        let allow_implicit = skills
            .values()
            .filter(|skill| skill.allow_implicit_invocation)
            .collect::<Vec<_>>();
        if !allow_implicit.is_empty() {
            let mut lines = vec![
                "## Skills".to_string(),
                "The following skills can be suggested to the user, but they are only injected after the user explicitly types `/skill_name`.".to_string(),
            ];
            lines.extend(
                allow_implicit
                    .into_iter()
                    .map(|skill| format!("- /{}: {}", skill.name, skill.description)),
            );
            sections.push(lines.join("\n"));
        }

        if !plugins.is_empty() {
            let mut lines = vec!["## Active Plugins".to_string()];
            lines.extend(
                plugins
                    .iter()
                    .map(|plugin| format!("- {}: {}", plugin.id, plugin.description)),
            );
            sections.push(lines.join("\n"));
        }

        if !memories.is_empty() {
            let mut lines = vec!["## Memories".to_string()];
            lines.extend(
                memories
                    .iter()
                    .map(|memory| format!("- {}: {}", memory.id, memory.content)),
            );
            sections.push(lines.join("\n"));
        }

        if let Some(environment_context) = &agent_config.environment_context {
            sections.push(environment_context.serialize_to_xml());
        }

        sections.extend(
            dynamic_sections
                .iter()
                .map(|section| section.trim())
                .filter(|section| !section.is_empty())
                .map(ToOwned::to_owned),
        );

        RenderedPrompt {
            system_prompt: sections.join("\n\n"),
        }
    }
}
