use actix_web::{web, App, HttpServer, HttpResponse, Responder};
use actix_web_actors::ws;
use async_std::net::TcpStream;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres, postgres::PgPoolOptions, Transaction};
use sv::{block::Block, network::Network, node::Node, transaction::Transaction};
use tokio::sync::mpsc;
use log::{info, error, warn};
use rayon::prelude::*;
use dashmap::DashMap;
use regex::Regex;
use zmq::Context;
use backoff::{ExponentialBackoff, future::retry};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Configuration struct for database and node connection
#[derive(Clone)]
struct Config {
    db_url: String,
    bsv_node: String,
    zmq_addr: String,
    network: Network,
}

// Transaction data stored in the database
#[derive(Debug, Serialize, Deserialize, Clone)]
struct IndexedTx {
    txid: String,
    block_height: i64,
    tx_type: String, // e.g., "RUN", "MAP", "B", "BCAT", "AIP", "METANET", "STANDARD"
    op_return: Option<String>,
    tx_hex: String,
}

// Block header stored in the database
#[derive(Debug, Serialize, Deserialize)]
struct BlockHeader {
    block_hash: String,
    height: i64,
    prev_hash: String,
}

// Subscription filter for clients
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Subscription {
    client_id: String,
    filter_type: Option<String>,
    op_return_pattern: Option<String>,
}

// Transaction classifier for protocol detection
struct TransactionClassifier {
    protocols: Vec<(String, Regex)>,
}

impl TransactionClassifier {
    fn new() -> Self {
        let protocols = vec![
            ("RUN".to_string(), Regex::new(r"run://").expect("Invalid RUN regex")),
            ("MAP".to_string(), Regex::new(r"1PuQa7").expect("Invalid MAP regex")),
            ("B".to_string(), Regex::new(r"19HxigV4QyBv3tHpQVcUEQyq1pzZVdoAut").expect("Invalid B regex")),
            ("BCAT".to_string(), Regex::new(r"15PciHG22SNLQJXMoSUaWVi7WSqc7hCfva").expect("Invalid BCAT regex")),
            ("AIP".to_string(), Regex::new(r"1J7Gm3UGv5R3vRjAf9nV7oJ3yF3nD4r93r").expect("Invalid AIP regex")),
            ("METANET".to_string(), Regex::new(r"1Meta").expect("Invalid Metanet regex")),
        ];
        TransactionClassifier { protocols }
    }

    fn classify(&self, tx: &Transaction) -> String {
        if let Some(op_return) = extract_op_return(tx) {
            for (protocol, regex) in &self.protocols {
                if regex.is_match(&op_return) {
                    return protocol.clone();
                }
            }
        }
        "STANDARD".to_string()
    }
}

// WebSocket actor for client subscriptions
struct Subscriber {
    id: String,
    tx: mpsc::Sender<IndexedTx>,
    state: Arc<AppState>,
}

impl actix::Actor for Subscriber {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let tx = self.tx.clone();
        let mut rx = tx.subscribe();
        ctx.run_interval(Duration::from_millis(100), move |act, ctx| {
            while let Ok(tx) = rx.try_recv() {
                if act.state.subscriptions.contains_key(&act.id) {
                    ctx.text(serde_json::to_string(&tx).unwrap_or_default());
                }
            }
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        self.state.subscriptions.remove(&self.id);
        info!("Subscriber {} disconnected", self.id);
    }
}

impl ws::Websocket for Subscriber {
    fn on_message(&mut self, msg: String, ctx: &mut Self::Context) {
        match serde_json::from_str::<Subscription>(&msg) {
            Ok(sub) => {
                let valid = sub.filter_type.is_some() || sub.op_return_pattern.is_some();
                if valid {
                    self.state.subscriptions.insert(self.id.clone(), sub.clone());
                    info!("New subscription for client {}: {:?}", self.id, sub);
                    ctx.text(format!("Subscribed: {:?}", sub));
                } else {
                    warn!("Invalid subscription from {}: no filters specified", self.id);
                    ctx.text("Invalid subscription: must specify filter_type or op_return_pattern");
                }
            }
            Err(e) => {
                warn!("Invalid subscription format from {}: {}", self.id, e);
                ctx.text("Invalid subscription format");
            }
        }
    }

    fn on_binary(&mut self, _data: bytes::Bytes, _ctx: &mut Self::Context) {}
}

// Shared application state
struct AppState {
    db_pool: Pool<Postgres>,
    subscriptions: Arc<DashMap<String, Subscription>>,
    tx_channel: mpsc::Sender<IndexedTx>,
}

// Block fetcher using rust-sv
struct BlockFetcher {
    node: Node,
}

impl BlockFetcher {
    async fn new(node_addr: &str, network: Network) -> Result<Self, Box<dyn std::error::Error>> {
        let backoff = ExponentialBackoff::default();
        let stream = retry(backoff, || async {
            TcpStream::connect(node_addr).await.map_err(|e| backoff::Error::transient(e))
        }).await?;
        let node = Node::new(stream, network, None)?;
        info!("Connected to BSV node at {}", node_addr);
        Ok(BlockFetcher { node })
    }

    async fn fetch_block(&mut self, block_hash: &str) -> Result<Block, Box<dyn std::error::Error>> {
        let backoff = ExponentialBackoff::default();
        let block = retry(backoff, || async {
            self.node.get_block(&block_hash).await.map_err(|e| backoff::Error::transient(e))
        }).await?;
        info!("Fetched block {} with hash {}", block.height, block_hash);
        Ok(block)
    }

    async fn get_block_hash(&mut self, height: u64) -> Result<String, Box<dyn std::error::Error>> {
        let backoff = ExponentialBackoff::default();
        let hash = retry(backoff, || async {
            self.node.get_block_hash(height).await.map_err(|e| backoff::Error::transient(e))
        }).await?;
        Ok(hash)
    }
}

// Initialize database schema
async fn init_db(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS transactions (
            txid TEXT PRIMARY KEY,
            block_height BIGINT NOT NULL,
            tx_type TEXT NOT NULL,
            op_return TEXT,
            tx_hex TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS blocks (
            block_hash TEXT PRIMARY KEY,
            height BIGINT NOT NULL,
            prev_hash TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tx_type ON transactions (tx_type);
        CREATE INDEX IF NOT EXISTS idx_op_return ON transactions USING gin (op_return gin_trgm_ops);
        CREATE INDEX IF NOT EXISTS idx_block_height ON blocks (height);
        "#
    )
    .execute(pool)
    .await?;
    Ok(())
}

// Handle blockchain reorganization
async fn handle_reorg(
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    new_block: &Block,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = pool.begin().await?;
    
    // Check if new block's prev_hash matches stored block at height-1
    let prev_block: Option<BlockHeader> = sqlx::query_as(
        "SELECT block_hash, height, prev_hash FROM blocks WHERE height = $1"
    )
    .bind(new_block.height.saturating_sub(1) as i64)
    .fetch_optional(&mut tx)
    .await?;

    if let Some(prev) = prev_block {
        if prev.block_hash != new_block.prev_hash {
            warn!("Reorg detected at height {}. Rolling back...", new_block.height);
            // Delete transactions and blocks from this height onward
            sqlx::query("DELETE FROM transactions WHERE block_height >= $1")
                .bind(new_block.height as i64)
                .execute(&mut tx)
                .await?;
            sqlx::query("DELETE FROM blocks WHERE height >= $1")
                .bind(new_block.height as i64)
                .execute(&mut tx)
                .await?;

            // Reindex from the fork point
            let mut current_height = new_block.height;
            let mut current_hash = new_block.hash.clone();
            while current_height >= prev.height {
                let block = fetcher.fetch_block(&current_hash).await?;
                index_block(&mut tx, &block).await?;
                current_hash = block.prev_hash.clone();
                current_height -= 1;
            }
        }
    }

    tx.commit().await?;
    Ok(())
}

// Index a single block
async fn index_block(
    tx: &mut Transaction<'_, Postgres>,
    block: &Block,
) -> Result<(), Box<dyn std::error::Error>> {
    // Store block header
    sqlx::query(
        r#"
        INSERT INTO blocks (block_hash, height, prev_hash)
        VALUES ($1, $2, $3)
        ON CONFLICT (block_hash) DO NOTHING
        "#
    )
    .bind(&block.hash)
    .bind(block.height as i64)
    .bind(&block.prev_hash)
    .execute(tx)
    .await?;

    Ok(())
}

// Fetch and index blocks using ZMQ
async fn index_blocks(config: Config, pool: Pool<Postgres>, state: Arc<AppState>) {
    let mut fetcher = match BlockFetcher::new(&config.bsv_node, config.network).await {
        Ok(fetcher) => fetcher,
        Err(e) => {
            error!("Failed to connect to BSV node: {}. Exiting...", e);
            return;
        }
    };
    let classifier = TransactionClassifier::new();

    let zmq_context = Context::new();
    let subscriber = zmq_context.socket(zmq::SUB).expect("Failed to create ZMQ socket");
    let backoff = ExponentialBackoff {
        max_elapsed_time: Some(Duration::from_secs(3600)),
        ..Default::default()
    };

    loop {
        let result = retry(backoff.clone(), || async {
            subscriber
                .connect(&config.zmq_addr)
                .map_err(|e| backoff::Error::transient(e))?;
            subscriber
                .set_subscribe(b"hashblock")
                .map_err(|e| backoff::Error::transient(e))?;
            Ok(())
        }).await;

        if result.is_err() {
            error!("Failed to reconnect to ZMQ after retries. Exiting...");
            return;
        }

        info!("Listening for ZMQ block notifications at {}", config.zmq_addr);

        while let Ok(parts) = subscriber.recv_multipart(0) {
            if parts.len() < 2 || parts[0] != b"hashblock" {
                continue;
            }

            let start_time = Instant::now();
            let block_hash = hex::encode(&parts[1]);
            let mut db_tx = match pool.begin().await {
                Ok(tx) => tx,
                Err(e) => {
                    warn!("Failed to start DB transaction: {}. Retrying...", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            match fetcher.fetch_block(&block_hash).await {
                Ok(block) => {
                    // Check for reorg
                    if let Err(e) = handle_reorg(&pool, &mut fetcher, &block).await {
                        warn!("Reorg handling failed: {}. Skipping block {}", e, block_hash);
                        continue;
                    }

                    // Index transactions
                    let indexed_txs: Vec<IndexedTx> = block.transactions.par_iter().filter_map(|tx| {
                        match tx.txid() {
                            Ok(_) => Some(IndexedTx {
                                txid: tx.txid().unwrap().to_string(),
                                block_height: block.height as i64,
                                tx_type: classifier.classify(tx),
                                op_return: extract_op_return(tx),
                                tx_hex: tx.to_hex(),
                            }),
                            Err(e) => {
                                warn!("Invalid transaction in block {}: {}", block.height, e);
                                None
                            }
                        }
                    }).collect();

                    for tx in &indexed_txs {
                        if let Err(e) = sqlx::query(
                            r#"
                            INSERT INTO transactions (txid, block_height, tx_type, op_return, tx_hex)
                            VALUES ($1, $2, $3, $4, $5)
                            ON CONFLICT (txid) DO NOTHING
                            "#
                        )
                        .bind(&tx.txid)
                        .bind(tx.block_height)
                        .bind(&tx.tx_type)
                        .bind(&tx.op_return)
                        .bind(&tx.tx_hex)
                        .execute(&mut db_tx)
                        .await {
                            warn!("Error inserting tx {}: {}", tx.txid, e);
                            continue;
                        }

                        state.subscriptions.iter().par_bridge().for_each(|entry| {
                            let sub = entry.value();
                            if matches_subscription(&tx, sub) {
                                let _ = state.tx_channel.send(tx.clone()).await;
                            }
                        });
                    }

                    // Store block header
                    if let Err(e) = index_block(&mut db_tx, &block).await {
                        warn!("Error indexing block {}: {}. Skipping...", block.height, e);
                        continue;
                    }

                    if let Err(e) = db_tx.commit().await {
                        warn!("Failed to commit DB transaction: {}. Retrying block {}", e, block_hash);
                        continue;
                    }

                    let elapsed = start_time.elapsed();
                    info!(
                        "Indexed block {} with {} transactions in {:.2}s",
                        block.height,
                        indexed_txs.len(),
                        elapsed.as_secs_f64()
                    );
                }
                Err(e) => {
                    warn!("Error fetching block {}: {}. Retrying...", block_hash, e);
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
        }

        warn!("ZMQ connection lost. Reconnecting...");
    }
}

// Check if a transaction matches a subscription
fn matches_subscription(tx: &IndexedTx, sub: &Subscription) -> bool {
    let type_match = sub
        .filter_type
        .as_ref()
        .map_or(true, |t| t == &tx.tx_type);
    let op_return_match = sub.op_return_pattern.as_ref().map_or(true, |pattern| {
        tx.op_return.as_ref().map_or(false, |op_return| {
            Regex::new(pattern)
                .map(|re| re.is_match(op_return))
                .unwrap_or(false)
        })
    });
    type_match && op_return_match
}

// Extract OP_RETURN data
fn extract_op_return(tx: &Transaction) -> Option<String> {
    tx.outputs.iter()
        .find(|out| out.script.is_op_return())
        .and_then(|out| Some(hex::encode(&out.script.data)))
}

// WebSocket route
async fn ws_route(
    req: actix_web::HttpRequest,
    stream: web::Payload,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, actix_web::Error> {
    let (tx, _rx) = mpsc::channel(100);
    let subscriber = Subscriber {
        id: uuid::Uuid::new_v4().to_string(),
        tx,
        state: state.get_ref().clone(),
    };
    ws::start(subscriber, &req, stream)
}

// Main function
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    let config = Config {
        db_url: "postgres://user:pass@localhost/rustbus".to_string(),
        bsv_node: "127.0.0.1:18333".to_string(),
        zmq_addr: "tcp://127.0.0.1:28332".to_string(),
        network: Network::Testnet,
    };

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.db_url)
        .await
        .expect("Failed to connect to database");
    init_db(&pool).await.expect("Failed to initialize database");

    let (tx_channel, mut rx_channel) = mpsc::channel::<IndexedTx>(1000);
    let state = Arc::new(AppState {
        db_pool: pool.clone(),
        subscriptions: Arc::new(DashMap::new()),
        tx_channel,
    });

    let index_config = config.clone();
    tokio::spawn(index_blocks(index_config, pool.clone(), state.clone()));

    tokio::spawn(async move {
        while let Some(tx) = rx_channel.recv().await {
            info!("Broadcasting tx: {}", tx.txid);
        }
    });

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/ws", web::get().to(ws_route))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
