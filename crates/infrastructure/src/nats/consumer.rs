use core::fmt::Debug;
use core::marker::PhantomData;

use async_nats::jetstream;
use borsh::BorshDeserialize;
use futures::StreamExt as _;

use super::NatsError;
use crate::interfaces;

impl From<interfaces::consumer::AckKind> for jetstream::AckKind {
    fn from(val: interfaces::consumer::AckKind) -> Self {
        match val {
            interfaces::consumer::AckKind::Ack => Self::Ack,
            interfaces::consumer::AckKind::Nak => Self::Nak(None),
            interfaces::consumer::AckKind::Progress => Self::Progress,
        }
    }
}

/// decoded queue message
#[derive(Debug)]
pub struct NatsMessage<T> {
    decoded: T,
    msg: jetstream::Message,
}

impl<T: BorshDeserialize + Debug> NatsMessage<T> {
    fn decode(msg: jetstream::Message) -> Result<Self, NatsError> {
        tracing::debug!(?msg, "decoding msg");
        let decoded = T::deserialize(&mut msg.payload.as_ref()).map_err(NatsError::Deserialize)?;
        tracing::debug!(?decoded, "decoded msg");
        Ok(Self { decoded, msg })
    }
}

impl<T: Debug + Send + Sync> interfaces::consumer::QueueMessage<T> for NatsMessage<T> {
    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn ack(&self, ack_kind: interfaces::consumer::AckKind) -> Result<(), NatsError> {
        tracing::debug!(?ack_kind, "sending ack");
        self.msg
            .ack_with(ack_kind.into())
            .await
            .map_err(NatsError::Ack)?;
        tracing::debug!("ack sent");
        Ok(())
    }

    fn decoded(&self) -> &T {
        &self.decoded
    }
}

/// Queue consumer
#[allow(clippy::module_name_repetitions, reason = "Descriptive name")]
pub struct NatsConsumer<T> {
    consumer_inner: jetstream::consumer::Consumer<jetstream::consumer::push::Config>,
    _phantom: PhantomData<T>,
}

impl<T> NatsConsumer<T> {
    pub(crate) const fn new(
        consumer: jetstream::consumer::Consumer<jetstream::consumer::push::Config>,
    ) -> Self {
        Self {
            consumer_inner: consumer,
            _phantom: PhantomData,
        }
    }
}

impl<T> interfaces::consumer::Consumer<T> for NatsConsumer<T>
where
    T: BorshDeserialize + Debug + Send + Sync,
{
    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip_all)]
    async fn messages(
        &self,
    ) -> Result<
        impl futures::Stream<Item = Result<impl interfaces::consumer::QueueMessage<T>, NatsError>>,
        NatsError,
    > {
        tracing::debug!("getting message stream");
        let stream = self
            .consumer_inner
            .messages()
            .await
            .map_err(NatsError::MessagesStream)?;

        let decoded_stream = stream.then(|msg_result| async {
            match msg_result {
                Ok(msg) => NatsMessage::decode(msg),
                Err(err) => Err(NatsError::from(err)),
            }
        });

        Ok(decoded_stream)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    async fn check_health(&self) -> Result<(), NatsError> {
        tracing::debug!("checking health");

        // We have to clone the consumer because `info` mutates its state
        self.consumer_inner.clone().info().await?;

        Ok(())
    }
}
