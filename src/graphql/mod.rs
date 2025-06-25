// GraphQL module for defining the application state and GraphQL schema
pub mod schema;
pub mod routes;

// Re-export key types for external use
pub use schema::QueryRoot;
pub use routes::{graphql, graphiql};
use sqlx::postgres::PgPool;
use async_graphql::{Schema, EmptyMutation, EmptySubscription};
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::broadcast;
use super::models::{IndexedTx, Subscription};

// Application state shared across GraphQL and WebSocket handlers
#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool, // Database connection pool for queries
    pub subscriptions: Arc<DashMap<String, Subscription>>, // Concurrent map of active subscriptions
    pub tx_channel: broadcast::Sender<IndexedTx>, // Broadcast channel for transaction notifications
    pub schema: Schema<QueryRoot, EmptyMutation, EmptySubscription>, // GraphQL schema instance
}

impl AppState {
    // Initializes the application state with database and GraphQL configuration
    pub async fn new(pool: PgPool, _config: &super::config::Config) -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        // Build the GraphQL schema with the query root and database pool
        let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
            .data(pool.clone()) // Inject database pool for query execution
            .finish();

        // Create the application state
        Ok(AppState {
            db_pool: pool, // Store the database pool
            subscriptions: Arc::new(DashMap::new()), // Initialize empty subscription map
            tx_channel: broadcast::channel(100).0, // Create broadcast channel with capacity 100
            schema, // Store the GraphQL schema
        })
    }
}
