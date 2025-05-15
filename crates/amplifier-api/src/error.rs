#![expect(missing_docs, reason = "the error macro already is descriptive enough")]

/// Error variants for the Amplifier API
#[derive(thiserror::Error, Debug)]
pub enum AmplifierApiError {
    #[error("Reqwest error {0}")]
    ReqwestWithMiddleware(#[from] reqwest_middleware::Error),
    #[error("Reqwest error {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Url parse error {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("JSON error {0}")]
    Json(#[from] simd_json::Error),
}
