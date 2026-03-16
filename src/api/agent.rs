use crate::api::session::SessionCatalog;
use crate::api::session::SessionHandle;
use crate::application::plugin_manager::PluginManager;
use crate::application::session_service::SessionService;
use crate::domain::config::AgentConfig;
use crate::domain::error::AgentError;
use crate::domain::skill::SkillDefinition;
use crate::domain::tool::ToolDescriptor;
use crate::ports::memory_storage::MemoryStorage;
use crate::ports::model::ModelProvider;
use crate::ports::session_storage::SessionStorage;
use crate::ports::tool::ToolHandler;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct ModelRegistration {
    pub provider: Arc<dyn ModelProvider>,
    pub model_info: crate::domain::model::ModelInfo,
}

#[derive(Clone)]
pub(crate) struct ToolRegistration {
    pub descriptor: ToolDescriptor,
    pub handler: Arc<dyn ToolHandler>,
}

#[derive(Clone)]
pub(crate) struct AgentCore {
    pub config: AgentConfig,
    pub models: HashMap<String, ModelRegistration>,
    pub tools: HashMap<String, ToolRegistration>,
    pub skills: HashMap<String, SkillDefinition>,
    pub plugin_manager: PluginManager,
    pub session_storage: Option<Arc<dyn SessionStorage>>,
    pub memory_storage: Option<Arc<dyn MemoryStorage>>,
    pub session_service: SessionService,
}

impl AgentCore {
    pub fn model(&self, model_id: &str) -> Option<&ModelRegistration> {
        self.models.get(model_id)
    }

    pub fn tool_descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools
            .values()
            .map(|registration| registration.descriptor.clone())
            .collect()
    }
}

#[derive(Clone)]
pub struct Agent {
    pub(crate) core: Arc<AgentCore>,
}

impl Agent {
    pub(crate) fn new(core: AgentCore) -> Self {
        Self {
            core: Arc::new(core),
        }
    }

    pub async fn new_session(
        &self,
        session_config: crate::domain::config::SessionConfig,
    ) -> Result<SessionHandle, AgentError> {
        let runtime = self
            .core
            .session_service
            .new_session(self.core.clone(), session_config)
            .await?;
        Ok(SessionHandle::new(self.core.clone(), runtime))
    }

    pub async fn resume_session(
        &self,
        session_id: impl Into<String>,
    ) -> Result<SessionHandle, AgentError> {
        let runtime = self
            .core
            .session_service
            .resume_session(self.core.clone(), session_id.into())
            .await?;
        Ok(SessionHandle::new(self.core.clone(), runtime))
    }

    pub fn session_catalog(&self) -> SessionCatalog {
        SessionCatalog::new(self.core.clone())
    }

    pub async fn shutdown(&self) -> Result<(), AgentError> {
        self.core.session_service.shutdown(self.core.clone()).await
    }
}
