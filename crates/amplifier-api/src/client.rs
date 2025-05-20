pub mod requests;
use core::marker::PhantomData;

use requests::AmplifierApiRequest;
use reqwest::header;
use reqwest_middleware::{ClientBuilder, Extension};
use reqwest_tracing::{OtelName, TracingMiddleware};
use tracing::instrument;

use crate::error::AmplifierApiError;

/// Client for the Amplifier API
#[derive(Clone, Debug)]
pub struct AmplifierApiClient {
    inner: reqwest_middleware::ClientWithMiddleware,
    url: url::Url,
}

/// Type of TLS
pub enum TlsType {
    /// Embedded pem certificate
    Certificate(Box<identity::Identity>),
    /// Custom tls, like use of HSM
    CustomProvider(Box<rustls::ClientConfig>),
}

impl AmplifierApiClient {
    /// Create a new `AmplifierApiClient`.
    ///
    /// It requires a `.pem` encoded certificate to be attached to the client. The certificate is
    /// issued by Axelar.
    ///
    /// # Errors
    ///
    /// This function will return an error if the underlying reqwest client cannot be constructed
    #[tracing::instrument(skip(tls_type), name = "creating amplifier api client")]
    pub fn new(url: url::Url, tls_type: TlsType) -> Result<Self, AmplifierApiError> {
        let authenticated_client = authenticated_client(tls_type)?;
        Ok(Self {
            url,
            inner: authenticated_client,
        })
    }

    /// Send a request to Axelars Amplifier API
    #[instrument(name = "build_request", skip(self, request))]
    pub fn build_request<T>(
        &self,
        request: &T,
    ) -> Result<AmplifierRequest<T::Res, T::Error>, AmplifierApiError>
    where
        T: AmplifierApiRequest + core::fmt::Debug,
    {
        let endpoint = request.path(&self.url)?;
        let method = T::METHOD;
        let client = self.inner.clone();
        let payload = simd_json::to_vec(&request.payload())?;

        let json = String::from_utf8_lossy(payload.as_slice());
        tracing::trace!(request_body = %json, "Request JSON");

        let reqwest_req = client.request(method, endpoint.as_str()).body(payload);

        Ok(AmplifierRequest {
            request: reqwest_req,
            result: PhantomData,
            err: PhantomData,
        })
    }
}

/// Encalpsulated HTTP request for the Amplifier API
#[derive(Debug)]
pub struct AmplifierRequest<T, E> {
    request: reqwest_middleware::RequestBuilder,
    result: PhantomData<T>,
    err: PhantomData<E>,
}

impl<T, E> AmplifierRequest<T, E> {
    /// execute an Amplifier API request
    #[instrument(name = "execute_request", skip(self))]
    pub async fn execute(self) -> Result<AmplifierResponse<T, E>, AmplifierApiError> {
        let (client, request) = self.request.build_split();
        let request = request?;

        // Capture the current span
        let span = tracing::Span::current();
        span.record("method", request.method().as_str());
        span.record("url", request.url().as_str());

        // execute the request
        let response = client.execute(request).await?;

        Ok(AmplifierResponse {
            response,
            result: PhantomData,
            err: PhantomData,
            span,
        })
    }
}

/// The raw response of the Amplifier API request
pub struct AmplifierResponse<T, E> {
    response: reqwest::Response,
    result: PhantomData<T>,
    err: PhantomData<E>,
    // this span carries the context of the `AmplifierRequest`
    span: tracing::Span,
}

impl<T, E> AmplifierResponse<T, E> {
    /// Only check if the returtned HTTP response is of error type; don't parse the data
    ///
    /// Useful when you don't care about the actual response besides if it was an error.
    #[instrument(name = "response_ok", skip(self), err, parent = &self.span)]
    pub fn ok(self) -> Result<(), AmplifierApiError> {
        self.response.error_for_status()?;
        Ok(())
    }

    /// Check if the returned HTTP result is an error;
    /// Only parse the error type if we received an error.
    ///
    /// Useful when you don't care about the actual response besides if it was an error.
    #[instrument(name = "parse_response_json_err", skip(self), err, parent = &self.span)]
    pub async fn json_err(self) -> Result<Result<(), E>, AmplifierApiError>
    where
        E: serde::de::DeserializeOwned,
    {
        let status = self.response.status();
        if status.is_success() {
            Ok(Ok(()))
        } else {
            let bytes = self.response.bytes().await?.to_vec();
            let res = parse_amplifier_error::<E>(bytes, status)?;
            Ok(Err(res))
        }
    }

    /// Parse the response json
    #[instrument(name = "parse_response_json", skip(self), err, parent = &self.span)]
    pub async fn json(self) -> Result<Result<T, E>, AmplifierApiError>
    where
        T: serde::de::DeserializeOwned,
        E: serde::de::DeserializeOwned,
    {
        let status = self.response.status();
        let mut bytes = self.response.bytes().await?.to_vec();
        if status.is_success() {
            let json = String::from_utf8_lossy(bytes.as_ref());
            tracing::trace!(response_body = %json, "Response JSON");

            let result = simd_json::from_slice::<T>(bytes.as_mut())?;
            Ok(Ok(result))
        } else {
            let res = parse_amplifier_error::<E>(bytes, status)?;
            Ok(Err(res))
        }
    }
}

fn parse_amplifier_error<E>(
    mut bytes: Vec<u8>,
    status: reqwest::StatusCode,
) -> Result<E, AmplifierApiError>
where
    E: serde::de::DeserializeOwned,
{
    let json = String::from_utf8_lossy(bytes.as_ref());
    tracing::error!(
        status = %status,
        body = %json,
        "Failed to execute request"
    );

    let error = simd_json::from_slice::<E>(bytes.as_mut())?;
    Ok(error)
}
fn authenticated_client(
    tls_type: TlsType,
) -> Result<reqwest_middleware::ClientWithMiddleware, AmplifierApiError> {
    const KEEP_ALIVE_INTERVAL: core::time::Duration = core::time::Duration::from_secs(15);
    let mut headers = header::HeaderMap::new();
    headers.insert(
        "Accept",
        header::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        "Accept-Encoding",
        header::HeaderValue::from_static("gzip, deflate"),
    );
    headers.insert(
        "Content-Type",
        header::HeaderValue::from_static("application/json"),
    );

    let client = reqwest::Client::builder().use_rustls_tls();

    let client = match tls_type {
        TlsType::Certificate(identity) => client.identity(identity.0.expose_secret().clone()),
        TlsType::CustomProvider(client_config) => client.use_preconfigured_tls(*client_config),
    };

    let client = client
        .http2_keep_alive_interval(KEEP_ALIVE_INTERVAL)
        .http2_keep_alive_while_idle(true)
        .default_headers(headers)
        .build()?;
    let client = ClientBuilder::new(client)
        .with_init(Extension(OtelName("amplifier-api-client".into())))
        .with(TracingMiddleware::default())
        .build();
    Ok(client)
}

/// helpers for deserializing `.pem` encoded certificates
pub mod identity {
    /// Represents a `.pem` encoded certificate
    #[derive(Debug, Clone, serde::Deserialize)]
    pub struct Identity(
        #[serde(deserialize_with = "serde_utils::deserialize_identity")]
        pub  redact::Secret<reqwest::Identity>,
    );

    impl PartialEq for Identity {
        fn eq(&self, _other: &Self) -> bool {
            // Note: we don't have any access to reqwest::Identity internal fields.
            // So we'll just assume that "if Identity is valid, then all of them are equal".
            // And "validity" is defined by the ability to parse it.
            true
        }
    }

    impl Identity {
        /// Creates a new [`Identity`].
        #[must_use]
        pub const fn new(identity: reqwest::Identity) -> Self {
            Self(redact::Secret::new(identity))
        }

        /// Creates a new [`Identity`].
        ///
        /// # Errors
        ///
        /// When the pem file is invalid
        pub fn new_from_pem_bytes(identity: &[u8]) -> reqwest::Result<Self> {
            let identity = reqwest::Identity::from_pem(identity)?;
            Ok(Self::new(identity))
        }
    }

    mod serde_utils {
        use serde::{Deserialize as _, Deserializer};

        pub(crate) fn deserialize_identity<'de, D>(
            deserializer: D,
        ) -> Result<redact::Secret<reqwest::Identity>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let raw_string = String::deserialize(deserializer)?;
            let identity = reqwest::Identity::from_pem(raw_string.as_bytes())
                .inspect_err(|err| {
                    tracing::error!(?err, "cannot parse identity");
                })
                .map_err(serde::de::Error::custom)?;
            Ok(redact::Secret::new(identity))
        }
    }

    #[cfg(test)]
    mod tests {
        use serde::Deserialize;
        use simd_json;

        use super::*;

        fn identity_fixture() -> String {
            include_str!("../fixtures/example_cert.pem").to_owned()
        }

        #[test]
        fn test_deserialize_identity() {
            #[derive(Debug, Deserialize)]
            struct DesiredOutput {
                #[expect(dead_code, reason = "we don't care about reading the data in the test")]
                identity: Identity,
            }

            let identity_str = identity_fixture();

            let mut data = simd_json::to_string(&simd_json::json!({ "identity": identity_str }))
                .unwrap()
                .into_bytes();

            let _output: DesiredOutput =
                simd_json::from_slice(data.as_mut()).expect("Failed to deserialize identity");
        }
    }
}
