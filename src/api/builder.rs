use crate::api::agent::Agent;
use crate::api::agent::AgentCore;
use crate::api::agent::ModelRegistration;
use crate::api::agent::ToolRegistration;
use crate::application::plugin_manager::PluginEntry;
use crate::application::plugin_manager::PluginManager;
use crate::application::session_service::SessionService;
use crate::domain::config::AgentConfig;
use crate::domain::config::resolve_session_runtime_policy;
use crate::domain::error::AgentError;
use crate::domain::error::AgentErrorCode;
use crate::domain::skill::SkillDefinition;
use crate::ports::memory_storage::MemoryStorage;
use crate::ports::model::ModelProvider;
use crate::ports::plugin::Plugin;
use crate::ports::session_storage::SessionStorage;
use crate::ports::tool::ToolHandler;
use serde_json::Value;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub struct NoProvider;

#[derive(Debug, Clone, Copy)]
pub struct HasProvider;

pub struct AgentBuilder<S = NoProvider> {
    config: AgentConfig,
    providers: Vec<Arc<dyn ModelProvider>>,
    tools: Vec<Arc<dyn ToolHandler>>,
    skills: Vec<SkillDefinition>,
    plugins: Vec<(Arc<dyn Plugin>, Value)>,
    session_storage: Option<Arc<dyn SessionStorage>>,
    memory_storage: Option<Arc<dyn MemoryStorage>>,
    duplicate_session_storage: bool,
    duplicate_memory_storage: bool,
    _state: PhantomData<S>,
}

impl AgentBuilder<NoProvider> {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            providers: Vec::new(),
            tools: Vec::new(),
            skills: Vec::new(),
            plugins: Vec::new(),
            session_storage: None,
            memory_storage: None,
            duplicate_session_storage: false,
            duplicate_memory_storage: false,
            _state: PhantomData,
        }
    }
}

impl<S> AgentBuilder<S> {
    pub fn register_model_provider(
        mut self,
        provider: impl ModelProvider + 'static,
    ) -> AgentBuilder<HasProvider> {
        self.providers.push(Arc::new(provider));
        AgentBuilder {
            config: self.config,
            providers: self.providers,
            tools: self.tools,
            skills: self.skills,
            plugins: self.plugins,
            session_storage: self.session_storage,
            memory_storage: self.memory_storage,
            duplicate_session_storage: self.duplicate_session_storage,
            duplicate_memory_storage: self.duplicate_memory_storage,
            _state: PhantomData,
        }
    }

    pub fn register_tool_handler(mut self, tool: impl ToolHandler + 'static) -> Self {
        self.tools.push(Arc::new(tool));
        self
    }

    pub fn register_skill(mut self, skill: SkillDefinition) -> Self {
        self.skills.push(skill);
        self
    }

    pub fn register_plugin(mut self, plugin: impl Plugin + 'static, config: Value) -> Self {
        self.plugins.push((Arc::new(plugin), config));
        self
    }

    pub fn register_session_storage(mut self, storage: impl SessionStorage + 'static) -> Self {
        if self.session_storage.is_some() {
            self.duplicate_session_storage = true;
        } else {
            self.session_storage = Some(Arc::new(storage));
        }
        self
    }

    pub fn register_memory_storage(mut self, storage: impl MemoryStorage + 'static) -> Self {
        if self.memory_storage.is_some() {
            self.duplicate_memory_storage = true;
        } else {
            self.memory_storage = Some(Arc::new(storage));
        }
        self
    }
}

impl AgentBuilder<HasProvider> {
    pub fn build(self) -> Result<Agent, AgentError> {
        if self.duplicate_session_storage || self.duplicate_memory_storage {
            return Err(AgentError::new(
                AgentErrorCode::StorageDuplicate,
                "session storage and memory storage can each only be registered once",
            ));
        }

        let mut models = HashMap::new();
        let mut provider_ids = HashMap::<String, ()>::new();
        for provider in &self.providers {
            if provider_ids
                .insert(provider.provider_id().to_string(), ())
                .is_some()
            {
                return Err(AgentError::new(
                    AgentErrorCode::NameConflict,
                    format!("duplicate provider id `{}`", provider.provider_id()),
                ));
            }
            for model in provider.models() {
                // 规格要求 model_id 在所有 provider 之间全局唯一，避免 session resume 时路由歧义。
                if models.contains_key(&model.model_id) {
                    return Err(AgentError::new(
                        AgentErrorCode::NameConflict,
                        format!("duplicate model id `{}`", model.model_id),
                    ));
                }
                models.insert(
                    model.model_id.clone(),
                    ModelRegistration {
                        provider: provider.clone(),
                        model_info: model.clone(),
                    },
                );
            }
        }

        if self.config.default_model.trim().is_empty()
            || !models.contains_key(&self.config.default_model)
        {
            return Err(AgentError::new(
                AgentErrorCode::InvalidDefaultModel,
                "default_model must reference a registered model",
            ));
        }

        if self.config.tool_timeout_ms == 0
            || self.config.max_tool_calls_per_turn == 0
            || self.config.memory_checkpoint_interval == 0
            || self.config.memory_max_items == 0
            || !(0.0..1.0).contains(&self.config.compact_threshold)
        {
            return Err(AgentError::new(
                AgentErrorCode::InvalidConfig,
                "agent config contains invalid runtime limits",
            ));
        }

        let preview_runtime_policy = resolve_session_runtime_policy(
            &self.config,
            &crate::domain::config::SessionPinnedConfig {
                model_id: self.config.default_model.clone(),
                system_prompt_override: None,
                memory_namespace: self.config.memory_namespace.clone(),
            },
        );
        if !models.contains_key(&preview_runtime_policy.compact_model_id)
            || !models.contains_key(&preview_runtime_policy.memory_model_id)
        {
            return Err(AgentError::new(
                AgentErrorCode::InvalidConfig,
                "compact_model and memory_model must reference registered models",
            ));
        }

        let mut tools = HashMap::new();
        for handler in &self.tools {
            let descriptor = handler.descriptor().clone();
            if tools.contains_key(&descriptor.name) {
                return Err(AgentError::new(
                    AgentErrorCode::NameConflict,
                    format!("duplicate tool name `{}`", descriptor.name),
                ));
            }
            tools.insert(
                descriptor.name.clone(),
                ToolRegistration {
                    descriptor,
                    handler: handler.clone(),
                },
            );
        }

        let mut skills = HashMap::new();
        for skill in self.skills {
            if skills.contains_key(&skill.name) {
                return Err(AgentError::new(
                    AgentErrorCode::NameConflict,
                    format!("duplicate skill name `{}`", skill.name),
                ));
            }
            if let Some(missing_dependency) = skill
                .tool_dependencies
                .iter()
                .find(|dependency| !tools.contains_key(*dependency))
            {
                return Err(AgentError::new(
                    AgentErrorCode::SkillDependencyNotMet,
                    format!(
                        "skill `{}` depends on missing tool `{}`",
                        skill.name, missing_dependency
                    ),
                ));
            }
            skills.insert(skill.name.clone(), skill);
        }

        let mut plugin_ids = HashMap::<String, ()>::new();
        let mut plugin_entries = Vec::new();
        for (plugin, config) in self.plugins {
            let descriptor = plugin.descriptor().clone();
            if plugin_ids.insert(descriptor.id.clone(), ()).is_some() {
                return Err(AgentError::new(
                    AgentErrorCode::NameConflict,
                    format!("duplicate plugin id `{}`", descriptor.id),
                ));
            }
            plugin.initialize(&config)?;
            plugin_entries.push(PluginEntry {
                descriptor,
                config,
                plugin,
            });
        }

        Ok(Agent::new(AgentCore {
            config: self.config,
            models,
            tools,
            skills,
            plugin_manager: PluginManager::new(plugin_entries),
            session_storage: self.session_storage,
            memory_storage: self.memory_storage,
            session_service: SessionService::new(),
        }))
    }
}
