//! # Health Check
//!
//! A lightweight HTTP server for implementing health and readiness probes.
//!
//! This crate provides functionality to create an HTTP server that responds to
//! health check requests on `/healthz` and readiness check requests on `/readyz`.
//! It allows registration of custom health check functions that determine the
//! server's health status.
//!
//! ## Example
//!
//! ```rust
//! use bin_util::health_check::{Server, CheckHealth};
//! use tokio_util::sync::CancellationToken;
//! use eyre::Result;
//! use std::sync::Arc;
//!
//! struct MyChecker;
//!
//! impl CheckHealth for MyChecker {
//!     async fn check_health(&self) -> Result<()> {
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let cancel_token = CancellationToken::new();
//!     let token_clone = cancel_token.clone();
//!
//!     let checker = Arc::new(MyChecker);
//!     let server = Server::new(8080, checker);
//!
//!     // Run the server in a separate tokio task
//!     tokio::spawn(async move {
//!         server.run(token_clone).await;
//!     });
//!
//!     // Your application logic here
//!     // When ready to shut down:
//!     cancel_token.cancel();
//!
//!     Ok(())
//! }
//! ```
use core::future::Future;
use std::sync::Arc;

use axum::Router;
use axum::extract::Extension;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use eyre::Context as _;
use serde::Deserialize;
use serde_json::json;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Trait for types that can perform health checks.
///
/// This trait should be implemented by services that need to report their health status
/// through the health check server endpoints.
pub trait CheckHealth: Send + Sync + 'static {
    /// Checks the health status of the service.
    ///
    /// This method verifies the service's health and returns an error if the health check fails.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the service is healthy
    /// * `Err(...)` - If there are any issues with the service's health
    fn check_health(&self) -> impl Future<Output = eyre::Result<()>> + Send;
}

/// Implement `CheckHealth` for `Arc<T>` where T implements `CheckHealth`
impl<T: CheckHealth> CheckHealth for Arc<T> {
    async fn check_health(&self) -> eyre::Result<()> {
        T::check_health(self).await
    }
}

/// Healthcheck config
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Port for the health check server
    pub health_check_port: u16,
}

impl crate::ValidateConfig for Config {
    fn validate(&self) -> eyre::Result<()> {
        eyre::ensure!(
            self.health_check_port > 0,
            "specific port expected for health check"
        );
        eyre::Ok(())
    }
}

/// Spawns a health check server that runs until cancellation.
///
/// This function loads health check configuration from a file and starts an HTTP server
/// that responds to health and readiness probes. The server runs asynchronously in a
/// separate task and can be gracefully shut down using the provided cancellation token.
///
/// # Arguments
///
/// * `config_path` - Path to the configuration file containing health check settings
/// * `service` - The service implementing health check logic, shared across threads
/// * `cancel_token` - Token for graceful shutdown of the health check server
///
/// # Returns
///
/// Returns a `JoinHandle` to the spawned task, allowing the caller to await completion
/// or check if the server is still running.
///
/// # Errors
///
/// Returns an error if:
/// - The configuration file cannot be read or parsed
/// - The configuration format is invalid
pub fn run_health_check_server<Service: CheckHealth>(
    config_path: &str,
    service: Arc<Service>,
    cancel_token: CancellationToken,
) -> eyre::Result<JoinHandle<()>> {
    let config: Config =
        crate::try_deserialize(config_path).wrap_err("health check config parse error")?;

    let handle = tokio::task::spawn(async move {
        tracing::trace!("Starting health check server...");

        Server::new(config.health_check_port, service)
            .run(cancel_token)
            .await;

        tracing::warn!("Shutting down health check server...");
    });

    Ok(handle)
}

/// A server that handles health check and readiness probe requests.
///
/// The server responds to HTTP requests on two endpoints:
/// - `/healthz`: For health checks (liveness probes)
/// - `/readyz`: For readiness probes
///
/// Both endpoints return 200 OK when all registered health checks pass,
/// or 503 Service Unavailable when any check fails.
pub struct Server<Checker> {
    port: u16,
    checker: Arc<Checker>,
}

impl<Checker: CheckHealth> Server<Checker> {
    /// Creates a new `Server` bound to the specified port.
    ///
    /// # Arguments
    ///
    /// * `port` - The TCP port on which the server will listen for HTTP requests
    /// * `checker` - The health checker implementation wrapped in Arc
    ///
    /// # Returns
    ///
    /// A new `Server` instance with the provided health checker.
    #[must_use]
    pub const fn new(port: u16, checker: Arc<Checker>) -> Self {
        Self { port, checker }
    }
    /// Starts the HTTP server and runs until the cancellation token is triggered.
    ///
    /// The server will bind to `0.0.0.0` on the configured port and respond to
    /// health check and readiness requests. When the cancellation token is triggered,
    /// the server will perform a graceful shutdown.
    ///
    /// # Arguments
    ///
    /// * `cancel_token` - A cancellation token that will trigger server shutdown when canceled
    ///
    /// # Panics
    ///
    /// This function will panic if it fails to bind to the specified port. This can happen
    /// if the port is already in use or if the process doesn't have permission to bind to
    /// the requested port.
    pub async fn run(self, cancel_token: CancellationToken) {
        let app = Router::new()
            .route("/healthz", get(handle_healthz::<Checker>))
            .route("/readyz", get(handle_readyz::<Checker>))
            .layer(Extension(self.checker));

        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.port))
            .await
            .expect("Failed to bind to address");

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                cancel_token.cancelled().await;
            })
            .await
            .expect("Health check Server error");
    }
}

async fn handle_healthz<Checker: CheckHealth>(
    Extension(checker): Extension<Arc<Checker>>,
) -> impl IntoResponse {
    match checker.check_health().await {
        Ok(()) => {
            tracing::trace!("Health check succeeded");
            (StatusCode::OK, Json(json!({ "status": "HEALTHY" })))
        }
        Err(err) => {
            tracing::trace!(?err, "Health check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "UNHEALTHY" })),
            )
        }
    }
}

async fn handle_readyz<Checker: CheckHealth>(
    Extension(checker): Extension<Arc<Checker>>,
) -> impl IntoResponse {
    match checker.check_health().await {
        Ok(()) => {
            tracing::trace!("Readiness check succeeded");
            (StatusCode::OK, Json(json!({ "status": "READY" })))
        }
        Err(err) => {
            tracing::trace!(?err, "Readiness check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "UNREADY" })),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicBool, Ordering};
    use core::time::Duration;

    use tokio::time::sleep;

    use super::*;

    struct TestChecker<F> {
        check_fn: F,
    }

    impl<F, Fut> CheckHealth for TestChecker<F>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = eyre::Result<()>> + Send + 'static,
    {
        async fn check_health(&self) -> eyre::Result<()> {
            (self.check_fn)().await
        }
    }

    async fn run_server<F, Fut>(port: u16, health_check: F) -> CancellationToken
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = eyre::Result<()>> + Send + 'static,
    {
        let cancel_token = CancellationToken::new();
        let token_clone = cancel_token.clone();

        let checker = Arc::new(TestChecker {
            check_fn: health_check,
        });
        let server = Server::new(port, checker);
        tokio::spawn(async move {
            server.run(token_clone).await;
        });

        // Give the server time to start
        sleep(Duration::from_millis(100)).await;

        cancel_token
    }

    fn get_free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[tokio::test]
    async fn test_health_check_success() {
        let port = get_free_port();
        run_server(port, || async { Ok(()) }).await;

        let url = format!("http://127.0.0.1:{port}/healthz");
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), r#"{"status":"HEALTHY"}"#);
    }

    #[tokio::test]
    async fn test_health_check_failure() {
        let port = get_free_port();
        run_server(port, || async { Err(eyre::eyre!("Health check failed")) }).await;

        let url = format!("http://127.0.0.1:{port}/healthz");
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 503);
        assert_eq!(resp.text().await.unwrap(), r#"{"status":"UNHEALTHY"}"#);
    }

    #[tokio::test]
    async fn test_dynamic_health_status() {
        let port = get_free_port();
        let flag = Arc::new(AtomicBool::new(false));
        let cancel_token = CancellationToken::new();
        let token_clone = cancel_token.clone();

        let is_healthy_flag = Arc::clone(&flag);
        let checker = Arc::new(TestChecker {
            check_fn: move || {
                let is_ok = !is_healthy_flag.load(Ordering::Relaxed);
                async move {
                    if is_ok {
                        Ok(())
                    } else {
                        Err(eyre::eyre!("Intentional failure"))
                    }
                }
            },
        });
        let server = Server::new(port, checker);

        tokio::spawn(async move {
            server.run(token_clone).await;
        });

        sleep(Duration::from_millis(100)).await;

        let url = format!("http://127.0.0.1:{port}/healthz");
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), r#"{"status":"HEALTHY"}"#);

        flag.store(true, Ordering::SeqCst);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 503);
        assert_eq!(resp.text().await.unwrap(), r#"{"status":"UNHEALTHY"}"#);

        cancel_token.cancel();
    }

    #[tokio::test]
    async fn test_readyz_endpoint() {
        let port = get_free_port();
        let cancel_token = run_server(port, || async { Ok(()) }).await;

        let url = format!("http://127.0.0.1:{port}/readyz");
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), r#"{"status":"READY"}"#);

        cancel_token.cancel();

        let port2 = get_free_port();
        let cancel_token2 =
            run_server(port2, || async { Err(eyre::eyre!("Readiness failed")) }).await;

        let url2 = format!("http://127.0.0.1:{port2}/readyz");
        let resp2 = reqwest::get(&url2).await.unwrap();
        assert_eq!(resp2.status(), 503);
        assert_eq!(resp2.text().await.unwrap(), r#"{"status":"UNREADY"}"#);

        cancel_token2.cancel();
    }

    #[tokio::test]
    async fn test_not_found() {
        let port = get_free_port();
        let cancel_token = run_server(port, || async { Ok(()) }).await;

        let url = format!("http://127.0.0.1:{port}/notfound");
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 404);

        cancel_token.cancel();
    }

    #[tokio::test]
    async fn test_cancellation() {
        let port = get_free_port();
        let cancel_token = run_server(port, || async { Ok(()) }).await;

        // Verify server is running
        let url = format!("http://127.0.0.1:{port}/healthz");
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);

        // Cancel the server
        cancel_token.cancel();
        sleep(Duration::from_millis(100)).await;

        // After cancellation, the server should no longer respond
        let result = reqwest::get(&url).await;
        assert!(
            result.is_err(),
            "Server should no longer respond after cancellation"
        );
    }
}
