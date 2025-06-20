use actix::{Actor, StreamHandler, AsyncContext};
use actix_web::{web, App, HttpServer, HttpResponse, Responder, get, post};
use actix_web_actors::ws;
use async_graphql::{Schema, EmptyMutation, EmptySubscription, Object, Context, http::PlaygroundConfig};
use async_graphql_actix_web::{GraphQLRequest, GraphQLResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Postgres, postgres::{PgPoolOptions, PgTransaction}};
use sv::messages::{Block, Tx};
use sv::network::Network;
use sv::util::{sha256d, Serializable};
use tokio::sync::broadcast;
use log::{info, error, warn};
use rayon::prelude::*;
use dashmap::DashMap;
use regex::Regex;
use zmq::Context as ZmqContext;
use backoff::{ExponentialBackoff, future::retry};
use prometheus::{register_counter, register_gauge, register_histogram, Counter, Gauge, Histogram};
use dotenvy::dotenv;
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};
use bitcoinsv_rpc::{Client as RpcClient, RpcApi};
use bitcoinsv_rpc::BlockHash;
use std::io::{Cursor, Write};

const TX_CHANNEL_SIZE: usize = 1000;
const ZMQ_RECONNECT_DELAY: u64 = 5;
const BLOCK_FETCH_RETRY_DELAY: u64 = 10;

// Prometheus metrics
lazy_static::lazy_static! {
    static ref TXS_INDEXED: Counter = register_counter!("rustbus_txs_indexed_total", "Total transactions indexed").unwrap();
    static ref BLOCK_PROCESS_TIME: Histogram = register_histogram!("rustbus_block_process_seconds", "Block processing time in seconds").unwrap();
    static ref ACTIVE_SUBS: Gauge = register_gauge!("rustbus_active_subscriptions", "Number of active WebSocket subscriptions").unwrap();
}

// Configuration struct
#[derive(Clone)]
struct Config {
    db_url: String,
    bsv_node: String,
    zmq_addr: String,
    network: Network,
    start_height: i64,
    metrics_port: u16,
    bind_addr: String,
    rpc_url: String,
    rpc_user: String,
    rpc_password: String,
}

// Transaction data
#[derive(Debug, Serialize, Deserialize, Clone, async_graphql::SimpleObject, sqlx::FromRow)]
struct IndexedTx {
    txid: String,
    block_height: i64,
    tx_type: String,
    op_return: Option<String>,
    tx_hex: String,
}

// Block header
#[derive(Debug, Serialize, Deserialize, async_graphql::SimpleObject, sqlx::FromRow)]
struct BlockHeader {
    block_hash: String,
    height: i64,
    prev_hash: String,
}

// Subscription filter
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Subscription {
    client_id: String,
    filter_type: Option<String>,
    op_return_pattern: Option<String>,
}

// Extension trait for Tx
trait TxExt {
    fn to_hex(&self) -> String;
}

impl TxExt for Tx {
    fn to_hex(&self) -> String {
        let mut bytes = Vec::new();
        self.write(&mut bytes).unwrap();
        hex::encode(bytes)
    }
}

// GraphQL schema
struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn transaction(&self, ctx: &Context<'_>, txid: String) -> async_graphql::Result<Option<IndexedTx>> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();
        let tx: Option<IndexedTx> = sqlx::query_as(
            "SELECT txid, block_height, tx_type, op_return, tx_hex FROM transactions WHERE txid = $1"
        )
        .bind(&txid)
        .fetch_optional(pool)
        .await?;
        Ok(tx)
    }

    async fn transactions(
        &self,
        ctx: &Context<'_>,
        tx_type: Option<String>,
        block_height: Option<i64>,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<IndexedTx>> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();
        let limit = limit.unwrap_or(100).clamp(1, 1000);
        let mut query_builder = sqlx::QueryBuilder::new("SELECT txid, block_height, tx_type, op_return, tx_hex FROM transactions WHERE 1=1");
        if let Some(t) = tx_type {
            query_builder.push(" AND tx_type = ");
            query_builder.push_bind(t);
        }
        if let Some(h) = block_height {
            query_builder.push(" AND block_height = ");
            query_builder.push_bind(h);
        }
        query_builder.push(" LIMIT ");
        query_builder.push_bind(limit);

        let txs: Vec<IndexedTx> = query_builder
            .build_query_as()
            .fetch_all(pool)
            .await?;
        Ok(txs)
    }

    async fn block(&self, ctx: &Context<'_>, height: i64) -> async_graphql::Result<Option<BlockHeader>> {
        let pool = ctx.data_unchecked::<Pool<Postgres>>();
        let block: Option<BlockHeader> = sqlx::query_as(
            "SELECT block_hash, height, prev_hash FROM blocks WHERE height = $1"
        )
        .bind(height)
        .fetch_optional(pool)
        .await?;
        Ok(block)
    }
}

// Transaction classifier
struct TransactionClassifier {
    protocols: Vec<(String, Regex)>,
}

impl TransactionClassifier {
    fn new() -> Self {
        let protocols_str = env::var("PROTOCOLS").unwrap_or_default();
        let protocols = protocols_str
            .split(';')
            .filter_map(|p| {
                let parts: Vec<&str> = p.split(':').collect();
                if parts.len() == 2 {
                    Regex::new(parts[1]).ok().map(|re| (parts[0].to_string(), re))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let defaults = vec![
            ("RUN".to_string(), Regex::new(r"run://").unwrap()),
            ("MAP".to_string(), Regex::new(r"1PuQa7").unwrap()),
            ("B".to_string(), Regex::new(r"19HxigV4QyBv3tHpQVcUEQyq1pzZVdoAut").unwrap()),
            ("BCAT".to_string(), Regex::new(r"15PciHG22SNLQJXMoSUaWVi7WSqc7hCfva").unwrap()),
            ("AIP".to_string(), Regex::new(r"1J7Gm3UGv5R3vRjAf9nV7oJ3yF3nD4r93r").unwrap()),
            ("METANET".to_string(), Regex::new(r"1Meta").unwrap()),
        ];
        TransactionClassifier {
            protocols: if protocols.is_empty() { defaults } else { protocols },
        }
    }

    fn classify(&self, tx: &Tx) -> String {
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

// WebSocket actor
struct Subscriber {
    id: String,
    tx: broadcast::Sender<IndexedTx>,
    state: Arc<AppState>,
}

impl Actor for Subscriber {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ACTIVE_SUBS.inc();
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
        ACTIVE_SUBS.dec();
        self.state.subscriptions.remove(&self.id);
        info!("Subscriber {} disconnected", self.id);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Subscriber {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                match serde_json::from_str::<Subscription>(&text) {
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
            Ok(ws::Message::Binary(bin)) => {
                info!("Received binary message from {}: {} bytes", self.id, bin.len());
            }
            Ok(ws::Message::Close(reason)) => {
                info!("WebSocket closed for {}: {:?}", self.id, reason);
                ctx.close(reason);
            }
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Nop) => {}
            Err(e) => {
                warn!("WebSocket protocol error: {:?}", e);
                ctx.close(None);
            }
        }
    }
}

// Shared application state
struct AppState {
    db_pool: Pool<Postgres>,
    subscriptions: Arc<DashMap<String, Subscription>>,
    tx_channel: broadcast::Sender<IndexedTx>,
    schema: Schema<QueryRoot, EmptyMutation, EmptySubscription>,
}

// Block fetcher
struct BlockFetcher {
    rpc: RpcClient,
}

impl BlockFetcher {
    fn new(rpc_url: &str, rpc_user: &str, rpc_password: &str, _network: Network) -> Result<Self, Box<dyn std::error::Error>> {
        let rpc = RpcClient::new(rpc_url, bitcoinsv_rpc::Auth::UserPass(rpc_user.to_string(), rpc_password.to_string()), None)?;
        info!("Connected to BSV node RPC at {}", rpc_url);
        Ok(BlockFetcher { rpc })
    }

    fn fetch_block(&mut self, block_hash: &str) -> Result<(Block, i64), Box<dyn std::error::Error>> {
        let block_hash = BlockHash::from_str(block_hash)?;
        let block_hex: Value = self.rpc.call("getblock", &[block_hash.to_string().into(), 0.into()])?;
        let block_hex = block_hex.as_str().ok_or_else(|| "Expected string for block hex")?;
        let block_bytes = hex::decode(block_hex)?;
        let block = Block::read(&mut Cursor::new(&block_bytes))?;
        let block_json: Value = self.rpc.call("getblock", &[block_hash.to_string().into(), 1.into()])?;
        let height = block_json["height"].as_i64().ok_or_else(|| "Missing height")?;
        info!("Fetched block {} with hash {}", height, block_hash);
        Ok((block, height))
    }

    fn get_block_hash(&mut self, height: i64) -> Result<String, Box<dyn std::error::Error>> {
        let hash = self.rpc.get_block_hash(height as u64)?;
        Ok(hash.to_string())
    }
}

// Initialize database schema
async fn init_db(pool: &Pool<Postgres>, max_height: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS transactions (
            txid TEXT NOT NULL,
            block_height BIGINT NOT NULL,
            tx_type TEXT NOT NULL,
            op_return TEXT,
            tx_hex TEXT NOT NULL,
            PRIMARY KEY (txid, block_height)
        ) PARTITION BY RANGE (block_height);
        CREATE TABLE IF NOT EXISTS blocks (
            block_hash TEXT NOT NULL,
            height BIGINT NOT NULL,
            prev_hash TEXT NOT NULL,
            PRIMARY KEY (block_hash, height)
        ) PARTITION BY RANGE (height);
        "#
    )
    .execute(pool)
    .await?;

    let partition_size = 100_000;
    for start in (0..=max_height).step_by(partition_size as usize) {
        let end = start + partition_size;
        sqlx::query(&format!(
            r#"
            CREATE TABLE IF NOT EXISTS transactions_{start}_{end} PARTITION OF transactions
                FOR VALUES FROM ({start}) TO ({end});
            CREATE TABLE IF NOT EXISTS blocks_{start}_{end} PARTITION OF blocks
                FOR VALUES FROM ({start}) TO ({end});
            CREATE INDEX IF NOT EXISTS idx_tx_type_{start}_{end} ON transactions_{start}_{end} (tx_type);
            CREATE INDEX IF NOT EXISTS idx_op_return_{start}_{end} ON transactions_{start}_{end} USING gin (op_return gin_trgm_ops);
            CREATE INDEX IF NOT EXISTS idx_block_height_{start}_{end} ON blocks_{start}_{end} (height);
            "#
        ))
        .execute(pool)
        .await?;
    }
    Ok(())
}

// Handle blockchain reorganization
async fn handle_reorg(
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    new_block: &Block,
    new_height: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = pool.begin().await?;
    
    let prev_block: Option<BlockHeader> = sqlx::query_as(
        "SELECT block_hash, height, prev_hash FROM blocks WHERE height = $1"
    )
    .bind(new_height.saturating_sub(1))
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(prev) = prev_block {
        let prev_hash = hex::encode(&new_block.header.prev_hash.0);
        if prev.block_hash != prev_hash {
            warn!("Reorg detected at height {}. Rolling back...", new_height);
            sqlx::query("DELETE FROM transactions WHERE block_height >= $1")
                .bind(new_height)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM blocks WHERE height >= $1")
                .bind(new_height)
                .execute(&mut *tx)
                .await?;

            let mut current_height = new_height;
            let mut current_hash = hex::encode(&sha256d(&{
                let mut bytes = Vec::new();
                new_block.header.write(&mut bytes)?;
                bytes
            }).0);
            while current_height >= prev.height {
                let (block, height) = fetcher.fetch_block(&current_hash)?;
                index_block(&mut tx, &block, height).await?;
                current_hash = hex::encode(&block.header.prev_hash.0);
                current_height -= 1;
            }
        }
    }

    tx.commit().await?;
    Ok(())
}

// Index a single block
async fn index_block(
    tx: &mut PgTransaction<'_>,
    block: &Block,
    height: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let block_hash = hex::encode(&sha256d(&{
        let mut bytes = Vec::new();
        block.header.write(&mut bytes)?;
        bytes
    }).0);
    let prev_hash = hex::encode(&block.header.prev_hash.0);
    sqlx::query(
        r#"
        INSERT INTO blocks (block_hash, height, prev_hash)
        VALUES ($1, $2, $3)
        ON CONFLICT (block_hash, height) DO NOTHING
        "#
    )
    .bind(&block_hash)
    .bind(height)
    .bind(&prev_hash)
    .execute(tx)
    .await?;

    Ok(())
}

// Sync historical blocks
async fn sync_historical_blocks(
    config: &Config,
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    classifier: &TransactionClassifier,
    state: &Arc<AppState>,
) -> Result<i64, Box<dyn std::error::Error>> {
    let latest_height: Option<i64> = sqlx::query_scalar("SELECT MAX(height) FROM blocks")
        .fetch_one(pool)
        .await?;
    let start_height = latest_height.map(|h| h + 1).unwrap_or(config.start_height);
    let tip_height = match fetcher.get_block_hash(i64::MAX) {
        Ok(hash) => hash.parse::<i64>().unwrap_or(start_height),
        Err(_) => start_height,
    };

    for height in start_height..=tip_height {
        let block_hash = fetcher.get_block_hash(height)?;
        let (block, height) = fetcher.fetch_block(&block_hash)?;
        let mut db_tx = pool.begin().await?;

        let indexed_txs: Vec<IndexedTx> = block.txns.par_iter().filter_map(|tx| {
            match tx.hash() {
                Ok(hash) => Some(IndexedTx {
                    txid: hex::encode(&hash.0),
                    block_height: height,
                    tx_type: classifier.classify(tx),
                    op_return: extract_op_return(tx),
                    tx_hex: tx.to_hex(),
                }),
                Err(e) => {
                    warn!("Invalid transaction in block {}: {}", height, e);
                    None
                }
            }
        }).collect();

        if !indexed_txs.is_empty() {
            let mut query = sqlx::QueryBuilder::new("INSERT INTO transactions (txid, block_height, tx_type, op_return, tx_hex) VALUES ");
            for (i, tx) in indexed_txs.iter().enumerate() {
                if i > 0 { query.push(", "); }
                query.push("(");
                query.push_bind(&tx.txid);
                query.push(", ");
                query.push_bind(tx.block_height);
                query.push(", ");
                query.push_bind(&tx.tx_type);
                query.push(", ");
                query.push_bind(&tx.op_return);
                query.push(", ");
                query.push_bind(&tx.tx_hex);
                query.push(")");
            }
            query.build().execute(&mut *db_tx).await?;
            TXS_INDEXED.inc_by(indexed_txs.len() as f64);
        }

        index_block(&mut db_tx, &block, height).await?;
        db_tx.commit().await?;

        info!("Synced historical block {} with {} transactions", height, indexed_txs.len());
    }

    Ok(tip_height)
}

// Fetch and index blocks using ZMQ
async fn index_blocks(config: Config, pool: Pool<Postgres>, state: Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    let mut fetcher = BlockFetcher::new(&config.rpc_url, &config.rpc_user, &config.rpc_password, config.network)?;
    let classifier = TransactionClassifier::new();
    let latest_height = sync_historical_blocks(&config, &pool, &mut fetcher, &classifier, &state).await?;

    let zmq_context = ZmqContext::new();
    let subscriber = zmq_context.socket(zmq::SUB).expect("Failed to create ZMQ socket");
    let backoff = ExponentialBackoff {
        max_elapsed_time: Some(Duration::from_secs(3600)),
        ..Default::default()
    };

    loop {
        let result = retry(backoff.clone(), || async {
            subscriber.connect(&config.zmq_addr).map_err(|e| backoff::Error::transient(e))?;
            subscriber.set_subscribe(b"hashblock").map_err(|e| backoff::Error::transient(e))?;
            Ok(())
        }).await;

        if result.is_err() {
            error!("Failed to reconnect to ZMQ after retries. Exiting...");
            return Err("ZMQ connection failed".into());
        }

        info!("Listening for ZMQ block notifications at {}", config.zmq_addr);

        while let Ok(parts) = subscriber.recv_multipart(0) {
            if parts.len() < 2 || parts[0] != b"hashblock" {
                continue;
            }

            let start_time = Instant::now();
            let block_hash = hex::encode(&parts[1]);
            let mut db_tx = pool.begin().await?;

            match fetcher.fetch_block(&block_hash) {
                Ok((block, height)) => {
                    if let Err(e) = handle_reorg(&pool, &mut fetcher, &block, height).await {
                        warn!("Reorg handling failed: {}. Skipping block {}", e, block_hash);
                        continue;
                    }

                    let indexed_txs: Vec<IndexedTx> = block.txns.par_iter().filter_map(|tx| {
                        match tx.hash() {
                            Ok(hash) => Some(IndexedTx {
                                txid: hex::encode(&hash.0),
                                block_height: height,
                                tx_type: classifier.classify(tx),
                                op_return: extract_op_return(tx),
                                tx_hex: tx.to_hex(),
                            }),
                            Err(e) => {
                                warn!("Invalid transaction in block {}: {}", height, e);
                                None
                            }
                        }
                    }).collect();

                    if !indexed_txs.is_empty() {
                        let mut query = sqlx::QueryBuilder::new("INSERT INTO transactions (txid, block_height, tx_type, op_return, tx_hex) VALUES ");
                        for (i, tx) in indexed_txs.iter().enumerate() {
                            if i > 0 { query.push(", "); }
                            query.push("(");
                            query.push_bind(&tx.txid);
                            query.push(", ");
                            query.push_bind(tx.block_height);
                            query.push(", ");
                            query.push_bind(&tx.tx_type);
                            query.push(", ");
                            query.push_bind(&tx.op_return);
                            query.push(", ");
                            query.push_bind(&tx.tx_hex);
                            query.push(")");
                        }
                        query.build().execute(&mut *db_tx).await?;
                        TXS_INDEXED.inc_by(indexed_txs.len() as f64);
                    }

                    index_block(&mut db_tx, &block, height).await?;
                    db_tx.commit().await?;

                    let elapsed = start_time.elapsed();
                    BLOCK_PROCESS_TIME.observe(elapsed.as_secs_f64());
                    info!(
                        "Indexed block {} with {} transactions in {:.2}s",
                        height,
                        indexed_txs.len(),
                        elapsed.as_secs_f64()
                    );
                }
                Err(e) => {
                    warn!("Error fetching block {}: {}. Retrying...", block_hash, e);
                    tokio::time::sleep(Duration::from_secs(ZMQ_RECONNECT_DELAY)).await;
                    continue;
                }
            }
        }

        warn!("ZMQ connection lost. Reconnecting...");
        tokio::time::sleep(Duration::from_secs(ZMQ_RECONNECT_DELAY)).await;
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
fn extract_op_return(tx: &Tx) -> Option<String> {
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
    let subscriber = Subscriber {
        id: uuid::Uuid::new_v4().to_string(),
        tx: state.tx_channel.clone(),
        state: state.get_ref().clone(),
    };
    ws::start(subscriber, &req, stream)
}

// GraphQL route
#[post("/graphql")]
async fn graphql(
    state: web::Data<Arc<AppState>>,
    request: GraphQLRequest,
) -> GraphQLResponse {
    state.schema.execute(request.into_inner()).await.into()
}

// GraphiQL playground
#[get("/graphql")]
async fn graphiql() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(async_graphql::http::playground_source(
            PlaygroundConfig::new("/graphql"),
        ))
}

// REST API: Get transaction by txid
#[get("/tx/{txid}")]
async fn get_tx(
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, actix_web::Error> {
    let txid = path.into_inner();
    let tx: Option<IndexedTx> = sqlx::query_as(
        "SELECT txid, block_height, tx_type, op_return, tx_hex FROM transactions WHERE txid = $1"
    )
    .bind(&txid)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    match tx {
        Some(tx) => Ok(HttpResponse::Ok().json(tx)),
        None => Ok(HttpResponse::NotFound().body("Transaction not found")),
    }
}

// REST API: List transactions by type and/or height
#[get("/txs")]
async fn list_txs(
    query: web::Query<std::collections::HashMap<String, String>>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, actix_web::Error> {
    let tx_type = query.get("type");
    let height = query.get("height").and_then(|h| h.parse::<i64>().ok());
    let limit = query.get("limit").and_then(|l| l.parse::<i32>().ok()).unwrap_or(100).clamp(1, 1000);

    let mut query_builder = sqlx::QueryBuilder::new("SELECT txid, block_height, tx_type, op_return, tx_hex FROM transactions WHERE 1=1");
    if let Some(t) = tx_type {
        query_builder.push(" AND tx_type = ");
        query_builder.push_bind(t);
    }
    if let Some(h) = height {
        query_builder.push(" AND block_height = ");
        query_builder.push_bind(h);
    }
    query_builder.push(" LIMIT ");
    query_builder.push_bind(limit);

    let txs: Vec<IndexedTx> = query_builder
        .build_query_as()
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    Ok(HttpResponse::Ok().json(txs))
}

// Metrics endpoint
async fn metrics() -> impl Responder {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = prometheus::gather();
    let encoded = encoder.encode_to_string(&metric_families).unwrap_or_default();
    HttpResponse::Ok().body(encoded)
}

// Main function
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init();

    let config = Config {
        db_url: env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
        bsv_node: env::var("BSV_NODE").unwrap_or("127.0.0.1:8333".to_string()),
        zmq_addr: env::var("ZMQ_ADDR").unwrap_or("tcp://127.0.0.1:28332".to_string()),
        network: match env::var("NETWORK").unwrap_or("mainnet".to_string()).as_str() {
            "testnet" => Network::Testnet,
            _ => Network::Mainnet,
        },
        start_height: env::var("START_HEIGHT").unwrap_or("0".to_string()).parse().unwrap_or(0),
        metrics_port: env::var("METRICS_PORT").unwrap_or("9090".to_string()).parse().unwrap_or(9090),
        bind_addr: env::var("BIND_ADDR").unwrap_or("0.0.0.0:8080".to_string()),
        rpc_url: env::var("BSV_RPC_URL").unwrap_or("http://127.0.0.1:8332".to_string()),
        rpc_user: env::var("BSV_RPC_USER").unwrap_or_default(),
        rpc_password: env::var("BSV_RPC_PASSWORD").unwrap_or_default(),
    };

    if config.db_url.is_empty() || config.rpc_url.is_empty() || config.rpc_user.is_empty() || config.rpc_password.is_empty() {
        panic!("DATABASE_URL, BSV_RPC_URL, BSV_RPC_USER, and BSV_RPC_PASSWORD must be set");
    }

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.db_url)
        .await
        .expect("Failed to connect to database");
    init_db(&pool, config.start_height + 1_000_000).await?;

    let (tx_channel, _rx_channel) = broadcast::channel::<IndexedTx>(TX_CHANNEL_SIZE);
    let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .data(pool.clone())
        .finish();
    let state = Arc::new(AppState {
        db_pool: pool.clone(),
        subscriptions: Arc::new(DashMap::new()),
        tx_channel: tx_channel.clone(),
        schema,
    });

    let index_config = config.clone();
    tokio::spawn(async move {
        if let Err(e) = index_blocks(index_config, pool.clone(), state.clone()).await {
            error!("Index blocks task failed: {}", e);
        }
    });

    let metrics_config = config.clone();
    tokio::spawn(async move {
        if let Err(e) = HttpServer::new(|| {
            App::new()
                .route("/metrics", web::get().to(metrics))
        })
        .bind(("0.0.0.0", metrics_config.metrics_port))
        .await
        .and_then(|server| server.run())
        {
            error!("Metrics server failed: {}", e);
        }
    });

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/ws", web::get().to(ws_route))
            .service(get_tx)
            .service(list_txs)
            .service(web::resource("/graphql").route(web::post().to(graphql)).route(web::get().to(graphiql)))
    })
    .bind(&config.bind_addr)?
    .run()
    .await
}
