use core::time::Duration;

use bin_util::{ValidateConfig, deserialize_duration_from_secs};
use eyre::ensure;

/// Top-level configuration for the relayer.
#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    /// Configuration for the Amplifier API processor
    pub amplifier_component: relayer_amplifier_api_integration::Config,
    /// Duration (in seconds) to wait between consecutive polling
    /// operations Used to prevent overwhelming the network with requests
    #[serde(rename = "tickrate_secs")]
    #[serde(deserialize_with = "deserialize_duration_from_secs")]
    pub tickrate: Duration,
    pub telemetry: bin_util::telemetry::Config,
    #[serde(rename = "health_check_server")]
    pub health_check: bin_util::health_check::Config,
}


impl ValidateConfig for Config {
    fn validate(&self) -> eyre::Result<()> {
        ensure!(
            !self.amplifier_component.chain.trim().is_empty(),
            "chain could not be empty",
        );

        Ok(())
    }
}
