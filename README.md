# RustBus
The Legendary Bitcoin Indexing Engine

![RustBus Logo](https://img.shields.io/badge/RustBus-BSV%20Explorer-blue) [![CI/CD](https://github.com/murphsicles/RustBus/actions/workflows/ci.yml/badge.svg)](https://github.com/murphsicles/RustBus/actions) [![Docker](https://img.shields.io/docker/pulls/rustbus.svg)](https://hub.docker.com/r/rustbus)

**RustBus** is a cutting-edge, high-performance blockchain explorer for Bitcoin SV (BSV), designed to outperform traditional indexers by several orders of magnitude on all metrics with unparalleled scalability and efficiency. Built to support BSV's Teranode transactional volume, our pure Rust implementation, `RustBus` leverages modern technologies GraphQL, REST, WebSocket APIs, PostgreSQL partitioning, and ZeroMQ to deliver real-time transaction and block indexing for mainnet and testnet. Whether you're a developer querying BSV transactions, a miner tracking blocks, or an enterprise building on BSV's scalable blockchain, `RustBus` is your only solution.

## ✨ Features

- **Scalable Indexing**: Handles BSV's massive blocks and transactions with PostgreSQL table partitioning by block height, optimized for mainnet scale.
- **Modern APIs**:
  - **GraphQL**: Query transactions, blocks, and OP_RETURN data with a flexible `/graphql` endpoint and GraphiQL playground.
  - **REST**: Fetch transactions via `/tx/{txid}` and filter by type at `/txs?type=BCAT`.
  - **WebSocket**: Subscribe to real-time transaction notifications at `/ws` for protocols like RUN or BCAT.
- **Real-Time Notifications**: Integrates with BSV nodes via ZeroMQ for instant block updates.
- **Prometheus Metrics**: Monitor performance at `/metrics` (port 9090) with metrics like `rustbus_txs_indexed_total`.
- **Mainnet & Testnet Support**: Configurable via `NETWORK=mainnet` or `NETWORK=testnet`.
- **Historical Syncing**: Syncs blocks from any starting height (e.g., `START_HEIGHT=800000`) with reorg handling.
- **Dockerized Deployment**: Lightweight (~100MB) Docker image for easy CI/CD and production setups.
- **Efficient Storage**: Selective indexing and GIN trigram indexes on OP_RETURN for fast searches.
- **CI/CD Pipeline**: Automated testing, linting, and Docker builds via GitHub Actions.

## 🚀 Quick Start

Get `RustBus` running in minutes with Docker, PostgreSQL, and a BSV node.

### Prerequisites

- **Docker**: Install from [docker.com](https://www.docker.com/get-started).
- **PostgreSQL 15+**: Install locally or use a managed service.
- **BSV Node**: Run a Bitcoin SV node with RPC (port 8333) and ZeroMQ (port 28332) enabled.
- **Rust 1.81+** (optional for local builds): Install via [rustup.rs](https://rustup.rs).

### Setup

1. **Clone the Repository**:
   ```bash
   git clone https://github.com/murphsicles/RustBus
   cd RustBus
   ```
   ```bash
   cp .env.example .env


   Configure Environment:Copy the example environment file:cp .env.example .envEdit .env with your settings:DATABASE_URL=postgres://user:pass@localhost/rustbus
BSV_NODE=127.0.0.1:8333
ZMQ_ADDR=tcp://127.0.0.1:28332
NETWORK=mainnet
START_HEIGHT=800000
METRICS_PORT=8000Set Up PostgreSQL:Create the database:sudo -u postgres createdb rustbus
sudo -u postgres psql -c "ALTER USER postgres WITH PASSWORD 'pass';"Apply the partitioned schema:psql -h localhost -U postgres -d rustbus -f schema.sqlBuild and Run with Docker:docker build -t rustbus:latest .
docker run -d --name rustbus -p 8080:8080 -p 8000:9000 --env-file .env rustbus:latest:latestVerify:GraphQL Playground: Open http://localhost:8080/graphql.Health Check: curl http://localhost:8000/api/v1/health.Metrics: curl http://localhost:8000/metrics.Check indexed blocks:psql -d rustbus -c "SELECT COUNT(*) FROM blocks;"🛠️ Environment VariablesConfigure RustBus via .env or Docker -e flags:VariableDescriptionDefault/ExampleDATABASE_URLPostgreSQL connection stringpostgres://user:pass:pass@localhost/rustbusBSVBSV_NODEnode RPC address (host:port)127.0.1:8333ZMQZMQ_ADDRZeroMQ address for block notificationstcp://127.0.0.1:28332NETWORKBlockchain networkmainnet or testnetSTARTSTART_HEIGHTStarting block height for sync800000METRICSMETRICS_PORTPort for Prometheus metrics8000📚 API ExamplesGraphQL QueryFetch a transaction by TXID:query {
  transaction(txid: "abc123...") {
    txid
    block_height
    tx_type
    op_return
  }
}Run via http://localhost:8080/graphql or curl:curl -X POST -H "Content-Type: application/json" \
  -d '{"query":"query { transaction(txid: \"abc123...\") { txid block_height tx_type op_return } }"}' \
  http://localhost:8080/graphqlREST APIGet transactions by type:curl "http://localhost:8080/txs?type=RUN&limit=10"Get a single transaction:curl http://localhost:8080/tx/abc123...WebSocket SubscriptionSubscribe to RUN transactions:wscat -c ws://127.0.0.1:8080/wsSend:{"client_id": "test1", "filter_type": "RUN"}Prometheus MetricsMonitor indexing performance:curl http://localhost:8000/metricsExample output:rustbus_txs_indexed_total{network="mainnet"} 123456
rustbus_block_process_seconds{network="mainnet"} 0.05
rustbus_active_subscriptions 10🗄️ Database SchemaRustBus uses partitioned PostgreSQL tables for scalability. Key tables (schema.sql):transactions:CREATE TABLE transactions (
    txid TEXT NOT NULL,
    block_height BIGINT NOT NULL,
    tx_type TEXT NOT NULL,
    op_return TEXT,
    tx_hex TEXT NOT NULL,
    PRIMARY KEY (txid, block_height)
) PARTITION BY RANGE (block_height);Partitions: transactions_p0_100000, transactions_p100000_200000, etc.Indexes: GIN trigram on op_return, B-tree on tx_type, block_height.blocks:CREATE TABLE blocks (
    height BIGINT PRIMARY KEY,
    hash TEXT NOT NULL,
    timestamp BIGINT NOT NULL
) PARTITION BY RANGE (height);Apply:psql -d rustbus -f schema.sql🐳 Docker DeploymentBuild and push to Docker Hub:docker build -t yourusername/rustbus:latest .
docker push yourusername/rustbus:latestDeploy with Fly.io (example fly.toml):app = "rustbus"
primary_region = "iad"

[http_service]
  internal_port = 8080
  force_https = true
  auto_stop_machines = true
  auto_start_machines = true
  min_machines_running = 1Run:flyctl deploy🔧 DevelopmentBuild and run locally without Docker:cargo build --release
cargo runRun tests:cargo test --verboseLint:cargo clippy --all-targets --all-features -- -D warnings🤝 ContributingWe welcome contributions! Fork the repo, create a branch, and submit a PR. See CONTRIBUTING.md for guidelines.📜 LicenseMIT License. See LICENSE.🌟 Why RustBus?RustBus redefines BSV exploration with:Performance: Rust's speed and safety for mainnet-scale indexing.Flexibility: GraphQL, REST, and WebSocket APIs for all use cases.Scalability: Partitioned storage and efficient indexing.Reliability: CI/CD with GitHub Actions and Docker.Join the BSV revolution with RustBus—the explorer that scales as big as your ambitions!
