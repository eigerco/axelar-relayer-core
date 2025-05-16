//! Centralized OpenTelemetry initialization for tracing, metrics, and logging.
use eyre::Context as _;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::{MetricExporter, Protocol, SpanExporter, WithExportConfig as _};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use serde::Deserialize;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// Configuration for telemetry
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Service name for metrics and traces
    pub service_name: String,
    /// Per-crate log levels (e.g. `my_crate` = "debug")
    pub filters: Option<Vec<String>>,
    /// OTLP endpoint URL
    pub otlp_endpoint: String,
    /// Protocol to use for OTLP (grpc or http)
    pub otlp_transport: Transport,
}

/// Tracing/Metrics telemetry transport
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// Http (binary)
    Http,
    /// Grp
    Grpc,
}

/// Initializes the application with tracing and metrics systems.
///
/// # Arguments
///
/// * `config` - A reference to the application configuration containing settings for tracing and
///   metrics subsystems.
///
/// # Returns
///
/// * `Ok(())` - If initialization was successful.
/// * `Err(...)` - If any initialization step failed, with the underlying error.
///
/// # Errors
///
/// This function may fail if:
/// * Tracing system initialization fails
/// * Metrics system initialization fails
pub fn init(config: &Config) -> eyre::Result<()> {
    let (span_exporter, metric_exporter) = get_exporters(config)?;

    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(
            Resource::builder_empty()
                .with_attributes(vec![KeyValue::new(
                    "service.name",
                    config.service_name.clone(),
                )])
                .build(),
        )
        .build();

    global::set_tracer_provider(tracer_provider.clone());

    let tracer_name = config.service_name.clone();
    let tracer = tracer_provider.tracer(tracer_name);

    let mut filter = EnvFilter::new("");
    if let Some(filters) = &config.filters {
        for directive in filters {
            filter = filter.add_directive(directive.parse()?);
        }
    }

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_ansi(true))
        .with(OpenTelemetryLayer::new(tracer))
        .try_init()?;
    let filters = config.filters.clone();

    #[allow(
        clippy::print_stdout,
        reason = "need to get filters before bootstrapping tracing"
    )]
    match &filters {
        Some(filters_vec) => println!("tracing filters: {filters_vec:?}"),
        None => println!("no tracing filters provided"),
    };

    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(
            Resource::builder_empty()
                .with_attributes(vec![KeyValue::new(
                    "service.name",
                    config.service_name.clone(),
                )])
                .build(),
        )
        .build();

    global::set_meter_provider(meter_provider);
    Ok(())
}

fn get_exporters(config: &Config) -> eyre::Result<(SpanExporter, MetricExporter)> {
    match config.otlp_transport {
        Transport::Http => {
            let span_exporter = SpanExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(format!("{}/v1/traces", config.otlp_endpoint))
                .build()
                .wrap_err("set up http trace exporter")?;

            let metric_exporter = MetricExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(format!("{}/v1/metrics", config.otlp_endpoint))
                .build()
                .wrap_err("set up http metric exporter")?;

            Ok((span_exporter, metric_exporter))
        }
        Transport::Grpc => {
            let span_exporter = SpanExporter::builder()
                .with_tonic()
                .with_protocol(Protocol::Grpc)
                .with_endpoint(&config.otlp_endpoint)
                .build()
                .wrap_err("set up grpc trace exporter")?;

            let metric_exporter = MetricExporter::builder()
                .with_tonic()
                .with_protocol(Protocol::Grpc)
                .with_endpoint(&config.otlp_endpoint)
                .build()
                .wrap_err("set up grcp metric exporter")?;

            Ok((span_exporter, metric_exporter))
        }
    }
}
