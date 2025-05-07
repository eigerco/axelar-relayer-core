use core::fmt::Debug;

use borsh::{BorshDeserialize, BorshSerialize};
use google_cloud_pubsub::client::{Client, ClientConfig};
use tokio_util::sync::CancellationToken;

use super::GcpError;
use super::consumer::GcpConsumer;
use super::kv_store::RedisClient;
use super::publisher::{GcpPublisher, PeekableGcpPublisher};
use crate::interfaces::publisher::QueueMsgId;

/// Establishes a connection to Google Cloud Pub/Sub and creates a consumer for a specific
/// subscription.
///
/// This function creates a GCP Pub/Sub consumer that can receive and process messages of type `T`
/// from the specified subscription. The consumer handles connection management, message
/// deserialization, and allows acknowledgement/negative/extend acknowledgement.
///
/// # Type Parameters
///
/// * `T` - The type of messages to be consumed. Must implement [`Debug`], [`Send`], [`Sync`],
///   [`BorshDeserialize`], and have a `'static` lifetime.
///
/// # Arguments
///
/// * `subscription` - The GCP Pub/Sub subscription path to consume from, typically in the format
///   `projects/{project}/subscriptions/{subscription}`.
/// * `message_buffer_size` - The size of the internal message buffer. Controls how many message
///   will be in buffer to process.
/// * `nak_deadline_secs` - The deadline (in seconds) for message processing. If a message isn't
///   acknowledged within this deadline, GCP will attempt to redeliver it.
/// * `cancel_token` - A [`CancellationToken`] used to gracefully shut down the consumer when
///   needed.
///
/// # Returns
///
/// * `Result<GcpConsumer<T>, GcpError>` - On success, returns a configured GCP Pub/Sub consumer
///   ready to receive messages of type `T`. On failure, returns a [`GcpError`].
///
/// # Errors
///
/// This function may return an error if:
/// * Connection to GCP Pub/Sub cannot be established (i.e. no standard way to auth in gcp - gcloud
///   or env vars)
/// * The specified subscription doesn't exist or isn't accessible
/// * Authentication or authorization fails
/// * There are network issues during connection
///
/// # Example
///
/// ```
/// use std::time::Duration;
/// use borsh::BorshDeserialize;
/// use tokio_util::sync::CancellationToken;
/// use infrastructure::gcp::connectors::connect_consumer;
/// use infrastructure::gcp::GcpError;
/// use crate::infrastructure::interfaces::consumer::Consumer;
/// use futures::StreamExt as _;
/// use infrastructure::interfaces::consumer::AckKind;
/// use crate::infrastructure::interfaces::consumer::QueueMessage;
///
/// #[derive(Debug, BorshDeserialize)]
/// struct EventMessage {
///     id: String,
///     timestamp: u64,
///     payload: Vec<u8>,
/// }
///
/// async fn setup_consumer() -> Result<(), GcpError> {
///     // Create a cancellation token for graceful shutdown
///     let cancel_token = CancellationToken::new();
///
///     // Set up the consumer with a 30-second NAK deadline
///     let consumer = connect_consumer::<EventMessage>(
///         "projects/my-project/subscriptions/my-events",
///         100, // buffer size
///         30,  // NAK deadline in seconds
///         cancel_token.clone(),
///     ).await?;
///
///     // Process messages until cancellation is requested
///
///     consumer
///       .messages()
///       .await
///       .expect("could not retrieve messages")
///        .for_each_concurrent(10, move |queue_msg| async move {
///             let queue_msg = match queue_msg {
///                 Ok(queue_msg) => queue_msg,
///                 Err(err) => {
///                     tracing::error!(?err, "could not receive queue msg");
///                     return;
///                 }
///             };
///
///             queue_msg.ack(AckKind::Ack).await.expect("Failed to ack message");
///        });
///
///     // Later, when you want to shut down:
///     cancel_token.cancel();
///
///     Ok(())
/// }
/// ```
#[tracing::instrument]
pub async fn connect_consumer<T>(
    subscription: &str,
    redis_connection: String,
    message_buffer_size: usize,
    nak_deadline_secs: i32,
    cancel_token: CancellationToken,
) -> Result<GcpConsumer<T>, GcpError>
where
    T: BorshDeserialize + Send + Sync + Debug + 'static,
{
    let client = connect_client().await?;
    let consumer = GcpConsumer::new(
        &client,
        subscription,
        redis_connection,
        message_buffer_size,
        nak_deadline_secs,
        cancel_token,
    )
    .await?;

    Ok(consumer)
}
/// Creates and connects a Google Cloud Platform Publisher for the specified topic.
///
/// This function establishes a connection to Google Cloud Pub/Sub and creates a publisher
/// for the specified topic. It handles the client connection and publisher initialization
/// in a single convenient function.
///
/// # Type Parameters
///
/// * `T` - The type of messages that will be published. Must implement `Send` and `Sync` traits to
///   ensure thread safety when publishing messages.
///
/// # Arguments
///
/// * `topic` - The name of the Pub/Sub topic to connect to.
///
/// # Returns
///
/// * `Result<GcpPublisher<T>, GcpError>` - A Result containing either:
///   * `GcpPublisher<T>` - A connected publisher instance ready to publish messages of type `T`
///   * `GcpError` - Error that occurred during client connection or publisher initialization
///
/// # Errors
///
/// This function may fail if:
/// * The underlying client connection fails (authentication issues, network problems)
/// * The specified topic doesn't exist or the authenticated account lacks permissions
/// * The publisher creation fails for any reason
///
/// # Examples
///
/// ```
/// use infrastructure::gcp::connectors::connect_publisher;
/// use infrastructure::gcp::publisher::GcpPublisher;
/// use infrastructure::gcp::GcpError;
/// use crate::infrastructure::interfaces::publisher::Publisher;
///
///
/// #[derive(Debug, borsh::BorshSerialize)]
/// struct EventMessage {
///     id: String,
///     timestamp: u64,
///     payload: Vec<u8>,
/// }
///
/// async fn publish_example() -> Result<(), GcpError> {
///     // Connect to the "blockchain-transactions" topic
///     let publisher: GcpPublisher<EventMessage> = connect_publisher("events").await?;
///
///     let msg = EventMessage {
///       id: "something".to_owned(),
///       timestamp: 6,
///       payload: Vec::<_>::default()
///     };
///
///     // Create and publish
///     publisher.publish("".to_owned(), &msg).await?;
///
///     Ok(())
/// }
/// ```
#[tracing::instrument]
pub async fn connect_publisher<T>(topic: &str) -> Result<GcpPublisher<T>, GcpError> {
    let client = connect_client().await?;
    let publisher = GcpPublisher::new(&client, topic).await?;
    Ok(publisher)
}

/// Creates and connects a Peekable Google Cloud Platform Publisher for the specified topic with
/// Redis integration.
///
/// This function establishes a connection to Google Cloud Pub/Sub and creates a peekable publisher
/// for the specified topic. The peekable publisher extends the standard GCP Publisher functionality
/// by integrating with Redis to enable inspecting latest message without consuming it.
///
/// # Type Parameters
///
/// * `T` - The type of messages that will be published. Must implement the following traits:
///   * `Id` - For associating a unique identifier with each message, pushed as last msg id to Redis
///     and returned when peeking at last msg
///   * `Send` and `Sync` - To ensure thread safety when publishing messages
///   * `T::MessageId` must implement `BorshSerialize`, `BorshDeserialize` (to save in redis), ///
///     Display and Debug traits
///
/// # Arguments
///
/// * `topic` - The name of the Pub/Sub topic to connect to
/// * `redis_connection` - Connection string for the Redis instance
/// * `redis_key` - Key prefix to use for storing message data in Redis
///
/// # Returns
///
/// * `Result<PeekableGcpPublisher<T>, GcpError>` - A Result containing either:
///   * `PeekableGcpPublisher<T>` - A connected peekable publisher instance ready to publish
///     messages of type `T`
///   * `GcpError` - Error that occurred during client connection, publisher initialization, or
///     Redis connection
///
/// # Errors
///
/// This function may fail if:
/// * The Redis connection cannot be established
/// * The underlying GCP client connection fails (authentication issues, network problems)
/// * The specified topic doesn't exist or the authenticated account lacks permissions
/// * The peekable publisher creation fails for any reason
///
/// # Examples
///
/// ```
/// use infrastructure::gcp::connectors::connect_publisher;
/// use infrastructure::gcp::publisher::GcpPublisher;
/// use infrastructure::gcp::GcpError;
/// use crate::infrastructure::interfaces::publisher::Publisher;
/// use infrastructure::gcp::connectors::connect_peekable_publisher;
/// use infrastructure::gcp::publisher::PeekableGcpPublisher;
/// use crate::infrastructure::interfaces::publisher::PeekMessage;
///
///
/// #[derive(Clone, Debug, borsh::BorshSerialize)]
/// struct EventMessage {
///     id: String,
///     timestamp: u64,
///     payload: Vec<u8>,
/// }
///
/// // Implement common::Id for EventMessage
/// impl infrastructure::interfaces::publisher::QueueMsgId for EventMessage {
///     type MessageId = String;
///     fn id(&self) -> String {
///         self.id.clone()
///     }
/// }
/// async fn publish_with_peek_ability() -> Result<(), GcpError> {
///     let mut publisher: PeekableGcpPublisher<EventMessage> = connect_peekable_publisher(
///         "events-topic",
///         "redis://redis-server:6379".to_owned(),
///         "my-events".to_owned()
///     ).await?;
///
///     let msg = EventMessage {
///       id: "something".to_owned(),
///       timestamp: 6,
///       payload: Vec::<_>::default()
///     };
///
///     // Create and publish
///     publisher.publish("".to_owned(), &msg).await?;
///
///     // Later, we can peek at the transaction status
///     let msg_id = publisher.peek_last().await?;
///
///     Ok(())
/// }
/// ```
///
/// # Note
///
/// The `PeekableGcpPublisher` allows you to get latest published message id without consuming it
#[tracing::instrument]
pub async fn connect_peekable_publisher<T>(
    topic: &str,
    redis_connection: String,
    redis_key: String,
) -> Result<PeekableGcpPublisher<T>, GcpError>
where
    T: QueueMsgId,
    T::MessageId: BorshSerialize + BorshDeserialize + core::fmt::Display,
{
    let kv_store = RedisClient::connect(redis_key, redis_connection).await?;
    let client = connect_client().await?;
    let publisher = PeekableGcpPublisher::new(&client, topic, kv_store).await?;
    Ok(publisher)
}

async fn connect_client() -> Result<Client, GcpError> {
    let config = ClientConfig::default().with_auth().await?;
    let client = Client::new(config).await?;
    Ok(client)
}
