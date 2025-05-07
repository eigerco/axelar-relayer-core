use core::fmt::{Debug, Display};
use core::marker::PhantomData;
use std::collections::HashMap;

use borsh::{BorshDeserialize, BorshSerialize};
use google_cloud_gax::retry::RetrySetting;
use google_cloud_googleapis::pubsub::v1::PubsubMessage;
use google_cloud_pubsub::client::Client;
use google_cloud_pubsub::publisher::{Awaiter, Publisher, PublisherConfig};
use interfaces::kv_store::KvStore as _;

use super::kv_store::RedisClient;
use super::util::get_topic;
use super::{GcpError, MESSAGE_CAPACITY};
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
    pub(crate) async fn new(client: &Client, topic: &str) -> Result<Self, GcpError> {
        let topic = get_topic(client, topic).await?;
        let num_cpu = num_cpus::get();

        let config = PublisherConfig {
            // NOTE: scaling factor of 2-4 is recommended for io-bound work
            workers: num_cpu.checked_mul(2).unwrap_or(num_cpu),
            bundle_size: MESSAGE_CAPACITY,
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

impl<T> GcpPublisher<T>
where
    T: BorshSerialize + Debug,
{
    async fn pubsub_publish(&self, msg: PublishMessage<T>) -> Result<Awaiter, GcpError> {
        let encoded = borsh::to_vec(&msg.data).map_err(GcpError::Serialize)?;
        let mut attributes = HashMap::new();
        attributes.insert(MSG_ID.to_owned(), msg.deduplication_id);
        let message = PubsubMessage {
            data: encoded,
            attributes,
            ..Default::default()
        };

        Ok(self.publisher.publish(message.clone()).await)
    }
}

impl<T> interfaces::publisher::Publisher<T> for GcpPublisher<T>
where
    T: BorshSerialize + Debug,
{
    type Return = String;

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn publish(&self, msg: PublishMessage<T>) -> Result<Self::Return, GcpError> {
        tracing::debug!(?msg.deduplication_id, ?msg.data, "publishing message");
        let awaiter = self.pubsub_publish(msg).await?;

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

        let mut publish_handles = Vec::with_capacity(batch.len());

        for msg in batch {
            tracing::debug!(?msg.deduplication_id, ?msg.data, "publishing message");
            let awaiter = self.pubsub_publish(msg).await?;
            publish_handles.push(awaiter);
            tracing::debug!("message added to publish queue");
        }

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
    ) -> Result<Self, GcpError> {
        let publisher = GcpPublisher::new(client, topic).await?;

        Ok(Self {
            publisher,
            last_message_id_store: kv_store,
        })
    }
}

impl<T> interfaces::publisher::Publisher<T> for PeekableGcpPublisher<T>
where
    T: QueueMsgId + BorshSerialize + Debug + Clone,
    T::MessageId: BorshSerialize + BorshDeserialize + Debug + Display,
{
    type Return = String;

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn publish(&self, msg: PublishMessage<T>) -> Result<Self::Return, GcpError> {
        let id = msg.data.id();
        let res = self.publisher.publish(msg).await?;
        self.last_message_id_store.upsert(&id).await?;
        tracing::info!("sent of messages to queue. Last message id is set to {id}");
        Ok(res)
    }

    // NOTE: all messages are batched and send independently via workers, on success last message
    // task id is saved as last processed in redis. If any of them fail
    // entire batch is regarded failed and will be retried. Deduplication happens on consumers side
    // per gcp recommendation
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

        let id = last_msg.data.id();
        let res = self.publisher.publish_batch(batch).await?;

        self.last_message_id_store.upsert(&id).await?;

        tracing::info!("sent {count} of messages to queue. Last message id is set to {id}");

        Ok(res)
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
