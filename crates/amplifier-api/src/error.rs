#![expect(missing_docs, reason = "the error macro already is descriptive enough")]

/// Error variants for the Amplifier API
#[allow(clippy::module_name_repetitions, reason = "Descriptive name")]
#[derive(thiserror::Error, Debug)]
pub enum AmplifierApiError {
    #[error("Reqwest error {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Url parse error {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("JSON error {0}")]
    Json(#[from] simd_json::Error),
}
