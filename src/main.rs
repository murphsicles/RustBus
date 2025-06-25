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
use tokio::select;

async fn graphiql_handler() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init();

    let config = Config::from_env().map_err(|e| {
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
        .map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e)
        })?;
    let index_config = config.clone();
    let index_state = Arc::new(state.clone());
    let indexer_task = tokio::spawn(async move {
        if let Err(e) = index_blocks(index_config, pool, index_state).await {
            error!("Index blocks task failed: {}", e);
        }
    });

    let metrics_config = config.clone();
    let metrics_server = HttpServer::new(|| {
        App::new().route("/metrics", web::get().to(metrics))
    })
    .bind(("0.0.0.0", metrics_config.metrics_port))?;

    let main_server = HttpServer::new(move || {
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
                    .route(web::get().to(graphiql_handler)),
            )
    })
    .bind(&config.bind_addr)?;

    info!("Starting HTTP server at {}", config.bind_addr);
    info!("Starting metrics server at 0.0.0.0:{}", metrics_config.metrics_port);

    select! {
        result = main_server.run() => {
            if let Err(e) = result {
                error!("Main server failed: {}", e);
                return Err(e);
            }
        }
        result = metrics_server.run() => {
            if let Err(e) = result {
                error!("Metrics server failed: {}", e);
                return Err(e);
            }
        }
        _ = indexer_task => {
            error!("Indexer task completed unexpectedly");
        }
    }

    Ok(())
}
