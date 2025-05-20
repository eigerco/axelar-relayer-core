use core::fmt::{Debug, Display};
use core::marker::PhantomData;

use borsh::{BorshDeserialize, BorshSerialize};
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands as _, Client};

use super::GcpError;
use crate::interfaces;
use crate::interfaces::kv_store::WithRevision;

/// Redis client
pub struct RedisClient<T> {
    key: String,
    connection: MultiplexedConnection,
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

        Ok(Self {
            key,
            connection,
            _phantom: PhantomData,
        })
    }

    /// Ping the Redis server to check connectivity
    /// # Errors
    ///  on connection issues
    pub async fn ping(&self) -> Result<(), redis::RedisError> {
        let mut connection = self.connection.clone();
        redis::cmd("PING").query_async(&mut connection).await
    }

    pub(crate) async fn upsert(&self, value: &T) -> Result<(), GcpError> {
        tracing::debug!(%value, "upserting value");
        let bytes = borsh::to_vec(value).map_err(|err| GcpError::RedisSerialize {
            value: value.to_string(),
            err,
        })?;

        let _: () = self
            .connection
            .clone()
            .set(&self.key, bytes)
            .await
            .map_err(GcpError::RedisSave)?;

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
        tracing::debug!(?data, "updating");
        self.upsert(&data.value).await?;
        Ok(0)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip(self))]
    async fn put(&self, value: &T) -> Result<u64, GcpError> {
        tracing::debug!(?value, "updating");
        self.upsert(value).await?;
        Ok(0)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip(self))]
    async fn get(&self) -> Result<Option<WithRevision<T>>, GcpError> {
        tracing::debug!("getting value");
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

                tracing::debug!(?value, "got value");

                Ok(interfaces::kv_store::WithRevision { value, revision: 0 })
            })
            .transpose()
    }
}
