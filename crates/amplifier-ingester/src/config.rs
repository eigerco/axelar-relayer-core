use core::time::Duration;

use bin_util::ValidateConfig;
use eyre::ensure;
use serde::{Deserialize, Deserializer};

/// Top-level configuration for the relayer.
#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct Config {
    /// Configuration for the Amplifier API processor
    pub amplifier_component: relayer_amplifier_api_integration::Config,
    /// Duration (in seconds) to wait between consecutive polling
    /// operations Used to prevent overwhelming the network with requests
    #[serde(rename = "tickrate_secs")]
    #[serde(deserialize_with = "deserialize_duration_from_secs")]
    pub tickrate: Duration,

    /// Configuration for health check server
    #[serde(rename = "health_check_server")]
    pub health_check: HealthCheckConfig,
}

/// Configuration for health check server
#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct HealthCheckConfig {
    /// Port for the health check server
    #[serde(rename = "port")]
    pub port: u16,
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

pub(crate) fn deserialize_duration_from_secs<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let seconds = u64::deserialize(deserializer)?;
    Ok(Duration::from_secs(seconds))
}
