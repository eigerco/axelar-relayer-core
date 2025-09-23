//! Simple example that allows you to validate if your specific Identity is able to reach the
//! Amplifier API by calling the /healthcheck endpoint

use std::path::PathBuf;

use amplifier_api::Client;
use reqwest::Identity;
use tracing::info;

const AMPLIFIER_API_URL: &str = "https://amplifier-devnet-amplifier.devnet.axelar.dev";

async fn read_identity() -> eyre::Result<Identity> {
    // Read the identity from ENV variable
    // the user is responsible to set it
    let identity_path: PathBuf = std::env::var("IDENTITY_PATH")?.parse()?;
    let identity = tokio::fs::read(identity_path).await?;

    Ok(Identity::from_pem(&identity)?)
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let identity = read_identity().await?;

    let client_builder = reqwest::ClientBuilder::new().identity(identity);
    let client = client_builder.build()?;

    let client = Client::new_with_client(AMPLIFIER_API_URL, client);

    let response = client.health_check().await?;
    info!("Response header: {:?}", response.headers());
    info!("Response status: {:?}", response.into_inner());

    Ok(())
}
