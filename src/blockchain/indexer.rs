use super::fetcher::BlockFetcher;
use super::classifier::TransactionClassifier;
use super::super::config::Config;
use super::super::models::{IndexedTx, BlockHeader};
use super::super::AppState;
use crate::utils::{extract_op_return, TxExt};
use sv::messages::Block;
use sv::util::{sha256d, Serializable};
use sqlx::{Pool, Postgres, postgres::PgTransaction, Executor};
use log::{info, warn, error};
use rayon::prelude::*;
use backoff::{ExponentialBackoff, future::retry};
use std::time::{Duration, Instant};
use super::super::metrics::{TXS_INDEXED, BLOCK_PROCESS_TIME};

const ZMQ_RECONNECT_DELAY: u64 = 5;
const BATCH_SIZE: usize = 1000;

pub async fn index_blocks(config: Config, pool: Pool<Postgres>, state: std::sync::Arc<AppState>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut fetcher = BlockFetcher::new(&config.rpc_url, &config.rpc_user, &config.rpc_password, config.network)?;
    let classifier = TransactionClassifier::new();
    let _latest_height = sync_historical_blocks(&config, &pool, &mut fetcher, &classifier, &state).await?;

    let zmq_context = zmq::Context::new();
    let backoff = ExponentialBackoff {
        max_elapsed_time: Some(Duration::from_secs(3600)),
        ..Default::default()
    };

    loop {
        let subscriber = zmq_context.socket(zmq::SUB).expect("Failed to create ZMQ socket");
        
        let zmq_addr = config.zmq_addr.clone();
        let result: Result<(), backoff::Error<zmq::Error>> = retry(backoff.clone(), || {
            let subscriber_ref = &subscriber;
            async {
                subscriber_ref.connect(&zmq_addr).map_err(|e| backoff::Error::transient(e))?;
                subscriber_ref.set_subscribe(b"hashblock").map_err(|e| backoff::Error::transient(e))?;
                Ok(())
            }
        }).await;

        if result.is_err() {
            warn!("Failed to reconnect to ZMQ after retries. Exiting...");
            return Err("ZMQ connection failed".into());
        }

        info!("Listening for ZMQ block notifications at {}", config.zmq_addr);

        while let Ok(parts) = subscriber.recv_multipart(0) {
            if parts.len() < 2 || parts[0] != b"hashblock" {
                continue;
            }

            let start_time = Instant::now();
            let block_hash = hex::encode(&parts[1]);
            
            match process_new_block(&pool, &mut fetcher, &classifier, &block_hash).await {
                Ok(tx_count) => {
                    let elapsed = start_time.elapsed();
                    BLOCK_PROCESS_TIME.observe(elapsed.as_secs_f64());
                    info!(
                        "Indexed block {} with {} transactions in {:.2}s",
                        block_hash,
                        tx_count,
                        elapsed.as_secs_f64()
                    );
                }
                Err(e) => {
                    warn!("Error processing block {}: {}. Retrying...", block_hash, e);
                    tokio::time::sleep(Duration::from_secs(ZMQ_RECONNECT_DELAY)).await;
                    continue;
                }
            }
        }

        warn!("ZMQ connection lost. Reconnecting...");
        tokio::time::sleep(Duration::from_secs(ZMQ_RECONNECT_DELAY)).await;
    }
}

async fn process_new_block(
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    classifier: &TransactionClassifier,
    block_hash: &str,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let mut db_tx = pool.begin().await?;
    
    let (block, height) = fetcher.fetch_block(block_hash)?;
    
    handle_reorg(&pool, fetcher, &block, height, &mut db_tx).await?;
    let tx_count = process_block_transactions(&mut db_tx, &block, height, classifier).await?;
    index_block(&mut db_tx, &block, height).await?;

    db_tx.commit().await.map_err(|e| {
        error!("Failed to commit transaction for block {}: {}", block_hash, e);
        e
    })?;

    Ok(tx_count)
}

async fn sync_historical_blocks(
    config: &Config,
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    classifier: &TransactionClassifier,
    _state: &std::sync::Arc<AppState>,
) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    let latest_height: Option<i64> = sqlx::query_scalar("SELECT MAX(height) FROM blocks")
        .fetch_one(pool)
        .await?;
    let start_height = latest_height.map(|h| h + 1).unwrap_or(config.start_height);
    let tip_height = fetcher.get_best_block_height().unwrap_or(start_height);

    info!("Syncing historical blocks from {} to {}", start_height, tip_height);

    for height in start_height..=tip_height {
        let block_hash = fetcher.get_block_hash(height)?;
        let (block, height) = fetcher.fetch_block(&block_hash)?;
        let mut db_tx = pool.begin().await?;

        handle_reorg(&pool, fetcher, &block, height, &mut db_tx).await?;
        let tx_count = process_block_transactions(&mut db_tx, &block, height, classifier).await?;
        index_block(&mut db_tx, &block, height).await?;

        db_tx.commit().await.map_err(|e| {
            error!("Failed to commit transaction for historical block {}: {}", height, e);
            e
        })?;

        info!("Synced historical block {} with {} transactions", height, tx_count);
    }

    Ok(tip_height)
}

async fn process_block_transactions(
    db_tx: &mut PgTransaction<'_>,
    block: &Block,
    height: i64,
    classifier: &TransactionClassifier,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let indexed_txs: Vec<IndexedTx> = block.txns.par_iter().filter_map(|tx| {
        let hash = tx.hash();
        Some(IndexedTx {
            txid: hex::encode(&hash.0),
            block_height: height,
            tx_type: classifier.classify(tx),
            op_return: extract_op_return(tx),
            tx_hex: tx.to_hex(),
        })
    }).collect();

    if indexed_txs.is_empty() {
        return Ok(0);
    }

    for batch in indexed_txs.chunks(BATCH_SIZE) {
        insert_transaction_batch(db_tx, batch).await?;
    }

    TXS_INDEXED.inc_by(indexed_txs.len() as f64);
    Ok(indexed_txs.len())
}

async fn insert_transaction_batch(
    db_tx: &mut PgTransaction<'_>,
    batch: &[IndexedTx],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut query = sqlx::QueryBuilder::new(
        "INSERT INTO transactions (txid, block_height, tx_type, op_return, tx_hex) VALUES "
    );
    
    for (i, tx) in batch.iter().enumerate() {
        if i > 0 { 
            query.push(", "); 
        }
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
    
    query.push(" ON CONFLICT (txid) DO NOTHING");
    
    (&mut *db_tx).execute(query.build()).await.map_err(|e| {
        error!("Failed to insert transaction batch of {} transactions: {}", batch.len(), e);
        e
    })?;
    
    Ok(())
}

pub async fn handle_reorg(
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    new_block: &Block,
    new_height: i64,
    tx: &mut PgTransaction<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let prev_block: Option<BlockHeader> = (&mut *tx).fetch_optional(sqlx::query_as(
        "SELECT block_hash, height, prev_hash FROM blocks WHERE height = $1"
    )
    .bind(new_height - 1)).await?;

    if let Some(prev) = prev_block {
        let expected_prev_hash = hex::encode(&new_block.header.prev_hash.0);
        
        if prev.block_hash != expected_prev_hash {
            warn!("Reorg detected at height {}. Expected prev hash: {}, got: {}", 
                  new_height, expected_prev_hash, prev.block_hash);
            
            let fork_height = find_fork_point(pool, fetcher, new_height).await?;
            
            warn!("Fork point found at height {}. Rolling back to height {}", fork_height, fork_height);
            
            (&mut *tx).execute(sqlx::query("DELETE FROM transactions WHERE block_height > $1")
                .bind(fork_height)).await?;
                
            (&mut *tx).execute(sqlx::query("DELETE FROM blocks WHERE height > $1")
                .bind(fork_height)).await?;
            
            info!("Rolled back blocks from height {} to {}", new_height, fork_height + 1);
        }
    }

    Ok(())
}

async fn find_fork_point(
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    start_height: i64,
) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    let mut height = start_height - 1;
    
    while height > 0 {
        let our_hash: Option<String> = sqlx::query_scalar(
            "SELECT block_hash FROM blocks WHERE height = $1"
        )
        .bind(height)
        .fetch_optional(pool)
        .await?;
        
        if let Some(our_hash) = our_hash {
            let canonical_hash = fetcher.get_block_hash(height)?;
            
            if our_hash == canonical_hash {
                return Ok(height);
            }
        }
        
        height -= 1;
    }
    
    Ok(0)
}

async fn index_block(
    tx: &mut PgTransaction<'_>,
    block: &Block,
    height: i64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let block_hash = hex::encode(&sha256d(&{
        let mut bytes = Vec::new();
        block.header.write(&mut bytes)?;
        bytes
    }).0);
    
    let prev_hash = hex::encode(&block.header.prev_hash.0);
    
    (&mut *tx).execute(sqlx::query(
        r#"
        INSERT INTO blocks (block_hash, height, prev_hash, timestamp)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (block_hash, height) DO NOTHING
        "#
    )
    .bind(&block_hash)
    .bind(height)
    .bind(&prev_hash)
    .bind(block.header.timestamp as i64)).await?;
    
    Ok(())
}
