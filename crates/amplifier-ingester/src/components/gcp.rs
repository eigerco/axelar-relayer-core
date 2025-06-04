use bin_util::ValidateConfig;
use eyre::{Context as _, ensure, eyre};
use infrastructure::gcp;
use infrastructure::gcp::connectors::KmsConfig;
use infrastructure::gcp::consumer::{GcpConsumer, GcpConsumerConfig};
use relayer_amplifier_api_integration::amplifier_api::{self, AmplifierApiClient};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::config::Config;

#[derive(Debug, Deserialize)]
pub(crate) struct GcpSectionConfig {
    gcp: GcpConfig,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GcpConfig {
    pub kms: KmsConfig,
    pub redis_connection: String,
    pub events_subscription: String,
    pub ack_deadline_secs: i32,
    #[serde(default)]
    pub channel_capacity: Option<usize>,
    pub worker_count: usize,
}

impl ValidateConfig for GcpSectionConfig {
    fn validate(&self) -> eyre::Result<()> {
        self.gcp.kms.validate().map_err(|err| eyre::eyre!(err))?;
        ensure!(
            !self.gcp.redis_connection.is_empty(),
            eyre!("gcp redis_connection should be set")
        );
        ensure!(
            !self.gcp.events_subscription.is_empty(),
            eyre!("gcp events_subscription should be set")
        );
        ensure!(
            self.gcp.ack_deadline_secs > 0_i32,
            eyre!("gcp nak_deadline_secs should be positive set")
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

    let consumer_cfg = GcpConsumerConfig {
        redis_connection: infra_config.gcp.redis_connection.clone(),
        ack_deadline_secs: infra_config.gcp.ack_deadline_secs,
        channel_capacity: infra_config.gcp.channel_capacity,
        worker_count: infra_config.gcp.worker_count,
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
        config.concurrent_queue_items,
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
