use crate::api::agent::AgentCore;
use crate::application::context_manager::ContextManager;
use crate::application::memory_manager::MemoryManager;
use crate::domain::config::ResolvedSessionConfig;
use crate::domain::config::SessionConfig;
use crate::domain::config::resolve_session_pinned_config;
use crate::domain::config::resolve_session_runtime_policy;
use crate::domain::content::ContentBlock;
use crate::domain::error::AgentError;
use crate::domain::error::AgentErrorCode;
use crate::domain::ledger::LedgerEvent;
use crate::domain::ledger::SESSION_PROFILE_KEY;
use crate::domain::ledger::SessionEventPayload;
use crate::domain::ledger::SessionLedger;
use crate::domain::memory::SessionMemoryState;
use crate::domain::session::ContextState;
use crate::domain::session::SessionSummary;
use chrono::Utc;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleState {
    Running,
    ShuttingDown,
    Shutdown,
}

#[derive(Clone)]
enum SessionSlotState {
    Empty,
    Reserved {
        reservation_id: String,
        session_id: String,
    },
    Active(Arc<SessionRuntime>),
}

#[derive(Clone)]
struct AgentControlState {
    lifecycle: LifecycleState,
    slot: SessionSlotState,
}

#[derive(Clone)]
struct AgentControl {
    inner: Arc<Mutex<AgentControlState>>,
}

impl AgentControl {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AgentControlState {
                lifecycle: LifecycleState::Running,
                slot: SessionSlotState::Empty,
            })),
        }
    }

    fn reserve_slot(&self, session_id: String) -> Result<String, AgentError> {
        let mut guard = self.inner.lock().expect("agent control poisoned");
        if guard.lifecycle != LifecycleState::Running {
            return Err(AgentError::new(
                AgentErrorCode::AgentShutdown,
                "agent is shutting down",
            ));
        }
        if !matches!(guard.slot, SessionSlotState::Empty) {
            return Err(AgentError::new(
                AgentErrorCode::SessionBusy,
                "another session is already active",
            ));
        }
        let reservation_id = Uuid::new_v4().to_string();
        // 用 reservation 先占坑，避免异步初始化失败时误释放掉后续并发路径写入的活跃会话。
        guard.slot = SessionSlotState::Reserved {
            reservation_id: reservation_id.clone(),
            session_id,
        };
        Ok(reservation_id)
    }

    fn commit_slot(
        &self,
        reservation_id: &str,
        runtime: Arc<SessionRuntime>,
    ) -> Result<(), AgentError> {
        let mut guard = self.inner.lock().expect("agent control poisoned");
        match &guard.slot {
            SessionSlotState::Reserved {
                reservation_id: current_id,
                ..
            } if current_id == reservation_id => {
                guard.slot = SessionSlotState::Active(runtime);
                Ok(())
            }
            _ => Err(AgentError::new(
                AgentErrorCode::SessionBusy,
                "session slot reservation was lost",
            )),
        }
    }

    fn active_runtime(&self) -> Option<Arc<SessionRuntime>> {
        let guard = self.inner.lock().expect("agent control poisoned");
        match &guard.slot {
            SessionSlotState::Active(runtime) => Some(runtime.clone()),
            SessionSlotState::Empty | SessionSlotState::Reserved { .. } => None,
        }
    }

    fn rollback_slot(&self, reservation_id: &str) {
        let mut guard = self.inner.lock().expect("agent control poisoned");
        if matches!(
            &guard.slot,
            SessionSlotState::Reserved {
                reservation_id: current_id,
                ..
            } if current_id == reservation_id
        ) {
            guard.slot = SessionSlotState::Empty;
        }
    }

    fn release_session(&self, session_id: &str) {
        let mut guard = self.inner.lock().expect("agent control poisoned");
        if matches!(
            &guard.slot,
            SessionSlotState::Active(runtime) if runtime.session_id == session_id
        ) {
            guard.slot = SessionSlotState::Empty;
        }
    }

    fn active_session_id(&self) -> Option<String> {
        let guard = self.inner.lock().expect("agent control poisoned");
        match &guard.slot {
            SessionSlotState::Reserved { session_id, .. } => Some(session_id.clone()),
            SessionSlotState::Active(runtime) => Some(runtime.session_id.clone()),
            SessionSlotState::Empty => None,
        }
    }

    fn begin_shutdown(&self) {
        let mut guard = self.inner.lock().expect("agent control poisoned");
        guard.lifecycle = LifecycleState::ShuttingDown;
    }

    fn finish_shutdown(&self) {
        let mut guard = self.inner.lock().expect("agent control poisoned");
        guard.lifecycle = LifecycleState::Shutdown;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SessionRuntimeState {
    pub ledger: SessionLedger,
    pub context_state: ContextState,
    pub memory_state: SessionMemoryState,
    pub summary: SessionSummary,
    pub turn_index: u64,
    pub active_turn: bool,
    pub closed: bool,
}

#[derive(Clone)]
pub struct SessionRuntime {
    pub session_id: String,
    pub resolved_config: ResolvedSessionConfig,
    pub provider: Arc<dyn crate::ports::model::ModelProvider>,
    pub model_info: crate::domain::model::ModelInfo,
    state: Arc<AsyncMutex<SessionRuntimeState>>,
}

impl SessionRuntime {
    pub async fn begin_turn(&self) -> Result<u64, AgentError> {
        let mut guard = self.state.lock().await;
        if guard.closed {
            return Err(AgentError::new(
                AgentErrorCode::SessionNotFound,
                "session is already closed",
            ));
        }
        if guard.active_turn {
            return Err(AgentError::new(
                AgentErrorCode::TurnBusy,
                "another turn is already running",
            ));
        }
        guard.active_turn = true;
        guard.turn_index += 1;
        Ok(guard.turn_index)
    }

    pub async fn finish_turn(&self) {
        self.state.lock().await.active_turn = false;
    }

    pub async fn summary(&self) -> SessionSummary {
        self.state.lock().await.summary.clone()
    }

    pub async fn ledger(&self) -> SessionLedger {
        self.state.lock().await.ledger.clone()
    }

    pub async fn context_state(&self) -> ContextState {
        self.state.lock().await.context_state.clone()
    }

    pub async fn memory_state(&self) -> SessionMemoryState {
        self.state.lock().await.memory_state.clone()
    }

    pub async fn set_last_turn_memories(&self, memories: Vec<crate::domain::memory::Memory>) {
        self.state.lock().await.memory_state.last_turn_memories = memories;
    }

    pub async fn mark_closed(&self) {
        self.state.lock().await.closed = true;
    }
}

#[derive(Clone)]
pub struct SessionService {
    control: AgentControl,
}

impl SessionService {
    pub fn new() -> Self {
        Self {
            control: AgentControl::new(),
        }
    }

    pub fn active_session_id(&self) -> Option<String> {
        self.control.active_session_id()
    }

    pub(crate) async fn new_session(
        &self,
        core: Arc<AgentCore>,
        session_config: SessionConfig,
    ) -> Result<Arc<SessionRuntime>, AgentError> {
        let session_id = Uuid::new_v4().to_string();
        let reservation_id = self.control.reserve_slot(session_id.clone())?;

        let result = async {
            let pinned = resolve_session_pinned_config(&core.config, session_config)?;
            let runtime_policy = resolve_session_runtime_policy(&core.config, &pinned);
            let model_registration = core.model(&pinned.model_id).ok_or_else(|| {
                AgentError::new(
                    AgentErrorCode::InvalidDefaultModel,
                    format!("unknown model `{}`", pinned.model_id),
                )
            })?;

            let bootstrap_memories = MemoryManager::load_bootstrap_memories(
                core.memory_storage.as_ref(),
                &pinned.memory_namespace,
                runtime_policy.memory_max_items,
            )
            .await?;

            let summary = SessionSummary {
                session_id: session_id.clone(),
                title: "Untitled session".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                message_count: 0,
            };
            let runtime = Arc::new(SessionRuntime {
                session_id: session_id.clone(),
                resolved_config: ResolvedSessionConfig {
                    pinned: pinned.clone(),
                    runtime_policy,
                },
                provider: model_registration.provider.clone(),
                model_info: model_registration.model_info.clone(),
                state: Arc::new(AsyncMutex::new(SessionRuntimeState {
                    ledger: SessionLedger::new(),
                    context_state: ContextState::default(),
                    memory_state: SessionMemoryState {
                        bootstrap_memories,
                        last_turn_memories: Vec::new(),
                    },
                    summary,
                    turn_index: 0,
                    active_turn: false,
                    closed: false,
                })),
            });

            // `session_profile` 是恢复时的唯一 pinned config 事实源，必须作为第一条 durable 事件写入。
            self.append_event(
                core.clone(),
                runtime.clone(),
                SessionEventPayload::Metadata {
                    key: SESSION_PROFILE_KEY.to_string(),
                    value: serde_json::to_value(pinned).map_err(|error| {
                        AgentError::new(
                            AgentErrorCode::InvalidConfig,
                            format!("failed to serialize session profile: {error}"),
                        )
                    })?,
                },
            )
            .await?;

            core.plugin_manager
                .on_session_start(&session_id, &runtime.resolved_config.pinned.model_id)
                .await?;

            self.control.commit_slot(&reservation_id, runtime.clone())?;
            Ok(runtime)
        }
        .await;

        if result.is_err() {
            self.control.rollback_slot(&reservation_id);
        }
        result
    }

    pub(crate) async fn resume_session(
        &self,
        core: Arc<AgentCore>,
        session_id: String,
    ) -> Result<Arc<SessionRuntime>, AgentError> {
        let storage = core.session_storage.as_ref().ok_or_else(|| {
            AgentError::new(
                AgentErrorCode::SessionNotFound,
                "session storage is not registered",
            )
        })?;
        let reservation_id = self.control.reserve_slot(session_id.clone())?;

        let result = async {
            let events = storage.load_events(&session_id).await?;
            let ledger = SessionLedger::with_events(events);
            let pinned = Self::load_pinned_config(&ledger)?;
            let runtime_policy = resolve_session_runtime_policy(&core.config, &pinned);
            let model_registration = core.model(&pinned.model_id).ok_or_else(|| {
                AgentError::new(
                    AgentErrorCode::InvalidDefaultModel,
                    format!("unknown model `{}`", pinned.model_id),
                )
            })?;
            let bootstrap_memories = MemoryManager::load_bootstrap_memories(
                core.memory_storage.as_ref(),
                &pinned.memory_namespace,
                runtime_policy.memory_max_items,
            )
            .await?;
            let summary = Self::project_summary(&session_id, &ledger);
            let context_state = ContextManager::rebuild_visible_messages(&ledger);
            let runtime = Arc::new(SessionRuntime {
                session_id: session_id.clone(),
                resolved_config: ResolvedSessionConfig {
                    pinned: pinned.clone(),
                    runtime_policy,
                },
                provider: model_registration.provider.clone(),
                model_info: model_registration.model_info.clone(),
                state: Arc::new(AsyncMutex::new(SessionRuntimeState {
                    ledger,
                    context_state,
                    memory_state: SessionMemoryState {
                        bootstrap_memories,
                        last_turn_memories: Vec::new(),
                    },
                    summary,
                    turn_index: 0,
                    active_turn: false,
                    closed: false,
                })),
            });

            core.plugin_manager
                .on_session_start(&session_id, &runtime.resolved_config.pinned.model_id)
                .await?;
            self.control.commit_slot(&reservation_id, runtime.clone())?;
            Ok(runtime)
        }
        .await;

        if result.is_err() {
            self.control.rollback_slot(&reservation_id);
        }
        result
    }

    pub(crate) async fn close_session(
        &self,
        core: Arc<AgentCore>,
        runtime: Arc<SessionRuntime>,
    ) -> Result<(), AgentError> {
        if runtime.state.lock().await.active_turn {
            return Err(AgentError::new(
                AgentErrorCode::TurnBusy,
                "cannot close a session while a turn is still running",
            ));
        }
        let summary = runtime.summary().await;
        core.plugin_manager
            .on_session_end(&runtime.session_id, summary.message_count)
            .await?;
        runtime.mark_closed().await;
        self.control.release_session(&runtime.session_id);
        Ok(())
    }

    pub(crate) async fn shutdown(&self, core: Arc<AgentCore>) -> Result<(), AgentError> {
        self.control.begin_shutdown();
        if let Some(runtime) = self.control.active_runtime() {
            if !runtime.state.lock().await.active_turn {
                self.close_session(core.clone(), runtime).await?;
            }
        }
        core.plugin_manager.shutdown_all()?;
        self.control.finish_shutdown();
        Ok(())
    }

    pub(crate) async fn append_event(
        &self,
        core: Arc<AgentCore>,
        runtime: Arc<SessionRuntime>,
        payload: SessionEventPayload,
    ) -> Result<LedgerEvent, AgentError> {
        let mut guard = runtime.state.lock().await;
        // 账本是唯一事实源，summary/context 都必须由追加后的 ledger 投影出来，而不是各自独立更新。
        let event = guard.ledger.append_payload(payload);
        guard.summary = Self::project_summary(&runtime.session_id, &guard.ledger);
        guard.context_state = ContextManager::rebuild_visible_messages(&guard.ledger);
        if let Some(storage) = &core.session_storage {
            storage
                .append_event(&runtime.session_id, &event, &guard.summary)
                .await?;
        }
        Ok(event)
    }

    fn load_pinned_config(
        ledger: &SessionLedger,
    ) -> Result<crate::domain::config::SessionPinnedConfig, AgentError> {
        let payload = ledger
            .events()
            .iter()
            .find_map(|event| match &event.payload {
                SessionEventPayload::Metadata { key, value } if key == SESSION_PROFILE_KEY => {
                    Some(value.clone())
                }
                _ => None,
            })
            .ok_or_else(|| {
                AgentError::new(
                    AgentErrorCode::SessionNotFound,
                    "session profile metadata is missing",
                )
            })?;
        serde_json::from_value(payload).map_err(|error| {
            AgentError::new(
                AgentErrorCode::InvalidConfig,
                format!("failed to deserialize session profile: {error}"),
            )
        })
    }

    fn project_summary(session_id: &str, ledger: &SessionLedger) -> SessionSummary {
        let created_at = ledger
            .events()
            .first()
            .map(|event| event.timestamp)
            .unwrap_or_else(Utc::now);
        let updated_at = ledger
            .events()
            .last()
            .map(|event| event.timestamp)
            .unwrap_or(created_at);
        SessionSummary {
            session_id: session_id.to_string(),
            title: Self::project_title(ledger),
            created_at,
            updated_at,
            message_count: ledger.message_count(),
        }
    }

    fn project_title(ledger: &SessionLedger) -> String {
        for event in ledger.events() {
            if let SessionEventPayload::UserMessage { content } = &event.payload {
                for block in content {
                    match block {
                        ContentBlock::Text { text } => {
                            let title = Self::normalize_text_title(text);
                            if !title.is_empty() {
                                return title;
                            }
                        }
                        ContentBlock::FileContent { text, .. } => {
                            let title = text.lines().next().unwrap_or("").trim().to_string();
                            if !title.is_empty() {
                                return title;
                            }
                        }
                        ContentBlock::Image { .. } => {}
                    }
                }
            }
        }
        "Untitled session".to_string()
    }

    fn normalize_text_title(text: &str) -> String {
        let first_line = text.lines().next().unwrap_or("").trim();
        if first_line.is_empty() {
            return String::new();
        }
        let filtered = first_line
            .split_whitespace()
            .filter(|token| !token.starts_with('/'))
            .collect::<Vec<_>>()
            .join(" ");
        filtered.trim().to_string()
    }
}
