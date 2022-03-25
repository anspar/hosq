use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct Event {
    pub event: String,
    pub last_update: i64,
    pub update_block: i64,
    pub update_duration: u64,
    pub count: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct Monitoring {
    pub current_block: u64,
    pub socket_create_time: i64,
    pub chain_name: String,
    pub events: Vec<Event>,
}

impl Monitoring {
    pub fn default() -> Self {
        Self {
            current_block: 0,
            socket_create_time: 0,
            chain_name: "".to_owned(),
            events: vec![],
        }
    }
}
