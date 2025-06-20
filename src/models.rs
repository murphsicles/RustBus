use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, async_graphql::SimpleObject, sqlx::FromRow)]
pub struct IndexedTx {
    pub txid: String,
    pub block_height: i64,
    pub tx_type: String,
    pub op_return: Option<String>,
    pub tx_hex: String,
}

#[derive(Debug, Serialize, Deserialize, async_graphql::SimpleObject, sqlx::FromRow)]
pub struct BlockHeader {
    pub block_hash: String,
    pub height: i64,
    pub prev_hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Subscription {
    pub client_id: String,
    pub filter_type: Option<String>,
    pub op_return_pattern: Option<String>,
}
