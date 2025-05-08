//! Crate with amplifier ingester component
use std::sync::Arc;

use eyre::Context as _;
use futures::StreamExt as _;
use infrastructure::interfaces::consumer::{AckKind, Consumer, QueueMessage};
use infrastructure::interfaces::publisher::QueueMsgId as _;
use relayer_amplifier_api_integration::amplifier_api::requests::{self, WithTrailingSlash};
use relayer_amplifier_api_integration::amplifier_api::types::{Event, PublishEventsRequest};
use relayer_amplifier_api_integration::amplifier_api::{self, AmplifierApiClient};

// TODO: adjust based on metrics
const CONCURRENCY_SCALE_FACTOR: usize = 4;

/// Consumes events queue and sends it to include to amplifier api
pub struct Ingester<EventQueueConsumer> {
    ampf_client: AmplifierApiClient,
    event_queue_consumer: Arc<EventQueueConsumer>,
    concurrent_queue_items: usize,
    chain: String,
}

impl<EventQueueConsumer> Ingester<EventQueueConsumer>
where
    EventQueueConsumer: Consumer<amplifier_api::types::Event>,
{
    /// create ingester
    pub fn new(
        amplifier_client: AmplifierApiClient,
        event_queue_consumer: EventQueueConsumer,
        chain: String,
    ) -> Self {
        let event_queue_consumer = Arc::new(event_queue_consumer);
        let num_cpus = num_cpus::get();
        let concurrent_queue_items = num_cpus
            .checked_mul(CONCURRENCY_SCALE_FACTOR)
            .unwrap_or(num_cpus);
        Self {
            ampf_client: amplifier_client,
            event_queue_consumer,
            concurrent_queue_items,
            chain,
        }
    }

    /// process queue message
    pub async fn process_queue_msg<Msg: QueueMessage<Event>>(&self, mut queue_msg: Msg) {
        let chain_with_trailing_slash = WithTrailingSlash::new(self.chain.clone());

        let event = queue_msg.decoded().clone();
        tracing::info!(%event, "processing");
        let payload = PublishEventsRequest {
            events: vec![event.clone()],
        };

        let result: eyre::Result<()> = async {
            let request = requests::PostEvents::builder()
                .payload(&payload)
                .chain(&chain_with_trailing_slash)
                .build();

            let request = self
                .ampf_client
                .build_request(&request)
                .wrap_err("could not build amplifier request")?;

            tracing::debug!(?request, "request sending");

            let response = request
                .execute()
                .await
                .wrap_err("could not send amplifier request")?;

            tracing::debug!("reading response");

            let response = response
                .json()
                .await
                .map_err(|err| eyre::Report::new(err).wrap_err("amplifier api failed"))?
                .map_err(|err| eyre::Report::new(err).wrap_err("failed to decode response"))?;

            tracing::debug!(?response, "response from amplifier api");

            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                if let Err(err) = queue_msg.ack(AckKind::Ack).await {
                    tracing::error!(%event, %err, "could not ack message");
                }

                tracing::info!(event_id = %event.id(), "event ack'ed");
            }
            Err(err) => {
                tracing::error!(%event, %err, "error during task processing");
                if let Err(err) = queue_msg.ack(AckKind::Nak).await {
                    tracing::error!(%event, %err, "could not nak message");
                }
            }
        }
    }

    /// consume queue messages and ingest to amplifier api
    #[tracing::instrument(skip_all, name = "[amplifier-ingester]")]
    pub async fn ingest(&self) -> eyre::Result<()> {
        tracing::debug!("refresh");

        self.event_queue_consumer
            .messages()
            .await
            .wrap_err("could not retrieve messages from queue")?
            .for_each_concurrent(self.concurrent_queue_items, |queue_msg| async {
                match queue_msg {
                    Ok(msg) => self.process_queue_msg(msg).await,
                    Err(err) => tracing::error!(?err, "could not receive queue msg"),
                }
            })
            .await;

        Ok(())
    }

    /// Checks the health of the ingester.
    ///
    /// This function performs various health checks to ensure the ingester is operational.
    ///
    /// # Errors
    ///
    /// This function will return an error if any of the health checks fail.
    pub async fn check_health(&self) -> eyre::Result<()> {
        tracing::debug!("checking health");

        // Check if the event queue consumer is healthy
        match self.event_queue_consumer.check_health().await {
            Ok(()) => {
                tracing::debug!("event queue consumer is healthy");
            }
            Err(err) => {
                tracing::warn!(%err, "event queue consumer health check failed");
                return Err(err.into());
            }
        }

        // Check if the amplifier client is healthy
        match self
            .ampf_client
            .build_request(&requests::HealthCheck)
            .wrap_err("could not build health check request")?
            .execute()
            .await
        {
            Ok(_) => {
                tracing::debug!("amplifier client is healthy");
            }
            Err(err) => {
                tracing::warn!(%err, "amplifier client health check failed");
                return Err(err.into());
            }
        }

        Ok(())
    }
}

#[cfg(feature = "supervisor")]
impl<EventQueueConsumer> supervisor::Worker for Ingester<EventQueueConsumer>
where
    EventQueueConsumer: Consumer<amplifier_api::types::Event> + Send + Sync,
{
    fn do_work<'s>(
        &'s mut self,
    ) -> core::pin::Pin<Box<dyn Future<Output = eyre::Result<()>> + 's>> {
        Box::pin(async { self.ingest().await })
    }
}
