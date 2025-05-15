//! Simple example that allows you to validate if your specific Identity is able to reach the
//! Amplifier API by calling the /healthcheck endpoint

use std::path::PathBuf;

use amplifier_api::identity::Identity;
use amplifier_api::requests::HealthCheck;

#[tokio::main]
async fn main() {
    let identity: PathBuf = std::env::var("IDENTITY_PATH")
        .expect("identity path not set")
        .parse()
        .unwrap();
    let amplifier_api_url = "https://amplifier-devnet-amplifier.devnet.axelar.dev/"
        .parse()
        .expect("invalid url");
    let identity = std::fs::read(identity).expect("cannot read identity path");
    let identity =
        Identity::new(reqwest::Identity::from_pem(&identity).expect("invalid identity file"));

    let client = amplifier_api::AmplifierApiClient::new(
        amplifier_api_url,
        amplifier_api::TlsType::Certificate(Box::new(identity)),
    )
    .expect("could not construct client");
    client
        .build_request(&HealthCheck)
        .expect("works")
        .execute()
        .await
        .expect("works")
        .json_err()
        .await
        .expect("works")
        .expect("works");
}
