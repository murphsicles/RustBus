use actix_web::HttpResponse;
use prometheus::{register_counter, register_gauge, register_histogram, Counter, Gauge, Histogram};

lazy_static::lazy_static! {
    pub static ref TXS_INDEXED: Counter = register_counter!("rustbus_txs_indexed_total", "Total transactions indexed").unwrap();
    pub static ref BLOCK_PROCESS_TIME: Histogram = register_histogram!("rustbus_block_process_seconds", "Block processing time in seconds").unwrap();
    pub static ref ACTIVE_SUBS: Gauge = register_gauge!("rustbus_active_subscriptions", "Number of active WebSocket subscriptions").unwrap();
}

pub async fn metrics() -> HttpResponse {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = prometheus::gather();
    let encoded = encoder.encode_to_string(&metric_families).unwrap_or_default();
    HttpResponse::Ok().body(encoded)
}
