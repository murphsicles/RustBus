use super::fetcher::BlockFetcher;
use super::classifier::TransactionClassifier;
use super::super::config::Config;
use super::super::models::{IndexedTx, BlockHeader};
use super::super::AppState;
use super::super::metrics::{TXS_INDEXED, BLOCK_PROCESS_TIME};
use crate::utils::{extract_op_return, TxExt};
use sv::messages::Block;
use sv::util::{sha256d, Serializable};
use sqlx::{Pool, Postgres, postgres::PgTransaction};
use log::{info, warn, error};
use rayon::prelude::*;
use backoff::{ExponentialBackoff, future::retry};
use std::time::{Duration, Instant};
use std::sync::Arc;

// Constants for ZMQ reconnection delay and transaction batch size
const ZMQ_RECONNECT_DELAY: u64 = 5;
const BATCH_SIZE: usize = 1000;

// Indexes blocks by fetching historical and new blocks, processing transactions, and storing in the database
pub async fn index_blocks(config: Config, pool: Pool<Postgres>, state: Arc<AppState>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize the block fetcher for RPC communication with the Bitcoin SV node
    let mut fetcher = BlockFetcher::new(&config.rpc_url, &config.rpc_user, &config.rpc_password, config.network)?;
    // Initialize the transaction classifier for protocol identification
    let classifier = TransactionClassifier::new();
    // Sync historical blocks from the configured start height to the current tip
    let _latest_height = sync_historical_blocks(&config, &pool, &mut fetcher, &classifier, &state).await?;

    // Configure exponential backoff for ZMQ reconnection retries (up to 1 hour)
    let backoff = ExponentialBackoff {
        max_elapsed_time: Some(Duration::from_secs(3600)),
        ..Default::default()
    };
    // Create a ZMQ context for subscribing to block notifications
    let zmq_context = zmq::Context::new();

    // Continuously listen for new blocks via ZMQ, with reconnection on failure
    loop {
        // Retry establishing a ZMQ subscription until successful or retries are exhausted
        let result: Result<zmq::Socket, backoff::Error<zmq::Error>> = retry(backoff.clone(), || async {
            // Create a new ZMQ SUB socket
            let subscriber = zmq_context.socket(zmq::SUB).map_err(backoff::Error::transient)?;
            // Connect to the configured ZMQ address
            subscriber.connect(&config.zmq_addr).map_err(backoff::Error::transient)?;
            // Subscribe to hashblock notifications
            subscriber.set_subscribe(b"hashblock").map_err(backoff::Error::transient)?;
            Ok(subscriber)
        }).await;

        // Handle the result of the ZMQ subscription attempt
        let subscriber = match result {
            Ok(sub) => sub,
            Err(_) => {
                warn!("Failed to reconnect to ZMQ after retries. Exiting...");
                return Err("ZMQ connection failed".into());
            }
        };

        // Log successful ZMQ connection
        info!("Listening for ZMQ block notifications at {}", config.zmq_addr);

        // Process incoming ZMQ messages (block hashes)
        while let Ok(parts) = subscriber.recv_multipart(0) {
            // Validate the message format (hashblock topic + block hash)
            if parts.len() < 2 || parts[0] != b"hashblock" {
                continue;
            }

            // Measure block processing time
            let start_time = Instant::now();
            let block_hash = hex::encode(&parts[1]);
            
            // Process the new block, handling transactions and database storage
            match process_new_block(&pool, &mut fetcher, &classifier, &block_hash).await {
                Ok(tx_count) => {
                    let elapsed = start_time.elapsed();
                    // Record block processing time metric
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
                    break; // Break to reconnect
                }
            }
        }

        // Log ZMQ connection loss and attempt reconnection
        warn!("ZMQ connection lost. Reconnecting...");
        tokio::time::sleep(Duration::from_secs(ZMQ_RECONNECT_DELAY)).await;
    }
}

// Processes a new block by fetching it, handling reorgs, indexing transactions, and storing in the database
async fn process_new_block(
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    classifier: &TransactionClassifier,
    block_hash: &str,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Begin a database transaction for atomic operations
    let mut db_tx = pool.begin().await?;
    
    // Fetch the block and its height via RPC
    let (block, height) = fetcher.fetch_block(block_hash)?;
    
    // Check for and handle blockchain reorganizations
    handle_reorg(pool, fetcher, &block, height, &mut db_tx).await?;
    // Process and index transactions in the block
    let tx_count = process_block_transactions(&mut db_tx, &block, height, classifier).await?;
    // Store block metadata in the database
    index_block(&mut db_tx, &block, height).await?;

    // Commit the database transaction
    db_tx.commit().await.map_err(|e| {
        error!("Failed to commit transaction for block {}: {}", block_hash, e);
        e
    })?;

    // Return the number of transactions processed
    Ok(tx_count)
}

// Syncs historical blocks from the start height to the current chain tip
async fn sync_historical_blocks(
    config: &Config,
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    classifier: &TransactionClassifier,
    _state: &Arc<AppState>,
) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    // Query the maximum block height stored in the database
    let latest_height: Option<i64> = sqlx::query_scalar("SELECT MAX(height) FROM blocks")
        .fetch_one(pool)
        .await?;
    // Determine the starting height for syncing (database max + 1 or configured start height)
    let start_height = latest_height.map(|h| h + 1).unwrap_or(config.start_height);
    // Get the current chain tip height via RPC
    let tip_height = fetcher.get_best_block_height().unwrap_or(start_height);

    // Log the syncing range
    info!("Syncing historical blocks from {} to {}", start_height, tip_height);

    // Process each block from start_height to tip_height
    for height in start_height..=tip_height {
        // Fetch the block hash for the given height
        let block_hash = fetcher.get_block_hash(height)?;
        // Fetch the block data
        let (block, height) = fetcher.fetch_block(&block_hash)?;
        // Begin a database transaction
        let mut db_tx = pool.begin().await?;

        // Handle potential reorganizations
        handle_reorg(pool, fetcher, &block, height, &mut db_tx).await?;
        // Process and index transactions
        let tx_count = process_block_transactions(&mut db_tx, &block, height, classifier).await?;
        // Store block metadata
        index_block(&mut db_tx, &block, height).await?;

        // Commit the transaction
        db_tx.commit().await.map_err(|e| {
            error!("Failed to commit transaction for historical block {}: {}", height, e);
            e
        })?;

        // Log the synced block
        info!("Synced historical block {} with {} transactions", height, tx_count);
    }

    // Return the tip height
    Ok(tip_height)
}

// Processes transactions in a block, classifying and indexing them
async fn process_block_transactions(
    db_tx: &mut PgTransaction<'_>,
    block: &Block,
    height: i64,
    classifier: &TransactionClassifier,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Parallelize transaction processing using rayon
    let indexed_txs: Vec<IndexedTx> = block.txns.par_iter().filter_map(|tx| {
        let hash = tx.hash();
        Some(IndexedTx {
            txid: hex::encode(&hash.0), // Transaction ID as hex
            block_height: height, // Block height
            tx_type: classifier.classify(tx), // Protocol type (e.g., RUN, STANDARD)
            op_return: extract_op_return(tx), // OP_RETURN data, if any
            tx_hex: tx.to_hex(), // Transaction as hex
        })
    }).collect();

    // Return 0 if no transactions are found
    if indexed_txs.is_empty() {
        return Ok(0);
    }

    // Insert transactions in batches to optimize database performance
    for batch in indexed_txs.chunks(BATCH_SIZE) {
        insert_transaction_batch(db_tx, batch).await?;
    }

    // Update the transaction count metric
    TXS_INDEXED.inc_by(indexed_txs.len() as f64);
    // Return the number of transactions processed
    Ok(indexed_txs.len())
}

// Inserts a batch of transactions into the database
async fn insert_transaction_batch(
    db_tx: &mut PgTransaction<'_>,
    batch: &[IndexedTx],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Build an SQL INSERT query for the batch
    let mut query = sqlx::QueryBuilder::new(
        "INSERT INTO transactions (txid, block_height, tx_type, op_return, tx_hex) VALUES "
    );
    
    // Construct the VALUES clause for each transaction
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
    
    // Add ON CONFLICT to ignore duplicate transactions
    query.push(" ON CONFLICT (txid) DO NOTHING");
    
    // Execute the query within the transaction
    query.build().execute(&mut **db_tx).await.map_err(|e| {
        error!("Failed to insert transaction batch of {} transactions: {}", batch.len(), e);
        e
    })?;
    
    // Return success
    Ok(())
}

// Handles blockchain reorganizations by checking for fork points and rolling back invalid blocks
pub async fn handle_reorg(
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    new_block: &Block,
    new_height: i64,
    tx: &mut PgTransaction<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Query the previous block’s metadata
    let prev_block: Option<BlockHeader> = sqlx::query_as(
        "SELECT block_hash, height, prev_hash FROM blocks WHERE height = $1"
    )
    .bind(new_height - 1)
    .fetch_optional(&mut **tx)
    .await?;

    // Check for a reorganization by comparing previous block hashes
    if let Some(prev) = prev_block {
        let expected_prev_hash = hex::encode(&new_block.header.prev_hash.0);
        
        if prev.block_hash != expected_prev_hash {
            // Log detected reorganization
            warn!("Reorg detected at height {}. Expected prev hash: {}, got: {}", 
                  new_height, expected_prev_hash, prev.block_hash);
            
            // Find the fork point where the chain diverged
            let fork_height = find_fork_point(pool, fetcher, new_height).await?;
            
            // Log the rollback operation
            warn!("Fork point found at height {}. Rolling back to height {}", fork_height, fork_height);
            
            // Delete transactions above the fork point
            sqlx::query("DELETE FROM transactions WHERE block_height > $1")
                .bind(fork_height)
                .execute(&mut **tx)
                .await?;
                
            // Delete blocks above the fork point
            sqlx::query("DELETE FROM blocks WHERE height > $1")
                .bind(fork_height)
                .execute(&mut **tx)
                .await?;
            
            // Log successful rollback
            info!("Rolled back blocks from height {} to {}", new_height, fork_height + 1);
        }
    }

    // Return success if no reorg or after handling it
    Ok(())
}

// Finds the height where the blockchain forked by comparing stored and canonical block hashes
async fn find_fork_point(
    pool: &Pool<Postgres>,
    fetcher: &mut BlockFetcher,
    start_height: i64,
) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    let mut height = start_height - 1;
    
    // Iterate backward until a matching block hash is found
    while height > 0 {
        // Query the stored block hash for the height
        let our_hash: Option<String> = sqlx::query_scalar(
            "SELECT block_hash FROM blocks WHERE height = $1"
        )
        .bind(height)
        .fetch_optional(pool)
        .await?;
        
        if let Some(our_hash) = our_hash {
            // Fetch the canonical block hash via RPC
            let canonical_hash = fetcher.get_block_hash(height)?;
            
            // Return the height if hashes match
            if our_hash == canonical_hash {
                return Ok(height);
            }
        }
        
        height -= 1;
    }
    
    // Return 0 if no matching hash is found
    Ok(0)
}

// Indexes a block’s metadata in the database
async fn index_block(
    tx: &mut PgTransaction<'_>,
    block: &Block,
    height: i64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Compute the block hash
    let block_hash = hex::encode(&sha256d(&{
        let mut bytes = Vec::new();
        block.header.write(&mut bytes)?;
        bytes
    }).0);
    
    // Get the previous block hash
    let prev_hash = hex::encode(&block.header.prev_hash.0);
    
    // Insert block metadata into the database, ignoring duplicates
    sqlx::query(
        r#"
        INSERT INTO blocks (block_hash, height, prev_hash, timestamp)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (block_hash, height) DO NOTHING
        "#
    )
    .bind(&block_hash)
    .bind(height)
    .bind(&prev_hash)
    .bind(block.header.timestamp as i64)
    .execute(&mut **tx)
    .await?;
    
    // Return success
    Ok(())
}
