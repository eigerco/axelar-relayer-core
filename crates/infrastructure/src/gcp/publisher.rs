use core::fmt::{Debug, Display};
use core::marker::PhantomData;
use std::collections::HashMap;

use borsh::{BorshDeserialize, BorshSerialize};
use google_cloud_gax::retry::RetrySetting;
use google_cloud_googleapis::pubsub::v1::PubsubMessage;
use google_cloud_pubsub::client::Client;
use google_cloud_pubsub::publisher::{Publisher, PublisherConfig};
use interfaces::kv_store::KvStore as _;
use opentelemetry::metrics::{Counter, Histogram};
use opentelemetry::{KeyValue, global};

use super::GcpError;
use super::kv_store::RedisClient;
use super::util::get_topic;
use crate::interfaces;
use crate::interfaces::publisher::{PublishMessage, QueueMsgId};

/// Deduplication Id
pub const MSG_ID: &str = "Msg-Id";

/// Queue publisher
#[allow(clippy::module_name_repetitions, reason = "Descriptive name")]
pub struct GcpPublisher<T> {
    publisher: Publisher,
    metrics: Metrics,
    _phantom: PhantomData<T>,
}

impl<T> GcpPublisher<T> {
    #[tracing::instrument(
        name = "create_gcp_publisher",
        skip(client),
        fields(
            topic = %topic,
        )
    )]
    pub(crate) async fn new(
        client: &Client,
        topic: &str,
        worker_count: usize,
        max_bundle_size: usize,
    ) -> Result<Self, GcpError> {
        tracing::info!("initializing GCP PubSub publisher for topic");
        let topic = get_topic(client, topic).await?;

        let config = PublisherConfig {
            workers: worker_count,
            bundle_size: max_bundle_size,
            retry_setting: Some(RetrySetting::default()),
            ..Default::default()
        };

        let publisher = topic.new_publisher(Some(config));
        tracing::info!("GCP PubSub publisher successfully initialized");
        let metrics = Metrics::new(topic.fully_qualified_name());

        Ok(Self {
            publisher,
            metrics,
            _phantom: PhantomData,
        })
    }
}

#[tracing::instrument(skip_all, level = "debug")]
fn to_pubsub_message<T>(msg: PublishMessage<T>) -> Result<PubsubMessage, GcpError>
where
    T: BorshSerialize + Debug,
{
    tracing::debug!("serializing message to PubSub format");
    let encoded = borsh::to_vec(&msg.data).map_err(GcpError::Serialize)?;
    let mut attributes = HashMap::new();
    attributes.insert(MSG_ID.to_owned(), msg.deduplication_id.clone());
    let message = PubsubMessage {
        data: encoded,
        attributes,
        ..Default::default()
    };
    tracing::debug!(
        deduplication_id = %msg.deduplication_id,
        message_size = message.data.len(),
        "message prepared for publishing"
    );
    Ok(message)
}

impl<T> interfaces::publisher::Publisher<T> for GcpPublisher<T>
where
    T: BorshSerialize + Debug + Send + Sync,
{
    type Return = String;

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(
        skip_all,
        fields(
            deduplication_id = %msg.deduplication_id
        )
    )]
    async fn publish(&self, msg: PublishMessage<T>) -> Result<Self::Return, GcpError> {
        tracing::debug!(?msg.deduplication_id, ?msg.data, "preparing to publish message to PubSub");
        let msg = to_pubsub_message(msg)?;
        tracing::debug!("publishing message to PubSub queue");

        let start_time = std::time::Instant::now();

        let awaiter = self.publisher.publish(msg).await;
        tracing::debug!("waiting for publish confirmation");

        // NOTE: await until message is sent
        let result = awaiter
            .get()
            .await
            .map_err(|err| GcpError::Publish(Box::new(err)))?;
        self.metrics.record_publish(start_time);
        tracing::info!(message_id = %result, "message successfully published to PubSub");
        Ok(result)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn publish_batch(
        &self,
        batch: Vec<PublishMessage<T>>,
    ) -> Result<Vec<Self::Return>, GcpError> {
        if batch.is_empty() {
            tracing::warn!("attempt to publish empty batch");
            return Ok(Vec::new());
        }
        let batch_size = batch.len();

        tracing::info!("publishing batch of {} messages to PubSub", batch.len());
        let bulk = batch
            .into_iter()
            .map(to_pubsub_message)
            .collect::<Result<Vec<PubsubMessage>, GcpError>>()?;

        tracing::debug!("submitting batch to publish queue");
        let start_time = std::time::Instant::now();
        let publish_handles = self.publisher.publish_bulk(bulk).await;
        tracing::debug!("waiting for batch publish confirmations");

        // NOTE: await until all messages are sent
        let mut output = Vec::new();
        for (index, handle) in publish_handles.into_iter().enumerate() {
            let res = handle
                .get()
                .await
                .map_err(|err| GcpError::Publish(Box::new(err)))?;
            self.metrics.record_publish(start_time);
            tracing::debug!(message_index = index, message_id = %res, "message confirmed");
            output.push(res);
        }

        tracing::info!("successfully published batch of {} messages", output.len());

        Ok(output)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn check_health(&self) -> Result<(), GcpError> {
        tracing::debug!("checking health for GCP publisher");

        // Create a small health check message
        let health_attributes = HashMap::from([("health_check".to_owned(), "true".to_owned())]);

        let health_message = PubsubMessage {
            data: Vec::from("health_check"),
            attributes: health_attributes,
            ..Default::default()
        };
        tracing::debug!("sending health check message to PubSub");
        // Try to publish the message and await the result
        let awaiter = self.publisher.publish(health_message).await;

        // Check if we can get a result from the awaiter
        match awaiter.get().await {
            Ok(_) => {
                tracing::debug!("GCP publisher health check successful");
                Ok(())
            }
            Err(err) => {
                tracing::error!("GCP publisher health check failed: {}", err);
                Err(GcpError::Publish(Box::new(err)))
            }
        }
    }
}

/// Queue publisher with ability to get last message (without consuming)
#[allow(clippy::module_name_repetitions, reason = "Descriptive name")]
pub struct PeekableGcpPublisher<T: QueueMsgId> {
    publisher: GcpPublisher<T>,
    last_message_id_store: RedisClient<T::MessageId>,
}

impl<T> PeekableGcpPublisher<T>
where
    T: QueueMsgId,
    T::MessageId: BorshSerialize + BorshDeserialize + Display,
{
    #[tracing::instrument(name = "create_peekable_publisher", skip_all)]
    pub(crate) async fn new(
        client: &Client,
        topic: &str,
        kv_store: RedisClient<T::MessageId>,
        worker_count: usize,
        max_bundle_size: usize,
    ) -> Result<Self, GcpError> {
        tracing::info!("initializing peekable GCP PubSub publisher");
        let publisher = GcpPublisher::new(client, topic, worker_count, max_bundle_size).await?;
        tracing::info!("peekable GCP PubSub publisher successfully initialized");

        Ok(Self {
            publisher,
            last_message_id_store: kv_store,
        })
    }
}

impl<T> interfaces::publisher::Publisher<T> for PeekableGcpPublisher<T>
where
    T: QueueMsgId + BorshSerialize + Debug + Clone + Send + Sync,
    T::MessageId: BorshSerialize + BorshDeserialize + Debug + Display + Send + Sync,
{
    type Return = String;

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(
        skip_all,
        fields(
            deduplication_id = %msg.deduplication_id,
        )
    )]
    async fn publish(&self, msg: PublishMessage<T>) -> Result<Self::Return, GcpError> {
        let last_msg_id = msg.data.id();
        tracing::debug!(
            last_message_id = %last_msg_id,
            "publishing message with peekable publisher"
        );
        let res = self.publisher.publish(msg).await?;
        tracing::debug!(
            last_message_id = %last_msg_id,
            "updating last message ID in Redis"
        );
        self.last_message_id_store.upsert(&last_msg_id).await?;
        tracing::info!(
            last_message_id = %last_msg_id,
            "message published and ID stored in Redis"
        );
        Ok(res)
    }

    // NOTE: all messages are batched and send independently via workers, on success last message
    // task id is SAVED as last processed in redis so ORDER IN THE BATCH ARG MATTERS. If any of them
    // fail entire batch is regarded failed and will be retried. Deduplication happens on
    // consumers side per gcp recommendation
    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(
        skip_all,
        fields(
            batch_size = batch.len()
        )
    )]
    async fn publish_batch(
        &self,
        batch: Vec<PublishMessage<T>>,
    ) -> Result<Vec<Self::Return>, GcpError> {
        let Some(last_msg) = batch.last() else {
            return Err(GcpError::NoMsgToPublish);
        };

        let last_msg_id = last_msg.data.id();
        tracing::info!(
            last_message_id = %last_msg_id,
            "publishing batch with peekable publisher"
        );

        let res = self.publisher.publish_batch(batch).await?;
        tracing::debug!(
            last_message_id = %last_msg_id,
            "updating last message ID in Redis after batch publish"
        );
        self.last_message_id_store.upsert(&last_msg_id).await?;

        tracing::info!(
            last_message_id = %last_msg_id,
            "batch successfully published and last ID stored"
        );

        Ok(res)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    async fn check_health(&self) -> Result<(), GcpError> {
        tracing::debug!("checking health for PeekableGcpPublisher");

        // Check the GCP publisher health first
        self.publisher.check_health().await?;

        // Check Redis client health by performing a simple operation
        match self.last_message_id_store.ping().await {
            Ok(()) => {
                tracing::debug!("Redis client health check successful");
                Ok(())
            }
            Err(err) => {
                tracing::error!("Redis client health check failed: {}", err);
                Err(GcpError::Redis(err))
            }
        }
    }
}

impl<T> interfaces::publisher::PeekMessage<T> for PeekableGcpPublisher<T>
where
    T: QueueMsgId,
    T::MessageId: BorshSerialize + BorshDeserialize + Debug + Display,
{
    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn peek_last(&mut self) -> Result<Option<T::MessageId>, GcpError> {
        tracing::debug!("retrieving last message ID from Redis");
        self.last_message_id_store
            .get()
            .await?
            .map(|data| Ok(data.value))
            .transpose()
    }
}

struct Metrics {
    published_count: Counter<u64>,
    publish_duration: Histogram<f64>,
    attributes: [KeyValue; 1],
}

impl Metrics {
    pub fn new(topic_name: &str) -> Self {
        let meter = global::meter("pubsub_publisher");

        let published_count = meter
            .u64_counter("messages.count")
            .with_description("Total number of messages published to PubSub")
            .build();

        let publish_duration = meter
            .f64_histogram("publisher.duration")
            .with_description("Time taken to publish messages to PubSub in seconds")
            .with_unit("s")
            .build();

        let batch_size = meter
            .u64_histogram("atch.size")
            .with_description("Size of message batches published to PubSub")
            .build();

        let attributes = [KeyValue::new("topic.name", topic_name.to_owned())];

        Self {
            published_count,
            publish_duration,
            attributes,
        }
    }

    pub fn record_publish(&self, start_time: std::time::Instant) {
        self.published_count.add(1, &[]);
        self.publish_duration
            .record(start_time.elapsed().as_secs_f64(), &self.attributes);
    }
}
