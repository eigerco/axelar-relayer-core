//! Crate for interacting with the Amplifier API.
//! Intended to be used by Relayers supporting the Axelar infrastructure

mod client;
pub use client::*;
mod error;
pub use error::AmplifierApiError;
pub mod types;
pub use chrono;
