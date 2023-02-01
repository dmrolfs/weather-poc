mod command_relay;
mod event_broadcast;

pub use command_relay::CommandRelay;
pub use event_broadcast::{EventBroadcastQuery, EventSubscriber, SubscribeCommand};

use cqrs_es::Aggregate;
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::sync::Arc;

pub struct EventEnvelope<A: Aggregate> {
    inner: Arc<EventEnvelopeRef<A>>,
}

impl<A: Aggregate> Clone for EventEnvelope<A> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<A: Aggregate> EventEnvelope<A> {
    pub fn new(aggregate_id: impl Into<String>, event: A::Event) -> Self {
        Self::new_with_metadata(aggregate_id, event, HashMap::new())
    }

    pub fn new_with_metadata(
        aggregate_id: impl Into<String>, event: A::Event, metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            inner: Arc::new(EventEnvelopeRef {
                publisher_id: aggregate_id.into(),
                event,
                metadata,
            }),
        }
    }

    pub fn from_cqrs(
        aggregate_id: impl Into<String>, envelope: &cqrs_es::EventEnvelope<A>,
    ) -> Self {
        Self::new_with_metadata(
            aggregate_id,
            envelope.payload.clone(),
            envelope.metadata.clone(),
        )
    }

    pub fn as_parts(&self) -> (String, A::Event, HashMap<String, String>) {
        (
            self.publisher_id().to_string(),
            self.payload().clone(),
            self.metadata().clone(),
        )
    }

    pub fn publisher_id(&self) -> &str {
        self.inner.publisher_id.as_str()
    }

    pub fn payload(&self) -> &A::Event {
        &self.inner.event
    }

    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.inner.metadata
    }
}

struct EventEnvelopeRef<A: Aggregate> {
    pub publisher_id: String,
    pub event: A::Event,
    pub metadata: HashMap<String, String>,
}

impl<A: Aggregate> fmt::Debug for EventEnvelope<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventEnvelope")
            .field("publisher_id", &self.inner.publisher_id)
            .field("payload", &self.inner.event)
            .field("metadata", &self.inner.metadata)
            .finish()
    }
}

pub struct CommandEnvelope<A>
where
    A: Aggregate,
    A::Command: Debug,
{
    inner: Arc<CommandEnvelopeRef<A>>,
}

impl<A> Clone for CommandEnvelope<A>
where
    A: Aggregate,
    A::Command: Debug,
{
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<A> CommandEnvelope<A>
where
    A: Aggregate,
    A::Command: Debug,
{
    pub fn new(aggregate_id: impl Into<String>, command: A::Command) -> Self {
        Self::new_with_metadata(aggregate_id, command, HashMap::new())
    }

    pub fn new_with_metadata(
        aggregate_id: impl Into<String>, command: A::Command, metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            inner: Arc::new(CommandEnvelopeRef {
                target_id: aggregate_id.into(),
                command,
                metadata,
            }),
        }
    }

    pub fn target_id(&self) -> &str {
        self.inner.target_id.as_str()
    }

    pub fn payload(&self) -> &A::Command {
        &self.inner.command
    }

    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.inner.metadata
    }
}

impl<A> CommandEnvelope<A>
where
    A: Aggregate,
    A::Command: Debug + Clone,
{
    pub fn as_parts(&self) -> (String, A::Command, HashMap<String, String>) {
        (
            self.target_id().to_string(),
            self.payload().clone(),
            self.metadata().clone(),
        )
    }
}

#[derive(Debug)]
struct CommandEnvelopeRef<A>
where
    A: Aggregate,
    A::Command: Debug,
{
    pub target_id: String,
    pub command: A::Command,
    pub metadata: HashMap<String, String>,
}

impl<A> fmt::Debug for CommandEnvelope<A>
where
    A: Aggregate,
    A::Command: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandEnvelope")
            .field("target_id", &self.inner.target_id)
            .field("payload", &self.inner.command)
            .field("metadata", &self.inner.metadata)
            .finish()
    }
}
