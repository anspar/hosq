use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventUpdateValidBlock{
    pub chain_id: i64,
    pub cid: String, 
    pub donor: String,
    pub update_block: i64,
    pub end_block: i64,
    pub manual_add: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventAddProvider{
    pub chain_id: i64,
    pub update_block: i64,
    pub owner: String,
    pub provider_id: i64,
    pub block_price_gwei: i64,
    pub api_url: String, 
    pub name: String, 
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedCIDs{
    pub update_block: i64,
    pub donor: String,
    pub end_block: i64,
    pub cid: String, 
    pub node_count: i64,
    pub failed_node_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventAddProviderResponse{
    pub provider_id: i64,
    pub block_price_gwei: i64,
    pub api_url: String, 
    pub name: String, 
    pub update_block: Option<i64>
}
