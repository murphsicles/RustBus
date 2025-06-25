use async_graphql::{Object, Context};
use sqlx::{Pool, Postgres};
use super::super::models::{IndexedTx, BlockHeader};

// Root object for GraphQL queries
pub struct QueryRoot;

// Implements GraphQL query resolvers for transaction and block data
#[Object]
impl QueryRoot {
    // Queries a single transaction by its transaction ID (txid)
    async fn transaction(&self, ctx: &Context<'_>, txid: String) -> async_graphql::Result<Option<IndexedTx>> {
        // Retrieve the PostgreSQL connection pool from the GraphQL context
        let pool = ctx.data_unchecked::<Pool<Postgres>>();
        
        // Execute an SQL query to fetch the transaction by txid
        let tx: Option<IndexedTx> = sqlx::query_as(
            "SELECT txid, block_height, tx_type, op_return, tx_hex FROM transactions WHERE txid = $1"
        )
        .bind(&txid)
        .fetch_optional(pool)
        .await?;
        
        // Return the transaction if found, or None if no match
        Ok(tx)
    }

    // Queries a filtered and paginated list of transactions
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        tx_type: Option<String>,
        block_height: Option<i64>,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<IndexedTx>> {
        // Retrieve the PostgreSQL connection pool from the GraphQL context
        let pool = ctx.data_unchecked::<Pool<Postgres>>();
        
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
        let txs: Vec<IndexedTx> = query_builder
            .build_query_as()
            .fetch_all(pool)
            .await?;
        
        // Return the list of transactions
        Ok(txs)
    }

    // Queries a block by its height
    async fn block(&self, ctx: &Context<'_>, height: i64) -> async_graphql::Result<Option<BlockHeader>> {
        // Retrieve the PostgreSQL connection pool from the GraphQL context
        let pool = ctx.data_unchecked::<Pool<Postgres>>();
        
        // Execute an SQL query to fetch the block by height
        let block: Option<BlockHeader> = sqlx::query_as(
            "SELECT block_hash, height, prev_hash FROM blocks WHERE height = $1"
        )
        .bind(height)
        .fetch_optional(pool)
        .await?;
        
        // Return the block if found, or None if no match
        Ok(block)
    }
}
