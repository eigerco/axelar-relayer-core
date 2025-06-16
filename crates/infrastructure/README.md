# Infrastructure

This crate provides storage bus implementations for various backends used by the Axelar relayer system.

## Overview

The `infrastructure` crate serves as an abstraction layer for message queuing and storage, allowing the relayer to work with different backends without changing the core business logic. It provides a unified interface for:

- **Message Consumption**: Pull messages from queues with acknowledgment support
- **Message Publishing**: Push messages to queues with deduplication
- **Key-Value Storage**: Store and retrieve data with versioning support

## Supported Backends

### Google Cloud Platform (GCP)
- **Message Queue**: Google Cloud Pub/Sub
- **Storage**: Redis
- Feature flag: `gcp` (enabled by default)

### NATS
- **Message Queue**: NATS JetStream
- **Storage**: NATS Key-Value Store
- Feature flag: `nats`

## Core Interfaces

### Consumer Interface

Consumes messages from a queue with acknowledgment support:

```rust
use infrastructure::Consumer;

#[async_trait]
pub trait Consumer: Send + Sync {
    async fn consume(&self) -> Result<Option<Message>>;
}
```

### Publisher Interface

Publishes messages to a queue with deduplication:

```rust
use infrastructure::Publisher;

#[async_trait]
pub trait Publisher: Send + Sync {
    async fn publish(&self, message: Message) -> Result<()>;
}
```

### KV Store Interface

Key-value storage with versioning support:

```rust
use infrastructure::Store;

#[async_trait]
pub trait Store: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn put(&self, key: &str, value: Vec<u8>) -> Result<()>;
    async fn delete(&self, key: &str) -> Result<()>;
}
```

## Custom Message Types

To add custom message types (separate from amplifier task/event), implement `infrastructure::interfaces::publisher::QueueMsgId`. The `id()` method should return a unique identifier that will be used for message deduplication:

```rust
use infrastructure::interfaces::publisher::QueueMsgId;

#[derive(Debug)]
struct MyCustomMessage {
    transaction_id: String,  // Must be unique for deduplication
    amount: u64,
    timestamp: u64,
}

impl QueueMsgId for MyCustomMessage {
    type MessageId = String;

    fn id(&self) -> Self::MessageId {
        self.transaction_id.clone()  // Used as deduplication ID
    }
}

// Publish using the From trait conversion
publisher.publish(my_message.into()).await?;
```

## Usage Examples

### Using GCP Backend

```rust
use infrastructure::gcp::{GcpConsumer, GcpPublisher, GcpStore};

// Create a consumer
let consumer = GcpConsumer::new(
    "my-project",
    "my-subscription",
    "my-topic"
).await?;

// Create a publisher
let publisher = GcpPublisher::new(
    "my-project",
    "my-topic"
).await?;

// Create a key-value store
let store = GcpStore::new("redis://localhost:6379").await?;
```

### Using NATS Backend

```rust
use infrastructure::nats::{NatsConsumer, NatsPublisher, NatsStore};

// Create a consumer
let consumer = NatsConsumer::new(
    "nats://localhost:4222",
    "my-stream",
    "my-consumer"
).await?;

// Create a publisher
let publisher = NatsPublisher::new(
    "nats://localhost:4222",
    "my-stream"
).await?;

// Create a key-value store
let store = NatsStore::new(
    "nats://localhost:4222",
    "my-bucket"
).await?;
```

## Health Checks

All components implement health check capabilities:

```rust
use infrastructure::CheckHealth;

// Check consumer health
consumer.check().await?;

// Check publisher health
publisher.check().await?;

// Check store health
store.check().await?;
```

## Configuration

Each backend requires specific configuration:

### GCP Configuration
```toml
[gcp]
project_id = "my-project"
topic = "my-topic"
subscription = "my-subscription"
redis_url = "redis://localhost:6379"
```

### NATS Configuration
```toml
[nats]
urls = ["nats://localhost:4222"]
stream = "my-stream"
consumer = "my-consumer"
bucket = "my-bucket"
```

## Feature Flags

Build with specific backends:

```bash
# GCP only (default)
cargo build --features gcp

# NATS only
cargo build --features nats --no-default-features

# Both backends
cargo build --features gcp,nats
```

## Design Philosophy

The infrastructure crate follows these principles:

1. **Backend Agnostic**: Core business logic doesn't depend on specific queue/storage implementations
2. **Trait-Based Design**: All backends implement common traits for easy swapping
3. **Async-First**: All operations are async for optimal performance
4. **Health Check Support**: Built-in health monitoring for production deployments
5. **Message Acknowledgment**: Ensures reliable message processing with at-least-once delivery

## Performance Considerations

- **Batching**: Publishers support message batching for improved throughput
- **Connection Pooling**: Backends maintain connection pools for efficiency
- **Retry Logic**: Built-in retry mechanisms for transient failures
- **Deduplication**: Publishers can deduplicate messages to prevent duplicate processing

## Error Handling

The crate provides comprehensive error types for different failure scenarios:

- Connection errors
- Serialization/deserialization errors
- Backend-specific errors (e.g., Pub/Sub quotas, NATS stream limits)
- Timeout errors

## Testing

Mock implementations are provided for testing:

```rust
#[cfg(test)]
use infrastructure::mock::{MockConsumer, MockPublisher, MockStore};

#[tokio::test]
async fn test_my_component() {
    let consumer = MockConsumer::new();
    let publisher = MockPublisher::new();
    let store = MockStore::new();
    
    // Test your component with mock backends
}
```
