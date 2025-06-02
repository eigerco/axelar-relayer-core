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
//! use bin_util::health_check::{Server, HealthCheck};
//! use tokio_util::sync::CancellationToken;
//! use eyre::Result;
//! use async_trait::async_trait;
//!
//! struct MyHealthChecker;
//!
//! #[async_trait]
//! impl HealthCheck for MyHealthChecker {
//!     async fn check(&self) -> eyre::Result<bool> {
//!         // Your health check logic here
//!         Ok(true)
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let cancel_token = CancellationToken::new();
//!     let token_clone = cancel_token.clone();
//!
//!     let checker = MyHealthChecker;
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
use async_trait::async_trait;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Healthcheck config
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Port for the health check server
    pub port: u16,
}

/// A trait for implementing health checks.
///
/// Types implementing this trait should provide a method to check the health status.
/// The `check` method should return a `Result` indicating whether the health check passed.
#[async_trait]
pub trait HealthCheck {
    /// Performs a health check.
    ///
    /// # Returns
    ///
    /// A `Result` containing `true` if the health check passed, or `false` if it failed.
    /// Errors can be returned to indicate issues during the health check.
    async fn check(&self) -> eyre::Result<bool>;
}

/// A server that handles health check and readiness probe requests.
///
/// The server responds to HTTP requests on two endpoints:
/// - `/healthz`: For health checks (liveness probes)
/// - `/readyz`: For readiness probes
///
/// Both endpoints return 200 OK when the health check passes,
/// or 503 Service Unavailable when the check fails.
pub struct Server<H> {
    port: u16,
    checker: H,
}

impl<H: HealthCheck + Send + Sync + 'static> Server<H> {
    /// Creates a new `Server` bound to the specified port with a health checker.
    ///
    /// # Arguments
    ///
    /// * `port` - The TCP port on which the server will listen for HTTP requests
    /// * `checker` - An implementation of the `HealthCheck` trait that will handle health checks
    ///
    /// # Returns
    ///
    /// A new `Server` instance with the provided health checker.
    #[must_use]
    pub fn new(port: u16, checker: H) -> Server<H> {
        Server { port, checker }
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
        let app_state = AppState {
            checker: Arc::new(self.checker),
        };
        let app_state = Arc::new(app_state);
        let app = Router::new()
            .route("/healthz", get(Self::handle_healthz))
            .route("/readyz", get(Self::handle_readyz))
            .with_state(app_state);

        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.port))
            .await
            .expect("Failed to bind to address");

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                cancel_token.cancelled().await;
            })
            .await
            .expect("Server error");
    }

    async fn handle_healthz(State(state): State<Arc<AppState<H>>>) -> impl IntoResponse {
        let health = state.checker.check().await;
        match health {
            Ok(true) => (StatusCode::OK, Json(json!({ "status": "HEALTHY" }))),
            Ok(false) | Err(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "UNHEALTHY" })),
            ),
        }
    }

    async fn handle_readyz(State(state): State<Arc<AppState<H>>>) -> impl IntoResponse {
        let health = state.checker.check().await;
        match health {
            Ok(true) => (StatusCode::OK, Json(json!({ "status": "READY" }))),
            Ok(false) | Err(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "UNREADY" })),
            ),
        }
    }
}

struct AppState<H: HealthCheck + Send + Sync> {
    checker: Arc<H>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicBool, Ordering};
    use core::time::Duration;
    use tokio::time::sleep;

    struct TestChecker {
        should_fail: Arc<AtomicBool>,
    }

    impl TestChecker {
        fn new() -> Self {
            Self {
                should_fail: Arc::new(AtomicBool::new(false)),
            }
        }

        fn set_should_fail(&self, should_fail: bool) {
            self.should_fail.store(should_fail, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl HealthCheck for TestChecker {
        async fn check(&self) -> eyre::Result<bool> {
            if self.should_fail.load(Ordering::Relaxed) {
                Err(eyre::eyre!("Health check failed"))
            } else {
                Ok(true)
            }
        }
    }

    async fn run_server(
        port: u16,
        checker: impl HealthCheck + Send + Sync + 'static,
    ) -> CancellationToken {
        let cancel_token = CancellationToken::new();
        let token_clone = cancel_token.clone();
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
        let checker = TestChecker::new();
        run_server(port, checker).await;

        let url = format!("http://127.0.0.1:{port}/healthz");
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), r#"{"status":"HEALTHY"}"#);
    }

    #[tokio::test]
    async fn test_health_check_failure() {
        let port = get_free_port();
        let checker = TestChecker::new();
        checker.set_should_fail(true);
        run_server(port, checker).await;

        let url = format!("http://127.0.0.1:{port}/healthz");
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 503);
        assert_eq!(resp.text().await.unwrap(), r#"{"status":"UNHEALTHY"}"#);
    }

    #[tokio::test]
    async fn test_readyz_endpoint() {
        let port = get_free_port();
        let checker = TestChecker::new();
        let cancel_token = run_server(port, checker).await;

        let url = format!("http://127.0.0.1:{port}/readyz");
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), r#"{"status":"READY"}"#);

        cancel_token.cancel();
    }

    #[tokio::test]
    async fn test_cancellation() {
        let port = get_free_port();
        let checker = TestChecker::new();
        let cancel_token = run_server(port, checker).await;

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
