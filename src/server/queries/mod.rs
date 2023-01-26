mod weather;

pub use weather::{WeatherView, WeatherQuery, WeatherViewProjection};

use async_trait::async_trait;
use cqrs_es::{Aggregate, EventEnvelope, Query};
use std::marker::PhantomData;

#[derive(Debug, Default)]
pub struct TracingQuery<A: Aggregate> {
    marker: PhantomData<A>,
}

#[async_trait]
impl<A: Aggregate + std::fmt::Debug> Query<A> for TracingQuery<A> {
    #[tracing::instrument(level = "debug")]
    async fn dispatch(&self, aggregate_id: &str, events: &[EventEnvelope<A>]) {
        for event in events {
            match serde_json::to_string_pretty(&event.payload) {
                Ok(payload) => {
                    tracing::info!("EVENT_TRACE: {aggregate_id}-{}: {payload}", event.sequence);
                },

                Err(err) => {
                    let type_name = std::any::type_name::<A>();
                    tracing::error!(
                        "EVENT_TRACE: failed to convert {type_name} event to json: {err:?}"
                    );
                },
            }
        }
    }
}
