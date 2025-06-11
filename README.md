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
