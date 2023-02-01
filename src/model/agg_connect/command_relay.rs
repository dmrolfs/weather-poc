use super::CommandEnvelope;
use cqrs_es::{Aggregate, CqrsFramework, EventStore};
use std::fmt::{self, Debug};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub struct CommandRelay<A, ES>
where
    A: Aggregate,
    A::Command: Debug,
    ES: EventStore<A>,
{
    command_rx: mpsc::Receiver<CommandEnvelope<A>>,
    aggregate: Arc<CqrsFramework<A, ES>>,
}

impl<A, ES> fmt::Debug for CommandRelay<A, ES>
where
    A: Aggregate,
    A::Command: Debug,
    ES: EventStore<A>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandRelay").finish()
    }
}

impl<A, ES> CommandRelay<A, ES>
where
    A: Aggregate,
    A::Command: Debug,
    ES: EventStore<A>,
{
    pub fn new(
        aggregate: Arc<CqrsFramework<A, ES>>, command_rx: mpsc::Receiver<CommandEnvelope<A>>,
    ) -> Self {
        Self { command_rx, aggregate }
    }
}

impl<A, ES> CommandRelay<A, ES>
where
    A: Aggregate + 'static,
    A::Command: Debug + Clone + Send + Sync,
    ES: EventStore<A> + Send + 'static,
    <ES as EventStore<A>>::AC: Send,
{
    pub fn run(self) -> JoinHandle<()> {
        tokio::spawn(async move { self.do_run().await })
    }

    async fn do_run(mut self) {
        while let Some(command) = self.command_rx.recv().await {
            let (agg_id, cmd, meta) = command.as_parts();
            match self.aggregate.execute_with_metadata(&agg_id, cmd, meta).await {
                Ok(()) => tracing::debug!(?command, "command relayed to {}", A::aggregate_type()),
                Err(error) => {
                    tracing::error!(
                        ?error,
                        ?command,
                        "failed to relay command to {}",
                        A::aggregate_type()
                    )
                },
            }
        }
    }
}
