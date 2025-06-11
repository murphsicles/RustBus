# Stage 1: Build the Rust application
FROM rust:1.81 AS builder
WORKDIR /usr/src/rustbus

# Copy source code and configuration
COPY . .

# Install system dependencies for ZeroMQ
RUN apt-get update && apt-get install -y libzmq3-dev && rm -rf /var/lib/apt/lists/*

# Build the release binary
RUN cargo build --release

# Stage 2: Create the runtime image
FROM debian:bookworm-slim
WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y libzmq5 && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/rustbus/target/release/rustbus /usr/local/bin/rustbus

# Copy the .env file (optional, can be overridden at runtime)
COPY .env /app/.env

# Expose ports for HTTP (API, GraphQL, WebSocket) and Prometheus metrics
EXPOSE 8080 9090

# Set the entrypoint to run the application
CMD ["rustbus"]
