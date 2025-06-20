use actix_web::{web, HttpResponse};
use super::super::database::queries::{get_transaction, list_transactions};
use super::super::AppState;

#[actix_web::get("/tx/{txid}")]
pub async fn get_tx(
    path: web::Path<String>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let txid = path.into_inner();
    let tx = get_transaction(&state.db_pool, &txid)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    match tx {
        Some(tx) => Ok(HttpResponse::Ok().json(tx)),
        None => Ok(HttpResponse::NotFound().body("Transaction not found")),
    }
}

#[actix_web::get("/txs")]
pub async fn list_txs(
    query: web::Query<std::collections::HashMap<String, String>>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let tx_type = query.get("type").cloned();
    let height = query.get("height").and_then(|h| h.parse::<i64>().ok());
    let limit = query.get("limit").and_then(|l| l.parse::<i32>().ok());

    let txs = list_transactions(&state.db_pool, tx_type, height, limit)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    Ok(HttpResponse::Ok().json(txs))
}
