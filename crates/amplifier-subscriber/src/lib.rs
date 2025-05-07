//! Crate with amplifier subscriber component
use core::pin::Pin;

use amplifier_api::requests::WithTrailingSlash;
use amplifier_api::{AmplifierApiClient, requests};
use eyre::Context as _;
use infrastructure::interfaces::publisher::{PeekMessage, PublishMessage, Publisher};

/// subscribes to tasks from amplifier and sends them to queue
pub struct Subscriber<TaskQueuePublisher> {
    amplifier_client: AmplifierApiClient,
    task_queue_publisher: TaskQueuePublisher,
    chain: String,
}

impl<TaskQueuePublisher> Subscriber<TaskQueuePublisher>
where
    TaskQueuePublisher:
        Publisher<amplifier_api::types::TaskItem> + PeekMessage<amplifier_api::types::TaskItem>,
{
    /// create subscriber
    pub const fn new(
        amplifier_client: AmplifierApiClient,
        task_queue_publisher: TaskQueuePublisher,
        chain: String,
    ) -> Self {
        Self {
            amplifier_client,
            task_queue_publisher,
            chain,
        }
    }

    /// subscribe and process
    #[tracing::instrument(skip_all, name = "[amplifier-subscriber]")]
    pub async fn subscribe(&mut self) -> eyre::Result<()> {
        tracing::debug!("refresh");
        let chain_with_trailing_slash = WithTrailingSlash::new(self.chain.clone());

        let last_task_id = self
            .task_queue_publisher
            .peek_last()
            .await
            .wrap_err("could not get last retrieved task id")?;

        tracing::debug!(?last_task_id, "last retrieved task");

        let request = requests::GetChains::builder()
            .chain(&chain_with_trailing_slash)
            .limit(100_u8)
            .after(last_task_id)
            .build();

        tracing::debug!(?request, "request for amplifier api created");

        let request = self
            .amplifier_client
            .build_request(&request)
            .wrap_err("could not build amplifier request")?;

        tracing::debug!(?request, "sending");

        let response = request
            .execute()
            .await
            .wrap_err("could not sent amplifier api request")?;

        tracing::debug!("sent");

        let response = match response.json().await {
            Ok(response) => match response {
                Ok(response) => response,
                Err(err) => {
                    return Err(eyre::Report::new(err).wrap_err("failed to decode response"));
                }
            },
            Err(err) => return Err(eyre::Report::new(err).wrap_err("amplifier api failed")),
        };

        tracing::debug!(?response, "amplifier response");

        if response.tasks.is_empty() {
            tracing::debug!("no amplifier tasks");
            return Ok(());
        }

        tracing::info!(count = response.tasks.len(), "got amplifier tasks");

        let batch = response
            .tasks
            .into_iter()
            .map(|task| PublishMessage {
                deduplication_id: task.id.0.to_string(),
                data: task,
            })
            .collect();

        tracing::debug!("sending to queue");
        self.task_queue_publisher
            .publish_batch(batch)
            .await
            .wrap_err("could not publish tasks to queue")?;
        tracing::debug!("sent to queue");
        Ok(())
    }
}

impl<TaskQueuePublisher> supervisor::Worker for Subscriber<TaskQueuePublisher>
where
    TaskQueuePublisher: Publisher<amplifier_api::types::TaskItem>
        + PeekMessage<amplifier_api::types::TaskItem>
        + Send
        + Sync,
{
    fn do_work<'s>(&'s mut self) -> Pin<Box<dyn Future<Output = eyre::Result<()>> + 's>> {
        Box::pin(async { self.subscribe().await })
    }
}
