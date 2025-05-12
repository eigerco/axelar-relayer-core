use amplifier_api::identity::Identity;
use base64::Engine as _;
use base64::prelude::BASE64_STANDARD;
use clap::Parser;
use eyre::Result;
use serde::Deserialize;
use typed_builder::TypedBuilder;

/// global Amplifier component configuration
#[derive(Debug, Deserialize, Clone, PartialEq, TypedBuilder, Parser)]
pub struct Config {
    /// Identity certificate for the Amplifier API authentication to work
    #[arg(
        value_name = "AMPLIFIER_API_IDENTITY",
        env = "AMPLIFIER_API_IDENTITY",
        value_parser = parse_identity
    )]
    pub identity: Option<Identity>,
    /// The Amplifier API url to connect to
    #[arg(value_name = "AMPLIFIER_API_URL", env = "AMPLIFIER_API_URL")]
    pub url: url::Url,
    /// The name of the chain that we need to send / listen for
    #[arg(value_name = "AMPLIFIER_API_CHAIN", env = "AMPLIFIER_API_CHAIN")]
    pub chain: String,

    /// The interval between polling Amplifier API for new tasks
    #[builder(default = config_defaults::get_chains_poll_interval())]
    #[serde(
        rename = "get_chains_poll_interval_in_milliseconds",
        default = "config_defaults::get_chains_poll_interval",
        deserialize_with = "common_serde_utils::duration_ms_decode"
    )]
    #[arg(
        value_name = "AMPLIFIER_API_CHAINS_POLL_INTERVAL",
        env = "AMPLIFIER_API_CHAINS_POLL_INTERVAL",
        value_parser = parse_chains_poll_interval,
        default_value = config_defaults::chains_poll_interval_default_value().to_string()
    )]
    pub get_chains_poll_interval: core::time::Duration,

    /// The max amount of tasks that we want to receive in a batch.
    /// This goes hand-in-hand with the `get_chains_poll_interval`
    #[builder(default = config_defaults::get_chains_limit())]
    #[serde(default = "config_defaults::get_chains_limit")]
    #[arg(
        value_name = "AMPLIFIER_API_CHAINS_LIMIT",
        env = "AMPLIFIER_API_CHAINS_LIMIT",
        default_value = config_defaults::get_chains_limit().to_string()
    )]
    pub get_chains_limit: u8,

    /// How often we check the liveliness of the Amplifier API
    #[builder(default = config_defaults::healthcheck_interval())]
    #[serde(
        rename = "healthcheck_interval_in_milliseconds",
        default = "config_defaults::healthcheck_interval",
        deserialize_with = "common_serde_utils::duration_ms_decode"
    )]
    #[arg(
        value_name = "AMPLIFIER_API_HEALTHCHECK_INTERVAL",
        env = "AMPLIFIER_API_HEALTHCHECK_INTERVAL",
        value_parser = parse_healthcheck_interval,
        default_value = config_defaults::healthcheck_interval_default_value().to_string()
    )]
    pub healthcheck_interval: core::time::Duration,

    /// How many invalid healthchecks do we need to do before we deem that the service is down and
    /// we should shut down the component
    #[builder(default = config_defaults::invalid_healthchecks_before_shutdown())]
    #[serde(default = "config_defaults::invalid_healthchecks_before_shutdown")]
    #[arg(
        value_name = "AMPLIFIER_API_INVALID_HEALTHCHECKS_BEFORE_SHUTDOWN",
        env = "AMPLIFIER_API_INVALID_HEALTHCHECKS_BEFORE_SHUTDOWN",
        default_value = config_defaults::invalid_healthchecks_before_shutdown().to_string()
    )]
    pub invalid_healthchecks_before_shutdown: usize,
}

fn parse_identity(input: &str) -> Result<Option<Identity>> {
    if input.is_empty() {
        return Ok(None);
    }

    let identity_bytes = BASE64_STANDARD.decode(input)?;
    Ok(Some(Identity::new_from_pem_bytes(&identity_bytes)?))
}

fn parse_chains_poll_interval(input: &str) -> Result<core::time::Duration> {
    Ok(core::time::Duration::from_secs(input.parse::<u64>()?))
}

fn parse_healthcheck_interval(input: &str) -> Result<core::time::Duration> {
    Ok(core::time::Duration::from_secs(input.parse::<u64>()?))
}

pub(crate) mod config_defaults {
    use core::time::Duration;

    pub(crate) const fn healthcheck_interval() -> Duration {
        Duration::from_secs(healthcheck_interval_default_value())
    }

    pub(crate) const fn healthcheck_interval_default_value() -> u64 {
        10
    }

    pub(crate) const fn get_chains_poll_interval() -> Duration {
        Duration::from_secs(chains_poll_interval_default_value())
    }

    pub(crate) const fn chains_poll_interval_default_value() -> u64 {
        10
    }

    pub(crate) const fn invalid_healthchecks_before_shutdown() -> usize {
        5
    }
    pub(crate) const fn get_chains_limit() -> u8 {
        4
    }
}
