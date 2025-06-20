use actix_web::{web, App, HttpServer};
use rustbus::config::Config;
use rustbus::graphql::{graphql, graphiql};
use rustbus::rest::routes::{get_tx, list_txs};
use rustbus::websocket::route::ws_route;
use rustbus::blockchain::indexer::index_blocks;
use rustbus::database::schema::init_db;
use rustbus::metrics::metrics;
use log::{error, info};
use sqlx::postgres::PgPoolOptions;
use std::env;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init();

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.db_url)
        .await
        .expect("Failed to connect to database");
    init_db(&pool, config.start_height + 1_000_000).await?;

    let state = rustbus::AppState::new(pool, &config).await?;
    let index_config = config.clone();
    let index_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = index_blocks(index_config, index_state).await {
            error!("Index blocks task failed: {}", e);
        }
    });

    let metrics_config = config.clone();
    tokio::spawn(async move {
        if let Err(e) = HttpServer::new(|| {
            App::new()
                .route("/metrics", web::get().to(metrics))
        })
        .bind(("0.0.0.0", metrics_config.metrics_port))
        .await
        .and_then(|server| server.run())
        {
            error!("Metrics server failed: {}", e);
        }
    });

    info!("Starting HTTP server at {}", config.bind_addr);
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/ws", web::get().to(ws_route))
            .service(get_tx)
            .service(list_txs)
            .service(web::resource("/graphql").route(web::post().to(graphql)).route(web::get().to(graphiql)))
    })
    .bind(&config.bind_addr)?
    .run()
    .await
}
