# Amplifier Subscriber

The Amplifier Subscriber is a component that fetches tasks from the Amplifier API and publishes them to message queues for processing by blockchain-specific ingester. It enables the distribution of cross-chain tasks to the appropriate blockchain handlers.

## Overview

The subscriber is designed to:
- Poll the Amplifier API for new tasks
- Fetch tasks in configurable batches
- Publish tasks to message queues (NATS or GCP Pub/Sub)
- Maintain reliable task distribution with proper error handling

## Architecture

The subscriber acts as a task distributor in the Axelar Amplifier system:

```
Amplifier API → Subscriber → Tasks → Message Queue → Blockchain Ingester
```

## Usage

### For Blockchain Integration

**Important**: When integrating a new blockchain, you don't need to modify the amplifier-subscriber crate. Instead:

1. Implement a blockchain-specific ingester that:
   - Consumes tasks from the message queue pushed by amplifier subscriber
   - Processes tasks specific to your blockchain
   - Executes transactions or operations on the target blockchain

2. The amplifier-subscriber will automatically:
   - Fetch tasks from the Amplifier API
   - Publish them to the configured message queue
   - Handle authentication and error scenarios

### Configuration

**Important**: Configuration files are completely optional! All settings can be configured using environment variables with the `RELAYER_` prefix.

#### Environment Variables (Recommended)

Every configuration option can be set via environment variables:

```bash
# Basic configuration
export RELAYER_LIMIT_PER_REQUEST=50
export RELAYER_HEALTH_CHECK_PORT=8080
export RELAYER_TICKRATE="5s"
export RELAYER_MAX_ERRORS=10

# Amplifier component
export RELAYER_AMPLIFIER_URL="https://amplifier-api.example.com"
export RELAYER_AMPLIFIER_CHAIN="ethereum"

# For NATS backend
export RELAYER_NATS_URLS="nats://localhost:4222"
export RELAYER_NATS_STREAM_NAME="amplifier_tasks"
export RELAYER_NATS_STREAM_SUBJECT="tasks.*"
export RELAYER_NATS_STREAM_DESCRIPTION="Amplifier task stream"

# For GCP backend
export RELAYER_GCP_PROJECT_ID="your-project-id"
export RELAYER_GCP_TOPIC_ID="amplifier-tasks"
```

#### Configuration File (Optional)

Alternatively, you can use a configuration file with the following sections:

```toml
# General subscriber configuration
limit_per_request = 50

[amplifier]
url = "https://amplifier-api.example.com"
chain = "ethereum"

[amplifier.identity]
# TLS identity configuration - see main README for details
# Option 1: Direct certificate (development)
# identity = "..."
# Option 2: Use with GCP KMS (production) - configure [gcp.kms] section

# For NATS
[nats]
urls = ["nats://localhost:4222"]
stream_name = "amplifier_tasks"
stream_subject = "tasks.*"
stream_description = "Amplifier task stream"

# For GCP Pub/Sub
[gcp]
project_id = "your-project-id"
topic_id = "amplifier-tasks"
```

### Running

The subscriber supports two message queue backends that are mutually exclusive:

```bash
# Option 1: Using environment variables only (no config file)
export RELAYER_HEALTH_CHECK_PORT=8080
export RELAYER_AMPLIFIER_CHAIN="ethereum"
# ... set other required variables

# With GCP Pub/Sub (default)
cargo run --bin amplifier-subscriber

# With NATS (requires disabling default features)
cargo run --bin amplifier-subscriber --no-default-features --features nats

# Option 2: Using a configuration file
# With GCP (default)
cargo run --bin amplifier-subscriber -- --config config.toml

# With NATS
cargo run --bin amplifier-subscriber --no-default-features --features nats -- --config config.toml

# Option 3: Mix both (env vars override config file values)
export RELAYER_AMPLIFIER_CHAIN="polygon"  # overrides chain in config
cargo run --bin amplifier-subscriber --no-default-features --features nats -- --config config.toml
```

*supervisor* feature should be enabled if you want add it to development [supervisor](../supervisor) binary to start all relayer components as a single binary

## Features

- **Batch Processing**: Configurable batch size for task fetching
- **Multiple Queue Support**: NATS and GCP Pub/Sub implementations
- **TLS Authentication**: Secure communication with Amplifier API
- **Reliable Publishing**: Ensures tasks are successfully published to queues
- **Observability**: Integrated metrics and tracing
- **BigInt Precision**: Supports forwarding BigInt features to amplifier-api for blockchain-specific numeric precision (see below)

### Blockchain-Specific BigInt Configuration

The subscriber forwards BigInt features to `amplifier-api`. See the [main README](../../README.md#bigint-precision-for-token-amounts) for details.

```bash
# Solana with GCP
cargo build --bin amplifier-subscriber --features bigint-u64

# Solana with NATS
cargo build --bin amplifier-subscriber --no-default-features --features "nats,bigint-u64"
```

## Development

### Adding Support for New Message Queues

To add support for a new message queue system:

1. Implement the `Publisher` trait from the infrastructure crate
2. Add a new feature flag in `Cargo.toml`
3. Create a new component module similar to `components/nats.rs`
4. Update the main binary to support the new feature

### Testing

Since GCP and NATS features are mutually exclusive, test each backend separately:

```bash
# Test with GCP (default)
cargo test

# Test with NATS
cargo test --no-default-features --features nats
```

## Performance Considerations

- **Polling Interval**: The subscriber polls continuously; ensure your `limit_per_request` is appropriate for your load
- **Queue Capacity**: Ensure your message queue can handle the task throughput
- **Network Latency**: Consider network latency when configuring batch sizes

## Related Components

- [amplifier-ingester](../amplifier-ingester/README.md): Consumes events and forwards to Amplifier API
- [infrastructure](../infrastructure/README.md): Shared infrastructure components
