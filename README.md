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

The relayer is designed as 4 components: 2 ingestors & 2 subscribers - 1 for each chain (Axelar/amplifier API and the connecting chain)

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

The supervisor is optional, and each component can be started as a separate binary.RetryClaude can make mistakes. Please double-check responses.

## Running the Components

### Configuration

Before running the components, you need to create a configuration file. An example configuration file is provided in `relayer-config-example.toml`. You can copy this file and modify it according to your needs:

```bash
cp relayer-config-example.toml relayer-config.toml
```

Edit the `relayer-config.toml` file to configure:

- Amplifier API configuration
- Backend configuration (NATS or GCP Pub/Sub)
- Tickrate for processing events
- Health check endpoints

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

# Run with default config path (looks for relayer-config.toml in current directory)
./target/debug/amplifier-subscriber

# Or specify a custom config path
./target/debug/amplifier-subscriber --config /path/to/your/config.toml
```

### Running with Specific Backend

By default, both components are built with NATS support. To use GCP as the backend, compile with the `gcp` feature:

```bash
# Build with GCP backend support
cargo build --bin amplifier-ingester --features gcp --no-default-features
cargo build --bin amplifier-subscriber --features gcp --no-default-features
```

### Health Checks

Once running, you can check the health of the components by sending an HTTP request to the configured health check endpoints:

```bash
# Check health of a component (assuming default port 8080)
curl http://localhost:8080/healthz

# Check readiness of a component
curl http://localhost:8080/readyz
```
