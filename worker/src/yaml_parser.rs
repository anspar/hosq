use std::io::Read;
use serde::{self, Deserialize};

#[derive(Debug, Deserialize, Clone)]
pub struct Providers{
    pub contract_address: Option<String>,
    pub provider: Option<String>,
    pub chain_name: Option<String>,
    pub start_block: Option<i64>,
    pub block_time_sec: Option<u64>,
    pub update_interval_sec: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IPFSNode{
    pub api_url: Option<String>,
    pub login: Option<String>,
    pub password: Option<String>,
}
#[derive(Debug, Deserialize, Clone)]
pub struct Config{
    pub providers: Option<Vec<Providers>>,
    pub ipfs_nodes: Option<Vec<IPFSNode>>,
    pub retry_failed_cids_sec: Option<u64>
}

fn get_file_content(path: &String)->String{
    let mut f = std::fs::File::open(path).expect("error reading the yaml file");
    let mut content: String = String::new();
    f.read_to_string(&mut content).expect("Unable to read yml data"); 
    content
}

pub fn get_conf(path: &String) -> Config{
    let content = get_file_content(path);
    let deserialized_point: Config = serde_yaml::from_str(&content).expect("error parsing yaml");
    // println!("{:?}", deserialized_point);
    deserialized_point 
}
