use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use rocket_sync_db_pools::{database, postgres};
use serde::{Deserialize, Serialize};
use web3::transports::WebSocket;

pub mod config;
pub mod db;
pub mod errors;
pub mod monitoring;

#[database("pg")]
// #[derive(Debug)]
pub struct DbConn(postgres::Client);
#[derive(Debug, Serialize, Deserialize)]
pub struct IPFSAddResponse {
    #[serde(alias = "Name")]
    pub name: String,
    #[serde(alias = "Hash")]
    pub hash: Option<String>,
    #[serde(alias = "Size")]
    pub size: Option<String>,
    #[serde(alias = "Bytes")]
    pub bytes: Option<u64>,
    pub first_import: Option<bool>,
}

// impl IPFSAddResponse {
//     pub fn default() -> Self {
//         Self {
//             name: "".to_owned(),
//             hash: Option::None,
//             size: Option::None,
//             bytes: Option::None,
//             first_import: Option::None,
//         }
//     }
// }

#[derive(Debug, Clone)]
pub struct Web3Node {
    pub contract_address: String,
    pub url: String,
    pub chain_name: String,
    pub start_block: i64,
    pub block_time_sec: u64,
    pub block_update_sec: u64,
    pub provider_id: i64,
    pub chain_id: i64, //postgres takes i64
    pub batch_size: i64,
    pub log_update_sec: u64,
    pub skip_old: Option<bool>,
    pub web3: Arc<Mutex<web3::Web3<WebSocket>>>,
    pub latest_block: Arc<Mutex<Option<i64>>>,
}

#[derive(Debug, Clone)]
pub struct CIDInfo {
    pub chain_id: Option<i64>,
    pub cid: Option<String>,
    pub end_block: Option<i64>,
    pub node: Option<String>,       // used for failed pin service
    pub node_login: Option<String>, // used for failed pin service
    pub node_pass: Option<String>,  // used for failed pin service
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IpfsDagStat {
    #[serde(alias = "NumBlocks")]
    pub num_blocks: i64,
    #[serde(alias = "Size")]
    pub size: u64,
}

// #[derive(Debug)]
// pub struct BlockNum{
//     chain_id: u64,
//     block: Arc<Mutex<u64>>
// }
#[derive(Debug)]
pub struct State {
    pub nodes: Arc<Vec<config::IPFSNode>>,
    pub providers: Arc<Vec<Web3Node>>,
    pub admin_secret: String,
    pub monitoring: Arc<Mutex<HashMap<u64, monitoring::Monitoring>>>, // block_numbers: Vec<BlockNum>
}
