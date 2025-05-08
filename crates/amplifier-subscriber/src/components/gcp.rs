use std::path::PathBuf;

use eyre::{Context as _, ensure, eyre};
use infrastructure::gcp;
use infrastructure::gcp::publisher::PeekableGcpPublisher;
use relayer_amplifier_api_integration::amplifier_api::{self, AmplifierApiClient};
use serde::Deserialize;

use crate::config::{self, Config, Validate};

const TASK_KEY: &str = "last-task";

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct GcpSectionConfig {
    pub gcp: GcpConfig,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct GcpConfig {
    redis_connection: String,

    tasks_topic: String,
    tasks_subscription: String,

    events_topic: String,
    events_subscription: String,

    nak_deadline_secs: i32,

    message_buffer_size: usize,
}

impl Validate for GcpSectionConfig {
    fn validate(&self) -> eyre::Result<()> {
        ensure!(
            !self.gcp.redis_connection.is_empty(),
            eyre!("gcp redis_connection should be set")
        );
        ensure!(
            !self.gcp.tasks_topic.is_empty(),
            eyre!("gcp tasks_topic should be set")
        );
        ensure!(
            !self.gcp.tasks_subscription.is_empty(),
            eyre!("gcp tasks_subscription should be set")
        );
        ensure!(
            !self.gcp.events_topic.is_empty(),
            eyre!("gcp events_topic should be set")
        );
        ensure!(
            !self.gcp.events_subscription.is_empty(),
            eyre!("gcp events_subscription should be set")
        );
        ensure!(
            self.gcp.nak_deadline_secs > 0_i32,
            eyre!("gcp nak_deadline_secs should be positive set")
        );
        ensure!(
            self.gcp.message_buffer_size > 0,
            eyre!("gcp message_buffer_size should be set")
        );
        Ok(())
    }
}

pub(crate) async fn new_amplifier_subscriber(
    config_path: PathBuf,
) -> eyre::Result<
    amplifier_subscriber::Subscriber<PeekableGcpPublisher<amplifier_api::types::TaskItem>>,
> {
    let config = config::try_deserialize(&config_path).wrap_err("config file issues")?;
    let queue_config: GcpSectionConfig =
        config::try_deserialize(&config_path).wrap_err("gcp pubsub config issues")?;
    let amplifier_client = amplifier_client(&config)?;

    let task_queue_publisher = gcp::connectors::connect_peekable_publisher(
        &queue_config.gcp.tasks_topic,
        queue_config.gcp.redis_connection,
        TASK_KEY.to_owned(),
    )
    .await
    .wrap_err("task queue publisher connect err")?;

    Ok(amplifier_subscriber::Subscriber::new(
        amplifier_client,
        task_queue_publisher,
        config.amplifier_component.chain.clone(),
    ))
}

fn amplifier_client(config: &Config) -> eyre::Result<AmplifierApiClient> {
    AmplifierApiClient::new(
        config.amplifier_component.url.clone(),
        amplifier_api::TlsType::Certificate(Box::new(config.amplifier_component.identity.clone())),
    )
    .wrap_err("amplifier api client failed to create")
}
