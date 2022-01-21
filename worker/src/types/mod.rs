use std::sync::Arc;

use rocket_sync_db_pools::{database, postgres};
use serde::{Serialize, Deserialize};
use web3::transports::WebSocket;
pub mod db;
pub mod errors;

#[database("pg")]
#[derive(Debug)]
pub struct DbConn(postgres::Client);
#[derive(Debug, Serialize, Deserialize)]
pub struct IPFSAddResponse{
    #[serde(alias = "Name")]
    pub name: String,
    #[serde(alias = "Hash")]
    pub hash: Option<String>,
    #[serde(alias = "Size")]
    pub size: Option<String>,
    #[serde(alias = "Bytes")]
    pub bytes: Option<u64>
}

impl IPFSAddResponse{
    pub fn default()->Self{
        Self{
            name: "".to_owned(),
            hash: Option::None,
            size: Option::None,
            bytes: Option::None
        }
    }
}

#[derive(Debug, Clone)]
pub struct Web3Node{
    pub contract_address: String,
    pub chain_name: String,
    pub start_block: i64,
    pub block_time_sec: u64,
    pub update_interval_sec: u64,
    pub provider_id: i64,
    pub chain_id: i64, //postgres takes i64
    pub web3: Arc<web3::Web3<WebSocket>>
}