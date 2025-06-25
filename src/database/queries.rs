use sqlx::{Pool, Postgres};
use super::super::models::{IndexedTx, BlockHeader};

// Queries a single transaction by its transaction ID (txid) from the database
pub async fn get_transaction(pool: &Pool<Postgres>, txid: &str) -> Result<Option<IndexedTx>, sqlx::Error> {
    // Execute an SQL query to fetch the transaction by txid
    sqlx::query_as(
        "SELECT txid, block_height, tx_type, op_return, tx_hex FROM transactions WHERE txid = $1"
    )
    .bind(txid)
    .fetch_optional(pool)
    .await
    // Return the transaction if found, or None if no match
}

// Queries a filtered and paginated list of transactions from the database
pub async fn list_transactions(
    pool: &Pool<Postgres>,
    tx_type: Option<String>,
    block_height: Option<i64>,
    limit: Option<i32>,
) -> Result<Vec<IndexedTx>, sqlx::Error> {
    // Apply pagination limit with default 100, clamped between 1 and 1000
    let limit = limit.unwrap_or(100).clamp(1, 1000);
    
    // Build a dynamic SQL query for transactions with optional filters
    let mut query_builder = sqlx::QueryBuilder::new(
        "SELECT txid, block_height, tx_type, op_return, tx_hex FROM transactions WHERE 1=1"
    );
    
    // Add filter for transaction type, if specified
    if let Some(t) = tx_type {
        query_builder.push(" AND tx_type = ");
        query_builder.push_bind(t);
    }
    
    // Add filter for block height, if specified
    if let Some(h) = block_height {
        query_builder.push(" AND block_height = ");
        query_builder.push_bind(h);
    }
    
    // Add pagination limit
    query_builder.push(" LIMIT ");
    query_builder.push_bind(limit);

    // Execute the query and fetch all matching transactions
    query_builder.build_query_as().fetch_all(pool).await
    // Return the list of transactions
}

// Queries a block by its height from the database
pub async fn get_block(pool: &Pool<Postgres>, height: i64) -> Result<Option<BlockHeader>, sqlx::Error> {
    // Execute an SQL query to fetch the block by height
    sqlx::query_as(
        "SELECT block_hash, height, prev_hash FROM blocks WHERE height = $1"
    )
    .bind(height)
    .fetch_optional(pool)
    .await
    // Return the block if found, or None if no match
}
