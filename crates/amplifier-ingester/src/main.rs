//! # Amplifier Ingester
//!
//! The Amplifier Ingester service connects to the Axelar Amplifier network
//! and periodically ingests events from the network that need to be relayed.
//!
//! This binary provides:
//! - A configurable event ingestion service with different backend implementations (NATS, GCP)
//! - Health check endpoints for monitoring service health
//! - Graceful shutdown handling with signal trapping
//!
//! The service will periodically check for new events based on the configured tickrate
//! and process them according to the configured backend (NATS or GCP).
//!
//! ## Usage
//!
//! ```bash
//! amplifier-ingester --config path/to/config.toml
//! ```
//!
//! ## Configuration
//!
//! The service is configured via a TOML file that specifies:
//! - Health check server port
//! - Event processing tickrate
//! - Backend-specific configuration (NATS or GCP)

// Ensure only one backend is enabled at a time
#[cfg(all(feature = "nats", feature = "gcp"))]
compile_error!(
    "Features 'nats' and 'gcp' cannot be enabled at the same time. Please choose only one backend."
);

#[cfg(not(any(feature = "nats", feature = "gcp")))]
compile_error!("Either 'nats' or 'gcp' feature must be enabled. Please choose one backend.");

use std::sync::Arc;

use amplifier_ingester::config::Config;
use bin_util::health_check;
use clap::{Parser, crate_name, crate_version};
use tokio_util::sync::CancellationToken;

#[derive(Parser, Debug)]
#[command(author = "Eiger", name = "Axelar Relayer(amplifier-ingester)")]
pub(crate) struct Cli {
    #[arg(long, short, default_value = "relayer-config", help = "Config path")]
    pub config_path: String,
}

#[tokio::main]
async fn main() {
    bin_util::ensure_backtrace_set();
    let cli = Cli::parse();

    let config: Config = bin_util::try_deserialize(&cli.config_path).expect("config is correct");
    let (telemetry_tracer, _observer_handle) = if let Some(ref telemetry_cfg) = config.telemetry {
        let (tracer, observer_handle) =
            bin_util::telemetry::init(crate_name!(), crate_version!(), telemetry_cfg)
                .expect("telemetry wired up");
        (Some(tracer), Some(observer_handle))
    } else {
        (None, None)
    };

    let health_check_cfg: health_check::Config =
        bin_util::try_deserialize(&cli.config_path).expect("health check config is correct");

    let _stderr_logging_guard = bin_util::init_logging(telemetry_tracer).expect("logging wired up");

    let cancel_token = bin_util::register_cancel();

    run_ingester(
        &cli.config_path,
        config,
        health_check_cfg.health_check.port,
        cancel_token,
    )
    .await;

    tracing::info!("Amplifier ingester has been shut down");
}

async fn run_ingester(
    config_path: &str,
    config: Config,
    health_check_port: u16,
    cancel_token: CancellationToken,
) {
    #[cfg(feature = "nats")]
    let ingester = Arc::new(
        amplifier_ingester::nats::new_amplifier_ingester(config_path)
            .await
            .expect("ingester is created"),
    );

    #[cfg(feature = "gcp")]
    let ingester = Arc::new(
        amplifier_ingester::gcp::new_amplifier_ingester(config_path, cancel_token.clone())
            .await
            .expect("ingester is created"),
    );

    let worker_handle = tokio::task::spawn({
        let cancel_token = cancel_token.clone();
        let tickrate = config.tickrate;
        let max_errors = config.max_errors;
        let ingester = Arc::clone(&ingester);

        async move {
            tracing::trace!("Starting amplifier ingester...");

            cancel_token
                .run_until_cancelled(async move {
                    let mut work_interval = tokio::time::interval(tickrate);
                    let mut error_count: u32 = 0;

                    loop {
                        work_interval.tick().await;
                        if let Err(err) = ingester.ingest().await {
                            tracing::error!(?err, "error during ingest, skipping...");
                            error_count = error_count.saturating_add(1);
                            if error_count >= max_errors {
                                tracing::error!("Max error threshold reached. Exiting loop.");
                                break;
                            }
                        } else {
                            error_count = 0_u32;
                        }
                    }
                })
                .await;

            tracing::warn!("Shutting down amplifier ingester...");
        }
    });

    let health_check_handle = tokio::task::spawn(async move {
        tracing::trace!("Starting health check server...");

        health_check::Server::new(health_check_port, ingester)
            .run(cancel_token)
            .await;

        tracing::warn!("Shutting down health check server...");
    });

    tokio::try_join!(worker_handle, health_check_handle)
        .expect("Failed to join main loop and health server tasks");
}
