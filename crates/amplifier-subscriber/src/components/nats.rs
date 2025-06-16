use bin_util::ValidateConfig;
use bin_util::nats::{TASKS_PUBLISH_SUBJECT, TASKS_STREAM};
use eyre::{Context as _, ensure, eyre};
use infrastructure::nats::publisher::NatsPublisher;
use infrastructure::nats::{self, StreamArgs};
use relayer_amplifier_api_integration::amplifier_api::{self, AmplifierApiClient};
use serde::Deserialize;
use url::Url;

use crate::Subscriber;
use crate::config::Config;

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct NatsSectionConfig {
    pub nats: NatsConfig,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct NatsConfig {
    pub urls: Vec<Url>,
}

impl ValidateConfig for NatsSectionConfig {
    fn validate(&self) -> eyre::Result<()> {
        ensure!(
            !self.nats.urls.is_empty(),
            eyre!("nats urls should have at least one connection")
        );

        Ok(())
    }
}

/// Creates a new Amplifier subscriber configured for NATS messaging.
///
/// This function initializes a subscriber that fetches tasks from the Amplifier API
/// and publishes them to a NATS stream for processing by downstream consumers.
/// It establishes a NATS publisher connection and configures the Amplifier API
/// client with TLS authentication.
///
/// # Arguments
///
/// * `config_path` - Path to the configuration file containing both general subscriber settings and
///   NATS-specific configuration
///
/// # Returns
///
/// Returns a `Subscriber` instance configured with a NATS publisher for distributing
/// Amplifier API task items, or an error if initialization fails.
///
/// # Configuration
///
/// The configuration file must contain:
/// - General subscriber configuration (`limit_per_request`, `amplifier_component`)
/// - NATS configuration section with:
///   - `urls`: List of NATS server URLs
///
/// # Errors
///
/// This function will return an error if:
/// - Configuration file cannot be read or parsed
/// - NATS connection cannot be established
/// - Amplifier API client fails to initialize
/// - Required configuration fields are missing
pub async fn new_amplifier_subscriber(
    config_path: &str,
) -> eyre::Result<Subscriber<NatsPublisher<amplifier_api::types::TaskItem>>> {
    let config: Config = bin_util::try_deserialize(config_path)?;
    let nats_config: NatsSectionConfig = bin_util::try_deserialize(config_path)?;

    let amplifier_client = amplifier_client(&config)?;

    let stream = StreamArgs {
        name: TASKS_STREAM.to_owned(),
        subject: TASKS_PUBLISH_SUBJECT.to_owned(),
        description: "amplifier tasks for blockchain ingester".to_owned(),
    };

    let task_queue_publisher = nats::connectors::connect_publisher(
        &nats_config.nats.urls,
        stream,
        TASKS_PUBLISH_SUBJECT.to_owned(),
    )
    .await
    .wrap_err("task queue publisher connect err")?;

    Ok(Subscriber::new(
        amplifier_client,
        task_queue_publisher,
        config.limit_per_request,
        config.amplifier_component.chain.clone(),
    ))
}

fn amplifier_client(config: &Config) -> eyre::Result<AmplifierApiClient> {
    AmplifierApiClient::new(
        config.amplifier_component.url.clone(),
        amplifier_api::TlsType::Certificate(Box::new(
            config
                .amplifier_component
                .identity
                .clone()
                .ok_or_else(|| eyre::Report::msg("identity not set"))?,
        )),
    )
    .wrap_err("amplifier api client failed to create")
}
