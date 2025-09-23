use amplifier_api::{self, Client as AmplifierApiClient};
use bin_util::ValidateConfig;
use eyre::{Context as _, ensure, eyre};
use infrastructure::gcp;
use infrastructure::gcp::connectors::KmsConfig;
use infrastructure::gcp::consumer::{GcpConsumer, GcpConsumerConfig};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::Ingester;
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

/// Creates a new Amplifier Ingester instance configured for GCP infrastructure.
///
/// This function initializes an ingester that:
/// - Consumes events from a GCP Pub/Sub subscription
/// - Processes and forwards events to the Amplifier API
/// - Uses Redis for distributed locking and coordination
/// - Supports graceful shutdown via cancellation token
/// - Uses KMS for TLS
///
/// # Arguments
///
/// * `config_path` - Path to the TOML configuration file containing both general ingester settings
///   and GCP-specific infrastructure configuration
/// * `cancellation_token` - Token used to signal graceful shutdown to the consumer
///
/// # Configuration
///
/// The configuration file must include:
/// - General ingester config (`Config`):
///   - `concurrent_queue_items`: Number of events to process concurrently
///   - `amplifier.chain`: The blockchain chain identifier
///   - `amplifier.url`: The Amplifier API URL
///   - `amplifier.tls_public_certificate`: Public certificate for TLS
/// - GCP-specific config (`GcpSectionConfig`):
///   - `gcp.events_subscription`: GCP Pub/Sub subscription name for consuming events
///   - `gcp.redis_connection`: Redis connection string for distributed coordination
///   - `gcp.ack_deadline_secs`: Time before unacknowledged messages are redelivered
///   - `gcp.channel_capacity`: Buffer size for the consumer channel (optional)
///   - `gcp.worker_count`: Number of concurrent workers processing messages
///   - `gcp.kms`: KMS configuration for TLS key management
///
/// # Returns
///
/// Returns an `Ingester` instance with a `GcpConsumer` that can:
/// - Pull events from GCP Pub/Sub subscription
/// - Acknowledge or negative-acknowledge messages based on processing results
/// - Handle connection failures and message redelivery
/// - Perform health checks on both the consumer and Amplifier API connection
///
/// # Errors
///
/// This function will return an error if:
/// - Configuration file parsing fails
/// - GCP Pub/Sub consumer connection fails
/// - Redis connection for distributed locking fails
/// - KMS client configuration fails (if TLS is configured)
/// - Amplifier API client creation fails
///
/// # Example
///
/// ```no_run
/// # use tokio_util::sync::CancellationToken;
/// # use amplifier_ingester::gcp::new_amplifier_ingester;
/// # async fn example() -> eyre::Result<()> {
/// let cancel_token = CancellationToken::new();
/// let ingester = new_amplifier_ingester("config.toml", cancel_token).await?;
/// // ingester is now ready to consume events from GCP and forward to Amplifier
/// # Ok(())
/// # }
/// ```
///
/// # Shutdown Behavior
///
/// When the `cancellation_token` is triggered, the consumer will:
/// 1. Stop pulling new messages from Pub/Sub
/// 2. Complete processing of in-flight messages
/// 3. Perform a graceful shutdown of all connections
pub async fn new_amplifier_ingester(
    config_path: &str,
    cancellation_token: CancellationToken,
) -> eyre::Result<Ingester<GcpConsumer<amplifier_api::types::Event>>> {
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

    Ok(Ingester::new(
        amplifier_client,
        event_queue_consumer,
        config.concurrent_queue_items,
        config.amplifier.chain.clone(),
    ))
}

async fn amplifier_client(
    config: &Config,
    infra_config: GcpSectionConfig,
) -> eyre::Result<AmplifierApiClient> {
    let client_config = gcp::connectors::kms_tls_client_config(
        config
            .amplifier
            .tls_public_certificate
            .clone()
            .ok_or_else(|| eyre::Report::msg("tls_public_certificate should be set"))?
            .into_bytes(),
        infra_config.gcp.kms,
    )
    .await
    .wrap_err("kms connection failed")?;

    let tls_type = amplifier_api::identity::TlsType::CustomProvider(client_config);
    let authenticated_client = amplifier_api::identity::authenticated_client(tls_type)?;
    let client =
        AmplifierApiClient::new_with_client(config.amplifier.url.as_str(), authenticated_client);

    Ok(client)
}
