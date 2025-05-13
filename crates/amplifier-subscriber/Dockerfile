# ---- Build Stage ----
FROM rust:1.86-alpine AS builder

WORKDIR /app

# Install build dependencies
RUN apk add --no-cache \
    pkgconfig \
    openssl-dev \
    openssl-libs-static \
    build-base \
    ca-certificates \
    musl-dev \
    perl \
    make \
    git

# Set environment variables for static linking
ENV OPENSSL_STATIC=1
ENV OPENSSL_LIB_DIR=/usr/lib
ENV OPENSSL_INCLUDE_DIR=/usr/include

# Copy project files
COPY . .

# Build with optional features (default: "default")
ARG FEATURES=default
RUN cargo build --release --bin amplifier-subscriber --no-default-features --features ${FEATURES} && \
    strip target/release/amplifier-subscriber

# ---- Runtime Stage ----
FROM alpine:latest

WORKDIR /app

# Install minimal runtime dependencies
RUN apk add --no-cache \
    ca-certificates

# Copy built binary
COPY --from=builder /app/target/release/amplifier-subscriber /app/amplifier-subscriber

# Expose health check port
EXPOSE 8080

# Create non-root user
RUN adduser -D relayer
USER relayer

# Entrypoint and default command
ENTRYPOINT ["/app/amplifier-subscriber"]
CMD ["--config-path", "/app/relayer-config.toml"]

# Labels for clarity
LABEL org.opencontainers.image.source="https://github.com/axelarnetwork/axelar-relayer-core"
LABEL org.opencontainers.image.description="Axelar Relayer Amplifier Subscriber"
LABEL org.opencontainers.image.licenses="MIT"
