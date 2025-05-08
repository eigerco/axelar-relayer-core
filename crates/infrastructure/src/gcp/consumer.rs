use core::fmt::Debug;
use core::marker::PhantomData;

use borsh::BorshDeserialize;
use google_cloud_pubsub::client::Client;
use google_cloud_pubsub::subscriber::{ReceivedMessage, SubscriberConfig};
use google_cloud_pubsub::subscription::{ReceiveConfig, Subscription};
use redis::AsyncCommands as _;
use redis::aio::MultiplexedConnection;
use tokio_util::sync::CancellationToken;

use super::GcpError;
use super::util::get_subscription;
use crate::gcp::publisher::MSG_ID;
use crate::interfaces;

// NOTE: common max value in gcp pub/sub for apache beam & dataflow
const TEN_MINUTES_IN_SECS: u64 = 60 * 10;

/// Decoded queue message
#[derive(Debug)]
pub struct GcpMessage<T> {
    id: String,
    subscription_name: String,
    msg: ReceivedMessage,
    redis_connection: MultiplexedConnection,
    decoded: T,
    ack_deadline_secs: i32,
}

impl<T> GcpMessage<T> {
    fn redis_id_key(&self) -> String {
        format!("dedup:{}:id:{}", self.subscription_name, self.id)
    }
}

impl<T: BorshDeserialize + Send + Sync + Debug> GcpMessage<T> {
    fn decode(
        subscription_name: String,
        msg: ReceivedMessage,
        redis_connection: MultiplexedConnection,
        ack_deadline_secs: i32,
    ) -> Result<Self, GcpError> {
        tracing::debug!(?msg, "decoding msg");
        let decoded =
            T::deserialize(&mut msg.message.data.as_ref()).map_err(GcpError::Deserialize)?;
        tracing::debug!(?decoded, "decoded msg");

        let id = msg
            .message
            .attributes
            .get(MSG_ID)
            .ok_or(GcpError::MsgIdNotSet)?
            .clone();

        Ok(Self {
            id,
            subscription_name,
            msg,
            redis_connection,
            decoded,
            ack_deadline_secs,
        })
    }

    async fn is_processed(&mut self) -> Result<bool, GcpError> {
        let exists: bool = self.redis_connection.exists(self.redis_id_key()).await?;
        Ok(exists)
    }
}

impl<T: Debug + Send + Sync> interfaces::consumer::QueueMessage<T> for GcpMessage<T> {
    fn decoded(&self) -> &T {
        &self.decoded
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn ack(&mut self, ack_kind: interfaces::consumer::AckKind) -> Result<(), GcpError> {
        tracing::debug!(?ack_kind, "sending ack");

        match ack_kind {
            interfaces::consumer::AckKind::Ack => {
                self.msg
                    .ack()
                    .await
                    .map_err(|err| GcpError::Ack(Box::new(err)))?;

                tracing::debug!("acknowledged, updating redis...");

                // NOTE: regard this message with its id to be processed for the next 10 minutes
                let _: String = self
                    .redis_connection
                    .set_ex(self.redis_id_key(), "1", TEN_MINUTES_IN_SECS)
                    .await?;

                tracing::debug!("redis updated");
            }
            interfaces::consumer::AckKind::Nak => {
                self.msg
                    .nack()
                    .await
                    .map_err(|err| GcpError::Nak(Box::new(err)))?;
            }
            interfaces::consumer::AckKind::Progress => {
                self.ack_deadline_secs = self
                    .ack_deadline_secs
                    .checked_mul(2)
                    .unwrap_or(self.ack_deadline_secs);
                self.msg
                    .modify_ack_deadline(self.ack_deadline_secs)
                    .await
                    .map_err(|err| GcpError::ModifyAckDeadline(Box::new(err)))?;
            }
        }
        tracing::debug!("operation completed");
        Ok(())
    }
}

/// Queue consumer
#[allow(clippy::module_name_repetitions, reason = "Descriptive name")]
pub struct GcpConsumer<T> {
    receiver: flume::Receiver<Result<GcpMessage<T>, GcpError>>,
    cancel_token: CancellationToken,
    read_messages_handle: tokio::task::JoinHandle<Result<(), GcpError>>,
    _phantom: PhantomData<T>,
}

impl<T> GcpConsumer<T>
where
    T: BorshDeserialize + Send + Sync + Debug + 'static,
{
    pub(crate) async fn new(
        client: &Client,
        subscription: &str,
        redis_connection: String,
        message_buffer_size: usize,
        ack_deadline_secs: i32,
        cancel_token: CancellationToken,
    ) -> Result<Self, GcpError> {
        let subscription = get_subscription(client, subscription).await?;

        let (sender, receiver) = flume::bounded(message_buffer_size);

        // NOTE: clone for supervised monolithic binary
        let cancel_token = cancel_token.child_token();

        let redis_connection = redis::Client::open(redis_connection)
            .map_err(GcpError::Connection)?
            .get_multiplexed_async_connection()
            .await
            .map_err(GcpError::Connection)?;

        let read_messages_handle = start_read_messages_task(
            redis_connection,
            subscription,
            sender,
            ack_deadline_secs,
            cancel_token.clone(),
        );

        Ok(Self {
            receiver,
            cancel_token,
            read_messages_handle,
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

/// Starts tokio task to read from subscription to relay (messages) to
/// receiver channel
fn start_read_messages_task<T>(
    redis_connection: MultiplexedConnection,
    subscription: Subscription,
    sender: flume::Sender<Result<GcpMessage<T>, GcpError>>,
    ack_deadline_secs: i32,
    cancel_token: CancellationToken,
) -> tokio::task::JoinHandle<Result<(), GcpError>>
where
    T: BorshDeserialize + Send + Sync + Debug + 'static,
{
    let num_cpu = num_cpus::get();
    let receive_config = ReceiveConfig {
        channel_capacity: Some(num_cpu.checked_mul(10).unwrap_or(num_cpu)),
        worker_count: num_cpu.checked_mul(2).unwrap_or(num_cpu),
        subscriber_config: Some(SubscriberConfig {
            stream_ack_deadline_seconds: ack_deadline_secs,
            ..Default::default()
        }),
    };

    let subscription_name = subscription.fully_qualified_name().to_owned();
    tokio::spawn(async move {
        subscription
            .receive(
                move |message, cancel| {
                    let sender = sender.clone();
                    let subscription_name = subscription_name.clone();
                    let redis_connection = redis_connection.clone();
                    async move {
                        tracing::debug!(?message, "got message");
                        match GcpMessage::decode(
                            subscription_name,
                            message,
                            redis_connection,
                            ack_deadline_secs,
                        ) {
                            Ok(mut message) => match message.is_processed().await {
                                Ok(true) => {
                                    tracing::debug!(
                                        "message with id {} already processed, skipping",
                                        message.id
                                    );
                                    if let Err(err) = message
                                        .msg
                                        .ack()
                                        .await
                                        .map_err(|err| GcpError::Ack(Box::new(err)))
                                    {
                                        tracing::error!(?err, "failed to ack duplicate message");
                                        forward_message(Err(err), sender, cancel).await;
                                    }
                                }
                                Ok(false) => {
                                    forward_message(Ok(message), sender, cancel).await;
                                }
                                Err(err) => {
                                    forward_message(Err(err), sender, cancel).await;
                                }
                            },
                            Err(err) => {
                                forward_message(Err(err), sender, cancel).await;
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
    cancel: CancellationToken,
) {
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
