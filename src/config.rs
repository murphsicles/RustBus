use std::env;
use sv::network::Network;

// Configuration for the Bitcoin SV blockchain indexer
#[derive(Clone)]
pub struct Config {
    pub db_url: String, // Database connection URL
    pub bsv_node: String, // Bitcoin SV node address
    pub zmq_addr: String, // ZMQ address for block notifications
    pub network: Network, // Bitcoin SV network (Mainnet or Testnet)
    pub start_height: i64, // Starting block height for indexing
    pub metrics_port: u16, // Port for Prometheus metrics server
    pub bind_addr: String, // Address for main HTTP server
    pub rpc_url: String, // Bitcoin SV RPC URL
    pub rpc_user: String, // RPC username
    pub rpc_password: String, // RPC password
}

impl Config {
    // Loads configuration from environment variables, with defaults for optional fields
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        // Construct the configuration struct
        let config = Config {
            // Required: Database connection URL
            db_url: env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set")?,
            // Optional: Bitcoin SV node address (defaults to localhost:8333)
            bsv_node: env::var("BSV_NODE").unwrap_or("127.0.0.1:8333".to_string()),
            // Optional: ZMQ address (defaults to tcp://127.0.0.1:28332)
            zmq_addr: env::var("ZMQ_ADDR").unwrap_or("tcp://127.0.0.1:28332".to_string()),
            // Optional: Network (defaults to Mainnet)
            network: match env::var("NETWORK").unwrap_or("mainnet".to_string()).as_str() {
                "testnet" => Network::Testnet,
                _ => Network::Mainnet,
            },
            // Optional: Starting block height (defaults to 0)
            start_height: env::var("START_HEIGHT").unwrap_or("0".to_string()).parse().unwrap_or(0),
            // Optional: Metrics server port (defaults to 9090)
            metrics_port: env::var("METRICS_PORT").unwrap_or("9090".to_string()).parse().unwrap_or(9090),
            // Optional: Main server bind address (defaults to 0.0.0.0:8080)
            bind_addr: env::var("BIND_ADDR").unwrap_or("0.0.0.0:8080".to_string()),
            // Required: Bitcoin SV RPC URL
            rpc_url: env::var("BSV_RPC_URL").unwrap_or("http://127.0.0.1:8332".to_string()),
            // Required: RPC username
            rpc_user: env::var("BSV_RPC_USER").unwrap_or_default(),
            // Required: RPC password
            rpc_password: env::var("BSV_RPC_PASSWORD").unwrap_or_default(),
        };

        // Validate required fields
        if config.db_url.is_empty() || config.rpc_url.is_empty() || config.rpc_user.is_empty() || config.rpc_password.is_empty() {
            return Err("DATABASE_URL, BSV_RPC_URL, BSV_RPC_USER, and BSV_RPC_PASSWORD must be set".into());
        }

        // Return the validated configuration
        Ok(config)
    }
}
