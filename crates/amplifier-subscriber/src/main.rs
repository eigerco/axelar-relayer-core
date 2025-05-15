//! # Amplifier Subscriber
//!
//! The Amplifier Subscriber service connects to the Axelar Amplifier network
//! and subscribes to events that need to be relayed.
//!
//! This binary provides:
//! - A configurable event subscriber with different backend implementations (NATS, GCP)
//! - Health check endpoints for monitoring service health
//! - Graceful shutdown handling with signal trapping
//!
//! The service will periodically poll for events based on the configured tickrate
//! and process them according to the configured backend (NATS or GCP Pub/Sub).
//!
//! ## Usage
//!
//! ```bash
//! amplifier-subscriber --config path/to/config.toml
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

use bin_util::health_check;
use clap::Parser;
use config::Config;
use tokio_util::sync::CancellationToken;

// TODO: Move this to config
const MAX_ERRORS: i32 = 20;

#[derive(Parser, Debug)]
#[command(author = "Eiger", name = "Axelar<>Blockchain Relayer ")]
pub(crate) struct Cli {
    #[arg(
        long,
        short,
        default_value = "relayer-config.toml",
        help = "Config path"
    )]
    pub config_path: String,
}

#[tokio::main]
async fn main() {
    bin_util::ensure_backtrace_set();
    let cli = Cli::parse();

    let config: Config = bin_util::try_deserialize(&cli.config_path).expect("config is correct");
    if let Err(err) = bin_util::telemetry::init(&config.telemetry) {
        tracing::error!(?err, "Failed to initialize telemetry");
    }

    let cancel_token = bin_util::register_cancel();
    tokio::try_join!(
        spawn_subscriber_worker(config.tickrate, cli.config_path.clone(), &cancel_token),
        spawn_health_check_server(
            config.health_check.port,
            cli.config_path.clone(),
            cancel_token.clone()
        )
    )
    .expect("Failed to join main loop and health server tasks");

    tracing::info!("Amplifier subscriber has been shut down");
}

fn spawn_subscriber_worker(
    tickrate: Duration,
    config_path: String,
    cancel_token: &CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn({
        let cancel_token = cancel_token.clone();

        async move {
            #[cfg(feature = "nats")]
            let mut subscriber = components::nats::new_amplifier_subscriber(&config_path)
                .await
                .expect("subscriber is created");

            #[cfg(feature = "gcp")]
            let mut subscriber = components::gcp::new_amplifier_subscriber(&config_path)
                .await
                .expect("subscriber is created");

            tracing::debug!("Starting amplifier subscriber...");

            let mut error_count: i32 = 0;

            cancel_token
                .run_until_cancelled(async move {
                    let mut work_interval = tokio::time::interval(tickrate);
                    loop {
                        work_interval.tick().await;
                        if let Err(err) = subscriber.subscribe().await {
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

            tracing::warn!("Shutting down amplifier subscriber...");
        }
    })
}

fn spawn_health_check_server(
    port: u16,
    config_path: String,
    cancel_token: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn(async move {
        tracing::debug!("Starting health check server...");

        health_check::new(port)
            .add_health_check(move || {
                let config_path = config_path.clone();
                async move {
                    #[cfg(feature = "nats")]
                    let subscriber = components::nats::new_amplifier_subscriber(&config_path)
                        .await
                        .expect("subscriber is created");

                    #[cfg(feature = "gcp")]
                    let subscriber = components::gcp::new_amplifier_subscriber(&config_path)
                        .await
                        .expect("subscriber is created");
                    subscriber.check_health().await
                }
            })
            .run(cancel_token)
            .await;

        tracing::warn!("Shutting down health check server...");
    })
}
