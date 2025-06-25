use actix_web::HttpResponse;
use prometheus::{register_counter, register_gauge, register_histogram, Counter, Gauge, Histogram};

// Defines Prometheus metrics for monitoring the blockchain indexer
lazy_static::lazy_static! {
    // Counter for the total number of transactions indexed
    pub static ref TXS_INDEXED: Counter = register_counter!(
        "rustbus_txs_indexed_total",
        "Total transactions indexed"
    ).unwrap();

    // Histogram for measuring block processing time in seconds
    pub static ref BLOCK_PROCESS_TIME: Histogram = register_histogram!(
        "rustbus_block_process_seconds",
        "Block processing time in seconds"
    ).unwrap();

    // Gauge for tracking the number of active WebSocket subscriptions
    pub static ref ACTIVE_SUBS: Gauge = register_gauge!(
        "rustbus_active_subscriptions",
        "Number of active WebSocket subscriptions"
    ).unwrap();
}

// Handles GET /metrics requests to expose Prometheus metrics
pub async fn metrics() -> HttpResponse {
    // Create a Prometheus text encoder for formatting metrics
    let encoder = prometheus::TextEncoder::new();
    
    // Gather all registered metric families (e.g., TXS_INDEXED, BLOCK_PROCESS_TIME, ACTIVE_SUBS)
    let metric_families = prometheus::gather();
    
    // Encode the metrics to a text string, defaulting to empty if encoding fails
    let encoded = encoder.encode_to_string(&metric_families).unwrap_or_default();
    
    // Return an HTTP response with the encoded metrics in text/plain format
    HttpResponse::Ok().body(encoded)
}
