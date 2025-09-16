//! Crate for interacting with the Amplifier API.
//! Intended to be used by Relayers supporting the Axelar infrastructure

/// TODO: add docs
pub mod amplifier_open_api {
    #![allow(missing_docs)]

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
            crate::inject_opentelemetry_context_into_request(request);

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

pub use self::amplifier_open_api::{Client as AmplifierApiClient, *};

mod config;
pub mod util;
use std::str::FromStr;
pub mod identity;
pub mod error;

pub use config::Config;
use reqwest::Request;
use reqwest::header::{HeaderName, HeaderValue};

/// Injects the given OpenTelemetry Context into a reqwest::Request headers to allow propagation
/// downstream.
pub fn inject_opentelemetry_context_into_request(request: &mut reqwest::Request) {
    opentelemetry::global::get_text_map_propagator(|injector| {
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        let context = tracing::Span::current().context();
        injector.inject_context(&context, &mut RequestCarrier::new(request))
    });
}

/// Injector used via opentelemetry propagator to tell the extractor how to insert the "traceparent"
/// header value This will allow the propagator to inject opentelemetry context into a standard data
/// structure. Will basically insert a "traceparent" string value
/// "{version}-{trace_id}-{span_id}-{trace-flags}" of the spans context into the headers.
/// Listeners can then re-hydrate the context to add additional spans to the same trace.
struct RequestCarrier<'a> {
    request: &'a mut Request,
}

impl<'a> RequestCarrier<'a> {
    fn new(request: &'a mut Request) -> Self {
        RequestCarrier { request }
    }

    fn set_inner(&mut self, key: &str, value: String) {
        let header_name = HeaderName::from_str(key).expect("Must be header name");
        let header_value = HeaderValue::from_str(&value).expect("Must be a header value");
        self.request.headers_mut().insert(header_name, header_value);
    }
}

impl<'a> opentelemetry::propagation::Injector for RequestCarrier<'a> {
    fn set(&mut self, key: &str, value: String) {
        self.set_inner(key, value)
    }
}
