use actix_web::{web, App, HttpServer, HttpResponse, Responder};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use sv::{block::Block, transaction::Transaction};
use tokio::sync::mpsc;
use log::{info, error};
use rayon::prelude::*;
use std::sync::Arc;

// Configuration struct for database and node connection
#[derive(Clone)]
struct Config {
    db_url: String,
    bsv_node: String,
}

// Transaction data stored in the database
#[derive(Debug, Serialize, Deserialize)]
struct IndexedTx {
    txid: String,
    block_height: i64,
    tx_type: String, // e.g., "RUN", "MAP"
    op_return: Option<String>,
    tx_hex: String,
}

// Subscription filter for clients
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Subscription {
    client_id: String,
    filter_type: String, // e.g., "RUN", "MAP"
    op_return_pattern: Option<String>,
}

// WebSocket actor for client subscriptions
struct Subscriber {
    id: String,
    tx: mpsc::Sender<IndexedTx>,
}

impl actix::Actor for Subscriber {
    type Context = ws::WebsocketContext<Self>;
}

impl ws::Websocket for Subscriber {
    fn on_message(&mut self, msg: String, ctx: &mut Self::Context) {
        // Parse subscription request
        if let Ok(sub) = serde_json::from_str::<Subscription>(&msg) {
            info!("New subscription: {:?}", sub);
            // In a real implementation, store subscription in a shared state
        }
    }

    fn on_binary(&mut self, _data: bytes::Bytes, _ctx: &mut Self::Context) {}
}

// Shared application state
struct AppState {
    db_pool: Pool<Postgres>,
    subscriptions: Arc<Vec<Subscription>>,
    tx_channel: mpsc::Sender<IndexedTx>,
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
        CREATE INDEX IF NOT EXISTS idx_tx_type ON transactions (tx_type);
        CREATE INDEX IF NOT EXISTS idx_op_return ON transactions USING gin (op_return gin_trgm_ops);
        "#
    )
    .execute(pool)
    .await?;
    Ok(())
}

// Fetch and index blocks
async fn index_blocks(config: Config, pool: Pool<Postgres>, tx_channel: mpsc::Sender<IndexedTx>) {
    loop {
        // Simulated block fetching (replace with rust-sv P2P connection)
        let block = fetch_block(&config.bsv_node).await.unwrap_or_default();

        // Process transactions in parallel
        let indexed_txs: Vec<IndexedTx> = block.transactions.par_iter().map(|tx| {
            let tx_type = classify_transaction(tx);
            let op_return = extract_op_return(tx);
            IndexedTx {
                txid: tx.txid().to_string(),
                block_height: block.height,
                tx_type,
                op_return,
                tx_hex: tx.to_hex(),
            }
        }).collect();

        // Batch insert to database
        for tx in &indexed_txs {
            sqlx::query(
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
            .execute(&pool)
            .await
            .unwrap_or_else(|e| error!("Error inserting tx: {}", e));

            // Send to subscribers
            let _ = tx_channel.send(tx.clone()).await;
        }

        info!("Indexed block {} with {} transactions", block.height, indexed_txs.len());
        tokio::time::sleep(std::time::Duration::from_secs(600)).await; // Simulate 10-min block time
    }
}

// Simulated block fetching (replace with rust-sv)
async fn fetch_block(_node: &str) -> Option<Block> {
    Some(Block {
        height: 0,
        transactions: vec![Transaction::default()], // Placeholder
    })
}

// Classify transaction type (simplified)
fn classify_transaction(tx: &Transaction) -> String {
    // Implement logic to detect RUN, MAP, etc.
    if tx.outputs.iter().any(|out| out.script.is_op_return()) {
        "RUN".to_string() // Example
    } else {
        "STANDARD".to_string()
    }
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
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (tx, rx) = mpsc::channel(100);
    let subscriber = Subscriber {
        id: uuid::Uuid::new_v4().to_string(),
        tx,
    };
    ws::start(subscriber, &req, stream)
}

// Main function
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    let config = Config {
        db_url: "postgres://user:pass@localhost/junglebus".to_string(),
        bsv_node: "bsv-node:8333".to_string(),
    };

    // Initialize database
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.db_url)
        .await
        .expect("Failed to connect to database");
    init_db(&pool).await.expect("Failed to initialize database");

    // Channel for transaction broadcasting
    let (tx_channel, mut rx_channel) = mpsc::channel::<IndexedTx>(1000);
    let state = Arc::new(AppState {
        db_pool: pool.clone(),
        subscriptions: Arc::new(vec![]),
        tx_channel,
    });

    // Start block indexing task
    let index_config = config.clone();
    tokio::spawn(index_blocks(index_config, pool.clone(), state.tx_channel.clone()));

    // Start subscription broadcaster
    tokio::spawn(async move {
        while let Some(tx) = rx_channel.recv().await {
            // Check against subscriptions and broadcast (simplified)
            info!("Broadcasting tx: {}", tx.txid);
        }
    });

    // Start HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/ws", web::get().to(ws_route))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
