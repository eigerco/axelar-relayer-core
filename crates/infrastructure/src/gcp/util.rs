use core::fmt::Debug;
use std::collections::HashMap;

use borsh::{BorshDeserialize, BorshSerialize};
use google_cloud_pubsub::client::Client;
use google_cloud_pubsub::subscription::Subscription;
use google_cloud_pubsub::topic::Topic;
use opentelemetry::Context;
use opentelemetry::propagation::{Extractor, Injector};

use super::GcpError;

pub(crate) async fn get_topic(client: &Client, topic_name: &str) -> Result<Topic, GcpError> {
    let topic = client.topic(topic_name);

    if !topic
        .exists(None)
        .await
        .map_err(|err| GcpError::TopicExistsCheck(Box::new(err)))?
    {
        return Err(GcpError::TopicNotFound {
            topic: topic_name.to_owned(),
        });
    }

    Ok(topic)
}

pub(crate) async fn get_subscription(
    client: &Client,
    subscription_name: &str,
) -> Result<Subscription, GcpError> {
    let subscription = client.subscription(subscription_name);
    if !subscription
        .exists(None)
        .await
        .map_err(|err| GcpError::SubscriptionExistsCheck(Box::new(err)))?
    {
        return Err(GcpError::SubscriptionNotFound {
            subscription: subscription_name.to_owned(),
        });
    }

    Ok(subscription)
}

/// Holds opentelemetry metadata
#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub(crate) struct MessageContent<T: Debug> {
    headers: HashMap<String, String>,
    data: T,
}

impl<T: Debug + BorshSerialize + BorshDeserialize> Extractor for MessageContent<T> {
    fn get(&self, key: &str) -> Option<&str> {
        self.headers.get(key).map(std::string::String::as_str)
    }

    fn keys(&self) -> Vec<&str> {
        self.headers
            .keys()
            .map(std::string::String::as_str)
            .collect()
    }
}

// Implement injector for outgoing messages
impl<T: Debug + BorshSerialize + BorshDeserialize> Injector for MessageContent<T> {
    fn set(&mut self, key: &str, value: String) {
        self.headers.insert(key.to_owned(), value);
    }
}

impl<T: Debug + BorshSerialize + BorshDeserialize> MessageContent<T> {
    pub(crate) fn new(data: T) -> Self {
        Self {
            headers: HashMap::new(),
            data,
        }
    }

    pub(crate) fn inject_context(&mut self) {
        opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&Context::current(), self);
        });
    }

    pub(crate) fn extract_context(&self) -> Context {
        opentelemetry::global::get_text_map_propagator(|propagator| propagator.extract(self))
    }

    pub(crate) fn data(self) -> T {
        self.data
    }
}
