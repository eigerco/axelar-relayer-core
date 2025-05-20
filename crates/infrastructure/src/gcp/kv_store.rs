use core::fmt::{Debug, Display};
use core::marker::PhantomData;

use borsh::{BorshDeserialize, BorshSerialize};
use opentelemetry::metrics::Counter;
use opentelemetry::{KeyValue, global};
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands as _, Client};

use super::GcpError;
use crate::interfaces;
use crate::interfaces::kv_store::WithRevision;

/// Redis client
pub struct RedisClient<T> {
    key: String,
    connection: MultiplexedConnection,
    metrics: Metrics,
    _phantom: PhantomData<T>,
}

impl<T> RedisClient<T>
where
    T: BorshSerialize + BorshDeserialize + Display,
{
    /// Connect to redis
    /// # Errors
    ///  on connection issues
    pub async fn connect(key: String, connection: String) -> Result<Self, GcpError> {
        let connection = Client::open(connection)
            .map_err(GcpError::Connection)?
            .get_multiplexed_async_connection()
            .await
            .map_err(GcpError::Connection)?;

        let metrics = Metrics::new(&key);

        Ok(Self {
            key,
            connection,
            metrics,
            _phantom: PhantomData,
        })
    }

    /// Ping the Redis server to check connectivity
    /// # Errors
    ///  on connection issues
    pub async fn ping(&self) -> Result<(), redis::RedisError> {
        let mut connection = self.connection.clone();
        redis::cmd("PING")
            .query_async(&mut connection)
            .await
            .map_err(|err| {
                tracing::error!(?err, "redis healthcheck error");
                self.metrics.record_error();
                err
            })
    }

    pub(crate) async fn upsert(&self, value: &T) -> Result<(), GcpError> {
        tracing::trace!(%value, "upserting value");
        let bytes = borsh::to_vec(value).map_err(|err| {
            self.metrics.record_error();
            GcpError::RedisSerialize {
                value: value.to_string(),
                err,
            }
        })?;

        let _: () = self
            .connection
            .clone()
            .set(&self.key, bytes)
            .await
            .map_err(|err| {
                self.metrics.record_error();
                GcpError::RedisSave(err)
            })?;

        self.metrics.record_write();
        Ok(())
    }
}

// Revision is not used here
// TODO: remove it from interfaces?
impl<T> interfaces::kv_store::KvStore<T> for RedisClient<T>
where
    T: BorshSerialize + BorshDeserialize + Display + Debug,
{
    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip(self))]
    async fn update(&self, data: &WithRevision<T>) -> Result<u64, GcpError> {
        tracing::trace!(?data, "updating");
        self.upsert(&data.value).await?;
        Ok(0)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip(self))]
    async fn put(&self, value: &T) -> Result<u64, GcpError> {
        tracing::trace!(?value, "updating");
        self.upsert(value).await?;
        Ok(0)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip(self))]
    async fn get(&self) -> Result<Option<WithRevision<T>>, GcpError> {
        tracing::trace!("getting value");
        let mut connection = self.connection.clone();
        let value: Option<Vec<u8>> = connection
            .get(&self.key)
            .await
            .map_err(GcpError::RedisGet)?;

        value
            .map(|bytes| {
                let value: T =
                    borsh::from_slice(&bytes).map_err(|err| GcpError::RedisDeserialize {
                        value: hex::encode(bytes),
                        err,
                    })?;

                tracing::trace!(?value, "got value");

                self.metrics.record_read();

                Ok(interfaces::kv_store::WithRevision { value, revision: 0 })
            })
            .transpose()
            .inspect_err(|_| {
                self.metrics.record_error();
            })
    }
}

#[derive(Clone)]
struct Metrics {
    errors_counter: Counter<u64>,
    writes_counter: Counter<u64>,
    reads_counter: Counter<u64>,
    attributes: [KeyValue; 1],
}

impl Metrics {
    fn new(key: &str) -> Self {
        let meter = global::meter("redis_kvstore");

        let attributes = [KeyValue::new("kvstore.key.name", key.to_owned())];

        let errors_counter = meter
            .u64_counter("kvstore.errors")
            .with_description("Total number of errors encountered during key-value operations")
            .build();

        let writes_counter = meter
            .u64_counter("kvstore.writes")
            .with_description("Total number of write operations")
            .build();

        let reads_counter = meter
            .u64_counter("kvstore.reads")
            .with_description("Total number of read operations")
            .build();

        Self {
            errors_counter,
            writes_counter,
            reads_counter,
            attributes,
        }
    }

    fn record_read(&self) {
        self.reads_counter.add(1, &self.attributes);
    }

    fn record_write(&self) {
        self.writes_counter.add(1, &self.attributes);
    }

    fn record_error(&self) {
        self.errors_counter.add(1, &self.attributes);
    }
}
