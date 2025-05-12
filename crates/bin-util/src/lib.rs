//! Binary utils

const ENV_APP_PREFIX: &str = "RELAYER";
const SEPARATOR: &str = "_";

pub mod health_check;
use core::time::Duration;

use config::{Config, Environment, File};
use eyre::Context as _;
use serde::{Deserialize as _, Deserializer};
use tokio_util::sync::CancellationToken;

/// Ensures backtrace is enabled
#[allow(dead_code, reason = "temporary")]
pub fn ensure_backtrace_set() {
    // SAFETY: safe in single thread
    unsafe {
        std::env::set_var("RUST_BACKTRACE", "full");
    }
}

/// Register cancel token and ctrl+c handler
///
/// # Panics
///   on failure to register ctr+c handler
#[allow(
    clippy::print_stdout,
    reason = "not a tracing msg, should always display"
)]
#[allow(dead_code, reason = "temporary")]
#[must_use]
pub fn register_cancel() -> CancellationToken {
    let cancel_token = CancellationToken::new();
    let ctrlc_token = cancel_token.clone();
    ctrlc::set_handler(move || {
        if ctrlc_token.is_cancelled() {
            #[expect(clippy::restriction, reason = "immediate exit")]
            std::process::exit(1);
        } else {
            println!("\nGraceful shutdown initiated. Press Ctrl+C again for immediate exit...");
            ctrlc_token.cancel();
        }
    })
    .expect("Failed to register ctrl+c handler");
    cancel_token
}

/// Deserializes configuration from a specified file path into a typed structure.
///
/// This function loads configuration settings from a file and environment variables,
/// then deserializes them into the specified type `T`.
///
/// # Type Parameters
///
/// * `T` - The target type for deserialization, must implement `DeserializeOwned`
///
/// # Parameters
///
/// * `config_path` - Path to the configuration file (without extension)
///
/// # Returns
///
/// * `eyre::Result<T>` - The deserialized configuration or an error
/// # Errors
///
/// This function will return an error in the following situations:
/// * If the configuration file at `config_path` cannot be found or read
/// * If the configuration format is invalid or corrupted
/// * If environment variables with the specified prefix cannot be parsed
/// * If the deserialization to type `T` fails due to missing or type-mismatched fields
pub fn try_deserialize<T: serde::de::DeserializeOwned + ValidateConfig>(
    config_path: &str,
) -> eyre::Result<T> {
    let cfg_deserializer = Config::builder()
        .add_source(File::with_name(config_path).required(false))
        .add_source(Environment::with_prefix(ENV_APP_PREFIX).separator(SEPARATOR))
        .build()
        .wrap_err("could not load config")?;

    let config: T = cfg_deserializer
        .try_deserialize()
        .wrap_err("could not parse config")?;

    config.validate()?;

    Ok(config)
}

/// A trait for validating configuration structures.
pub trait ValidateConfig {
    /// Validates the configuration data.
    ///
    /// # Returns
    ///
    /// * `eyre::Result<()>` - Ok if validation passes, or an error with a message describing why
    ///   validation failed
    /// # Errors
    ///
    /// This function will return an error if config is wrong
    fn validate(&self) -> eyre::Result<()>;
}

/// Deserializes a duration value from seconds.
///
/// This function deserializes an unsigned 64-bit integer representing seconds
/// and converts it into a `Duration` type.
///
/// # Type Parameters
///
/// * `'de` - The lifetime of the deserializer
/// * `D` - The deserializer type
///
/// # Parameters
///
/// * `deserializer` - The deserializer to use
///
/// # Returns
///
/// * `Result<Duration, D::Error>` - The deserialized duration or an error
///
/// # Errors
///
/// This function will return an error if:
/// * The value cannot be deserialized as a u64
pub fn deserialize_duration_from_secs<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let seconds = u64::deserialize(deserializer)?;
    Ok(Duration::from_secs(seconds))
}
