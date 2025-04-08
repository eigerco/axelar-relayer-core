use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use eyre::Context;
use futures::StreamExt;
use relayer_amplifier_api_integration::amplifier_api::requests::{self, WithTrailingSlash};
use relayer_amplifier_api_integration::amplifier_api::types::{Event, PublishEventsRequest};
use relayer_amplifier_api_integration::amplifier_api::{self, AmplifierApiClient};
use storage_bus::interfaces::consumer::{AckKind, Consumer, QueueMessage};
use supervisor::Worker;

pub struct Ingester<EventQueueConsumer> {
    ampf_client: AmplifierApiClient,
    event_queue_consumer: Arc<EventQueueConsumer>,
    chain: String,
}

impl<EventQueueConsumer> Ingester<EventQueueConsumer>
where
    EventQueueConsumer: Consumer<amplifier_api::types::Event>,
{
    pub fn new(
        amplifier_client: AmplifierApiClient,
        event_queue_consumer: EventQueueConsumer,
        chain: String,
    ) -> Self {
        let event_queue_consumer = Arc::new(event_queue_consumer);
        Self {
            ampf_client: amplifier_client,
            event_queue_consumer,
            chain,
        }
    }

    pub async fn process_queue_msg<Msg: QueueMessage<Event>>(&self, queue_msg: Msg) {
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

            let response = match response.json().await {
                Ok(response) => match response {
                    Ok(response) => response,
                    Err(err) => {
                        return Err(eyre::Report::new(err).wrap_err("failed to decode response"));
                    }
                },
                Err(err) => return Err(eyre::Report::new(err).wrap_err("amplifier api failed")),
            };

            tracing::debug!(?response, "response from amplifier api");

            Ok(())
        }
        .await;

        match result {
            Ok(_) => {
                if let Err(err) = queue_msg.ack(AckKind::Ack).await {
                    tracing::error!(%event, %err, "could not ack message")
                }

                tracing::info!(event_id = %event.event_id(), "processed");
            }
            Err(err) => {
                tracing::error!(%event, %err, "error during task processing");
                if let Err(err) = queue_msg.ack(AckKind::Nak(None)).await {
                    tracing::error!(%event, %err, "could not nak message")
                }
            }
        }
    }

    #[tracing::instrument(skip_all, name = "[amplifier-ingester]")]
    pub async fn ingest(&self) -> eyre::Result<()> {
        tracing::debug!("refresh");

        self.event_queue_consumer
            .messages()
            .await
            .wrap_err("could not retrieve messages from queue")?
            .for_each_concurrent(10, move |queue_msg| async move {
                let queue_msg = match queue_msg {
                    Ok(queue_msg) => queue_msg,
                    Err(err) => {
                        tracing::error!(?err, "could not receive queue msg");
                        return;
                    }
                };
                self.process_queue_msg(queue_msg).await;
            })
            .await;

        Ok(())
    }
}

impl<EventQueueConsumer> Worker for Ingester<EventQueueConsumer>
where
    EventQueueConsumer: Consumer<amplifier_api::types::Event> + Send + Sync,
{
    fn do_work<'s>(
        &'s mut self,
        _shutdown: &'s AtomicBool,
    ) -> Pin<Box<dyn Future<Output = eyre::Result<()>> + 's>> {
        Box::pin(async { self.ingest().await })
    }
}
