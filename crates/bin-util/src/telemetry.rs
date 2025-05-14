//! Centralized OpenTelemetry initialization for tracing, metrics, and logging.
use opentelemetry::metrics::{Counter, Histogram};
use opentelemetry::trace::TracerProvider;
use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::{MetricExporter, Protocol, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const DEFAULT_ENDPOINT: &str = "http://localhost:4318";

/// Configuration for telemetry
#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct TelemetryConfig {
    /// Per-crate log levels (e.g. "my_crate" = "debug")
    pub filters: Option<Vec<String>>,
    /// OTLP endpoint URL
    pub otlp_collector_endpoint: Option<String>,
    /// Protocol to use for OTLP (grpc or http)
    pub otlp_collector_protocol: Option<String>,
    /// Service name for metrics and traces
    pub service_name: String,
}

/// Initialize tracing, logging, and metrics
pub fn init(config: &TelemetryConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    setup_tracing(config)?;
    setup_metrics(config)?;

    Ok(())
}

fn setup_tracing(config: &TelemetryConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = config
        .otlp_collector_endpoint
        .clone()
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());

    let protocol = config
        .otlp_collector_protocol
        .clone()
        .unwrap_or_else(|| "http".to_string())
        .to_lowercase();

    let span_exporter_builder = SpanExporter::builder();

    let span_exporter = if protocol == "grpc" {
        // gRPC protocol
        span_exporter_builder
            .with_tonic()
            .with_protocol(Protocol::Grpc)
            .with_endpoint(&endpoint)
            .build()?
    } else {
        // HTTP protocol (default)
        let endpoint_with_path = format!("{}/v1/traces", endpoint);
        span_exporter_builder
            .with_http()
            .with_protocol(Protocol::HttpBinary)
            .with_endpoint(&endpoint_with_path)
            .build()?
    };

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

    // Install tracer
    global::set_tracer_provider(tracer_provider.clone());
    let tracer_name = config.service_name.clone();
    let tracer = tracer_provider.tracer(tracer_name);

    // ===== Logging Setup =====
    let filters = config.filters.clone();
    match &filters {
        Some(filters_vec) => println!("tracing filters: {:?}", filters_vec),
        None => println!("no tracing filters provided"),
    };

    let mut filter = EnvFilter::new("");
    if let Some(filters) = filters {
        for directive in filters {
            filter = filter.add_directive(directive.parse()?);
        }
    }

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_ansi(true))
        .with(OpenTelemetryLayer::new(tracer))
        .try_init()?;

    Ok(())
}

fn setup_metrics(config: &TelemetryConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = config
        .otlp_collector_endpoint
        .clone()
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());

    let protocol = config
        .otlp_collector_protocol
        .clone()
        .unwrap_or_else(|| "http".to_string())
        .to_lowercase();

    let metric_exporter_builder = MetricExporter::builder();

    let exporter = if protocol == "grpc" {
        // gRPC protocol
        metric_exporter_builder
            .with_tonic()
            .with_protocol(Protocol::Grpc)
            .with_endpoint(&endpoint)
            .build()?
    } else {
        // HTTP protocol (default)
        let endpoint_with_path = format!("{}/v1/metrics", endpoint);
        metric_exporter_builder
            .with_http()
            .with_protocol(Protocol::HttpBinary)
            .with_endpoint(&endpoint_with_path)
            .build()?
    };

    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)
        .with_resource(
            Resource::builder_empty()
                .with_attributes(vec![KeyValue::new(
                    "service.name",
                    config.service_name.clone(),
                )])
                .build(),
        )
        .build();

    global::set_meter_provider(meter_provider.clone());
    Ok(())
}

/// Create and register a counter metric (name and description must be `'static`)
pub fn register_counter(name: &'static str, description: &'static str) -> Counter<u64> {
    let meter = global::meter("telemetry");
    meter
        .u64_counter(name)
        .with_description(description)
        .build()
}

/// Update a counter metric
pub fn increment_counter(counter: &Counter<u64>, value: u64) {
    counter.add(value, &[]);
}

/// Create and register a histogram metric (name and description must be `'static`)
pub fn register_histogram(name: &'static str, description: &'static str) -> Histogram<f64> {
    let meter = global::meter("telemetry");
    meter
        .f64_histogram(name)
        .with_description(description)
        .build()
}
