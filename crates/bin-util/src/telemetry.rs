//! Centralized OpenTelemetry initialization for tracing, metrics, and logging.
use eyre::Context as _;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::{MetricExporter, Protocol, SpanExporter, WithExportConfig as _};
use opentelemetry_resource_detectors::{K8sResourceDetector, ProcessResourceDetector};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::resource::ResourceDetector as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_semantic_conventions::resource;
use opentelemetry_system_metrics::init_process_observer;
use serde::Deserialize;

/// Configuration for telemetry
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
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
/// * `service_name` - The name of the service, used as an identifier in traces and metrics.
/// * `service_version` - The version of the service, included in telemetry data for versioning.
/// * `config` - A reference to the application configuration containing settings for tracing and
///   metrics subsystems.
///
/// # Returns
///
/// * `eyre::Result<(opentelemetry_sdk::trace::Tracer, JoinHandle<eyre::Result<()>>)>` - A tuple
///   containing:
///   - The configured OpenTelemetry tracer that can be used for creating spans
///   - A ``JoinHandle`` for the process metrics observer task
///
///
/// # Errors
///
/// This function may fail if:
/// * Trace or metric exporters cannot be created from configuration
/// * Global tracer or meter provider cannot be set
/// * Process metrics observer registration fails
/// * Required telemetry configuration is missing or invalid
/// * Could not start process observer thread
#[allow(clippy::print_stdout, reason = "starts before tracing is initialized")]
pub fn init(
    service_name: &str,
    service_version: &str,
    config: &Config,
) -> eyre::Result<(
    opentelemetry_sdk::trace::Tracer,
    std::thread::JoinHandle<eyre::Result<()>>,
)> {
    println!("connecting telemetry");

    let (span_exporter, metric_exporter) = get_exporters(config)?;
    let service_resource = Resource::builder()
        .with_service_name(service_name.to_owned())
        .with_attribute(KeyValue::new(
            resource::SERVICE_VERSION,
            service_version.to_owned(),
        ))
        .build();
    let process_resource = ProcessResourceDetector.detect();
    let k8s_resource = K8sResourceDetector.detect();
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(service_resource.clone())
        .with_resource(process_resource.clone())
        .with_resource(k8s_resource.clone())
        .build();

    global::set_tracer_provider(tracer_provider.clone());

    println!("tracer provider set");

    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(service_resource)
        .with_resource(process_resource)
        .with_resource(k8s_resource)
        .build();

    global::set_meter_provider(meter_provider);

    println!("meter provider set");
    let observer_handle = std::thread::Builder::new()
        .name("process-observer".into())
        .spawn(move || {
            let meter = global::meter("process");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .wrap_err("could not create tokio runtime")?;
            rt.block_on(init_process_observer(meter))
        })
        .wrap_err("could not start process obverver thread")?;

    let tracer_name = service_name.to_owned();
    let tracer = tracer_provider.tracer(tracer_name);
    Ok((tracer, observer_handle))
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
