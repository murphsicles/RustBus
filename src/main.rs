use actix_web::{web, App, HttpServer};
use rustbus::config::Config;
use rustbus::graphql::graphiql;
use rustbus::rest::routes::{get_tx, list_txs};
use rustbus::websocket::route::ws_route;
use rustbus::blockchain::indexer::index_blocks;
use rustbus::database::schema::init_db;
use rustbus::metrics::metrics;
use log::{error, info};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use async_graphql_actix_web::{GraphQLRequest, GraphQLResponse};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init();

    let config = Config::from_env().map_err(|e: Box<dyn std::error::Error + Send + Sync + 'static>| {
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.db_url)
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    init_db(&pool, config.start_height + 1_000_000)
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let state = rustbus::AppState::new(pool.clone(), &config)
        .await
        .map_err(|e: Box<dyn std::error::Error + Send + Sync + 'static>| {
            std::io::Error::new(std::io::ErrorKind::Other, e)
        })?;
    let index_config = config.clone();
    let index_state = Arc::new(state.clone());
    tokio::spawn(async move {
        if let Err(e) = index_blocks(index_config, pool, index_state).await {
            error!("Index blocks task failed: {}", e);
        }
    });

    let metrics_config = config.clone();
    tokio::spawn(async move {
        let server = HttpServer::new(|| {
            App::new().route("/metrics", web::get().to(metrics))
        })
        .bind(("0.0.0.0", metrics_config.metrics_port))
        .map_err(|e| {
            error!("Metrics server bind failed: {}", e);
            e
        });
        if let Ok(server) = server {
            if let Err(e) = server.run().await {
                error!("Metrics server failed: {}", e);
            }
        }
    });

    info!("Starting HTTP server at {}", config.bind_addr);
    HttpServer::new(move || {
        let state_data = web::Data::new(state.clone());
        App::new()
            .app_data(state_data.clone())
            .route("/ws", web::get().to(ws_route))
            .service(get_tx)
            .service(list_txs)
            .service(
                web::resource("/graphql")
                    .app_data(state_data.clone())
                    .route(web::post().to(|state: web::Data<rustbus::AppState>, req: GraphQLRequest| async move {
                        let schema = state.schema.clone();
                        GraphQLResponse::from(schema.execute(req.into_inner()).await)
                    }))
                    .route(web::get().to(|| async { graphiql().await })),
            )
    })
    .bind(&config.bind_addr)?
    .run()
    .await
}
