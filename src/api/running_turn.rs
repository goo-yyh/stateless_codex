use crate::domain::event::AgentEventEnvelope;
use crate::domain::turn::TurnOutcome;
use tokio::sync::oneshot;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct TurnController {
    cancel_token: CancellationToken,
}

impl TurnController {
    pub fn new(cancel_token: CancellationToken) -> Self {
        Self { cancel_token }
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    pub(crate) fn token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }
}

pub struct RunningTurn {
    events: UnboundedReceiverStream<AgentEventEnvelope>,
    controller: TurnController,
    outcome_rx: oneshot::Receiver<TurnOutcome>,
}

impl RunningTurn {
    pub(crate) fn new(
        events: UnboundedReceiverStream<AgentEventEnvelope>,
        controller: TurnController,
        outcome_rx: oneshot::Receiver<TurnOutcome>,
    ) -> Self {
        Self {
            events,
            controller,
            outcome_rx,
        }
    }

    pub fn events_mut(&mut self) -> &mut UnboundedReceiverStream<AgentEventEnvelope> {
        &mut self.events
    }

    pub fn controller(&self) -> TurnController {
        self.controller.clone()
    }

    pub async fn join(self) -> TurnOutcome {
        self.outcome_rx.await.unwrap_or(TurnOutcome::Panicked)
    }
}
