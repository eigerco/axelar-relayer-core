use core::time::Duration;
use std::path::Path;

use eyre::{WrapErr as _, ensure, eyre};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

pub(crate) trait Validate {
    fn validate(&self) -> eyre::Result<()>;
}

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

impl Validate for Config {
    fn validate(&self) -> eyre::Result<()> {
        ensure!(
            !self.amplifier_component.chain.trim().is_empty(),
            "chain could not be empty",
        );

        Ok(())
    }
}

pub(crate) fn try_deserialize<T: DeserializeOwned + Validate>(
    config_path: &Path,
) -> eyre::Result<T> {
    ensure!(
        config_path.exists() && config_path.is_file(),
        eyre!("Path should be a file and exist {:?}", config_path)
    );
    let config_file = std::fs::read_to_string(config_path).wrap_err("cannot read config file")?;

    let config = toml::from_str::<T>(&config_file).wrap_err("invalid config file content")?;
    config.validate()?;
    Ok(config)
}

pub(crate) fn deserialize_duration_from_secs<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let seconds = u64::deserialize(deserializer)?;
    Ok(Duration::from_secs(seconds))
}
