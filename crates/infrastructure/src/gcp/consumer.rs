use core::fmt::Debug;
use core::marker::PhantomData;

use borsh::BorshDeserialize;
use google_cloud_pubsub::client::Client;
use google_cloud_pubsub::subscriber::{ReceivedMessage, SubscriberConfig};
use google_cloud_pubsub::subscription::{ReceiveConfig, Subscription};
use tokio_util::sync::CancellationToken;

use super::GcpError;
use super::util::get_subscription;
use crate::interfaces;

/// Decoded queue message
#[derive(Debug)]
pub struct GcpMessage<T> {
    msg: ReceivedMessage,
    decoded: T,
    ack_deadline_secs: i32,
}

impl<T: BorshDeserialize + Send + Sync + Debug> GcpMessage<T> {
    fn decode(msg: ReceivedMessage, ack_deadline_secs: i32) -> Result<Self, GcpError> {
        tracing::debug!(?msg, "decoding msg");
        let decoded =
            T::deserialize(&mut msg.message.data.as_ref()).map_err(GcpError::Deserialize)?;
        tracing::debug!(?decoded, "decoded msg");
        Ok(Self {
            msg,
            decoded,
            ack_deadline_secs,
        })
    }
}

impl<T: Debug> interfaces::consumer::QueueMessage<T> for GcpMessage<T> {
    fn decoded(&self) -> &T {
        &self.decoded
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn ack(&self, ack_kind: interfaces::consumer::AckKind) -> Result<(), GcpError> {
        tracing::debug!(?ack_kind, "sending ack");

        match ack_kind {
            interfaces::consumer::AckKind::Ack => self
                .msg
                .ack()
                .await
                .map_err(|err| GcpError::Ack(Box::new(err)))?,
            interfaces::consumer::AckKind::Nak => self
                .msg
                .nack()
                .await
                .map_err(|err| GcpError::Nak(Box::new(err)))?,
            interfaces::consumer::AckKind::Progress => self
                .msg
                .modify_ack_deadline(self.ack_deadline_secs)
                .await
                .map_err(|err| GcpError::ModifyAckDeadline(Box::new(err)))?,
        }
        tracing::debug!("ack sent");
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
        message_buffer_size: usize,
        ack_deadline_secs: i32,
        cancel_token: CancellationToken,
    ) -> Result<Self, GcpError> {
        let subscription = get_subscription(client, subscription).await?;

        let (sender, receiver) = flume::bounded(message_buffer_size);

        // NOTE: clone for supervised monolithic binary
        let cancel_token = cancel_token.child_token();

        let read_messages_handle = start_read_messages_task(
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
    T: BorshDeserialize + Debug,
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
}

/// Starts tokio task to read from subscription to relay (messages) to
/// receiver channel
fn start_read_messages_task<T>(
    subscription: Subscription,
    sender: flume::Sender<Result<GcpMessage<T>, GcpError>>,
    ack_deadline_secs: i32,
    cancel_token: CancellationToken,
) -> tokio::task::JoinHandle<Result<(), GcpError>>
where
    T: BorshDeserialize + Send + Sync + Debug + 'static,
{
    let num_cpu = num_cpus::get();
    // TODO: Move to config
    let receive_config = ReceiveConfig {
        channel_capacity: Some(100),
        worker_count: num_cpu.checked_mul(2).unwrap_or(num_cpu),
        subscriber_config: Some(SubscriberConfig {
            stream_ack_deadline_seconds: ack_deadline_secs,
            ..Default::default()
        }),
    };
    tokio::spawn(async move {
        subscription
            .receive(
                move |message, cancel| {
                    let sender = sender.clone();
                    async move {
                        tracing::debug!(?message, "got message");
                        match GcpMessage::decode(message, ack_deadline_secs) {
                            Ok(decoded_message) => {
                                if let Err(err) = sender.send_async(Ok(decoded_message)).await {
                                    tracing::info!(?err, "shutting down");
                                    cancel.cancel();
                                }
                            }
                            Err(err) => {
                                if let Err(err) = sender.send_async(Err(err)).await {
                                    tracing::info!(?err, "shutting down");
                                    cancel.cancel();
                                }
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

impl<T> Drop for GcpConsumer<T> {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}
