//! telemetry/lib.rs
//!
//! Centralized OpenTelemetry initialization for tracing, metrics, and logging.

use opentelemetry::metrics::{Counter, Histogram};
use opentelemetry::trace::TracerProvider;
use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::{MetricExporter, Protocol, SpanExporter, WithExportConfig};
use opentelemetry_sdk::{Resource, metrics::SdkMeterProvider, trace::SdkTracerProvider};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Configuration for telemetry
#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct TelemetryConfig {
    /// Per-crate log levels (e.g. "my_crate" = "debug")
    pub filters: Option<Vec<String>>,
    /// OTLP endpoint (e.g. "http://localhost:4317" or "http://localhost:4318")
    pub otlp_endpoint: Option<String>,
    /// Service name for metrics and traces
    pub service_name: String,
}

/// Initialize tracing, logging, and metrics
pub fn init(config: &TelemetryConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = config
        .otlp_endpoint
        .as_deref()
        .unwrap_or("http://localhost:4318/v1/traces");

    // ===== Tracing Setup =====
    let span_exporter = SpanExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(endpoint)
        .build()?;

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

    let filters = config.filters.clone();
    // ===== Logging Setup =====
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

    // Set up the subscriber with console output and OpenTelemetry export for traces
    // The OpenTelemetry collector will convert traces with log events into logs for Loki
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_ansi(true))
        .with(OpenTelemetryLayer::new(tracer))
        .try_init()?;

    // ===== Metrics Setup =====
    let exporter = MetricExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(endpoint)
        .build()?;

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

/// Create and register a histogram metric (name and description must be `'static`)
pub fn register_histogram(name: &'static str, description: &'static str) -> Histogram<f64> {
    let meter = global::meter("telemetry");
    meter
        .f64_histogram(name)
        .with_description(description)
        .build()
}
