/// Consumer interfaces
#[cfg(feature = "consumer-interfaces")]
pub mod consumer {
    use core::error::Error;
    use core::fmt::Debug;

    /// queue message trait
    pub trait QueueMessage<T>: Debug + Send {
        /// Decoded message
        fn decoded(&self) -> &T;
        /// Ack response
        fn ack(
            &mut self,
            ack_kind: AckKind,
        ) -> impl Future<Output = Result<(), impl Error + Send + Sync + 'static>> + Send;
    }

    /// consumer
    pub trait Consumer<T: Debug> {
        /// messages stream
        fn messages(
            &self,
        ) -> impl Future<
            Output = Result<
                impl futures::Stream<
                    Item = Result<impl QueueMessage<T>, impl Error + Send + Sync + 'static>,
                > + Send,
                impl Error + Send + Sync + 'static,
            >,
        > + Send;

        /// Checks the health status of the consumer connection.
        ///
        /// This method verifies the consumer's ability to connect to the underlying service
        /// and returns an error if the health check fails.
        ///
        /// # Returns
        ///
        /// * `Ok(())` - If the consumer is healthy and can connect to the service
        /// * `Err(...)` - If there are any issues with the consumer's health or connectivity
        fn check_health(
            &self,
        ) -> impl Future<Output = Result<(), impl Error + Send + Sync + 'static>> + Send;
    }

    /// Ack responses
    #[derive(Debug)]
    pub enum AckKind {
        /// Acknowledges a message was completely handled.
        Ack,
        /// Signals that the message will not be processed now
        /// and processing can move onto the next message, NAK'd
        /// message will be retried.
        Nak,
        /// When sent before the `AckWait` period indicates that
        /// work is ongoing and the period should be extended by
        /// another equal to `AckWait`.
        Progress,
    }
}

/// Publish interfaces
#[cfg(feature = "publisher-interfaces")]
pub mod publisher {
    use core::error::Error;
    use core::fmt::Display;

    /// Generic trait for Id on a type
    pub trait QueueMsgId {
        /// type of message id
        type MessageId: Display;
        /// return id
        fn id(&self) -> Self::MessageId;
    }

    /// trait for peekable publisher
    pub trait PeekMessage<T: QueueMsgId> {
        /// Get last message from queue without consuming
        fn peek_last(
            &mut self,
        ) -> impl Future<Output = Result<Option<T::MessageId>, impl Error + Send + Sync + 'static>>;
    }

    /// Publish Messsage
    pub struct PublishMessage<T> {
        /// Deduplication id
        pub deduplication_id: String,
        /// Data
        pub data: T,
    }

    impl<T: QueueMsgId> From<T> for PublishMessage<T> {
        fn from(value: T) -> Self {
            Self {
                deduplication_id: value.id().to_string(),
                data: value,
            }
        }
    }

    /// publisher
    #[allow(clippy::impl_trait_in_params, reason = "improves readability")]
    pub trait Publisher<T> {
        /// Return type
        type Return;
        /// Publish message to queue
        fn publish(
            &self,
            msg: PublishMessage<T>,
        ) -> impl Future<Output = Result<Self::Return, impl Error + Send + Sync + 'static>> + Send;

        /// Publish batch to queue
        fn publish_batch(
            &self,
            batch: Vec<PublishMessage<T>>,
        ) -> impl Future<Output = Result<Vec<Self::Return>, impl Error + Send + Sync + 'static>>;

        /// Checks the health status of the publisher connection.
        ///
        /// This method verifies the publisher's ability to connect to the underlying service
        /// and returns an error if the health check fails.
        ///
        /// # Returns
        ///
        /// * `Ok(())` - If the publisher is healthy and can connect to the service
        /// * `Err(...)` - If there are any issues with the publisher's health or connectivity
        fn check_health(
            &self,
        ) -> impl Future<Output = Result<(), impl Error + Send + Sync + 'static>> + Send;
    }
}

/// Kv store interfaces
#[cfg(feature = "storage-interfaces")]
pub mod kv_store {
    use core::error::Error;
    use core::fmt::{Debug, Display};

    /// Value with revision
    #[derive(Debug)]
    pub struct WithRevision<T> {
        /// value
        pub value: T,
        /// revision
        pub revision: u64,
    }

    impl<T: Display> Display for WithRevision<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "{} (rev: {})", self.value, self.revision)
        }
    }

    /// ``KvStore`` interface
    pub trait KvStore<T> {
        /// Update value in kvstore
        fn update(
            &self,
            data: &WithRevision<T>,
        ) -> impl Future<Output = Result<u64, impl Error + Send + Sync + 'static>> + Send;

        /// Create value in kvstore
        fn put(
            &self,
            value: &T,
        ) -> impl Future<Output = Result<u64, impl Error + Send + Sync + 'static>> + Send;

        /// Get value from kvstore
        fn get(
            &self,
        ) -> impl Future<
            Output = Result<Option<WithRevision<T>>, impl Error + Send + Sync + 'static>,
        > + Send;
    }
}
