use core::time::Duration;

use bin_util::{ValidateConfig, deserialize_duration_from_secs};
use eyre::{Context as _, ensure, eyre};
use infrastructure::nats::consumer::NatsConsumer;
use infrastructure::nats::{self, StreamArgs};
use relayer_amplifier_api_integration::amplifier_api::{self, AmplifierApiClient};
use serde::Deserialize;
use url::Url;

use crate::Config;

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct NatsSectionConfig {
    pub nats: NatsConfig,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct NatsConfig {
    pub urls: Vec<Url>,
    #[serde(rename = "connection_timeout_secs")]
    #[serde(deserialize_with = "deserialize_duration_from_secs")]
    pub connection_timeout: Duration,

    pub stream_name: String,
    pub stream_subject: String,
    pub stream_description: String,

    pub consumer_description: String,
    pub deliver_group: String,
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

pub(crate) async fn new_amplifier_ingester(
    config_path: String,
) -> eyre::Result<amplifier_ingester::Ingester<NatsConsumer<amplifier_api::types::Event>>> {
    let config: Config = bin_util::try_deserialize(&config_path)?;
    let nats_config: NatsSectionConfig = bin_util::try_deserialize(&config_path)?;

    let amplifier_client = amplifier_client(&config)?;

    let stream = StreamArgs {
        name: nats_config.nats.stream_name.clone(),
        subject: nats_config.nats.stream_subject.clone(),
        description: nats_config.nats.stream_description.clone(),
    };

    let event_queue_consumer = nats::connectors::connect_consumer(
        &nats_config.nats.urls,
        stream,
        nats_config.nats.consumer_description,
        nats_config.nats.deliver_group,
    )
    .await
    .wrap_err("event consumer connect err")?;

    Ok(amplifier_ingester::Ingester::new(
        amplifier_client,
        event_queue_consumer,
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
