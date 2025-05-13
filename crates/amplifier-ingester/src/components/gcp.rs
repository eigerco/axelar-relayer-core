use bin_util::ValidateConfig;
use eyre::{Context as _, ensure, eyre};
use infrastructure::gcp;
use infrastructure::gcp::connectors::KmsConfig;
use infrastructure::gcp::consumer::{GcpConsumer, GcpConsumerConfig};
use relayer_amplifier_api_integration::amplifier_api::{self, AmplifierApiClient};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::config::Config;

// TODO: Adsjust based on metrics
const WORKERS_SCALE_FACTOR: usize = 4;
const BUFFER_SCALE_FACTOR: usize = 4;
const CHANNEL_CAPACITY_SCALE_FACTOR: usize = 4;

#[derive(Debug, Deserialize)]
pub(crate) struct GcpSectionConfig {
    gcp: GcpConfig,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GcpConfig {
    pub kms: KmsConfig,
    pub redis_connection: String,
    pub tasks_topic: String,
    pub tasks_subscription: String,
    pub events_topic: String,
    pub events_subscription: String,
    pub ack_deadline_secs: i32,
    pub message_buffer_size: usize,
}

impl ValidateConfig for GcpSectionConfig {
    fn validate(&self) -> eyre::Result<()> {
        self.gcp.kms.validate().map_err(|err| eyre::eyre!(err))?;
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
            self.gcp.ack_deadline_secs > 0_i32,
            eyre!("gcp nak_deadline_secs should be positive set")
        );
        ensure!(
            self.gcp.message_buffer_size > 0,
            eyre!("gcp message_buffer_size should be set")
        );
        Ok(())
    }
}

pub(crate) async fn new_amplifier_ingester(
    config_path: &str,
    cancellation_token: CancellationToken,
) -> eyre::Result<amplifier_ingester::Ingester<GcpConsumer<amplifier_api::types::Event>>> {
    let config: Config = bin_util::try_deserialize(config_path)?;
    let infra_config: GcpSectionConfig = bin_util::try_deserialize(config_path)?;

    let num_cpus = num_cpus::get();

    let consumer_cfg = GcpConsumerConfig {
        redis_connection: infra_config.gcp.redis_connection.clone(),
        ack_deadline_secs: infra_config.gcp.ack_deadline_secs,
        channel_capacity: num_cpus.checked_mul(CHANNEL_CAPACITY_SCALE_FACTOR),
        message_buffer_size: num_cpus
            .checked_mul(BUFFER_SCALE_FACTOR)
            .unwrap_or(num_cpus),
        worker_count: num_cpus
            .checked_mul(WORKERS_SCALE_FACTOR)
            .unwrap_or(num_cpus),
    };

    let event_queue_consumer = gcp::connectors::connect_consumer(
        &infra_config.gcp.events_subscription,
        consumer_cfg,
        cancellation_token,
    )
    .await
    .wrap_err("event consumer connect err")?;

    let amplifier_client = amplifier_client(&config, infra_config).await?;

    Ok(amplifier_ingester::Ingester::new(
        amplifier_client,
        event_queue_consumer,
        config.amplifier_component.chain.clone(),
    ))
}

async fn amplifier_client(
    config: &Config,
    infra_config: GcpSectionConfig,
) -> eyre::Result<AmplifierApiClient> {
    let client_config = gcp::connectors::kms_tls_client_config(
        config
            .amplifier_component
            .tls_public_certificate
            .clone()
            .ok_or_else(|| eyre::Report::msg("tls_public_certificate should be set"))?
            .into_bytes(),
        infra_config.gcp.kms,
    )
    .await
    .wrap_err("kms connection failed")?;

    AmplifierApiClient::new(
        config.amplifier_component.url.clone(),
        amplifier_api::TlsType::CustomProvider(client_config),
    )
    .wrap_err("amplifier api client failed to create")
}
