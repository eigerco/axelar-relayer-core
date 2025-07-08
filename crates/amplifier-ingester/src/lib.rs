//! Crate with amplifier ingester component
use std::sync::Arc;

use amplifier_api::requests::{self, WithTrailingSlash};
use amplifier_api::types::{Event, PublishEventsRequest};
use amplifier_api::{self, AmplifierApiClient};
use bin_util::BlockChainIngesterMetrics;
use bin_util::health_check::CheckHealth;
use eyre::Context as _;
use futures::StreamExt as _;
use infrastructure::interfaces::consumer::{AckKind, Consumer, QueueMessage};
use tracing::Instrument as _;

mod components;

/// Configs
pub mod config;

pub use components::*;

#[cfg(not(any(feature = "gcp", feature = "nats")))]
compile_error!("Either feature 'gcp' or feature 'nats' must be enabled");

#[cfg(all(feature = "gcp", feature = "nats"))]
compile_error!("Features 'gcp' and 'nats' are mutually exclusive");

/// Consumes events queue and sends it to include to amplifier api
pub struct Ingester<EventQueueConsumer> {
    ampf_client: AmplifierApiClient,
    event_queue_consumer: Arc<EventQueueConsumer>,
    concurrent_queue_items: usize,
    chain: String,
    metrics: BlockChainIngesterMetrics,
}

impl<EventQueueConsumer> Ingester<EventQueueConsumer>
where
    EventQueueConsumer: Consumer<amplifier_api::types::Event>,
{
    /// create ingester
    pub fn new(
        amplifier_client: AmplifierApiClient,
        event_queue_consumer: EventQueueConsumer,
        concurrent_queue_items: usize,
        chain: String,
    ) -> Self {
        let event_queue_consumer = Arc::new(event_queue_consumer);
        let metrics = BlockChainIngesterMetrics::new("amplifier-ingester", vec![]);
        Self {
            ampf_client: amplifier_client,
            event_queue_consumer,
            concurrent_queue_items,
            chain,
            metrics,
        }
    }

    /// process queue message
    #[tracing::instrument(skip_all, fields(
        event = tracing::field::Empty,
    ))]
    pub async fn process_queue_msg<Msg: QueueMessage<Event>>(&self, mut queue_msg: Msg) {
        let chain_with_trailing_slash = WithTrailingSlash::new(self.chain.clone());

        let event = queue_msg.decoded().clone();
        tracing::Span::current().record("event", tracing::field::display(&event));

        // Record metrics for received event based on type
        self.record_event_received(&event);

        let payload = PublishEventsRequest {
            events: vec![event.clone()],
        };

        let result: eyre::Result<()> = async {
            tracing::trace!("processing");
            let request = requests::PostEvents::builder()
                .payload(&payload)
                .chain(&chain_with_trailing_slash)
                .build();

            let request = self
                .ampf_client
                .build_request(&request)
                .wrap_err("could not build amplifier request")?;

            let response = request
                .execute()
                .await
                .wrap_err("could not send amplifier request")?;

            let response = response
                .json()
                .await
                .map_err(|err| eyre::Report::new(err).wrap_err("amplifier api failed"))?
                .map_err(|err| eyre::Report::new(err).wrap_err("failed to decode response"))?;

            tracing::trace!(?response, "response from amplifier api");

            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                // Record successful processing based on event type
                self.record_event_processed(&event);

                if let Err(err) = queue_msg.ack(AckKind::Ack).await {
                    self.metrics.record_error();
                    tracing::error!(?err, "could not ack message, skipping...");
                }

                tracing::info!("event ack'ed");
            }
            Err(err) => {
                self.metrics.record_error();
                tracing::error!(?err, "error during task processing");
                if let Err(err) = queue_msg.ack(AckKind::Nak).await {
                    self.metrics.record_error();
                    tracing::error!(?err, "could not nak message, skipping...");
                }
            }
        }
    }

    /// Consume queue messages and ingest to amplifier api
    ///
    /// This method continuously processes messages from the event queue, forwarding them
    /// to the Amplifier API. Messages are processed concurrently up to the configured
    /// concurrency limit.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to retrieve messages from the queue (e.g., connection issues)
    /// - The underlying queue consumer encounters an unrecoverable error
    ///
    /// Note: Individual message processing errors are logged but do not cause this method
    /// to return an error. Failed messages are acknowledged with NAK for retry.
    #[tracing::instrument(skip_all)]
    pub async fn ingest(&self) -> eyre::Result<()> {
        self.event_queue_consumer
            .messages()
            .await
            .wrap_err("could not retrieve messages from queue")
            .inspect_err(|_| {
                self.metrics.record_error();
            })?
            .for_each_concurrent(self.concurrent_queue_items, |queue_msg| async {
                match queue_msg {
                    Ok(msg) => self.process_queue_msg(msg).await,
                    Err(err) => {
                        self.metrics.record_error();

                        tracing::error!(?err, "could not receive queue msg...");
                    }
                }
            })
            .instrument(tracing::info_span!("processing messages"))
            .await;

        Ok(())
    }

    /// Record metrics for received events based on event type
    fn record_event_received(&self, event: &Event) {
        match event {
            Event::GasCredit(_) => {
                self.metrics.record_gas_credit_received();
            }
            Event::GasRefunded(_) => {
                self.metrics.record_gas_refunded_received();
            }
            Event::Call(_) => {
                self.metrics.record_call_received();
            }
            Event::MessageApproved(_) => {
                self.metrics.record_message_approved_received();
            }
            Event::MessageExecuted(_) => {
                self.metrics.record_message_executed_received();
            }
            Event::CannotExecuteMessageV2(_) => {
                self.metrics.record_cannot_execute_message_received();
            }
            Event::SignersRotated(_) => {
                self.metrics.record_signers_rotated_received();
            }
        }
    }

    /// Record metrics for successfully processed events based on event type
    fn record_event_processed(&self, event: &Event) {
        match event {
            Event::GasCredit(_) => {
                self.metrics.record_gas_credit_processed();
            }
            Event::GasRefunded(_) => {
                self.metrics.record_gas_refunded_processed();
            }
            Event::Call(_) => {
                self.metrics.record_call_processed();
            }
            Event::MessageApproved(_) => {
                self.metrics.record_message_approved_processed();
            }
            Event::MessageExecuted(_) => {
                self.metrics.record_message_executed_processed();
            }
            Event::CannotExecuteMessageV2(_) => {
                self.metrics.record_cannot_execute_message_processed();
            }
            Event::SignersRotated(_) => {
                self.metrics.record_signers_rotated_processed();
            }
        }
    }
}

impl<EventQueueConsumer> CheckHealth for Ingester<EventQueueConsumer>
where
    EventQueueConsumer: Consumer<amplifier_api::types::Event> + Send + Sync + 'static,
{
    async fn check_health(&self) -> eyre::Result<()> {
        if let Err(err) = self.event_queue_consumer.check_health().await {
            tracing::error!(?err, "event queue consumer health check failed");
            return Err(err.into());
        }

        if let Err(err) = self
            .ampf_client
            .build_request(&requests::HealthCheck)
            .wrap_err("could not build health check request")?
            .execute()
            .await
        {
            tracing::error!(?err, "amplifier client health check failed");
            return Err(err.into());
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
