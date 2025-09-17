//! Crate for interacting with the Amplifier API.
//! Intended to be used by Relayers supporting the Axelar infrastructure

pub use self::amplifier_open_api::{Client as AmplifierApiClient, *};

mod config;
pub mod error;
pub mod identity;
mod util;

mod opentelemetry;

pub use config::Config;

pub mod amplifier_open_api {
    #![allow(
        clippy::all,
        clippy::pedantic,
        clippy::nursery,
        clippy::restriction,
        clippy::cargo,
        warnings,
        missing_docs,
        rustdoc::all,
        clippy::multiple_inherent_impl,
        clippy::std_instead_of_core,
        unused_unsafe,
        clippy::too_many_arguments,
        clippy::missing_safety_doc,
        clippy::missing_errors_doc,
        clippy::missing_panics_doc,
        clippy::must_use_candidate,
        clippy::doc_markdown,
        clippy::missing_const_for_fn,
        clippy::unnecessary_wraps,
        reason = "generated code"
    )]
    // Disable all compiler warnings and errors for generated code
    #![allow(
        dead_code,
        unused_imports,
        unused_variables,
        unused_mut,
        non_camel_case_types,
        non_snake_case,
        non_upper_case_globals,
        unreachable_code,
        unused_allocation,
        trivial_casts,
        trivial_numeric_casts,
        reason = "generated code"
    )]

    //! This module contains the generated OpenAPI client code for the Amplifier API,
    //! along with custom client hooks for tracing and OpenTelemetry context injection.

    // The generated code is using `elided_named_lifetimes` (#[allow(elided_named_lifetimes)])
    // which is renamed to `mismatched_lifetime_syntaxes` in newer Rust versions.
    // This should be removed once the generated code no longer uses this lint.

    // #![allow(clippy::all, reason = "generated code")]

    include!(concat!(env!("OUT_DIR"), "/amplifier_api_client.rs"));

    impl ClientHooks<()> for Client {
        async fn pre<E>(
            &self,
            request: &mut reqwest::Request,
            info: &OperationInfo,
        ) -> std::result::Result<(), Error<E>> {
            // Create a span for this API operation
            let span = tracing::info_span!(
                "amplifier_api_request",
                method = %request.method(),
                url = %request.url(),
                operation = info.operation_id
            );

            // Enter the span and inject OpenTelemetry context into the request
            let _guard = span.enter();
            crate::opentelemetry::inject_opentelemetry_context_into_request(request);

            Ok(())
        }

        async fn post<E>(
            &self,
            result: &reqwest::Result<reqwest::Response>,
            info: &OperationInfo,
        ) -> std::result::Result<(), Error<E>> {
            // Log the response status and handle any errors
            match result {
                Ok(response) => {
                    tracing::info!(
                        status = %response.status(),
                        operation = info.operation_id,
                        "API request completed"
                    );
                }
                Err(error) => {
                    tracing::error!(
                        error = %error,
                        operation = info.operation_id,
                        "API request failed"
                    );
                }
            }
            Ok(())
        }
    }
}
