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
mod components;
mod config;

use core::time::Duration;
use std::path::PathBuf;
use std::sync::Arc;

use bin_util::health_check;
use clap::Parser;
use config::Config;
use tokio_util::sync::CancellationToken;

// TODO: Move this to config
const MAX_ERRORS: i32 = 20;
#[derive(Parser, Debug)]
#[command(author = "Eiger", name = "Axelar<>Blockchain Relayer")]
pub(crate) struct Cli {
    #[arg(
        long,
        short,
        default_value = "relayer-config.toml",
        help = "Config path"
    )]
    pub config_path: PathBuf,
}

#[tokio::main]
async fn main() {
    bin_util::ensure_backtrace_set();

    let cli = Cli::parse();

    let config: Config = config::try_deserialize(&cli.config_path).expect("generic config");
    let cancel_token = setup_shutdown_signal();

    tokio::try_join!(
        spawn_subscriber_worker(
            config.tickrate,
            cli.config_path.clone(),
            cancel_token.clone()
        ),
        spawn_health_check_server(
            config.health_check.port,
            cli.config_path.clone(),
            cancel_token.clone()
        )
    )
    .expect("Failed to join tasks");

    tracing::info!("Amplifier ingester has been shut down");
}

fn setup_shutdown_signal() -> CancellationToken {
    let token = CancellationToken::new();
    let token_clone = token.clone();

    ctrlc::set_handler(move || {
        if token_clone.is_cancelled() {
            tracing::warn!("Immediate shutdown initiated.");
            #[expect(clippy::restriction, reason = "immediate exit")]
            std::process::exit(1);
        } else {
            tracing::info!("Graceful shutdown initiated. Press Ctrl+C again for immediate exit.");
            token_clone.cancel();
        }
    })
    .expect("Failed to register ctrl+c handler");

    token
}

fn spawn_subscriber_worker(
    tickrate: Duration,
    config_path: PathBuf,
    cancel_token: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    // Avoid shadowing by renaming the variable
    let shutdown_token = cancel_token;
    tokio::task::spawn(async move {
        #[cfg(feature = "nats")]
        let ingester = components::nats::new_amplifier_ingester(config_path)
            .await
            .expect("ingester is created");

        #[cfg(feature = "gcp")]
        let ingester = components::gcp::new_amplifier_ingester(config_path, shutdown_token.clone())
            .await
            .expect("ingester is created");

        tracing::debug!("Starting amplifier ingester...");

        shutdown_token
            .run_until_cancelled(async move {
                let mut work_interval = tokio::time::interval(tickrate);
                let mut error_count: i32 = 0;

                loop {
                    work_interval.tick().await;
                    if let Err(err) = ingester.ingest().await {
                        tracing::error!(?err, "error during ingest, skipping...");
                        error_count = error_count.saturating_add(1);
                        if error_count >= MAX_ERRORS {
                            tracing::error!("Max error threshold reached. Exiting loop.");
                            break;
                        }
                    } else {
                        error_count = 0_i32;
                    }
                }
            })
            .await;

        tracing::warn!("Shutting down amplifier ingester...");
    })
}

fn spawn_health_check_server(
    port: u16,
    config_path: PathBuf,
    cancel_token: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn(async move {
        #[cfg(feature = "nats")]
        let ingester = components::nats::new_amplifier_ingester(config_path)
            .await
            .expect("ingester is created");

        #[cfg(feature = "gcp")]
        let ingester = components::gcp::new_amplifier_ingester(config_path, cancel_token.clone())
            .await
            .expect("ingester is created");

        let ingester = Arc::new(ingester);

        tracing::debug!("Starting health check server...");

        health_check::new(port)
            .add_health_check(move || {
                let ingester = Arc::clone(&ingester);
                async move { ingester.check_health().await }
            })
            .run(cancel_token)
            .await;

        tracing::warn!("Shutting down health check server...");
    })
}
