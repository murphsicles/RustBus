use super::fetcher::BlockFetcher;
use super::classifier::TransactionClassifier;
use super::super::config::Config;
use super::super::models::{IndexedTx, BlockHeader};
use super::super::AppState;
use super::super::utils::{extract_op_return, TxExt};
use sv::messages::Block;
use sv::util::sha256d;
use sqlx::{Pool, Postgres, postgres::PgTransaction};
use log::{info, warn};
use rayon::prelude::*;
use backoff::{ExponentialBackoff, future::retry};
use std::time::{Duration, Instant};
use super::super::metrics::{TXS_INDEXED, BLOCK_PROCESS_TIME};

const ZMQ_RECONNECT_DELAY: u64 = 5;

pub async fn index_blocks(config: Config, pool: Pool<Postgres>, state: std::sync::Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    let mut fetcher = BlockFetcher::new(&config.rpc_url, &config.rpc_user, &config.rpc_password, config.network)?;
    let classifier = TransactionClassifier::new();
    let latest_height = sync_historical_blocks(&config, &pool, &mut fetcher, &classifier, &state).await?;

    let zmq_context = zmq::Context::new();
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
                        query.build().execute(&mut db_tx).await?;
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

async fn sync_historical_blocks(
    config: &Config,
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    classifier: &TransactionClassifier,
    state: &std::sync::Arc<AppState>,
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
            query.build().execute(&mut db_tx).await?;
            TXS_INDEXED.inc_by(indexed_txs.len() as f64);
        }

        index_block(&mut db_tx, &block, height).await?;
        db_tx.commit().await?;

        info!("Synced historical block {} with {} transactions", height, indexed_txs.len());
    }

    Ok(tip_height)
}

pub async fn handle_reorg(
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
    .fetch_optional(&mut tx)
    .await?;

    if let Some(prev) = prev_block {
        let prev_hash = hex::encode(&new_block.header.prev_hash.0);
        if prev.block_hash != prev_hash {
            warn!("Reorg detected at height {}. Rolling back...", new_height);
            sqlx::query("DELETE FROM transactions WHERE block_height >= $1")
                .bind(new_height)
                .execute(&mut tx)
                .await?;
            sqlx::query("DELETE FROM blocks WHERE height >= $1")
                .bind(new_height)
                .execute(&mut tx)
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
