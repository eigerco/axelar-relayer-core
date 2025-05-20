//! Crate with supervisor and worker trait
use core::panic::AssertUnwindSafe;
use core::pin::Pin;
use core::time::Duration;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::{panic, thread};

use eyre::Context as _;
use nix::sys::stat::Mode;
use tokio_util::sync::CancellationToken;

const SHUTDOWN_SIGNAL_CHECK_INTERVAL_MS: u64 = 500;
const WORKER_GRACEFUL_SHUTDOWN_MS: u64 = 2000;
const WORKER_CRASH_CHECK_MS: u64 = 2000;

/// ENV var to set pipe path triggered when supervisor starts everything
pub const READY_PIPE_PATH_ENV: &str = "READY_PIPE_PATH";

type WorkerName = String;

/// Fn that will create worker
pub type WorkerBuildFn =
    Box<dyn Fn() -> Pin<Box<dyn Future<Output = eyre::Result<Box<dyn Worker>>>>> + Send>;

/// i.e. ingester/subscriber
pub trait Worker: Send {
    /// do work
    fn do_work<'s>(&'s mut self) -> Pin<Box<dyn Future<Output = eyre::Result<()>> + 's>>;
}

/// Run supervisor
#[tracing::instrument(name = "supervisor", skip_all)]
pub fn run(
    worker_builders: BTreeMap<WorkerName, WorkerBuildFn>,
    cancel_token: &CancellationToken,
    tickrate: Duration,
) -> eyre::Result<()> {
    if cfg!(debug_assertions) {
        if let Ok(pipe_path) = std::env::var(READY_PIPE_PATH_ENV) {
            tracing::warn!("[Test mode] deleting pipe file: {pipe_path}");
            if std::fs::exists(&pipe_path).expect("pipe existance checked") {
                std::fs::remove_file(pipe_path).expect("pipe file deleted");
            }
        }
    }

    _ = std::thread::scope(|scope| {
        let (worker_crashed_tx, worker_crashed_rx) = std::sync::mpsc::channel();
        let mut active_workers = BTreeMap::new();

        let supervisor_handle = scope.spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .wrap_err("failed to create tokio runtime for supervisor")?;

            runtime.block_on(async move {
                for (worker_name, builder) in &worker_builders {
                    let worker = match builder().await {
                        Ok(worker) => worker,
                        Err(err) => {
                            tracing::error!(%worker_name, ?err, "error during building worker");
                            _ = worker_crashed_tx.send(worker_name.clone());
                            continue;
                        },
                    };

                    let worker_handle = spawn_worker(
                        worker_name,
                        worker,
                        worker_crashed_tx.clone(),
                        cancel_token,
                        scope,
                        tickrate
                    );

                    active_workers.insert(worker_name.clone(), Some(worker_handle));
                }

                if cfg!(debug_assertions) {
                    if let Ok(pipe_path) = std::env::var(READY_PIPE_PATH_ENV) {
                        tracing::warn!("[Test mode] signaling readiness");
                        // Owner: read(S_IRUSR) + write(S_IWUSR)
                        // Group: read(S_IRGRP)
                        // Others: read(S_IROTH)
                        let mode = Mode::S_IRUSR | Mode::S_IWUSR | Mode::S_IRGRP | Mode::S_IROTH;
                        nix::unistd::mkfifo(pipe_path.as_str(), mode).expect("named fife created");

                        OpenOptions::new()
                            .write(true)
                            .open(pipe_path)
                            .expect("open ready pipe")
                            .write_all(b"READY")
                            .expect("write READY to ready pipe");

                        tracing::warn!("Readiness signal sent");
                    }
                }

                tracing::info!("all workers are running");

                let mut interval = tokio::time::interval(Duration::from_millis(SHUTDOWN_SIGNAL_CHECK_INTERVAL_MS));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            if cancel_token.is_cancelled() {
                                tracing::trace!("supervisor detected shutdown signal");
                                tokio::time::sleep(Duration::from_millis(WORKER_GRACEFUL_SHUTDOWN_MS)).await;
                                break;
                            }
                        }
                        worker_result = async {
                            match worker_crashed_rx.try_recv() {
                                Ok(name) => Some(name),
                                Err(std::sync::mpsc::TryRecvError::Empty) => {
                                    tokio::time::sleep(Duration::from_millis(WORKER_CRASH_CHECK_MS)).await;
                                    None
                                }
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                    tracing::trace!("worker channel disconnected");
                                    None
                                }
                            }
                        } => {
                            if let Some(worker_name) = worker_result {
                                if cancel_token.is_cancelled() {
                                    tracing::trace!("worker crashed during shutdown, no restarting");
                                    continue;
                                }
                                tracing::info!(worker_name, "Restarting worker");
                                let builder = worker_builders.get(&worker_name).expect("builder should be present");

                                // TODO: rate limiter?
                                let worker = match builder().await {
                                    Ok(worker) => worker,
                                    Err(err) => {
                                        tracing::error!(%worker_name, ?err, "error during building worker");
                                        _ = worker_crashed_tx.send(worker_name.clone());
                                        continue;
                                    },
                                };
                                let worker_handle = spawn_worker(
                                    &worker_name,
                                    worker,
                                    worker_crashed_tx.clone(),
                                    cancel_token,
                                    scope,
                                    tickrate
                                );

                                if let Some(Some(old_handle)) = active_workers.remove(&worker_name) {
                                    _ = old_handle.join();
                                    tracing::trace!(worker_name, "Replaced existing worker handle");
                                }
                                active_workers.insert(worker_name.clone(), Some(worker_handle));
                            }
                        }
                    }
                }
            });

            eyre::Ok(())
        });

        _ = supervisor_handle
            .join()
            .map_err(|err| eyre::eyre!(format!("Supervisor thread panicked: {err:?}")))?;

        tracing::info!("Supervisor shutting down");

        if cfg!(debug_assertions) {
            if let Ok(pipe_path) = std::env::var(READY_PIPE_PATH_ENV) {
                tracing::warn!("[Test mode] deleting ready pipe");
                std::fs::remove_file(pipe_path).expect("pipe file deleted");
            }
        }

        eyre::Ok(())
    });

    eyre::Ok(())
}

#[allow(clippy::cognitive_complexity, reason = "used only dusing development")]
fn spawn_worker<'scope>(
    worker_name: &WorkerName,
    mut worker: Box<dyn Worker>,
    worker_crashed_tx: std::sync::mpsc::Sender<String>,
    cancel_token: &'scope CancellationToken,
    scope: &'scope std::thread::Scope<'scope, '_>,
    tickrate: Duration,
) -> std::thread::ScopedJoinHandle<'scope, ()> {
    tracing::trace!(worker_name, "starting worker");
    let worker_name = worker_name.clone();

    scope.spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(err) => {
                tracing::error!(?err, worker_name, "failed to create tokio runtime");
                worker_crashed_tx.send(worker_name).expect("shutting down");
                return;
            }
        };

        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            runtime.block_on(async {
                cancel_token
                    .run_until_cancelled(async {
                        let mut interval = tokio::time::interval(tickrate);
                        let result: Result<(), _> = loop {
                            interval.tick().await;
                            if let Err(err) = worker.do_work().await {
                                break Err(err);
                            }
                        };
                        result
                    })
                    .await
            })
        }));

        match result {
            Ok(Some(Ok(()))) => {
                tracing::error!(worker_name, "worker exited, logic issue?");
            }
            Ok(None) => {
                tracing::info!(worker_name, "worker exited, shutting down");
            }
            Ok(Some(Err(err))) => {
                tracing::error!(worker_name, ?err, "worker error");
            }
            Err(panic_err) => {
                let panic_msg = if let Some(err) = panic_err.downcast_ref::<String>() {
                    err.clone()
                } else if let Some(err) = panic_err.downcast_ref::<&str>() {
                    (*err).to_owned()
                } else {
                    format!("Unknown panic: {panic_err:?}")
                };
                tracing::error!(worker_name, %panic_msg, "worker panicked");
            }
        }

        if cancel_token.is_cancelled() {
            thread::sleep(Duration::from_secs(1));
            _ = worker_crashed_tx.send(worker_name.clone());
            tracing::trace!(worker_name, "Reported worker crash for restart");
        }

        tracing::trace!("worker exit post crash report");
    })
}
