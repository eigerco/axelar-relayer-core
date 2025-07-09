# Axelar Relayer core libraries

This repo provides building blocks for Axelar<>Blockchain Relayer

## Relayer Architecture Overview

```mermaid
graph TD
    subgraph "Relayer System"
        subgraph "Amplifier to Blockchain Flow"
            amp_sub[Amplifier Subscriber]
            tasks_queue[Amplifier Tasks Queue]
            bcx_ing[Blockchain Ingester]
        end

        subgraph "Blockchain to Amplifier Flow"
            bcx_sub[Blockchain Subscriber]
            events_queue[Amplifier Events Queue]
            amp_ing[Amplifier Ingester]
        end
    end

    %% External systems
    amp_api[/"Amplifier REST API"\]
    bcx["Blockchain"]

    %% Amplifier to Blockchain flow
    amp_api -->|amplifier tasks| amp_sub
    amp_sub -->|pushes amplifier tasks| tasks_queue
    tasks_queue -->|consumes amplifier tasks| bcx_ing
    bcx_ing -->|transforms & includes amplifier tasks| bcx

    %% Blockchain to Amplifier flow
    bcx -->|blockchain events| bcx_sub
    bcx_sub -->|transforms to amplifier events| events_queue
    events_queue -->|consumes amplifier events| amp_ing
    amp_ing -->|sends amplifier events| amp_api

    %% Styling
    classDef component fill:#d9d2e9,stroke:#674ea7,stroke-width:2px;
    classDef queue fill:#ffe6cc,stroke:#d79b00,stroke-width:2px;
    classDef api fill:#d5e8d4,stroke:#82b366,stroke-width:3px,color:#000,font-weight:bold,font-size:18px;
    classDef blockchain fill:#dae8fc,stroke:#6c8ebf,stroke-width:3px,color:#000,font-weight:bold,font-size:18px;

    class amp_sub,bcx_ing,bcx_sub,amp_ing component;
    class tasks_queue,events_queue queue;
    class amp_api api;
    class bcx blockchain;

    %% Styling
    classDef component stroke:#333,stroke-width:2px;
    classDef queue fill:#ff9,stroke:#333,stroke-width:2px;
    classDef external fill:#bbf,stroke:#333,stroke-width:1px;

    class amp_sub,bcx_ing,bcx_sub,amp_ing component;
    class tasks_queue,events_queue queue;
    class amp_api,bcx external;
```

## Bidirectional Flow Architecture

The relayer establishes bidirectional communication between an Amplifier API and a blockchain. It consists of these main flows:

### Amplifier to Blockchain Flow

- **Amplifier Subscriber**: Subscribes to the Amplifier REST API and receives amplifier tasks then sends them to queue
- **Amplifier Tasks Queue**: Stores amplifier tasks
- **Blockchain Ingester**: Consumes tasks from the queue, transforms them to a compatible format, and includes them in the blockchain

### Blockchain to Amplifier Flow

- **Blockchain Subscriber**: Subscribes to blockchain events, transforms them into amplifier events and sends to queue
- **Amplifier Events Queue**: Stores amplifier events
- **Amplifier Ingester**: Consumes events from the queue and sends them to the Amplifier API

## Internal Relayer Architecture

The relayer is designed as 4 components: 2 ingesters & 2 subscribers - 1 for each chain (Axelar/amplifier API and the connecting chain)

**Important for Blockchain Integration**: The [amplifier-ingester](./crates/amplifier-ingester/README.md) and [amplifier-subscriber](./crates/amplifier-subscriber/README.md) components are generic and don't need modification. To integrate a new blockchain, implement only the blockchain-specific ingester and subscriber that interact with your blockchain and the message queues.

**Implementation Reference**: When implementing blockchain-specific ingester and subscriber components, refer to the [amplifier-ingester](./crates/amplifier-ingester/) and [amplifier-subscriber](./crates/amplifier-subscriber/) implementations as examples of how to structure your components, handle message queues, implement health checks, and integrate with the infrastructure layer.

### Blockchain-Specific Configuration

#### BigInt Precision for Token Amounts

The `amplifier-api` crate provides compile-time configuration to optimize numeric precision for your specific blockchain. See the [amplifier-api README](./crates/amplifier-api/README.md#bigint-type-configuration) for details.

#### TLS Certificate Configuration

The relayer requires TLS certificates to authenticate with the Amplifier API. You have two options:

##### Option 1: Direct Certificate (Development/Testing)

Store the certificate and private key directly in the configuration:

```toml
[amplifier]
# Certificate + private key in PEM format
identity = '''
-----BEGIN CERTIFICATE-----
...
-----END CERTIFICATE-----
-----BEGIN PRIVATE KEY-----
...
-----END PRIVATE KEY-----
'''
```

**⚠️ Warning**: This method stores the private key in plaintext and should only be used for development.

##### Option 2: Google Cloud KMS (Production)

For production deployments, use Google Cloud KMS to secure your private keys:

```toml
[amplifier]
# Only the public certificate is stored locally
tls_public_certificate = '''
-----BEGIN CERTIFICATE-----
...
-----END CERTIFICATE-----
'''

[gcp.kms]
project_id = "your-gcp-project"
location = "global"
keyring = "amplifier_api_keyring"
cryptokey = "amplifier_api_signing_key"
```

With KMS:
- Private keys never leave Google Cloud
- All signing operations happen within KMS
- Keys can be rotated without changing the relayer configuration
- Access is controlled through GCP IAM

##### Environment Variables

TLS configuration can also be provided via environment variables:

```bash
# For direct certificate (base64 encoded)
export RELAYER_AMPLIFIER_IDENTITY="<base64-encoded-pem>"

# For KMS with public certificate
export RELAYER_AMPLIFIER_TLS_PUBLIC_CERTIFICATE="<base64-encoded-cert>"
export RELAYER_GCP_KMS_PROJECT_ID="your-project"
export RELAYER_GCP_KMS_LOCATION="global"
export RELAYER_GCP_KMS_KEYRING="amplifier_api_keyring"
export RELAYER_GCP_KMS_CRYPTOKEY="amplifier_api_signing_key"
```

### Core Libraries

- **[amplifier-api](./crates/amplifier-api/README.md)**: Rust client for the Axelar Amplifier API with configurable BigInt precision for different blockchains
- **[bin-util](./crates/bin-util/README.md)**: Common binary utilities for all relayer components including configuration management, health checks, telemetry, logging, and metrics
- **[infrastructure](./crates/infrastructure/README.md)**: Storage bus implementations providing abstraction for message queuing (GCP Pub/Sub, NATS) and key-value storage
- **[terraform](./terraform/README.md)**: Infrastructure as Code for provisioning GCP resources (Pub/Sub, KMS, Memorystore, IAM, K8s)

#### Components

1. **Supervisor**(optional):

   - Runs on its own dedicated thread with a Tokio runtime
   - Spawns and monitors worker threads only for the components selected via CLI
   - Detects crashes and automatically restarts failed components
   - Provides a graceful shutdown period when termination is requested
   - Is fully optional

2. **Termination Handling**:

   - A dedicated thread listens for Ctrl+C signals
   - Uses an CancellationToken as a shared termination flag
   - When Ctrl+C is received, the CancellationToken is set to true
   - All components, including optional supervisor, watch this CancellationToken
   - Upon termination, components have graceful period to finish current work

3. **Worker Components**:

   - Each component (Amplifier Subscriber, Blockchain Ingester, Blockchain Subscriber, Amplifier Ingester) runs isolated
   - Components check the termination flag regularly and shut down when needed
   - Isolation ensures a failure in one component doesn't affect others
   - It's up to the implementation to run all components at once (and isolate via supervisor) or as separate binaries

4. **Queue Abstraction**:
   - Queue is push based
   - Currently implemented using [NATS](https://nats.io/) and [GCP](https://cloud.google.com/pubsub?hl=en)
   - Abstraction allows for easy replacement with different queue technologies
   - Components interact with queues only through trait interfaces, maintaining loose coupling
   - Supports horizontal scaling by allowing multiple instances to consume from the same queue

The supervisor is optional, and each component can be started as a separate binary.

## Development Environment

### Using Nix

This project includes a Nix flake that provides a development shell with all necessary tooling. This ensures consistent development environments across different machines.

To use the Nix development environment:

1. **Install Nix** (if not already installed):
   ```bash
   # On macOS or Linux
   curl -L https://nixos.org/nix/install | sh
   ```

2. **Enable flakes** (if not already enabled):
   ```bash
   echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf
   ```

3. **Enter the development shell**:
   ```bash
   nix develop
   ```

The development shell includes:
- **Rust toolchain**: stable Rust with cargo, clippy, rustfmt, and rust-analyzer
- **NATS tools**: natscli and nats-server for local testing
- **Google Cloud SDK**: for working with GCP services
- **OpenTofu**: for managing Terraform infrastructure
- **cargo-make**: for task automation
- **Development tools**: nixd, pkg-config, vscode-lldb

All tools are automatically available in your PATH when you enter the Nix shell.

## Running the Components

### Configuration

Before running the components, you need to create a configuration file. An example configuration file is provided in `config-example.toml`. You can copy this file and modify it according to your needs:

```bash
cp config-example.toml relayer-config.toml
```

Edit the `relayer-config.toml` file to configure:

- Amplifier API configuration (chain name, URL, TLS certificates)
- Backend configuration (NATS or GCP Pub/Sub)
- TLS authentication (direct certificate or GCP KMS)
- Tickrate for processing events
- Health check endpoints

Config file is fully optional and app could be configured via env vars with prefix `RELAYER_`


### Running Amplifier Ingester

```bash
# Build the ingester
cargo build --bin amplifier-ingester

# Run with default config path (looks for relayer-config.toml in current directory)
./target/debug/amplifier-ingester

# Or specify a custom config path
./target/debug/amplifier-ingester --config /path/to/your/config.toml
```

### Running Amplifier Subscriber

```bash
# Build the subscriber
cargo build --bin amplifier-subscriber

# Option 1: Run with environment variables only (no config file needed)
export RELAYER_HEALTH_CHECK_PORT=8080
export RELAYER_CHAIN="ethereum"
export RELAYER_TICKRATE="5s"
# ... set other required variables
./target/debug/amplifier-subscriber

# Option 2: Run with default config path (looks for relayer-config.toml in current directory)
./target/debug/amplifier-subscriber

# Option 3: Specify a custom config path
./target/debug/amplifier-subscriber --config /path/to/your/config.toml
```

### Running with Specific Backend

By default, both components are built with GCP support as the message queue backend. To use NATS instead, compile with the `nats` feature:

```bash
# Build with NATS backend support (requires disabling default GCP features)
cargo build --bin amplifier-ingester --no-default-features --features nats
cargo build --bin amplifier-subscriber --no-default-features --features nats
```

### Health Checks

Once running, you can check the health of the components by sending an HTTP request to the configured health check endpoints:

```bash
# Check health of a component (assuming default port 8080)
curl http://localhost:8080/healthz

# Check readiness of a component
curl http://localhost:8080/readyz
```

## Docker Deployment

The Axelar Relayer Core components can be containerized using Docker. Dockerfiles are provided for both the amplifier-ingester and amplifier-subscriber components.

### Building Docker Images

To build the Docker images:

```bash
# For the ingester with default backend (GCP)
docker build -t axelar-amplifier-ingester -f crates/amplifier-ingester/Dockerfile .

# For the subscriber with default backend (GCP)
docker build -t axelar-amplifier-subscriber -f crates/amplifier-subscriber/Dockerfile .
```

### Using NATS Instead of GCP

Since GCP is the default backend, you can build with NATS support instead by using build arguments:

```bash
# Build with NATS backend
docker build -t axelar-amplifier-ingester-nats \
  --build-arg FEATURES=nats \
  -f crates/amplifier-ingester/Dockerfile .

docker build -t axelar-amplifier-subscriber-nats \
  --build-arg FEATURES=nats \
  -f crates/amplifier-subscriber/Dockerfile .
```

### Running Docker Containers

**Option 1: Using environment variables only (recommended for containers)**

```bash
# Run the ingester with environment variables only
docker run -p 8080:8080 \
  -e RELAYER_HEALTH_CHECK_PORT=8080 \
  -e RELAYER_TICKRATE="5s" \
  -e RELAYER_CHAIN="ethereum" \
  -e RELAYER_AMPLIFIER_URL="https://api.amplifier.axelar.network" \
  axelar-amplifier-ingester

# Run the subscriber with environment variables only
docker run -p 8081:8080 \
  -e RELAYER_HEALTH_CHECK_PORT=8080 \
  -e RELAYER_CHAIN="ethereum" \
  -e RELAYER_TICKRATE="5s" \
  axelar-amplifier-subscriber
```

**Option 2: Using a configuration file**

```bash
# Run the ingester
docker run -p 8080:8080 -v /path/to/your/relayer-config.toml:/app/relayer-config.toml axelar-amplifier-ingester

# Run the subscriber
docker run -p 8081:8080 -v /path/to/your/relayer-config.toml:/app/relayer-config.toml axelar-amplifier-subscriber
```

By default, the health check endpoints will be available at:

- http://localhost:8080/healthz and http://localhost:8080/readyz for the ingester
- http://localhost:8081/healthz and http://localhost:8081/readyz for the subscriber (mapped to a different host port to avoid conflicts)

### Environment Variables Override

When using a config file, you can still override specific configuration options using environment variables:

```bash
docker run -p 8080:8080 \
  -v /path/to/your/relayer-config.toml:/app/relayer-config.toml \
  -e "RELAYER_TICKRATE=10s" \
  -e "RELAYER_NATS_CONNECTION_URL=nats://nats-server:4222" \
  axelar-amplifier-ingester
```

### Docker Compose

The project includes dedicated Docker Compose files for both GCP and NATS backends:

- `docker-compose.gcp.yaml` - For running with GCP (default) backend
- `docker-compose.nats.yaml` - For running with NATS backend

#### Using the GCP Backend

To run the components with GCP as the backend:

```bash
# First ensure you have a configuration file
cp relayer-config-example.toml relayer-config.toml

# Edit the relayer-config.toml to configure your GCP settings
# Make sure your GCP configuration is properly set in the [gcp] section

# Start all services using the GCP compose file
docker compose -f docker-compose.gcp.yaml up -d

# Check the status of the services
docker compose -f docker-compose.gcp.yaml ps

# View the logs
docker compose -f docker-compose.gcp.yaml logs -f
```

For GCP, you may need to provide authentication credentials by uncommenting the relevant volume mounts and environment variables in the Docker Compose file.

#### Using the NATS Backend

To run the components with NATS as the backend:

```bash
# First ensure you have a configuration file
cp relayer-config-example.toml relayer-config.toml

# Edit the relayer-config.toml to configure your NATS settings
# Make sure your NATS configuration is properly set in the [nats] section

# Start all services using the NATS compose file
docker compose -f docker-compose.nats.yaml up -d

# Check the status of the services
docker compose -f docker-compose.nats.yaml ps

# View the logs
docker compose -f docker-compose.nats.yaml logs -f
```

The NATS server exposes:

- Port 4222 for client connections
- Port 8222 for HTTP monitoring

The ingester and subscriber configuration automatically connects to the NATS server using the internal Docker network.

#### Stopping Services

To stop the services:

```bash
docker compose -f docker-compose.<backend>.yaml down
```

Where `<backend>` is either `gcp` or `nats`.
