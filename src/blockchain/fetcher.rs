use bitcoinsv_rpc::{Client as RpcClient, RpcApi};
use bitcoinsv::bitcoin::BlockHash;
use sv::messages::Block;
use sv::network::Network;
use sv::util::Serializable;
use log::info;
use serde_json::{Value, to_value};
use std::io::Cursor;

fn into_json<T: serde::Serialize>(val: T) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    Ok(to_value(val)?)
}

pub struct BlockFetcher {
    rpc: RpcClient,
}

impl BlockFetcher {
    pub fn new(rpc_url: &str, rpc_user: &str, rpc_password: &str, _network: Network) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let rpc = RpcClient::new(rpc_url, bitcoinsv_rpc::Auth::UserPass(rpc_user.to_string(), rpc_password.to_string()), None)?;
        info!("Connected to BSV node RPC at {}", rpc_url);
        Ok(BlockFetcher { rpc })
    }

    pub fn fetch_block(&mut self, block_hash: &str) -> Result<(Block, i64), Box<dyn std::error::Error + Send + Sync>> {
        let block_bytes = hex::decode(block_hash)?;
        let block_hash = BlockHash::from(&block_bytes[..]);
        let block_hex: Value = self.rpc.call("getblock", &[into_json(block_hash)?, 0.into()])?;
        let block_hex = block_hex.as_str().ok_or_else(|| "Expected string for block hex")?;
        let block_bytes = hex::decode(block_hex)?;
        let block = Block::read(&mut Cursor::new(&block_bytes))?;
        let block_json: Value = self.rpc.call("getblock", &[into_json(block_hash)?, 1.into()])?;
        let height = block_json["height"].as_i64().ok_or_else(|| "Missing height")?;
        info!("Fetched block {} with hash {}", height, block_hash);
        Ok((block, height))
    }

    pub fn get_block_hash(&mut self, height: i64) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let hash = self.rpc.get_block_hash(height as u64)?;
        Ok(hash.to_string())
    }

    pub fn get_best_block_height(&mut self) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let hash = self.rpc.get_best_block_hash()?;
        let block: Value = self.rpc.call("getblock", &[into_json(hash)?, 1.into()])?;
        let height = block["height"].as_i64().ok_or_else(|| "Missing height")?;
        Ok(height)
    }
}
