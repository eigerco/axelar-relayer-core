#![expect(missing_docs, reason = "the error macro already is descriptive enough")]
use core::fmt::Debug;

/// google cloud platform implementation
use google_cloud_pubsub::client::google_cloud_auth;
use redis::RedisError;
use rustls_gcp_kms::KmsError;
use tokio::task::JoinError;

/// connectors to queue
pub mod connectors;
/// consumer
pub mod consumer;
/// redis keyvalue strore
pub mod kv_store;
/// publisher
pub mod publisher;
pub(crate) mod util;

/// Errors
#[allow(clippy::module_name_repetitions, reason = "Descriptive name")]
#[derive(Debug, thiserror::Error)]
pub enum GcpError {
    #[error("auth error {0}")]
    Auth(#[from] google_cloud_auth::error::Error),
    #[error("pubsub client error: {0}")]
    PubsubClient(#[from] google_cloud_pubsub::client::Error),
    #[error("kms client error: {0}")]
    KmsClient(google_cloud_gax::conn::Error),
    #[error("kms error: {0}")]
    Kms(#[from] KmsError),
    #[error("topic exists error {0}")]
    TopicExistsCheck(Box<tonic::Status>),
    #[error("topic not found: {topic}")]
    TopicNotFound { topic: String },
    #[error("could not read client certificate due to error: {0}")]
    CertificateRead(#[from] rustls::pki_types::pem::Error),
    #[error("build client error: {0}")]
    RustlsBuilder(#[from] rustls::Error),
    #[error("topic exists check error {0}")]
    SubscriptionExistsCheck(Box<tonic::Status>),
    #[error("subscription not found: {subscription}")]
    SubscriptionNotFound { subscription: String },
    #[error("publish failure, error: {0}")]
    Publish(Box<tonic::Status>),
    #[error("ack error {0}")]
    Ack(Box<tonic::Status>),
    #[error("nack error {0}")]
    Nak(Box<tonic::Status>),
    #[error("modify ack deadline error {0}")]
    ModifyAckDeadline(Box<tonic::Status>),
    #[error("failed to deserialize val error: {0}")]
    Deserialize(std::io::Error),
    #[error("messages receiver task error {0}")]
    ReceiverTaskCrash(Box<tonic::Status>),
    #[error("failed to serialize val, error: {0}")]
    Serialize(std::io::Error),
    #[error("error connecting to redis {0}")]
    Connection(RedisError),
    #[error("redis operation failed {0}")]
    Redis(RedisError),
    #[error("error serializing data `{value}` to redis {err}")]
    RedisSerialize { value: String, err: std::io::Error },
    #[error("error saving data to redis {0}")]
    RedisSave(RedisError),
    #[error("error getting data from redis {0}")]
    RedisGet(RedisError),
    #[error("error deserializing data (hex representation) `{value}` from redis {err}")]
    RedisDeserialize { value: String, err: std::io::Error },
    #[error("consumer read task join error {0}")]
    ConsumerReadTaskJoin(JoinError),
    #[error("consumer read task exited without error")]
    ConsumerReadTaskExited,
    #[error("no messages to publish")]
    NoMsgToPublish,
    #[error("redis err {0}")]
    GenericRedis(#[from] RedisError),
    #[error("message has no Msg-Id set")]
    MsgIdNotSet,
}
