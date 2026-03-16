use crate::domain::content::Message;
use crate::domain::error::AgentError;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedSkillInjection {
    pub skill_name: String,
    pub rendered_xml: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnAugmentation {
    pub origin_user_seq: u64,
    pub skill_injections: Vec<ResolvedSkillInjection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestContext {
    pub pre_anchor_messages: Vec<Message>,
    pub anchor_message: Message,
    pub post_anchor_augmentations: Vec<Message>,
    pub post_anchor_messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    Cancelled,
    Failed(AgentError),
    Panicked,
}
