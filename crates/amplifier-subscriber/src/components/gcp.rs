use bin_util::ValidateConfig;
use bin_util::config_defaults::{default_max_bundle_size, default_worker_count};
use eyre::{Context as _, ensure, eyre};
use infrastructure::gcp;
use infrastructure::gcp::connectors::KmsConfig;
use infrastructure::gcp::publisher::PeekableGcpPublisher;
use relayer_amplifier_api_integration::amplifier_api::{self, AmplifierApiClient};
use serde::Deserialize;

use crate::config::Config;

const TASK_KEY: &str = "last-task";

#[derive(Debug, Deserialize)]
pub(crate) struct GcpSectionConfig {
    gcp: GcpConfig,
    kms: KmsConfig,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GcpConfig {
    redis_connection: String,
    tasks_topic: String,
    nak_deadline_secs: i32,
    #[serde(default = "default_worker_count")]
    worker_count: usize,
    #[serde(default = "default_max_bundle_size")]
    max_bundle_size: usize,
}

impl ValidateConfig for GcpSectionConfig {
    fn validate(&self) -> eyre::Result<()> {
        self.kms.validate().map_err(|err| eyre::eyre!(err))?;
        ensure!(
            !self.gcp.redis_connection.is_empty(),
            eyre!("gcp redis_connection should be set")
        );
        ensure!(
            !self.gcp.tasks_topic.is_empty(),
            eyre!("gcp tasks_topic should be set")
        );
        ensure!(
            self.gcp.nak_deadline_secs > 0_i32,
            eyre!("gcp nak_deadline_secs should be positive set")
        );
        Ok(())
    }
}

pub(crate) async fn new_amplifier_subscriber(
    config_path: &str,
) -> eyre::Result<
    amplifier_subscriber::Subscriber<PeekableGcpPublisher<amplifier_api::types::TaskItem>>,
> {
    let config: Config = bin_util::try_deserialize(config_path)?;
    let infra_config: GcpSectionConfig = bin_util::try_deserialize(config_path)?;

    let task_queue_publisher = gcp::connectors::connect_peekable_publisher(
        &infra_config.gcp.tasks_topic,
        infra_config.gcp.redis_connection.clone(),
        TASK_KEY.to_owned(),
        infra_config.gcp.worker_count,
        infra_config.gcp.max_bundle_size,
    )
    .await
    .wrap_err("task queue publisher connect err")?;

    let amplifier_client = amplifier_client(&config, infra_config).await?;
    Ok(amplifier_subscriber::Subscriber::new(
        amplifier_client,
        task_queue_publisher,
        config.limit_per_request,
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
        infra_config.kms,
    )
    .await
    .wrap_err("kms connection failed")?;

    AmplifierApiClient::new(
        config.amplifier_component.url.clone(),
        amplifier_api::TlsType::CustomProvider(client_config),
    )
    .wrap_err("amplifier api client failed to create")
}
