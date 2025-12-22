# AIngle - Multi-stage Docker Build
# https://github.com/ApiliumCode/aingle

# =============================================================================
# Stage 1: Build
# =============================================================================
FROM rust:1.85-bookworm AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    libsodium-dev \
    libssl-dev \
    pkg-config \
    cmake \
    clang \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build dependencies (this layer will be cached)
RUN cargo build --release --workspace 2>/dev/null || true

# Copy source code
COPY . .

# Build the project
RUN cargo build --release --workspace

# =============================================================================
# Stage 2: Runtime
# =============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libsodium23 \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 -s /bin/bash aingle

# Create directories
RUN mkdir -p /app/data /app/config && chown -R aingle:aingle /app

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /app/target/release/aingle /usr/local/bin/
COPY --from=builder /app/target/release/ai /usr/local/bin/
COPY --from=builder /app/target/release/ai-sandbox /usr/local/bin/

# Copy default config if exists
COPY --from=builder /app/config* /app/config/ 2>/dev/null || true

# Switch to non-root user
USER aingle

# Environment variables
ENV RUST_LOG=info
ENV AINGLE_DATA_DIR=/app/data
ENV AINGLE_CONFIG_DIR=/app/config

# Expose ports
# 8888 - Admin API
# 8889 - App API
# 5353 - mDNS discovery
# 5684 - CoAP (UDP)
EXPOSE 8888 8889 5353/udp 5684/udp

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD aingle --version || exit 1

# Default command
ENTRYPOINT ["aingle"]
CMD ["--help"]

# =============================================================================
# Stage 3: Development
# =============================================================================
FROM rust:1.85-bookworm AS development

# Install system dependencies
RUN apt-get update && apt-get install -y \
    libsodium-dev \
    libssl-dev \
    pkg-config \
    cmake \
    clang \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install development tools
RUN rustup component add rustfmt clippy llvm-tools-preview \
    && cargo install cargo-watch cargo-audit cargo-llvm-cov

WORKDIR /app

# Mount point for source code
VOLUME ["/app"]

# Environment for development
ENV RUST_LOG=debug
ENV RUST_BACKTRACE=1

# Default command for development
CMD ["cargo", "watch", "-x", "check", "-x", "test"]
