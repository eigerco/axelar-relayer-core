# ---- Build Stage ----
FROM rust:1.86-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    build-essential && \
    rm -rf /var/lib/apt/lists/*

# Copy project files
COPY . .

# Build with optional features (default: "default")
ARG FEATURES=default
RUN cargo build --release --bin amplifier-subscriber --no-default-features --features ${FEATURES} && \
    strip target/release/amplifier-subscriber

# ---- Runtime Stage ----
FROM debian:bookworm-slim

WORKDIR /app

# Install minimal runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libssl3 \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy built binary
COPY --from=builder /app/target/release/amplifier-subscriber /app/amplifier-subscriber

# Expose health check port
EXPOSE 8080

# Create non-root user
RUN useradd --no-log-init --create-home relayer
USER relayer

# Entrypoint and default command
ENTRYPOINT ["/app/amplifier-subscriber"]
CMD ["--config-path", "/app/relayer-config.toml"]

# Labels for clarity
LABEL org.opencontainers.image.source="https://github.com/axelarnetwork/axelar-relayer-core"
LABEL org.opencontainers.image.description="Axelar Relayer Amplifier Subscriber"
LABEL org.opencontainers.image.licenses="MIT"
