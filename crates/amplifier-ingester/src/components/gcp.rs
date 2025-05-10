use std::path::PathBuf;
use std::sync::Arc;

use eyre::{Context as _, ensure, eyre};
use infrastructure::gcp;
use infrastructure::gcp::consumer::{GcpConsumer, GcpConsumerConfig};
use relayer_amplifier_api_integration::amplifier_api::{self, AmplifierApiClient};
use rustls_gcp_kms::KmsConfig;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::config::{self, Config, Validate};

// TODO: Adsjust based on metrics
const WORKERS_SCALE_FACTOR: usize = 4;
const BUFFER_SCALE_FACTOR: usize = 4;
const CHANNEL_CAPACITY_SCALE_FACTOR: usize = 4;

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct GcpSectionConfig {
    pub gcp: GcpConfig,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct GcpConfig {
    certificate_path: PathBuf,
    kms: KmsConfig,

    redis_connection: String,

    tasks_topic: String,
    tasks_subscription: String,

    events_topic: String,
    events_subscription: String,

    ack_deadline_secs: i32,

    message_buffer_size: usize,
}

impl Validate for GcpSectionConfig {
    fn validate(&self) -> eyre::Result<()> {
        ensure!(
            !self.gcp.kms.project_id.is_empty(),
            eyre!("gcp kms project_id should be set")
        );
        ensure!(
            !self.gcp.kms.location.is_empty(),
            eyre!("gcp kms location should be set")
        );
        ensure!(
            !self.gcp.kms.keyring.is_empty(),
            eyre!("gcp kms keyring should be set")
        );
        ensure!(
            !self.gcp.kms.cryptokey.is_empty(),
            eyre!("gcp kms cryptokey should be set")
        );
        ensure!(
            !self.gcp.kms.cryptokey_version.is_empty(),
            eyre!("gcp kms cryptokey_version should be set")
        );
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
    config_path: PathBuf,
    cancellation_token: CancellationToken,
) -> eyre::Result<amplifier_ingester::Ingester<GcpConsumer<amplifier_api::types::Event>>> {
    let config = config::try_deserialize(&config_path).wrap_err("config file issues")?;
    let queue_config: GcpSectionConfig =
        config::try_deserialize(&config_path).wrap_err("gcp pubsub config issues")?;
    let amplifier_client = amplifier_client(&config)?;

    let num_cpus = num_cpus::get();

    let consumer_cfg = GcpConsumerConfig {
        redis_connection: queue_config.gcp.redis_connection,
        ack_deadline_secs: queue_config.gcp.ack_deadline_secs,
        channel_capacity: num_cpus.checked_mul(CHANNEL_CAPACITY_SCALE_FACTOR),
        message_buffer_size: num_cpus
            .checked_mul(BUFFER_SCALE_FACTOR)
            .unwrap_or(num_cpus),
        worker_count: num_cpus
            .checked_mul(WORKERS_SCALE_FACTOR)
            .unwrap_or(num_cpus),
    };

    let event_queue_consumer = gcp::connectors::connect_consumer(
        &queue_config.gcp.events_subscription,
        consumer_cfg,
        cancellation_token,
    )
    .await
    .wrap_err("event consumer connect err")?;

    Ok(amplifier_ingester::Ingester::new(
        amplifier_client,
        event_queue_consumer,
        config.amplifier_component.chain.clone(),
    ))
}

async fn amplifier_client(
    config: &Config,
    gcp_config: &GcpConfig,
) -> eyre::Result<AmplifierApiClient> {
    let kms_provider =
        gcp::connectors::kms_tls_client_config(gcp_config.certificate_path, gcp_config.kms)
            .await
            .wrap_err("kms connection failed")?;

    let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(kms_provider));

    AmplifierApiClient::new(
        config.amplifier_component.url.clone(),
        amplifier_api::TlsType::CustomProvider(Box::new(
            config.amplifier_component.identity.clone(),
        )),
    )
    .wrap_err("amplifier api client failed to create")
}
