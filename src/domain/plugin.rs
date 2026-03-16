use crate::domain::hook::HookKind;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginDescriptor {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub tapped_hooks: Vec<HookKind>,
}
