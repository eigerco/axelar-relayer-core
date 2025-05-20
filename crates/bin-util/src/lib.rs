//! Binary utils

const ENV_APP_PREFIX: &str = "RELAYER";
const SEPARATOR: &str = "_";

pub mod health_check;
pub mod telemetry;
use core::time::Duration;

use config::{Config, Environment, File};
use eyre::Context as _;
use opentelemetry::global;
use opentelemetry::metrics::Counter;
use serde::{Deserialize as _, Deserializer};
use tokio_util::sync::CancellationToken;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_error::ErrorLayer;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// Ensures backtrace is enabled
#[allow(dead_code, reason = "temporary")]
pub fn ensure_backtrace_set() {
    // SAFETY: safe in single thread
    unsafe {
        std::env::set_var("RUST_BACKTRACE", "full");
    }
}

/// Initializes the logging system with optional filtering directives.
///
/// This function sets up the `color_eyre` error handling framework and configures
/// a tracing `EnvFilter` based on the provided filter directives.
///
/// # Arguments
///
/// * `env_filters` - Optional vector of filter directives to control log verbosity. Each directive
///   should follow the tracing filter syntax (e.g., "info", "`starknet_relayer=debug`",
///   "`warn,my_module=trace`"). If `None` is provided, a default empty filter will be created.
///
/// * `telemetry_tracer` - Optional OpenTelemetry SDK tracer for distributed tracing integration.
///   When provided, spans and events will be exported to the configured OpenTelemetry backend.
///
///
/// # Returns
///
/// * `eyre::Result<WorkerGuard>` - A worker guard that must be kept alive for the duration of the
///   program to ensure logs are properly processed and flushed to `stderr`. When this guard is
///   dropped, the background worker thread will be shut down.
///
/// # Errors
///
/// Returns an error if:
/// * `color_eyre` cannot be installed
/// * Any of the provided filter directives fail to parse
/// * The tracing subscriber cannot be initialized
pub fn init_logging(
    env_filters: Option<Vec<String>>,
    telemetry_tracer: Option<opentelemetry_sdk::trace::Tracer>,
) -> eyre::Result<WorkerGuard> {
    color_eyre::install().wrap_err("color eyre could not be installed")?;

    let mut env_filter = EnvFilter::new("");
    if let Some(filters) = env_filters {
        for directive in filters {
            env_filter = env_filter.add_directive(directive.parse()?);
        }
    }

    let (non_blocking, worker_guard) = tracing_appender::non_blocking(std::io::stderr());

    let output_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .log_internal_errors(true)
        .with_file(true)
        .with_line_number(true)
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(non_blocking)
        .with_ansi(cfg!(debug_assertions));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(ErrorLayer::default());
    if let Some(telemetry_tracer) = telemetry_tracer {
        if cfg!(debug_assertions) {
            subscriber
                .with(output_layer)
                .with(OpenTelemetryLayer::new(telemetry_tracer).with_level(true))
                .try_init()?;
        } else {
            subscriber
                .with(output_layer.json())
                .with(OpenTelemetryLayer::new(telemetry_tracer).with_level(true))
                .try_init()?;
        }
    } else if cfg!(debug_assertions) {
        subscriber.with(output_layer).try_init()?;
    } else {
        subscriber.with(output_layer.json()).try_init()?;
    }

    Ok(worker_guard)
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

/// Global metrics
pub struct GlobalMetrics {
    /// global count of errors
    errors_counter: Counter<u64>,
}

impl GlobalMetrics {
    /// Creates a new set of global metrics with the specified meter name.
    ///
    /// # Parameters
    ///
    /// * `name` - A static string that identifies the meter. This should be a descriptive name for
    ///   the component or subsystem being monitored.
    ///
    /// # Returns
    ///
    /// A new `Metrics` instance with initialized counters and other metrics.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        let meter = global::meter(name);

        let errors_counter = meter
            .u64_counter("global.errors")
            .with_description("Total number of errors encountered during operation and not tracked by inner components")
            .build();

        Self { errors_counter }
    }

    /// Record error
    pub fn record_error(&self) {
        self.errors_counter.add(1, &[]);
    }
}
