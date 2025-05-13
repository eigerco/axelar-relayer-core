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

# Copy the full project and build
COPY . .

# Allow feature override at build time (defaults to "default")
ARG FEATURES=default
RUN cargo build --release --bin amplifier-ingester --no-default-features --features ${FEATURES} && \
    strip target/release/amplifier-ingester

# ---- Runtime Stage ----
FROM alpine:latest

WORKDIR /app

# Install only runtime dependencies
RUN apk add --no-cache \
    ca-certificates

# Copy built binary from the builder stage
COPY --from=builder /app/target/release/amplifier-ingester /app/amplifier-ingester

# Expose healthcheck port
EXPOSE 8080

# Create non-root user and switch
RUN adduser -D relayer
USER relayer

# Entrypoint & default command
ENTRYPOINT ["/app/amplifier-ingester"]
CMD ["--config-path", "/app/relayer-config.toml"]

# OCI labels
LABEL org.opencontainers.image.source="https://github.com/axelarnetwork/axelar-relayer-core"
LABEL org.opencontainers.image.description="Axelar Relayer Amplifier Ingester"
LABEL org.opencontainers.image.licenses="MIT"
