# Build stage
FROM rust:1.85-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev

WORKDIR /build

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn lib() {}" > src/lib.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source
COPY src ./src
COPY examples ./examples
COPY benches ./benches
COPY tests ./tests

# Build for real
RUN touch src/main.rs src/lib.rs && \
    cargo build --release && \
    strip target/release/rexpipe

# Runtime stage
FROM alpine:3.19

# Install runtime dependencies (minimal)
RUN apk add --no-cache tini

# Create non-root user
RUN addgroup -g 1000 rexpipe && \
    adduser -D -u 1000 -G rexpipe rexpipe

# Copy binary from builder
COPY --from=builder /build/target/release/rexpipe /usr/local/bin/rexpipe

# Switch to non-root user
USER rexpipe
WORKDIR /data

# Use tini as init system
ENTRYPOINT ["/sbin/tini", "--", "rexpipe"]

# Default to help
CMD ["--help"]
