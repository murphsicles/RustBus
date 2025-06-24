pub mod schema;
pub mod routes;

pub use schema::QueryRoot;
pub use routes::{graphql, graphiql};
use sqlx::postgres::PgPool;
use async_graphql::{Schema, EmptyMutation, EmptySubscription};
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::broadcast;
use super::models::{IndexedTx, Subscription};

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub subscriptions: Arc<DashMap<String, Subscription>>,
    pub tx_channel: broadcast::Sender<IndexedTx>,
    pub schema: Schema<QueryRoot, EmptyMutation, EmptySubscription>,
}

impl AppState {
    pub async fn new(pool: PgPool, _config: &super::config::Config) -> Result<Self, Box<dyn std::error::Error>> {
        let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
            .data(pool.clone())
            .finish();
        Ok(AppState {
            db_pool: pool,
            subscriptions: Arc::new(DashMap::new()),
            tx_channel: broadcast::channel(100).0,
            schema,
        })
    }
}
