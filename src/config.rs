use std::env;
use sv::network::Network;

#[derive(Clone)]
pub struct Config {
    pub db_url: String,
    pub bsv_node: String,
    pub zmq_addr: String,
    pub network: Network,
    pub start_height: i64,
    pub metrics_port: u16,
    pub bind_addr: String,
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_password: String,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let config = Config {
            db_url: env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set")?,
            bsv_node: env::var("BSV_NODE").unwrap_or("127.0.0.1:8333".to_string()),
            zmq_addr: env::var("ZMQ_ADDR").unwrap_or("tcp://127.0.0.1:28332".to_string()),
            network: match env::var("NETWORK").unwrap_or("mainnet".to_string()).as_str() {
                "testnet" => Network::Testnet,
                _ => Network::Mainnet,
            },
            start_height: env::var("START_HEIGHT").unwrap_or("0".to_string()).parse().unwrap_or(0),
            metrics_port: env::var("METRICS_PORT").unwrap_or("9090".to_string()).parse().unwrap_or(9090),
            bind_addr: env::var("BIND_ADDR").unwrap_or("0.0.0.0:8080".to_string()),
            rpc_url: env::var("BSV_RPC_URL").unwrap_or("http://127.0.0.1:8332".to_string()),
            rpc_user: env::var("BSV_RPC_USER").unwrap_or_default(),
            rpc_password: env::var("BSV_RPC_PASSWORD").unwrap_or_default(),
        };

        if config.db_url.is_empty() || config.rpc_url.is_empty() || config.rpc_user.is_empty() || config.rpc_password.is_empty() {
            return Err("DATABASE_URL, BSV_RPC_URL, BSV_RPC_USER, and BSV_RPC_PASSWORD must be set".into());
        }
        Ok(config)
    }
}
