use runtime::{
    AssistantEvent, ConversationRuntime, PermissionMode, RuntimeError, Session, TurnSummary,
};

use crate::event_bus::{AgentSessionEvent, EventBus, SessionLifecycleEvent, TurnEvent};
use crate::tool_registry::{SdkToolExecutor, ToolRegistry};

/// An agent session that wraps the runtime and provides a high-level API.
///
/// `AgentSession` owns a `ConversationRuntime` and provides methods for
/// running turns, subscribing to events, and managing session state.
pub struct AgentSession {
    /// The model identifier being used.
    model: String,
    /// The system prompt.
    system_prompt: Vec<String>,
    /// The runtime instance.
    runtime: ConversationRuntime<DummyApiClient, SdkToolExecutor>,
    /// The underlying session state.
    session: Session,
    /// Event bus for subscribing to lifecycle events.
    event_bus: EventBus,
    /// Permission mode.
    permission_mode: PermissionMode,
}

/// A minimal API client used by the SDK when no real provider is configured.
/// Emits events but performs no actual API calls.
#[derive(Debug, Clone)]
pub struct DummyApiClient;

impl runtime::ApiClient for DummyApiClient {
    fn stream(
        &mut self,
        _request: runtime::ApiRequest,
    ) -> Result<Vec<AssistantEvent>, RuntimeError> {
        Err(RuntimeError::new(
            "SDK mode: no API client configured. \
             Provide a real ApiClient via AgentSessionBuilder.",
        ))
    }
}

impl AgentSession {
    /// Create a new agent session with an internally-managed event bus.
    ///
    /// Returns the session and an event bus you can subscribe to for events.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model: &str,
        system_prompt: Vec<String>,
        tool_registry: ToolRegistry,
        permission_mode: PermissionMode,
    ) -> Result<(Self, EventBus), String> {
        let session = Session::new();
        let mut event_bus = EventBus::new();
        let tool_executor = SdkToolExecutor::new(&tool_registry);

        let runtime = ConversationRuntime::new(
            session.clone(),
            DummyApiClient,
            tool_executor,
            PermissionPolicy::new(permission_mode),
            system_prompt.clone(),
        );

        event_bus.emit(AgentSessionEvent::SessionLifecycle(
            SessionLifecycleEvent::Created {
                session_id: session.session_id.clone(),
            },
        ));

        let returned_bus = event_bus.clone();
        Ok((
            Self {
                model: model.to_string(),
                system_prompt,
                runtime,
                session,
                event_bus,
                permission_mode,
            },
            returned_bus,
        ))
    }

    /// Get the session ID.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session.session_id
    }

    /// Get the model name.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Get the system prompt.
    #[must_use]
    pub fn system_prompt(&self) -> &[String] {
        &self.system_prompt
    }

    /// Get the permission mode.
    #[must_use]
    pub fn permission_mode(&self) -> PermissionMode {
        self.permission_mode
    }

    /// Get a reference to the underlying session.
    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Run a single turn with the given user input.
    pub fn run_turn(&mut self, input: &str) -> Result<TurnSummary, RuntimeError> {
        self.event_bus.emit(AgentSessionEvent::TurnStarted);

        let result = self.runtime.run_turn(input.to_string(), None);

        match &result {
            Ok(summary) => {
                self.event_bus
                    .emit(AgentSessionEvent::TurnCompleted(summary.clone()));
                self.event_bus
                    .emit(AgentSessionEvent::TurnEvent(TurnEvent::Completed(
                        summary.clone(),
                    )));
            }
            Err(e) => {
                self.event_bus.emit(AgentSessionEvent::Error(e.to_string()));
            }
        }

        result
    }

    /// Subscribe to session events.
    pub fn subscribe(&mut self) -> crate::EventSubscription {
        self.event_bus.subscribe()
    }

    /// Emit a lifecycle event manually.
    pub fn emit_event(&mut self, event: AgentSessionEvent) {
        self.event_bus.emit(event);
    }
}

use runtime::PermissionPolicy;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_registry::ToolRegistry;

    #[test]
    fn creates_session_with_valid_id() {
        let (session, _bus) = AgentSession::new(
            "claude-sonnet-4-6",
            vec!["You are a helpful assistant.".to_string()],
            ToolRegistry::new(),
            PermissionMode::DangerFullAccess,
        )
        .expect("session should create");

        assert!(!session.session_id().is_empty());
        assert_eq!(session.model(), "claude-sonnet-4-6");
    }

    #[test]
    fn run_turn_fails_with_dummy_client() {
        let (mut session, _bus) = AgentSession::new(
            "claude-sonnet-4-6",
            vec!["system".to_string()],
            ToolRegistry::new(),
            PermissionMode::DangerFullAccess,
        )
        .expect("session should create");

        let result = session.run_turn("hello");
        assert!(result.is_err(), "dummy client should fail");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("SDK mode"),
            "error should mention SDK mode: {err}"
        );
    }
}
