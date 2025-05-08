use core::fmt::Debug;

use async_nats::jetstream;
use url::Url;

use super::consumer::NatsConsumer;
use super::publisher::NatsPublisher;
use super::{NatsError, kv_store};
use crate::interfaces::publisher::QueueMsgId;
use crate::nats::{Builder, StreamArgs};

/// Establishes a connection to the NATS server and creates a consumer for a specific stream.
///
/// This function connects to a NATS server using the provided URLs, sets up a stream based on the
/// given stream arguments, and creates a consumer that will handle messages of type `Message`.
///
/// # Type Parameters
///
/// * `Message` - The type of messages to be consumed. Must implement `Debug` trait.
///
/// # Arguments
///
/// * `urls` - A slice of `Url`s pointing to NATS server instances to connect to. The function will
///   attempt to connect to any of these URLs.
/// * `stream` - A `StreamArgs` struct containing configuration for the stream to connect to,
///   including name, subject pattern, and description.
/// * `consumer_desc` - A description for the consumer being created. This helps identify the
///   purpose of this consumer in monitoring and debugging.
/// * `deliver_group` - The delivery group name for the consumer. Multiple consumers in the same
///   delivery group will load balance messages between them.
///
/// # Returns
///
/// * `Result<NatsConsumer<Message>, NatsError>` - On success, returns a NATS consumer configured to
///   receive messages of type `Message`. On failure, returns a `NatsError`.
///
/// # Errors
///
/// This function may fail if:
/// * Connection to the NATS server cannot be established.
/// * The stream cannot be created or accessed.
/// * The consumer cannot be created or configured.
///
/// # Example
///
/// ```rust
/// use url::Url;
/// use infrastructure::nats::StreamArgs;
/// use infrastructure::nats::connectors::connect_consumer;
/// use infrastructure::nats::NatsError;
/// use infrastructure::nats::consumer::NatsConsumer;
/// use borsh::BorshSerialize;
///
/// // Define a type that implements the required traits
/// #[derive(Debug, BorshSerialize)]
/// struct MyEvent {
///     id: String,
///     // other fields...
/// }
/// #[tokio::main]
/// async fn main() -> Result<(), NatsError> {
///     let urls = vec![
///         Url::parse("nats://localhost:4222").unwrap(),
///     ];
///
///     let stream = StreamArgs {
///         name: "events".to_owned(),
///         subject: "events.>".to_owned(),
///         description: "Stream for all system events".to_owned(),
///     };
///
///     let consumer: NatsConsumer<MyEvent> = connect_consumer(
///         &urls,
///         stream,
///         "Event processor".to_string(),
///         "event-processors".to_string(),
///     ).await?;
///
///     // Now use the consumer to process events...
///     Ok(())
/// }
/// ```
#[tracing::instrument]
pub async fn connect_consumer<Message: Debug>(
    urls: &[Url],
    stream: StreamArgs,
    consumer_desc: String,
    deliver_group: String,
) -> Result<NatsConsumer<Message>, NatsError> {
    let consumer = Builder::connect_to_nats(urls)
        .await?
        .stream(stream)
        .await?
        .consumer(consumer_desc, deliver_group)
        .await?;
    Ok(consumer)
}

/// Establishes a connection to the NATS server and creates a publisher for a specific stream.
///
/// This function connects to a NATS server using the provided URLs, sets up a stream based on the
/// given stream arguments, and creates a publisher that will send messages of the specified type.
///
/// # Type Parameters
///
/// * `Message` - The type of messages to be published. Must implement `QueueMsgId`
///
/// # Arguments
///
/// * `urls` - A slice of `Url`s pointing to NATS server instances to connect to. The function will
///   attempt to connect to any of these URLs.
/// * `stream` - A `StreamArgs` struct containing configuration for the stream to publish to,
///   including name, subject pattern, and description.
/// * `subject` - The specific subject to publish messages to. This should match the stream's
///   subject pattern.
///
/// # Returns
///
/// * `Result<NatsPublisher<Message>, NatsError>` - On success, returns a NATS publisher configured
///   to send messages of type `Message`. On failure, returns a `NatsError`.
///
/// # Errors
///
/// This function may fail if:
/// * Connection to the NATS server cannot be established.
/// * The stream cannot be created or accessed.
///
/// # Example
///
/// ```
/// use url::Url;
/// use borsh::BorshSerialize;
/// use infrastructure::nats::publisher::NatsPublisher;
/// use infrastructure::nats::connectors::connect_publisher;
/// use infrastructure::nats::StreamArgs;
/// use infrastructure::nats::NatsError;
/// use infrastructure::interfaces::publisher::{Publisher, PublishMessage};
/// use infrastructure::interfaces::publisher::QueueMsgId;
///
/// // Define a type that implements the required traits
/// #[derive(BorshSerialize, core::fmt::Debug)]
/// struct MyEvent {
///     id: String,
///     // other fields...
/// }
///
/// // Implement QueueMsgId for MyEvent
/// impl QueueMsgId for MyEvent {
///     type MessageId = String;
///     fn id(&self) -> String {
///         self.id.clone()
///     }
/// }
///
/// async fn publish_example() -> Result<(), NatsError> {
///     // Connect to NATS and create a publisher for MyEvent messages
///     let urls = vec![
///         Url::parse("nats://localhost:4222").unwrap(),
///     ];
///
///     let stream = StreamArgs {
///         name: "events".to_owned(),
///         subject: "events.>".to_owned(),
///         description: "Stream for all system events".to_owned(),
///     };
///
///     let publisher: NatsPublisher<MyEvent> = connect_publisher(
///         &urls,
///         stream,
///         "events.system".to_owned(),
///     ).await?;
///
///     // Create an event to publish
///     let event = MyEvent {
///         id: "example-id".to_owned(),
///         // set other fields...
///     };
///
///     let publish_message = PublishMessage {
///         deduplication_id: "unique_id".to_owned(),
///         data: event
///     };
///
///     // Publish the event
///     publisher.publish(publish_message).await?;
///
///     Ok(())
/// }
/// ```
#[tracing::instrument]
pub async fn connect_publisher<Message: QueueMsgId>(
    urls: &[Url],
    stream: StreamArgs,
    subject: String,
) -> Result<NatsPublisher<Message>, NatsError> {
    let publisher = Builder::connect_to_nats(urls)
        .await?
        .stream(stream)
        .await?
        .publisher(subject);
    Ok(publisher)
}

/// Connects to a NATS ``JetStream`` Key-Value store with the specified configuration.
///
/// This function establishes a connection to a NATS server and creates or gets a Key-Value
/// store (bucket) that can be used to store and retrieve values of type `T`.
///
/// # Type Parameters
///
/// * `T` - The type of values to be stored in the Key-Value store. The serialization and
///   deserialization mechanism is handled by the `NatsKvStore` implementation.
///
/// # Arguments
///
/// * `urls` - A slice of `Url`s pointing to NATS server instances to connect to. The function will
///   attempt to connect to any of these URLs with retry on initial connection.
/// * `bucket` - The name of the Key-Value bucket to create or use. This serves as a namespace for
///   the stored values.
/// * `description` - A description for the Key-Value store, useful for identifying its purpose in
///   monitoring and debugging.
///
/// # Returns
///
/// * `Result<NatsKvStore<T>, NatsError>` - On success, returns a client for the Key-Value store
///   that can be used to put, get, and delete values of type `T`. On failure, returns a
///   `NatsError`.
///
/// # Errors
///
/// This function may return an error if:
/// * Connection to the NATS server cannot be established
/// * The Key-Value store cannot be created due to permission issues or invalid configuration
/// * There are network issues during connection
///
/// # KV Store Configuration
///
/// The Key-Value store is created with default configuration settings except for:
/// * The specified bucket name
/// * The provided description
///
/// # Example
///
/// ```
/// use url::Url;
/// use borsh::{BorshDeserialize, BorshSerialize};
/// use infrastructure::nats::connectors::connect_kv_store;
/// use infrastructure::nats::kv_store::NatsKvStore;
/// use infrastructure::nats::NatsError;
///
/// // Define a type for our values
/// #[derive(BorshSerialize, BorshDeserialize)]
/// struct UserProfile {
///     username: String,
///     email: String,
/// }
///
/// async fn setup_kv_store() -> Result<NatsKvStore<UserProfile>, NatsError> {
///     let urls = vec![Url::parse("nats://localhost:4222").unwrap()];
///
///     // Connect to KV store for UserProfile data
///     let kv_client = connect_kv_store::<UserProfile>(
///         &urls,
///         "user_profiles".to_string(),
///         "Storage for user profile information".to_string(),
///     ).await?;
///
///     // Now the KV client is ready to store and retrieve UserProfile objects
///     Ok(kv_client)
/// }
/// ```
#[tracing::instrument]
pub async fn connect_kv_store<T>(
    urls: &[Url],
    bucket: String,
    description: String,
) -> Result<kv_store::NatsKvStore<T>, NatsError> {
    let connect_options = async_nats::ConnectOptions::default().retry_on_initial_connect();
    let client = async_nats::connect_with_options(urls, connect_options).await?;
    let context = jetstream::new(client);
    let store = context
        .create_key_value(async_nats::jetstream::kv::Config {
            bucket: bucket.clone(),
            description,
            ..Default::default()
        })
        .await?;

    let store = kv_store::NatsKvStore::new(bucket, store);
    Ok(store)
}
