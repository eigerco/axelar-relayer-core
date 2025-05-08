use core::fmt::{Debug, Display};
use core::marker::PhantomData;
use std::collections::HashMap;

use borsh::{BorshDeserialize, BorshSerialize};
use google_cloud_gax::retry::RetrySetting;
use google_cloud_googleapis::pubsub::v1::PubsubMessage;
use google_cloud_pubsub::client::Client;
use google_cloud_pubsub::publisher::{Publisher, PublisherConfig};
use interfaces::kv_store::KvStore as _;

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
    _phantom: PhantomData<T>,
}

impl<T> GcpPublisher<T> {
    pub(crate) async fn new(
        client: &Client,
        topic: &str,
        worker_count: usize,
        max_bundle_size: usize,
    ) -> Result<Self, GcpError> {
        let topic = get_topic(client, topic).await?;

        let config = PublisherConfig {
            workers: worker_count,
            bundle_size: max_bundle_size,
            retry_setting: Some(RetrySetting::default()),
            ..Default::default()
        };

        let publisher = topic.new_publisher(Some(config));

        Ok(Self {
            publisher,
            _phantom: PhantomData,
        })
    }
}

fn to_pubsub_message<T>(msg: PublishMessage<T>) -> Result<PubsubMessage, GcpError>
where
    T: BorshSerialize + Debug,
{
    let encoded = borsh::to_vec(&msg.data).map_err(GcpError::Serialize)?;
    let mut attributes = HashMap::new();
    attributes.insert(MSG_ID.to_owned(), msg.deduplication_id);
    let message = PubsubMessage {
        data: encoded,
        attributes,
        ..Default::default()
    };

    Ok(message)
}

impl<T> interfaces::publisher::Publisher<T> for GcpPublisher<T>
where
    T: BorshSerialize + Debug + Send + Sync,
{
    type Return = String;

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn publish(&self, msg: PublishMessage<T>) -> Result<Self::Return, GcpError> {
        tracing::debug!(?msg.deduplication_id, ?msg.data, "publishing message");
        let msg = to_pubsub_message(msg)?;
        let awaiter = self.publisher.publish(msg).await;

        // NOTE: await until message is sent
        let result = awaiter
            .get()
            .await
            .map_err(|err| GcpError::Publish(Box::new(err)))?;

        tracing::debug!("message published");
        Ok(result)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn publish_batch(
        &self,
        batch: Vec<PublishMessage<T>>,
    ) -> Result<Vec<Self::Return>, GcpError> {
        tracing::debug!("publishing {} count of messages as a batch", batch.len());

        let bulk = batch
            .into_iter()
            .map(to_pubsub_message)
            .collect::<Result<Vec<PubsubMessage>, GcpError>>()?;
        let publish_handles = self.publisher.publish_bulk(bulk).await;
        tracing::debug!("message added to publish queue");

        // NOTE: await until all messages are sent
        let mut output = Vec::new();
        for handle in publish_handles {
            let res = handle
                .get()
                .await
                .map_err(|err| GcpError::Publish(Box::new(err)))?;
            output.push(res);
        }
        tracing::debug!("batch published");

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
    pub(crate) async fn new(
        client: &Client,
        topic: &str,
        kv_store: RedisClient<T::MessageId>,
        worker_count: usize,
        max_bundle_size: usize,
    ) -> Result<Self, GcpError> {
        let publisher = GcpPublisher::new(client, topic, worker_count, max_bundle_size).await?;

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
    #[tracing::instrument(skip_all)]
    async fn publish(&self, msg: PublishMessage<T>) -> Result<Self::Return, GcpError> {
        let last_msg_id = msg.data.id();
        let res = self.publisher.publish(msg).await?;
        self.last_message_id_store.upsert(&last_msg_id).await?;
        tracing::info!("sent of messages to queue. Last message id is set to {last_msg_id}");
        Ok(res)
    }

    // NOTE: all messages are batched and send independently via workers, on success last message
    // task id is SAVED as last processed in redis so ORDER IN THE BATCH ARG MATTERS. If any of them
    // fail entire batch is regarded failed and will be retried. Deduplication happens on
    // consumers side per gcp recommendation
    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn publish_batch(
        &self,
        batch: Vec<PublishMessage<T>>,
    ) -> Result<Vec<Self::Return>, GcpError> {
        let Some(last_msg) = batch.last() else {
            return Err(GcpError::NoMsgToPublish);
        };

        let count = batch.len();

        let last_msg_id = last_msg.data.id();
        let res = self.publisher.publish_batch(batch).await?;

        self.last_message_id_store.upsert(&last_msg_id).await?;

        tracing::info!(
            "sent {count} of messages to queue. Last message id is set to {last_msg_id}"
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
        self.last_message_id_store
            .get()
            .await?
            .map(|data| Ok(data.value))
            .transpose()
    }
}
