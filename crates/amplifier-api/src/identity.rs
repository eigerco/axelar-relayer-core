//! TODO: add documentation

use reqwest::header;

use crate::error::AmplifierApiError;

/// Type of TLS
pub enum TlsType {
    /// Embedded pem certificate
    Certificate(Box<Identity>),
    /// Custom tls, like use of HSM
    CustomProvider(Box<rustls::ClientConfig>),
}

/// Helpers for deserializing `.pem` encoded certificates
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

/// Create a new authenticated `reqwest::Client` for amplifier-api.
///
/// It requires a `.pem` encoded certificate to be attached to the client. The certificate is
/// issued by Axelar.
///
/// # Errors
///
/// This function will return an error if the reqwest client cannot be constructed
pub fn authenticated_client(tls_type: TlsType) -> Result<reqwest::Client, AmplifierApiError> {
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

    Ok(client)
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
    fn deserialize_identity() {
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
