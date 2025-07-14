use core::fmt::Debug;
use core::marker::PhantomData;

use async_nats::jetstream::kv;
use borsh::{BorshDeserialize, BorshSerialize};

use super::NatsError;
use crate::interfaces;

/// ``KeyValue`` store
#[allow(clippy::module_name_repetitions, reason = "Descriptive name")]
pub struct NatsKvStore<T> {
    store: kv::Store,
    _phantom: PhantomData<T>,
}

impl<T> NatsKvStore<T> {
    pub(crate) const fn new(store: kv::Store) -> Self {
        Self {
            store,
            _phantom: PhantomData,
        }
    }
}

impl<T> interfaces::kv_store::KvStore<T> for NatsKvStore<T>
where
    T: BorshSerialize + BorshDeserialize + Debug + Send + Sync,
{
    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip(self))]
    async fn update(
        &self,
        key: &str,
        data: &interfaces::kv_store::WithRevision<T>,
    ) -> Result<u64, NatsError> {
        let value_bytes = borsh::to_vec(&data.value).map_err(NatsError::Serialize)?;
        let revision = data.revision;

        let revision = self
            .store
            .update(key, value_bytes.clone().into(), revision)
            .await
            .map_err(NatsError::Update)?;

        Ok(revision)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip(self))]
    async fn put(&self, key: &str, value: &T) -> Result<u64, NatsError> {
        let value_bytes = borsh::to_vec(value).map_err(NatsError::Serialize)?;
        let revision = self
            .store
            .put(key, value_bytes.clone().into())
            .await
            .map_err(NatsError::Put)?;

        Ok(revision)
    }

    #[allow(refining_impl_trait, reason = "simplification")]
    #[tracing::instrument(skip(self))]
    async fn get(
        &self,
        key: &str,
    ) -> Result<Option<interfaces::kv_store::WithRevision<T>>, NatsError> {
        let entry = self.store.entry(key).await.map_err(NatsError::Entry)?;

        entry
            .map(|entry| {
                let value =
                    T::deserialize(&mut entry.value.as_ref()).map_err(NatsError::Deserialize)?;

                Ok(interfaces::kv_store::WithRevision {
                    value,
                    revision: entry.revision,
                })
            })
            .transpose()
    }
}
