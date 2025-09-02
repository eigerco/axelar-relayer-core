use amplifier_api::{self, Client as AmplifierApiClient};
use bin_util::ValidateConfig;
use bin_util::config_defaults::{default_max_bundle_size, default_worker_count};
use eyre::{Context as _, ensure, eyre};
use infrastructure::gcp;
use infrastructure::gcp::connectors::KmsConfig;
use infrastructure::gcp::publisher::PeekableGcpPublisher;
use serde::Deserialize;

use crate::Subscriber;
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

/// Creates a new Amplifier Subscriber instance configured for GCP infrastructure.
///
/// This function initializes a subscriber that:
/// - Polls the Amplifier API for new tasks to relay
/// - Publishes tasks to a GCP Pub/Sub topic for processing
/// - Uses Redis for tracking the last processed task ID
/// - Supports TLS authentication via GCP KMS
///
/// # Arguments
///
/// * `config_path` - Path to the TOML configuration file containing both general subscriber
///   settings and GCP-specific infrastructure configuration
///
/// # Configuration
///
/// The configuration file must include:
/// - General subscriber config (`Config`):
///   - `limit_per_request`: Maximum number of tasks to fetch per API request
///   - `amplifier.chain`: The blockchain chain identifier
///   - `amplifier.url`: The Amplifier API URL
///   - `amplifier.tls_public_certificate`: Public certificate for TLS
/// - GCP-specific config (`GcpSectionConfig`):
///   - `gcp.tasks_topic`: GCP Pub/Sub topic name for publishing tasks
///   - `gcp.redis_connection`: Redis connection string for state management
///   - `gcp.nak_deadline_secs`: Deadline for negative acknowledgments
///   - `gcp.worker_count`: Number of concurrent workers (default from `config_defaults`)
///   - `gcp.max_bundle_size`: Maximum messages per bundle (default from `config_defaults`)
///   - `kms`: KMS configuration for TLS key management
///
/// # Returns
///
/// Returns a `Subscriber` instance with a `PeekableGcpPublisher` that can:
/// - Track the last processed task ID using Redis
/// - Publish tasks to GCP Pub/Sub in batches
/// - Handle health checks for both the publisher and Amplifier API connection
///
/// # Errors
///
/// This function will return an error if:
/// - Configuration file parsing fails
/// - GCP Pub/Sub publisher connection fails
/// - Redis connection for state tracking fails
/// - KMS client configuration fails
/// - Amplifier API client creation fails
///
/// # Example
///
/// ```no_run
/// # use amplifier_subscriber::gcp::new_amplifier_subscriber;
/// # async fn example() -> eyre::Result<()> {
/// let subscriber = new_amplifier_subscriber("config.toml").await?;
/// // subscriber is now ready to poll for tasks and publish to GCP
/// # Ok(())
/// # }
/// ```
pub async fn new_amplifier_subscriber(
    config_path: &str,
) -> eyre::Result<Subscriber<PeekableGcpPublisher<amplifier_api::types::TaskItem>>> {
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
    Ok(Subscriber::new(
        amplifier_client,
        task_queue_publisher,
        config.limit_per_request,
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
        infra_config.kms,
    )
    .await
    .wrap_err("kms connection failed")?;

    Ok(AmplifierApiClient::new(
        config.amplifier.url.as_str(),
        // amplifier_api::TlsType::CustomProvider(client_config),
    ))
}
