# Amplifier API

Rust client library for interacting with the Axelar Amplifier API. This crate provides types and utilities for relayers supporting the Axelar infrastructure.

## Features

### BigInt Type Configuration

The `BigInt` type used for token amounts can be configured at compile-time to match your blockchain's native precision:

- **Default**: U256 (256-bit) for Ethereum/EVM chains
- **`bigint-u64`**: 64-bit for Solana
- **`bigint-u128`**: 128-bit for intermediate precision

```toml
# Default (Ethereum/EVM)
amplifier-api = "0.1"

# Solana
amplifier-api = { version = "0.1", features = ["bigint-u64"] }

# Custom u128
amplifier-api = { version = "0.1", features = ["bigint-u128"] }
```

This avoids unnecessary overhead - e.g., Solana never uses values larger than u64.

## API Types

The crate provides comprehensive type definitions for:

- Events (MessageApproved, MessageExecuted, GasCredit, etc.)
- Tasks (Execute, Refund, Verify)
- Utilities for serialization/deserialization with both JSON and Borsh

## Examples

See the `examples/` directory for usage examples, including health check functionality.

## Dependencies

- `bnum` - For U256 support (default BigInt type)
- `borsh` - Binary serialization
- `serde` - JSON serialization
- `reqwest` - HTTP client with middleware support
- `chrono` - DateTime handling
