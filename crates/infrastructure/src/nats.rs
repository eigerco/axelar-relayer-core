#![expect(missing_docs, reason = "the error macro already is descriptive enough")]
/// nats implementation
use core::time::Duration;

use async_nats::jetstream::consumer::StreamError;
use async_nats::jetstream::consumer::push::MessagesError;
use async_nats::jetstream::context::{
    ConsumerInfoError, CreateKeyValueError, CreateStreamError, PublishError,
};
use async_nats::jetstream::kv::{EntryError, PutError, UpdateError};
use async_nats::jetstream::stream::{ConsumerError, DirectGetError, InfoError};
use async_nats::{self, ConnectError, jetstream};
use url::Url;

use crate::interfaces::publisher::QueueMsgId;

/// Nats clients builder
pub struct Builder {
    inbox: String,
    context: jetstream::context::Context,
}

/// connectors to queue
pub mod connectors;
/// consumer
pub mod consumer;
/// keyvalue store
pub mod kv_store;
/// publisher
pub mod publisher;

/// Errors
#[allow(clippy::module_name_repetitions, reason = "Descriptive name")]
#[derive(Debug, thiserror::Error)]
pub enum NatsError {
    #[error("connect to nats error: {0}")]
    Connect(#[from] ConnectError),
    #[error("create nats kvstore error: {0}")]
    CreateKeyValue(#[from] CreateKeyValueError),
    #[error("put nats kvstore val error: {0}")]
    Put(#[from] PutError),
    #[error("failed to deserialize val error: {0}")]
    Deserialize(std::io::Error),
    #[error("failed to serialize val, error: {0}")]
    Serialize(std::io::Error),
    #[error("update nats kvstore val error: {0}")]
    Update(#[from] UpdateError),
    #[error("get nats kvstore val error: {0}")]
    Entry(EntryError),
    #[error("create nats stream error: {0}")]
    CreateStream(#[from] CreateStreamError),
    #[error("publish nats message error: {0}")]
    Publish(#[from] PublishError),
    #[error("create nats consumer error: {0}")]
    CreateConsumer(ConsumerError),
    #[error("read nats stream message serror: {0}")]
    MessagesStream(StreamError),
    #[error("read nats consumer message error: {0}")]
    Messages(#[from] MessagesError),
    #[error("nats info error: {0}")]
    ConsumerInfo(#[from] ConsumerInfoError),
    #[error("nats info error: {0}")]
    PublisherInfo(#[from] InfoError),
    #[error("get direct nats message error: {0}")]
    DirectGet(#[from] DirectGetError),
    #[error("nats ack error: {0}")]
    Ack(async_nats::Error),
}

/// args for selecting stream
#[derive(Debug)]
pub struct StreamArgs {
    /// stream name
    pub name: String,
    /// stream subject
    pub subject: String,
    /// stream description
    pub description: String,
}

impl Builder {
    pub(crate) async fn connect_to_nats(urls: &[Url]) -> Result<Self, NatsError> {
        let connect_options = async_nats::ConnectOptions::default().retry_on_initial_connect();
        let client = async_nats::connect_with_options(urls, connect_options).await?;
        let inbox = client.new_inbox();
        tracing::debug!("connected to nats");
        let context = jetstream::new(client);

        Ok(Self { inbox, context })
    }

    pub(crate) async fn stream(self, args: StreamArgs) -> Result<ConfiguredStream, NatsError> {
        let StreamArgs {
            name,
            subject,
            description,
        } = args;
        let config = jetstream::stream::Config {
            name,
            discard: jetstream::stream::DiscardPolicy::Old,
            subjects: vec![subject],
            retention: jetstream::stream::RetentionPolicy::Limits,
            duplicate_window: Duration::from_secs(60 * 5),
            description: Some(description),
            allow_rollup: true,
            deny_delete: true,
            allow_direct: true,
            ..Default::default()
        };
        tracing::debug!(?config, "create or get stream with config");
        let stream = self.context.get_or_create_stream(config).await?;
        tracing::debug!("stream ready");

        Ok(ConfiguredStream {
            inbox: self.inbox,
            context: self.context,
            stream,
        })
    }
}

/// Nats stream
pub struct ConfiguredStream {
    inbox: String,
    context: jetstream::context::Context,
    stream: jetstream::stream::Stream,
}

impl ConfiguredStream {
    #[must_use]
    pub(crate) fn publisher<T: QueueMsgId>(self, subject: String) -> publisher::NatsPublisher<T> {
        publisher::NatsPublisher::new(self.context, self.stream, subject)
    }

    pub(crate) async fn consumer<T: core::fmt::Debug>(
        self,
        description: String,
        deliver_group: String,
    ) -> Result<consumer::NatsConsumer<T>, NatsError> {
        let config = jetstream::consumer::push::Config {
            deliver_subject: self.inbox.clone(),
            name: Some(uuid::Uuid::new_v4().to_string()),
            deliver_group: Some(deliver_group),
            description: Some(description),
            deliver_policy: jetstream::consumer::DeliverPolicy::New,
            ack_policy: jetstream::consumer::AckPolicy::Explicit,
            ack_wait: Duration::from_secs(10),
            max_deliver: 10,
            replay_policy: async_nats::jetstream::consumer::ReplayPolicy::Instant,
            sample_frequency: 10,
            max_ack_pending: 100,
            flow_control: true,
            idle_heartbeat: Duration::from_secs(10),
            inactive_threshold: Duration::from_secs(15),
            ..Default::default()
        };

        tracing::debug!(?config, "create or get consumer with config");
        let consumer = self
            .stream
            .create_consumer(config)
            .await
            .map_err(NatsError::CreateConsumer)?;
        tracing::debug!("consumer ready");
        let consumer = consumer::NatsConsumer::new(consumer);
        Ok(consumer)
    }
}
