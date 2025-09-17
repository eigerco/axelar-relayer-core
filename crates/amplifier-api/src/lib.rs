//! Crate for interacting with the Amplifier API.
//! Intended to be used by Relayers supporting the Axelar infrastructure


pub use self::amplifier_open_api::{Client as AmplifierApiClient, *};

mod config;
mod util;
pub mod identity;
pub mod error;

mod opentelemetry;

pub use config::Config;

pub mod amplifier_open_api {
    //! This module contains the generated OpenAPI client code for the Amplifier API,
    //! along with custom client hooks for tracing and OpenTelemetry context injection.
    #![allow(missing_docs)]

    // The generated code is using `elided_named_lifetimes` (#[allow(elided_named_lifetimes)])
    // which is renamed to ``mismatched_lifetime_syntaxes` in newer Rust versions.
    // This should be removed once the generated code no longer uses this lint.
    #![allow(renamed_and_removed_lints)]

    include!(concat!(env!("OUT_DIR"), "/codegen.rs"));

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
