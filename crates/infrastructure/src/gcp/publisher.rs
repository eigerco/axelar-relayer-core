use core::fmt::{Debug, Display};
use core::marker::PhantomData;
use std::collections::HashMap;

use borsh::{BorshDeserialize, BorshSerialize};
use google_cloud_googleapis::pubsub::v1::PubsubMessage;
use google_cloud_pubsub::client::Client;
use google_cloud_pubsub::publisher::{Publisher, PublisherConfig};
use interfaces::kv_store::KvStore as _;

use super::GcpError;
use super::kv_store::RedisClient;
use super::util::get_topic;
use crate::interfaces;
use crate::interfaces::publisher::QueueMsgId;

const MSG_ID: &str = "Msg-Id";

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
            // TODO: move to config
            bundle_size: 100,
            ..Default::default()
        };

        let publisher = topic.new_publisher(Some(config));

        Ok(Self {
            publisher,
            _phantom: PhantomData,
        })
    }
}

impl<T> interfaces::publisher::Publisher<T> for GcpPublisher<T>
where
    T: BorshSerialize + Debug + Send + Sync,
{
    type Return = String;

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn publish(
        &self,
        // Deduplication is automatic
        deduplication_id: impl Into<String>,
        data: &T,
    ) -> Result<Self::Return, GcpError> {
        let encoded = borsh::to_vec(&data).map_err(GcpError::Serialize)?;
        let mut attributes = HashMap::new();
        attributes.insert(MSG_ID.to_owned(), deduplication_id.into());
        let message = PubsubMessage {
            data: encoded,
            attributes,
            ..Default::default()
        };
        let awaiter = self.publisher.publish(message.clone()).await;

        // NOTE: We always await since messages should be sent sequentially
        let result = awaiter
            .get()
            .await
            .map_err(|err| GcpError::Publish(Box::new(err)))?;
        Ok(result)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
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
    T: QueueMsgId + BorshSerialize + Debug + Clone + Send + Sync,
    T::MessageId: BorshSerialize + BorshDeserialize + Debug + Display + Send + Sync,
{
    type Return = String;
    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn publish(
        &self,
        deduplication_id: impl Into<String>,
        data: &T,
    ) -> Result<Self::Return, GcpError> {
        let res = self.publisher.publish(deduplication_id, data).await?;
        self.last_message_id_store.upsert(&data.id()).await?;
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
