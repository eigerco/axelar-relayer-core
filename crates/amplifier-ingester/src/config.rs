use core::time::Duration;

use bin_util::config_defaults::{default_concurrent_queue_items, default_max_errors};
use bin_util::{ValidateConfig, deserialize_duration_from_secs};
use eyre::ensure;
use serde::Deserialize;

/// Top-level configuration for the relayer.
/// You also need `health_check_port` from `bin_util::health_check::Config`
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Configuration for the Amplifier API processor
    pub amplifier: amplifier_api::Config,
    /// Duration (in seconds) to wait between consecutive polling
    /// operations Used to prevent overwhelming the network with requests
    #[serde(rename = "tickrate_secs")]
    #[serde(deserialize_with = "deserialize_duration_from_secs")]
    pub tickrate: Duration,
    #[serde(default)]
    /// Telemetry config
    pub telemetry: Option<bin_util::telemetry::Config>,
    /// Maximum consecutive errors allowed before application termination.
    #[serde(default = "default_max_errors")]
    pub max_errors: u32,
    /// Max concurrent amplifier api requests
    #[serde(default = "default_concurrent_queue_items")]
    pub concurrent_queue_items: usize,
}

impl ValidateConfig for Config {
    fn validate(&self) -> eyre::Result<()> {
        ensure!(
            !self.amplifier.chain.trim().is_empty(),
            "chain could not be empty",
        );

        Ok(())
    }
}
