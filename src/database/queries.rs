use sqlx::{Pool, Postgres};
use super::super::models::{IndexedTx, BlockHeader};

pub async fn get_transaction(pool: &Pool<Postgres>, txid: &str) -> Result<Option<IndexedTx>, sqlx::Error> {
    sqlx::query_as(
        "SELECT txid, block_height, tx_type, op_return, tx_hex FROM transactions WHERE txid = $1"
    )
    .bind(txid)
    .fetch_optional(pool)
    .await
}

pub async fn list_transactions(
    pool: &Pool<Postgres>,
    tx_type: Option<String>,
    block_height: Option<i64>,
    limit: Option<i32>,
) -> Result<Vec<IndexedTx>, sqlx::Error> {
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

    query_builder.build_query_as().fetch_all(pool).await
}

pub async fn get_block(pool: &Pool<Postgres>, height: i64) -> Result<Option<BlockHeader>, sqlx::Error> {
    sqlx::query_as(
        "SELECT block_hash, height, prev_hash FROM blocks WHERE height = $1"
    )
    .bind(height)
    .fetch_optional(pool)
    .await
}
