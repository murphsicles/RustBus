use async_graphql::{Object, Context};
use sqlx::{Pool, Postgres};
use super::super::models::{IndexedTx, BlockHeader};

pub struct QueryRoot;

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
