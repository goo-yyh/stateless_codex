use crate::domain::content::ContentBlock;
use crate::domain::error::AgentError;
use crate::domain::error::AgentErrorCode;
use crate::domain::skill::SkillDefinition;
use crate::domain::turn::ResolvedSkillInjection;
use crate::domain::turn::TurnAugmentation;
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTurnInput {
    pub invoked_skill_names: Vec<String>,
    pub augmentation: TurnAugmentation,
}

pub struct SkillResolver;

impl SkillResolver {
    pub fn resolve(
        user_input: &[ContentBlock],
        skills: &HashMap<String, SkillDefinition>,
        origin_user_seq: u64,
    ) -> Result<ResolvedTurnInput, AgentError> {
        let mut seen = HashSet::new();
        let mut invoked_skill_names = Vec::new();
        let mut skill_injections = Vec::new();

        // 规格要求只扫描顶层 Text 块，避免把文件内容里的 `/commit` 之类误判成显式 skill 调用。
        for block in user_input {
            let Some(text) = block.explicit_text() else {
                continue;
            };
            for token in text.split_whitespace() {
                let Some(skill_name) = token.strip_prefix('/') else {
                    continue;
                };
                if skill_name.is_empty() || !seen.insert(skill_name.to_string()) {
                    continue;
                }
                let skill = skills.get(skill_name).ok_or_else(|| {
                    AgentError::new(
                        AgentErrorCode::SkillNotFound,
                        format!("skill `{skill_name}` is not registered"),
                    )
                })?;
                invoked_skill_names.push(skill.name.clone());
                skill_injections.push(ResolvedSkillInjection {
                    skill_name: skill.name.clone(),
                    rendered_xml: format!(
                        "<skill name=\"{}\">\n{}\n</skill>",
                        skill.name, skill.prompt
                    ),
                });
            }
        }

        Ok(ResolvedTurnInput {
            invoked_skill_names,
            augmentation: TurnAugmentation {
                origin_user_seq,
                skill_injections,
            },
        })
    }
}
