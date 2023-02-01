use super::{CommandEnvelope, EventEnvelope};
use async_trait::async_trait;
use cqrs_es::{Aggregate, Query};
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Debug};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct EventBroadcastQuery<A: Aggregate> {
    sender: broadcast::Sender<EventEnvelope<A>>,
}

impl<A: Aggregate> fmt::Debug for EventBroadcastQuery<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventBroadcast").finish()
    }
}

impl<A> EventBroadcastQuery<A>
where
    A: Aggregate + 'static,
{
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe<S, C>(
        &self, target_tx: mpsc::Sender<CommandEnvelope<S>>, convert_fn: C,
    ) -> EventSubscriber<A, S, C>
    where
        S: Aggregate + 'static,
        <S as Aggregate>::Command: Debug + Clone + Send + Sync,
        C: FnMut(EventEnvelope<A>) -> Vec<S::Command> + Send + Sync + 'static,
    {
        EventSubscriber::new(self.sender.clone(), target_tx, convert_fn)
    }
}

#[async_trait]
impl<A: Aggregate> Query<A> for EventBroadcastQuery<A> {
    #[tracing::instrument(level = "debug", skip(events))]
    async fn dispatch(&self, aggregate_id: &str, events: &[cqrs_es::EventEnvelope<A>]) {
        let b_events = events
            .iter()
            .map(|envelope| EventEnvelope::from_cqrs(aggregate_id, envelope));
        for event in b_events {
            match self.sender.send(event.clone()) {
                Ok(nr_subscribers) => {
                    tracing::debug!("Event broadcasted to {nr_subscribers}: {event:?}")
                },
                Err(error) => tracing::error!(?error, "failed to broadcast event: {event:?}"),
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum SubscribeCommand {
    Add {
        subscriber_id: String,
        publisher_ids: HashSet<String>,
    },
    Remove {
        subscriber_id: String,
    },
}

pub struct EventSubscriber<P, S, C>
where
    P: Aggregate,
    S: Aggregate,
    S::Command: Debug + Clone,
    C: FnMut(EventEnvelope<P>) -> Vec<S::Command> + Send + Sync,
{
    subscriber_admin_tx: mpsc::Sender<SubscribeCommand>,
    subscriber_admin_rx: mpsc::Receiver<SubscribeCommand>,
    publisher_subscribers: HashMap<String, HashSet<String>>,
    event_tx: broadcast::Sender<EventEnvelope<P>>,
    event_rx: broadcast::Receiver<EventEnvelope<P>>,
    target_tx: mpsc::Sender<CommandEnvelope<S>>,
    convert_event_fn: C,
}

impl<P, S, C> EventSubscriber<P, S, C>
where
    P: Aggregate + 'static,
    S: Aggregate + 'static,
    <S as Aggregate>::Command: Debug + Clone + Send + Sync,
    C: FnMut(EventEnvelope<P>) -> Vec<S::Command> + Send + Sync + 'static,
{
    pub fn new(
        event_tx: broadcast::Sender<EventEnvelope<P>>, target_tx: mpsc::Sender<CommandEnvelope<S>>,
        convert_event_fn: C,
    ) -> Self {
        let (subscriber_admin_tx, subscriber_admin_rx) = mpsc::channel(num_cpus::get());
        let event_rx = event_tx.subscribe();
        Self {
            subscriber_admin_tx,
            subscriber_admin_rx,
            publisher_subscribers: Default::default(),
            event_tx,
            event_rx,
            target_tx,
            convert_event_fn,
        }
    }

    pub fn event_rx(&self) -> broadcast::Receiver<EventEnvelope<P>> {
        self.event_tx.subscribe()
    }

    pub fn subscriber_admin_tx(&self) -> mpsc::Sender<SubscribeCommand> {
        self.subscriber_admin_tx.clone()
    }

    pub fn run(self) -> JoinHandle<()> {
        tokio::spawn(async move { self.do_run().await })
    }

    async fn do_run(mut self) {
        loop {
            tokio::select! {
                cmd = self.subscriber_admin_rx.recv() => match cmd {
                    Some(SubscribeCommand::Add { subscriber_id, publisher_ids }) => self.add_subscriber(subscriber_id, publisher_ids),
                    Some(SubscribeCommand::Remove { subscriber_id }) => self.remove_subscriber(&subscriber_id),
                    None => {
                        tracing::info!("event broadcast subscriber command channel closed - completing");
                        break;
                    },
                },

                event_envelope = self.event_rx.recv() => {
                    match event_envelope {
                        Ok(envelope) => self.handle_event(envelope).await,
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::info!("event broadcast channel closed - stopping");
                            break;
                        },
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!("broadcast channel lagged - skipped {skipped} evevnts");
                        },
                    }
                },

                else => {
                    tracing::info!("event feed closed - breaking...");
                    break;
                }
            }
        }
    }

    fn add_subscriber(&mut self, subscriber_id: String, publisher_ids: HashSet<String>) {
        for pid in publisher_ids {
            self.publisher_subscribers
                .entry(pid)
                .and_modify(|subscribers| {
                    subscribers.insert(subscriber_id.clone());
                })
                .or_insert(maplit::hashset! { subscriber_id.clone() });
        }
    }

    fn remove_subscriber(&mut self, subscriber_id: &str) {
        let mut nr_subscriptions = 0;
        for (publisher_id, subscribers) in self.publisher_subscribers.iter_mut() {
            if subscribers.remove(subscriber_id) {
                nr_subscriptions += 1;
            }

            tracing::info!("{publisher_id} event broadcast removed {subscriber_id} from {nr_subscriptions} subscriptions.");
        }
    }
}

impl<P, S, C> EventSubscriber<P, S, C>
where
    P: Aggregate,
    S: Aggregate,
    S::Command: Debug + Clone,
    C: FnMut(EventEnvelope<P>) -> Vec<S::Command> + Send + Sync,
{
    async fn handle_event(&mut self, envelope: EventEnvelope<P>) {
        if let Some(subscribers) = self.publisher_subscribers.get(envelope.publisher_id()) {
            let metadata = envelope.metadata().clone();
            let commands = (self.convert_event_fn)(envelope);
            for subscriber_id in subscribers {
                self.send_event_commands(subscriber_id, &commands, metadata.clone()).await;
            }
        }
    }

    async fn send_event_commands(
        &self, subscriber_id: &str, commands: &[S::Command], metadata: HashMap<String, String>,
    ) {
        for cmd in commands {
            let cmd = cmd.clone();
            let cmd_envelope =
                CommandEnvelope::new_with_metadata(subscriber_id, cmd.clone(), metadata.clone());
            let outcome = self.target_tx.send(cmd_envelope).await;
            if let Err(error) = outcome {
                tracing::error!(
                    ?error, command=?cmd, ?metadata,
                    "event subscriber forward to {}[{subscriber_id}] failed because the channel is closed!", S::aggregate_type()
                );
            }
        }
    }
}

impl<P, S, C> fmt::Debug for EventSubscriber<P, S, C>
where
    P: Aggregate,
    S: Aggregate,
    S::Command: Debug + Clone,
    C: FnMut(EventEnvelope<P>) -> Vec<S::Command> + Send + Sync,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventSubscriber")
            .field("from", &P::aggregate_type())
            .field("to", &S::aggregate_type())
            .finish()
    }
}
