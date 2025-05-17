//! Storage Bus crate provides implementations for various storage backends.
//!
//! This crate offers a unified interface for storing and retrieving data
//! across different platforms and services including Google Cloud Platform,
//! NATS, and potentially others.
//!
//! # Features
//!
//! - `gcp`: Google Cloud Platform implementation
//! - `nats`: NATS messaging system implementation
//! - Can be enabled via feature flags

#[cfg(feature = "gcp")]
pub mod gcp;

/// Interfaces for ingesters/subscribers
pub mod interfaces;

pub(crate) mod tracking;

/// Nats implementation
#[cfg(feature = "nats")]
pub mod nats;
