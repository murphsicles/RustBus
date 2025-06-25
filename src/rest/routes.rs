use actix_web::{web, HttpResponse};
use super::super::database::queries::{get_transaction, list_transactions};
use super::super::AppState;

// Handles GET /tx/{txid} requests to fetch a transaction by its transaction ID
#[actix_web::get("/tx/{txid}")]
pub async fn get_tx(
    path: web::Path<String>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    // Extract the transaction ID from the URL path
    let txid = path.into_inner();
    
    // Query the database for the transaction using the provided txid
    let tx = get_transaction(&state.db_pool, &txid)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    // Return the transaction as JSON if found, or a 404 response if not
    match tx {
        Some(tx) => Ok(HttpResponse::Ok().json(tx)),
        None => Ok(HttpResponse::NotFound().body("Transaction not found")),
    }
}

// Handles GET /txs requests to list transactions with optional filters
#[actix_web::get("/txs")]
pub async fn list_txs(
    query: web::Query<std::collections::HashMap<String, String>>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    // Extract optional query parameters for filtering and pagination
    let tx_type = query.get("type").cloned(); // Filter by transaction type (e.g., RUN)
    let height = query.get("height").and_then(|h| h.parse::<i64>().ok()); // Filter by block height
    let limit = query.get("limit").and_then(|l| l.parse::<i32>().ok()); // Pagination limit

    // Query the database for transactions with the specified filters
    let txs = list_transactions(&state.db_pool, tx_type, height, limit)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    // Return the list of transactions as JSON
    Ok(HttpResponse::Ok().json(txs))
}
