//! Binary utils for top level ingesters/subscribers

/// Env var prefix used to feed config to application
/// i.e. `RELAYER_ACK_DEADLINE_SECS=10`
pub const ENV_APP_PREFIX: &str = "RELAYER";
const SEPARATOR: &str = "_";

pub mod health_check;
pub mod telemetry;
use core::time::Duration;

use config::{Config, Environment, File};
use eyre::Context as _;
use opentelemetry::metrics::Counter;
use opentelemetry::{KeyValue, global};
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

/// export telemetry components for custom metrics
pub mod telemetry_components {
    pub use opentelemetry::metrics::*;
    pub use opentelemetry::*;
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

/// Simple metrics for tracking basic error and skip events.
///
/// `SimpleMetrics` provides a lightweight metrics collection system focused on tracking
/// error occurrences and skipped operations. This is useful for simple applications or
/// components that need basic observability without the overhead of comprehensive metrics.
///
/// # Examples
///
/// ```
/// use opentelemetry::KeyValue;
/// use bin_util::SimpleMetrics;
///
/// // Create metrics with component identification
/// let metrics = SimpleMetrics::new(
///     "simple_component",
///     vec![
///         KeyValue::new("component", "data_processor"),
///     ]
/// );
///
/// metrics.record_error();
///
/// // Record metrics during operations
pub struct SimpleMetrics {
    error_raised: Counter<u64>,
    skipped: Counter<u64>,
    attributes: Vec<KeyValue>,
}

impl SimpleMetrics {
    /// Creates a new set of simple metrics with the specified meter name.
    ///
    /// # Parameters
    ///
    /// * `name` - The name to use for the OpenTelemetry meter, typically identifying the component
    ///   or service (e.g., `"solana_ingester"`).
    /// * `attributes` - Common key-value pairs to attach to all metrics
    ///
    /// # Returns
    ///
    /// A new `SimpleMetrics` instance with initialized counters.
    #[must_use]
    pub fn new(name: &'static str, attributes: Vec<KeyValue>) -> Self {
        let meter = global::meter(name);

        let errors_counter = meter
            .u64_counter("errors")
            .with_description("Total number of errors encountered during operation and not tracked by inner components")
            .build();

        let skipped_counter = meter
            .u64_counter("skipped.count")
            .with_description("Total number of skipped operations")
            .build();

        Self {
            error_raised: errors_counter,
            skipped: skipped_counter,
            attributes,
        }
    }

    /// Record error
    pub fn record_error(&self) {
        self.error_raised.add(1, &self.attributes);
    }

    /// Records a skipped task.
    pub fn record_skipped(&self) {
        self.skipped.add(1, &self.attributes);
    }
}

/// Metrics for tracking the processing of various amplifier API tasks in the ingester of
/// counterparty chain.
///
/// This struct provides standardized instrumentation for cross-chain amplifier components,
/// tracking different task types processed by blockchain ingesters. It maintains counters
/// for gateway transactions, executions, verifications, refunds, proof construction,
/// errors, and skipped tasks.
///
/// All metrics share the same attributes provided during initialization, allowing for
/// consistent dimensions across different metric types.
////// # Examples
///
/// ```
/// use opentelemetry::KeyValue;
/// use bin_util::BlockChainIngesterMetrics;
///
/// // Create metrics for a Solana counterparty ingester
/// let metrics = BlockChainIngesterMetrics::new(
///     "solana_ingester",
///     vec![
///         KeyValue::new("type", "10"),
///     ]
/// );
///
/// metrics.record_gateway_tx_approved();
/// ```
///
/// // If you need to extend metrics simply add couter to "owner struct"" i.e.
/// ```
/// use opentelemetry::metrics::Counter;
/// use bin_util::BlockChainIngesterMetrics;
///
/// struct Ingester {
///     metrics: BlockChainIngesterMetrics,
///     counter: Counter<u64>
/// }
/// ```
/// // Or create another metrics struct
pub struct BlockChainIngesterMetrics {
    // received
    gateway_tx_received_task: Counter<u64>,
    execute_received_task: Counter<u64>,
    verify_received_task: Counter<u64>,
    refund_received_task: Counter<u64>,
    construct_proof_received_task: Counter<u64>,
    // processed
    gateway_tx_approved_task: Counter<u64>,
    executed_task: Counter<u64>,
    verified_stak: Counter<u64>,
    refunded_task: Counter<u64>,
    constructed_proof_task: Counter<u64>,

    error_raised: Counter<u64>,
    skipped_task: Counter<u64>,
    attributes: Vec<KeyValue>,
}

impl BlockChainIngesterMetrics {
    /// Creates a new `BlockChainIngesterMetrics` instance with the specified meter name and
    /// attributes.
    ///
    /// # Parameters
    ///
    /// * `name` - The name to use for the OpenTelemetry meter, typically identifying the component
    ///   (e.g., `"solana_ingester"`).
    /// * `attributes` - Common key-value pairs to attach to all metrics
    ///
    /// # Returns
    ///
    /// A new `BlockChainIngesterMetrics` instance with all counters initialized.
    #[must_use]
    pub fn new(name: &'static str, attributes: Vec<KeyValue>) -> Self {
        let meter = global::meter(name);

        // Received task counters
        let gateway_tx_received_task = meter
            .u64_counter("tasks.received.gateway_tx.count")
            .with_description("Number of received GatewayTx tasks")
            .build();

        let execute_received_task = meter
            .u64_counter("tasks.received.execute.count")
            .with_description("Number of received Execute tasks")
            .build();

        let verify_received_task = meter
            .u64_counter("tasks.received.verify.count")
            .with_description("Number of received Verify tasks")
            .build();

        let refund_received_task = meter
            .u64_counter("tasks.received.refund.count")
            .with_description("Number of received Refund tasks")
            .build();

        let construct_proof_received_task = meter
            .u64_counter("tasks.received.construct_proof.count")
            .with_description("Number of received ConstructProof tasks")
            .build();

        // Processed task counters
        let gateway_tx_approved_task = meter
            .u64_counter("tasks.processed.gateway_tx.count")
            .with_description("Number of processed GatewayTx tasks")
            .build();
        let executed_task = meter
            .u64_counter("tasks.processed.execute.count")
            .with_description("Number of processed Execute tasks")
            .build();
        let verified_stak = meter
            .u64_counter("tasks.processed.verify.count")
            .with_description("Number of processed Verify tasks")
            .build();
        let refunded_task = meter
            .u64_counter("tasks.processed.refund.count")
            .with_description("Number of processed Refund tasks")
            .build();
        let constructed_proof_task = meter
            .u64_counter("tasks.processed.construct_proof.count")
            .with_description("Number of processed ConstructProof tasks")
            .build();

        let error_raised = meter
            .u64_counter("errors.count")
            .with_description("Total number of errors encountered during operation")
            .build();
        let skipped_task = meter
            .u64_counter("skipped.count")
            .with_description("Total number of skipped tasks")
            .build();

        Self {
            // Received tasks
            gateway_tx_received_task,
            execute_received_task,
            verify_received_task,
            refund_received_task,
            construct_proof_received_task,

            // Processed tasks
            gateway_tx_approved_task,
            executed_task,
            verified_stak,
            refunded_task,
            constructed_proof_task,

            error_raised,
            skipped_task,
            attributes,
        }
    }

    // -- Received task methods

    /// Records the receipt of a gateway transaction task.
    pub fn record_gateway_tx_received(&self) {
        self.gateway_tx_received_task.add(1, &self.attributes);
    }

    /// Records the receipt of an execute task.
    pub fn record_execute_received(&self) {
        self.execute_received_task.add(1, &self.attributes);
    }

    /// Records the receipt of a verify task.
    pub fn record_verify_received(&self) {
        self.verify_received_task.add(1, &self.attributes);
    }

    /// Records the receipt of a refund task.
    pub fn record_refund_received(&self) {
        self.refund_received_task.add(1, &self.attributes);
    }

    /// Records the receipt of a proof construction task.
    pub fn record_construct_proof_received(&self) {
        self.construct_proof_received_task.add(1, &self.attributes);
    }

    // -- Processed task methods

    /// Records the successful completion of a gateway transaction task.
    pub fn record_gateway_tx_approved(&self) {
        self.gateway_tx_approved_task.add(1, &self.attributes);
    }

    /// Records the successful completion of an execute task.
    pub fn record_executed(&self) {
        self.executed_task.add(1, &self.attributes);
    }

    /// Records the successful completion of a verify task.
    pub fn record_verified(&self) {
        self.verified_stak.add(1, &self.attributes);
    }

    /// Records the successful completion of a refund task.
    pub fn record_refunded(&self) {
        self.refunded_task.add(1, &self.attributes);
    }

    /// Records the successful completion of a proof construction task.
    pub fn record_constructed_proof(&self) {
        self.constructed_proof_task.add(1, &self.attributes);
    }

    // -- Etc

    /// Records an error encountered during task processing.
    pub fn record_error(&self) {
        self.error_raised.add(1, &self.attributes);
    }
    /// Records a skipped task.
    pub fn record_skipped(&self) {
        self.skipped_task.add(1, &self.attributes);
    }
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
