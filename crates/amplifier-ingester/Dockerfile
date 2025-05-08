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

# Copy the full project and build
COPY . .

# Allow feature override at build time (defaults to "default")
ARG FEATURES=default
RUN cargo build --release --bin amplifier-ingester --no-default-features --features ${FEATURES}

# ---- Runtime Stage ----
FROM debian:bookworm-slim

WORKDIR /app

# Install only runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libssl3 \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy built binary from the builder stage
COPY --from=builder /app/target/release/amplifier-ingester /app/amplifier-ingester

# Expose healthcheck port
EXPOSE 8080

# Create non-root user and switch
RUN useradd --no-log-init --create-home relayer
USER relayer

# Entrypoint & default command
ENTRYPOINT ["/app/amplifier-ingester"]
CMD ["--config-path", "/app/relayer-config.toml"]

# OCI labels
LABEL org.opencontainers.image.source="https://github.com/axelarnetwork/axelar-relayer-core"
LABEL org.opencontainers.image.description="Axelar Relayer Amplifier Ingester"
LABEL org.opencontainers.image.licenses="MIT"
