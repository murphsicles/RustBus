use actix_web::{web, App, HttpServer};
use rustbus::config::Config;
use rustbus::rest::routes::{get_tx, list_txs};
use rustbus::websocket::route::ws_route;
use rustbus::blockchain::indexer::index_blocks;
use rustbus::database::schema::init_db;
use rustbus::metrics::metrics;
use log::{error, info};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use async_graphql_actix_web::{GraphQLRequest, GraphQLResponse};
use actix_web::HttpResponse;
use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use tokio::try_join;

// Serves the GraphiQL UI for interacting with the GraphQL API
async fn graphiql_handler() -> HttpResponse {
    // Returns an HTML response with the GraphiQL interface, configured to query /graphql
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load environment variables from .env file (e.g., DATABASE_URL, BSV_RPC_URL)
    dotenvy::dotenv().ok();
    // Initialize the logger for application-wide logging
    env_logger::init();

    // Load configuration from environment variables (e.g., database URL, RPC credentials)
    let config = Config::from_env().map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;

    // Create a PostgreSQL connection pool with up to 10 connections
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.db_url)
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // Initialize the database schema, creating tables for blocks and transactions
    init_db(&pool, config.start_height + 1_000_000)
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // Initialize the application state, including the database pool and GraphQL schema
    let state = rustbus::AppState::new(pool.clone(), &config)
        .await
        .map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e)
        })?;

    // Spawn the blockchain indexer task to process historical and new blocks
    let index_config = config.clone();
    let index_state = Arc::new(state.clone());
    let indexer_task = tokio::spawn(async move {
        // Indexes blocks using RPC and ZMQ, storing data in the database
        if let Err(e) = index_blocks(index_config, pool, index_state).await {
            error!("Index blocks task failed: {}", e);
        }
    });

    // Configure the metrics server to expose Prometheus metrics
    let metrics_config = config.clone();
    let metrics_server = HttpServer::new(|| {
        App::new()
            // Route /metrics to serve Prometheus metrics (e.g., TXS_INDEXED, BLOCK_PROCESS_TIME)
            .route("/metrics", web::get().to(metrics))
    })
    .bind(("0.0.0.0", metrics_config.metrics_port))?;

    // Configure the main HTTP server for REST, WebSocket, and GraphQL endpoints
    let main_server = HttpServer::new(move || {
        let state_data = web::Data::new(state.clone());
        App::new()
            // Share application state (database pool, GraphQL schema) across routes
            .app_data(state_data.clone())
            // WebSocket endpoint for real-time transaction updates
            .route("/ws", web::get().to(ws_route))
            // REST endpoints for transaction queries
            .service(get_tx)
            .service(list_txs)
            // GraphQL endpoint for querying transactions and blocks
            .service(
                web::resource("/graphql")
                    .app_data(state_data.clone())
                    // POST /graphql for GraphQL queries
                    .route(web::post().to(|state: web::Data<rustbus::AppState>, req: GraphQLRequest| async move {
                        let schema = state.schema.clone();
                        GraphQLResponse::from(schema.execute(req.into_inner()).await)
                    }))
                    // GET /graphql for GraphiQL UI
                    .route(web::get().to(graphiql_handler)),
            )
    })
    .bind(&config.bind_addr)?;

    // Log server startup information
    info!("Starting HTTP server at {}", config.bind_addr);
    info!("Starting metrics server at 0.0.0.0:{}", metrics_config.metrics_port);

    // Run the main and metrics servers concurrently, handling errors
    try_join!(
        async {
            main_server.run().await?;
            Ok::<(), std::io::Error>(())
        },
        async {
            metrics_server.run().await?;
            Ok::<(), std::io::Error>(())
        }
    )?;

    // Wait for the indexer task to complete, logging any errors
    indexer_task.await.map_err(|e| {
        error!("Indexer task failed: {}", e);
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;

    // Return success if all tasks complete without errors
    Ok(())
}
