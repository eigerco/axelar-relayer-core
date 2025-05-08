//! Binary utils

pub mod health_check;
use tokio_util::sync::CancellationToken;

/// Ensures backtrace is enabled
#[allow(dead_code, reason = "temporary")]
pub fn ensure_backtrace_set() {
    // SAFETY: safe in single thread
    unsafe {
        std::env::set_var("RUST_BACKTRACE", "full");
    }
}

/// Register cancel token and ctrl+c handler
///
/// # Panics
///   on failure to register ctr+c handler
#[allow(
    clippy::print_stdout,
    reason = "not a tracing msg, should always display"
)]
#[allow(dead_code, reason = "temporary")]
#[must_use]
pub fn register_cancel() -> CancellationToken {
    let cancel_token = CancellationToken::new();
    let ctrlc_token = cancel_token.clone();
    ctrlc::set_handler(move || {
        if ctrlc_token.is_cancelled() {
            #[expect(clippy::restriction, reason = "immediate exit")]
            std::process::exit(1);
        } else {
            println!("\nGraceful shutdown initiated. Press Ctrl+C again for immediate exit...");
            ctrlc_token.cancel();
        }
    })
    .expect("Failed to register ctrl+c handler");
    cancel_token
}
