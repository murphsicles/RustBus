use bitcoinsv_rpc::{Client as RpcClient, RpcApi, BlockHash};
use sv::messages::Block;
use sv::util::Serializable;
use log::info;
use serde_json::Value;
use std::io::Cursor;

pub struct BlockFetcher {
    rpc: RpcClient,
}

impl BlockFetcher {
    pub fn new(rpc_url: &str, rpc_user: &str, rpc_password: &str, _network: super::super::config::Network) -> Result<Self, Box<dyn std::error::Error>> {
        let rpc = RpcClient::new(rpc_url, bitcoinsv_rpc::Auth::UserPass(rpc_user.to_string(), rpc_password.to_string()), None)?;
        info!("Connected to BSV node RPC at {}", rpc_url);
        Ok(BlockFetcher { rpc })
    }

    pub fn fetch_block(&mut self, block_hash: &str) -> Result<(Block, i64), Box<dyn std::error::Error>> {
        let block_hash = BlockHash::from_str(block_hash)?;
        let block_hex: Value = self.rpc.call("getblock", &[block_hash.to_string().into(), 0.into()])?;
        let block_hex = block_hex.as_str().ok_or_else(|| "Expected string for block hex")?;
        let block_bytes = hex::decode(block_hex)?;
        let block = Block::read(&mut Cursor::new(&block_bytes))?;
        let block_json: Value = self.rpc.call("getblock", &[block_hash.to_string().into(), 1.into()])?;
        let height = block_json["height"].as_i64().ok_or_else(|| "Missing height")?;
        info!("Fetched block {} with hash {}", height, block_hash);
        Ok((block, height))
    }

    pub fn get_block_hash(&mut self, height: i64) -> Result<String, Box<dyn std::error::Error>> {
        let hash = self.rpc.get_block_hash(height as u64)?;
        Ok(hash.to_string())
    }
}
