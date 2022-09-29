use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct IPFSNode {
    pub api_url: String,
    pub gateway: String,
    pub login: Option<String>,
    pub password: Option<String>,
}
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub providers: Option<Vec<Provider>>,
    pub ipfs_nodes: Option<Vec<IPFSNode>>,
    pub retry_failed_cids_sec: u64,
    pub admin_secret: String,
    pub update_nodes_sec: u64,
    pub only_api: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Provider {
    pub contract_address: String,
    pub provider: String,
    pub chain_name: String,
    pub start_block: i64,
    pub block_time_sec: u64,
    pub block_update_sec: u64,
    pub log_update_sec: u64,
    pub provider_id: i64,
    pub batch_size: i64,
    pub skip_old: Option<bool>,
}
