use core::fmt::Debug;
use core::marker::PhantomData;
use std::sync::Arc;
use std::time::Instant;

use borsh::BorshDeserialize;
use google_cloud_pubsub::client::Client;
use google_cloud_pubsub::subscriber::{ReceivedMessage, SubscriberConfig};
use google_cloud_pubsub::subscription::{ReceiveConfig, Subscription};
use opentelemetry::metrics::{Counter, Histogram, ObservableGauge};
use opentelemetry::{KeyValue, global};
use redis::AsyncCommands as _;
use redis::aio::MultiplexedConnection;
use tokio_util::sync::CancellationToken;

use super::GcpError;
use super::util::get_subscription;
use crate::gcp::publisher::MSG_ID;
use crate::interfaces;
use crate::tracking::ThroughputTracker;

// NOTE: common max value in gcp pub/sub for apache beam & dataflow
const TEN_MINUTES_IN_SECS: u64 = 60 * 10;

/// Decoded queue message
pub struct GcpMessage<T> {
    id: String,
    decoded: T,
    subscription_name: String,
    msg: ReceivedMessage,
    redis_connection: MultiplexedConnection,
    received_timestamp: Instant,
    ack_deadline_secs: i32,
    metrics: Arc<Metrics>,
}

impl<T: Debug> Debug for GcpMessage<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GcpMessage")
            .field("id", &self.id)
            .field("subscription_name", &self.subscription_name)
            .field("msg", &self.msg)
            .field("decoded", &self.decoded)
            .finish_non_exhaustive()
    }
}

impl<T> GcpMessage<T> {
    fn redis_id_key(&self) -> String {
        format!("dedup:{}:id:{}", self.subscription_name, self.id)
    }
}

impl<T: BorshDeserialize + Send + Sync + Debug> GcpMessage<T> {
    #[tracing::instrument(skip_all)]
    fn decode(
        subscription_name: String,
        msg: ReceivedMessage,
        redis_connection: MultiplexedConnection,
        metrics: Arc<Metrics>,
        ack_deadline_secs: i32,
    ) -> Result<Self, GcpError> {
        let received_timestamp = Instant::now();
        tracing::debug!(?msg, "decoding msg");
        let id = msg
            .message
            .attributes
            .get(MSG_ID)
            .ok_or(GcpError::MsgIdNotSet)?
            .clone();

        let span = tracing::Span::current();
        span.record("message_id", &id);

        let decoded =
            T::deserialize(&mut msg.message.data.as_ref()).map_err(GcpError::Deserialize)?;
        tracing::debug!(?decoded, "successfully decoded message payload");

        Ok(Self {
            id,
            decoded,
            subscription_name,
            msg,
            redis_connection,
            received_timestamp,
            ack_deadline_secs,
            metrics,
        })
    }

    async fn is_processed(&mut self) -> Result<bool, GcpError> {
        tracing::debug!(message_id = %self.id, "checking if message was already processed");
        let exists: bool = self.redis_connection.exists(self.redis_id_key()).await?;
        if exists {
            tracing::info!(message_id = %self.id, "message already processed - will be skipped");
        }
        Ok(exists)
    }
}

impl<T: Debug + Send + Sync> interfaces::consumer::QueueMessage<T> for GcpMessage<T> {
    fn decoded(&self) -> &T {
        &self.decoded
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all, fields(message_id = %self.id, subscription = %self.subscription_name))]
    async fn ack(&mut self, ack_kind: interfaces::consumer::AckKind) -> Result<(), GcpError> {
        tracing::debug!(?ack_kind, "processing acknowledgment");

        match ack_kind {
            interfaces::consumer::AckKind::Ack => {
                tracing::debug!("sending positive acknowledgment to PubSub");
                self.msg
                    .ack()
                    .await
                    .map_err(|err| GcpError::Ack(Box::new(err)))?;

                self.metrics.record_ack(self.received_timestamp);
                tracing::debug!("acknowledged, storing message ID in Redis for deduplication...");

                // NOTE: regard this message with its id to be processed for the next 10 minutes
                let _: String = self
                    .redis_connection
                    .set_ex(self.redis_id_key(), "1", TEN_MINUTES_IN_SECS)
                    .await?;

                tracing::info!("message successfully acknowledged and marked as processed");
            }
            interfaces::consumer::AckKind::Nak => {
                tracing::debug!("sending negative acknowledgment to PubSub");
                self.msg
                    .nack()
                    .await
                    .map_err(|err| GcpError::Nak(Box::new(err)))?;
                self.metrics.record_nack();
                tracing::info!(
                    "message negatively acknowledged - might get redelivered based on configuration"
                );
            }
            interfaces::consumer::AckKind::Progress => {
                let old_deadline = self.ack_deadline_secs;
                self.ack_deadline_secs = self
                    .ack_deadline_secs
                    .checked_mul(2)
                    .unwrap_or(self.ack_deadline_secs);

                tracing::debug!(
                    old_deadline_seconds = old_deadline,
                    new_deadline_seconds = self.ack_deadline_secs,
                    "extending message acknowledgment deadline"
                );
                self.msg
                    .modify_ack_deadline(self.ack_deadline_secs)
                    .await
                    .map_err(|err| GcpError::ModifyAckDeadline(Box::new(err)))?;
                self.metrics.record_deadline_extension();
                tracing::info!("message deadline extended to allow more processing time");
            }
        }
        Ok(())
    }
}

/// Queue consumer
#[allow(clippy::module_name_repetitions, reason = "Descriptive name")]
pub struct GcpConsumer<T> {
    receiver: flume::Receiver<Result<GcpMessage<T>, GcpError>>,
    cancel_token: CancellationToken,
    read_messages_handle: tokio::task::JoinHandle<Result<(), GcpError>>,
    metrics: Arc<Metrics>,
    _phantom: PhantomData<T>,
}

/// Consumer config
#[derive(Debug)]
pub struct GcpConsumerConfig {
    /// Redis connection to store processed msg id for deduplication
    pub redis_connection: String,
    /// Deadline to ack message
    pub ack_deadline_secs: i32,
    /// Capacity of underlyng flume channel holding messages to consume
    pub channel_capacity: Option<usize>,
    /// Capacity of buffer to hold read messages from underlyng GCP provider
    pub message_buffer_size: usize,
    /// Underlying count of workers reading from GCP pubsub provider subscription
    pub worker_count: usize,
}

impl<T> GcpConsumer<T>
where
    T: BorshDeserialize + Send + Sync + Debug + 'static,
{
    #[tracing::instrument(
        name = "create_gcp_consumer",
        skip(client, cancel_token),
        fields(
            subscription = %subscription,
        )
    )]
    pub(crate) async fn new(
        client: &Client,
        subscription: &str,
        config: GcpConsumerConfig,
        cancel_token: CancellationToken,
    ) -> Result<Self, GcpError> {
        tracing::info!("initializing GCP PubSub consumer for subscription");
        let subscription = get_subscription(client, subscription).await?;

        let (sender, receiver) = flume::bounded(config.message_buffer_size);

        // NOTE: clone for supervised monolithic binary
        let cancel_token = cancel_token.child_token();

        tracing::debug!("connecting to Redis");
        let redis_connection = redis::Client::open(config.redis_connection)
            .map_err(GcpError::Connection)?
            .get_multiplexed_async_connection()
            .await
            .map_err(GcpError::Connection)?;

        tracing::info!(
            worker_count = config.worker_count,
            buffer_size = config.message_buffer_size,
            "starting message processing task"
        );

        let metrics = Arc::new(Metrics::new(subscription.fully_qualified_name()));

        let read_messages_handle = start_read_messages_task(ReadMessagesConfig {
            redis_connection,
            subscription,
            ack_deadline_secs: config.ack_deadline_secs,
            channel_capacity: config.channel_capacity,
            worker_count: config.worker_count,
            metrics: Arc::clone(&metrics),
            sender,
            cancel_token: cancel_token.clone(),
        });

        tracing::info!("GCP PubSub consumer successfully initialized");

        Ok(Self {
            receiver,
            cancel_token,
            read_messages_handle,
            metrics,
            _phantom: PhantomData,
        })
    }
}

impl<T> interfaces::consumer::Consumer<T> for GcpConsumer<T>
where
    T: BorshDeserialize + Debug + Send + Sync,
{
    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn messages(
        &self,
    ) -> Result<
        impl futures::Stream<Item = Result<impl interfaces::consumer::QueueMessage<T>, GcpError>>,
        GcpError,
    > {
        if self.read_messages_handle.is_finished() {
            self.metrics.record_error();
            return Err(GcpError::ConsumerReadTaskExited);
        }
        tracing::debug!("getting message stream");

        Ok(self.receiver.stream())
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    async fn check_health(&self) -> Result<(), GcpError> {
        tracing::debug!("checking health for GCP consumer");

        // Check if the read_messages_handle is still running
        if self.read_messages_handle.is_finished() {
            // The task has completed, which means the consumer is not healthy
            // We can't await the handle directly since we only have a shared reference
            tracing::error!("GCP consumer task has exited unexpectedly");
            return Err(GcpError::ConsumerReadTaskExited);
        }

        // If the join handle is still running, the consumer is healthy
        tracing::debug!("GCP consumer health check successful");
        Ok(())
    }
}

struct ReadMessagesConfig<T> {
    redis_connection: MultiplexedConnection,
    subscription: Subscription,
    ack_deadline_secs: i32,
    channel_capacity: Option<usize>,
    worker_count: usize,
    metrics: Arc<Metrics>,
    sender: flume::Sender<Result<GcpMessage<T>, GcpError>>,
    cancel_token: CancellationToken,
}

/// Starts tokio task to read from subscription to relay (messages) to
/// receiver channel
fn start_read_messages_task<T>(
    config: ReadMessagesConfig<T>,
) -> tokio::task::JoinHandle<Result<(), GcpError>>
where
    T: BorshDeserialize + Send + Sync + Debug + 'static,
{
    let ReadMessagesConfig {
        redis_connection,
        subscription,
        ack_deadline_secs,
        channel_capacity,
        worker_count,
        metrics,
        sender,
        cancel_token,
    } = config;

    let receive_config = ReceiveConfig {
        channel_capacity,
        worker_count,
        subscriber_config: Some(SubscriberConfig {
            stream_ack_deadline_seconds: ack_deadline_secs,
            ..Default::default()
        }),
    };
    let subscription_name = subscription.fully_qualified_name().to_owned();
    tracing::info!(
        subscription = %subscription_name,
        "starting message read task"
    );
    tokio::spawn(async move {
        let background_span = tracing::span!(tracing::Level::INFO, "pubsub_message_receiver");
        let _background_guard = background_span.enter();

        tracing::info!("starting PubSub receiver loop");
        subscription
            .receive(
                move |message, cancel| {
                    metrics.record_received();
                    let sender = sender.clone();
                    let subscription_name = subscription_name.clone();
                    let redis_connection = redis_connection.clone();
                    let metrics = Arc::clone(&metrics);
                    async move {
                        tracing::debug!(?message, "received message from PubSub");
                        match GcpMessage::decode(
                            subscription_name,
                            message,
                            redis_connection,
                            Arc::clone(&metrics),
                            ack_deadline_secs,
                        ) {
                            Ok(mut message) => match message.is_processed().await {
                                Ok(true) => {
                                    tracing::info!(
                                        message_id = %message.id,
                                        "message already processed, acknowledging duplicate"
                                    );
                                    metrics.record_duplicate();
                                    if let Err(err) = message
                                        .msg
                                        .ack()
                                        .await
                                        .map_err(|err| GcpError::Ack(Box::new(err)))
                                    {
                                        tracing::error!(
                                            ?err,
                                            "failed to acknowledge duplicate message, forwarding..."
                                        );
                                        forward_message(Err(err), sender, metrics, cancel).await;
                                    }
                                }
                                Ok(false) => {
                                    tracing::debug!(
                                        message_id = %message.id,
                                        "new message, forwarding..."
                                    );
                                    forward_message(Ok(message), sender, metrics, cancel).await;
                                }
                                Err(err) => {
                                    tracing::error!(
                                        ?err,
                                        message_id = %message.id,
                                        "failed to check if message was processed, forwarding..."
                                    );

                                    forward_message(Err(err), sender, metrics, cancel).await;
                                }
                            },
                            Err(err) => {
                                tracing::error!(
                                    ?err,
                                    "failed to decode message from PubSub, forwarding..."
                                );
                                forward_message(Err(err), sender, metrics, cancel).await;
                            }
                        }
                    }
                },
                cancel_token.clone(),
                Some(receive_config),
            )
            .await
            .map_err(|err| GcpError::ReceiverTaskCrash(Box::new(err)))
    })
}

async fn forward_message<T>(
    message_result: Result<GcpMessage<T>, GcpError>,
    sender: flume::Sender<Result<GcpMessage<T>, GcpError>>,
    metrics: Arc<Metrics>,
    cancel: CancellationToken,
) {
    if message_result.is_err() {
        metrics.record_error();
    }

    if let Err(err) = sender.send_async(message_result).await {
        tracing::info!(?err, "shutting down");
        cancel.cancel();
    }
}

impl<T> Drop for GcpConsumer<T> {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}

#[derive(Clone)]
struct Metrics {
    throughput_tracker: Arc<ThroughputTracker>,
    received_counter: Counter<u64>,
    duplicates_counter: Counter<u64>,
    errors_counter: Counter<u64>,

    processing_time: Histogram<f64>,
    #[allow(dead_code, reason = "called by pipes")]
    messages_per_second: ObservableGauge<f64>,
    acks_counter: Counter<u64>,
    nacks_counter: Counter<u64>,
    deadline_extensions_counter: Counter<u64>,

    attributes: [KeyValue; 1],
}

impl Metrics {
    fn new(subscription_name: &str) -> Self {
        let meter = global::meter("pubsub_consumer");
        let throughput_tracker = Arc::new(ThroughputTracker::new());

        let attributes = [KeyValue::new(
            "subscription.name",
            subscription_name.to_owned(),
        )];

        let received_counter = meter
            .u64_counter("messages.received")
            .with_description("Total number of messages received from PubSub")
            .build();

        let duplicates_counter = meter
            .u64_counter("messages.duplicates")
            .with_description("Total number of duplicate messages detected")
            .build();

        let errors_counter = meter
            .u64_counter("messages.errors")
            .with_description("Total number of errors encountered during message processing")
            .build();

        let processing_time = meter
            .f64_histogram("messages.processing_time")
            .with_description("Time taken to process messages in seconds")
            .with_unit("ms")
            .build();

        let observer_tracker = Arc::clone(&throughput_tracker);
        let throughput_attr = attributes.clone();
        let messages_per_second = meter
            .f64_observable_gauge("messages.throughput")
            .with_description("Messages processed per second")
            .with_unit("messages/s")
            .with_callback(move |observer| {
                if let Some(rate) = observer_tracker.update_and_get_rate() {
                    observer.observe(rate, &throughput_attr);
                }
            })
            .build();

        let acks_counter = meter
            .u64_counter("messages.acks")
            .with_description("Total number of messages acknowledged")
            .build();

        let nacks_counter = meter
            .u64_counter("messages.nacks")
            .with_description("Total number of messages negatively acknowledged")
            .build();

        let deadline_extensions_counter = meter
            .u64_counter("messages.deadline_extensions")
            .with_description("Total number of message deadline extensions")
            .build();

        Self {
            throughput_tracker,
            received_counter,
            duplicates_counter,
            errors_counter,
            processing_time,
            messages_per_second,
            acks_counter,
            nacks_counter,
            deadline_extensions_counter,
            attributes,
        }
    }

    fn record_received(&self) {
        self.received_counter.add(1, &self.attributes);
    }

    #[allow(clippy::as_conversions, reason = "checked")]
    #[allow(clippy::cast_precision_loss, reason = "checked")]
    fn record_ack(&self, received_timestamp: Instant) {
        let now = Instant::now();
        let elapsed_ms = now.duration_since(received_timestamp).as_millis();
        let elapsed_ms: f64 = if let Ok(ms_u64) = TryInto::<u64>::try_into(elapsed_ms) {
            ms_u64 as f64
        } else {
            tracing::warn!(
                "elapsed_ms u128 to f64 conversion overflow: {}ms",
                elapsed_ms
            );
            f64::MAX
        };

        self.processing_time.record(elapsed_ms, &self.attributes);
        self.acks_counter.add(1, &self.attributes);
        self.throughput_tracker.record_processed_message();
    }

    fn record_nack(&self) {
        self.nacks_counter.add(1, &self.attributes);
    }

    fn record_deadline_extension(&self) {
        self.deadline_extensions_counter.add(1, &self.attributes);
    }

    fn record_duplicate(&self) {
        self.duplicates_counter.add(1, &self.attributes);
    }

    fn record_error(&self) {
        self.errors_counter.add(1, &self.attributes);
    }
}
