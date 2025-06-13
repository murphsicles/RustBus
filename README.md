# RustBus: The Legendary BSV Microservices Engine

![RustBus Logo](https://img.shields.io/badge/RustBus-BSV%20Explorer-blue) [![CI/CD](https://github.com/murphsicles/RustBus/actions/workflows/ci.yml/badge.svg)](https://github.com/murphsicles/RustBus/actions) [![Docker](https://img.shields.io/docker/pulls/rustbus.svg)](https://hub.docker.com/r/rustbus)
## Status
[![Dependencies](https://deps.rs/repo/github/murphsicles/rust-sv/status.svg)](https://deps.rs/repo/github/murphsicles/rust-sv)

**RustBus** is a high-performance blockchain explorer, indexer and microservices engine for the BSV blockchain (BSV), engineered to far surpass traditional indexers with exceptional scalability and efficiency. Written in Rust, `RustBus` harnesses modern technologies—GraphQL, REST, WebSocket APIs, PostgreSQL partitioning, and ZeroMQ—to provide real-time transaction and block indexing for both *mainnet* and *testnet*. Ideal for developers querying BSV transactions, miners tracking blocks, or enterprises leveraging BSV's massive blockchain, `RustBus` is your premier solution.

## ✨ Features

- **Scalable Indexing**: Manages BSV's large-scale blocks and transactions using PostgreSQL table partitioning by block height, optimized for mainnet.
- **Modern APIs**:
  - **GraphQL**: Query transactions, blocks, and OP_RETURN data via a flexible `/graphql` endpoint with a GraphiQL playground.
  - **REST**: Retrieve transactions at `/tx/{txid}` or filter by type at `/txs?type=BCAT`.
  - **WebSocket**: Subscribe to real-time transaction notifications at `/ws` for protocols like RUN, MAP or BCAT.
- **Real-Time Notifications**: Connects to BSV nodes via ZeroMQ for instant block updates.
- **Prometheus Metrics**: Tracks performance at `/metrics` (port 8080) with metrics like `rustbus_txs_indexed_total`.
- **Mainnet & Testnet Support**: Configurable with `NETWORK=mainnet` or `NETWORK=testnet`.
- **Historical Syncing**: Syncs blocks from any starting height (e.g., `START_HEIGHT=800000`) with reorg handling.
- **Dockerized Deployment**: Lightweight (~100MB) Docker image for seamless CI/CD and production.
- **Efficient Storage**: Selective indexing with GIN trigram indexes on OP_RETURN for rapid searches.
- **CI/CD Pipeline**: Automated testing, linting, and Docker builds via GitHub Actions.

## 🚀 Quick Start

Launch `RustBus` in minutes with Docker, PostgreSQL, and a BSV node.

### Prerequisites

- **Docker**: Install from [docker.com](https://www.docker.com/get-started).
- **PostgreSQL 15+**: Install locally or use a managed service.
- **BSV Node**: Run a BSV node with RPC (port 8332) and ZeroMQ (port 28332) enabled.
- **Rust 1.81+** (optional for local builds): Install via [rustup.rs](https://rustup.rs).

### Setup

1. **Clone the Repository**:
   ```bash
   git clone https://github.com/murphsicles/RustBus
   cd RustBus
   ```

3. **Configure Environment**:
   - Copy the example environment file:
     ```bash
     cp .env.example .env
     ```
   - Edit `.env` with your settings:
     ```bash
     DATABASE_URL=postgres://user:pass@localhost:5432/rustbus
     BSV_NODE=127.0.0.1:8332
     ZMQ_ADDR=tcp://127.0.0.1:28332
     NETWORK=mainnet
     START_HEIGHT=800000
     METRICS_PORT=8080
     ```

4. **Set Up PostgreSQL**:
   - Create the database:
     ```bash
     sudo -u postgres createdb rustbus
     sudo -u postgres psql -c "ALTER USER postgres WITH PASSWORD 'pass';"
     ```
   - Apply the partitioned schema:
     ```bash
     psql -h localhost -U postgres -d rustbus -f schema.sql
     ```

5. **Build and Run with Docker**:
   ```bash
   docker build -t rustbus:latest .
   docker run -d --name rustbus -p 8080:8080 --env-file .env rustbus:latest
   ```

6. **Verify**:
   - GraphQL Playground: Open `http://localhost:8080/graphql`.
   - Health Check: `curl http://localhost:8080/health`
   - Metrics: `curl http://localhost:8080/metrics`
   - Check indexed blocks:
     ```bash
     psql -d rustbus -c "SELECT COUNT(*) FROM blocks;"
     ```

## 🛠️ Environment Variables

Configure `RustBus` via `.env` or Docker `-e` flags:

| Variable         | Description                              | Example                             |
|------------------|------------------------------------------|-------------------------------------|
| DATABASE_URL     | PostgreSQL connection string             | postgres://user:pass@localhost:5432/rustbus |
| BSV_NODE         | BSV node RPC address (host:port)         | 127.0.0.1:8332                      |
| ZMQ_ADDR         | ZeroMQ address for block notifications   | tcp://127.0.0.1:28332               |
| NETWORK          | Blockchain network                       | mainnet or testnet                  |
| START_HEIGHT     | Starting block height for historical sync| 800000                              |
| METRICS_PORT     | Port for Prometheus metrics              | 8080                                |

## 📚 API Examples

### GraphQL Query

Fetch a transaction by TXID:

```graphql

query {
  transaction(txid: "abc123...") {
    txid
    block_height
    tx_type
    op_return
  }
}
```
Run via `http://localhost:8080/graphql` or `curl`:
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"query":"query { transaction(txid: \"abc123...\") { txid block_height tx_type op_return } }"}' \
  http://localhost:8080/graphql
```

### REST API
Get transactions by type:
```bash
curl "http://localhost:8080/txs?type=RUN&limit=10"
```
Get a single transaction:
```bash
curl http://localhost:8080/tx/abc123...
```

### WebSocket Subscription
Subscribe to RUN transactions:
```bash
wscat -c ws://127.0.0.1:8080/ws
```
Send:
```json
{"client_id": "test1", "filter_type": "RUN"}
```
### Prometheus Metrics
Monitor indexing performance: 
```bash 
curl http://localhost:8080/metrics 
``` 

Example 
```bash
output:rustbus_txs_indexed_total{network="mainnet"} 123456
rustbus_block_process_seconds{network="mainnet"} 0.05
rustbus_active_subscriptions 10
```

### 🗄️ Database Schema
RustBus uses partitioned PostgreSQL tables for scalability. Key tables (schema.sql):
-transactions:
```sql
CREATE TABLE transactions (
    txid TEXT NOT NULL,
    block_height BIGINT NOT NULL,
    tx_type TEXT NOT NULL,
    op_return TEXT,
    tx_hex TEXT NOT NULL,
    PRIMARY KEY (txid, block_height)
) PARTITION BY RANGE (block_height);
```
-Partitions:
transactions_p0_100000, transactions_p100000_200000, etc.

-Indexes: GIN trigram on op_return, B-tree on tx_type, block_height.
-blocks:
```sql
CREATE TABLE blocks (
    height BIGINT PRIMARY KEY,
    hash TEXT NOT NULL,
    timestamp BIGINT NOT NULL
) PARTITION BY RANGE (height);
```
Apply: 
```bash 
psql -d rustbus -f schema.sql 
```

### 🐳 Docker Deployment
Build and push to Docker Hub: 
```bash 
docker build -t yourusername/rustbus:latest . docker push yourusername/rustbus:latest 
```
Deploy with Fly.io (example fly.toml):
```toml
app = "rustbus"
primary_region = "iad"

[http_service]
  internal_port = 8080
  force_https = true
  auto_stop_machines = true
  auto_start_machines = true
  min_machines_running = 1
```
Run: 
```bash 
flyctl deploy 
```

## 🔧 Development

Build and run locally without Docker:
```bash
cargo build --release
cargo run
```

Run tests:
```bash
cargo test --verbose
```

Lint:
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

## 🤝 Contributing

We welcome contributions! Fork the repo, create a branch, and submit a PR. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## 📜 License

MIT License. See [LICENSE](LICENSE).

## 🌟 Why RustBus?

`RustBus` redefines BSV exploration with:
- **Performance**: Rust's speed and safety for mainnet-scale indexing.
- **Flexibility**: GraphQL, REST, and WebSocket APIs for all use cases.
- **Scalability**: Partitioned storage and efficient indexing.
- **Reliability**: CI/CD with GitHub Actions and Docker.

Join the BSV revolution with `RustBus`—the explorer that scales as big as your ambitions!
