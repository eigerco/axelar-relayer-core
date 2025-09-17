use serde::Deserialize;

use crate::identity::Identity;

/// Amplifier config
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Identity keys for the Amplifier API
    pub identity: Option<Identity>,
    /// TLS public certificate for Amplifier API
    pub tls_public_certificate: Option<String>,
    /// The Amplifier API url to connect to
    pub url: url::Url,
    /// The name of the chain that we need to send / listen for
    pub chain: String,
    /// The max amount of tasks that we want to receive in a batch.
    /// This goes hand-in-hand with the `get_chains_poll_interval`
    pub get_chains_limit: u8,
}
