use rocket::{
    fairing::{Fairing, Info, Kind},
    tokio, Orbit, Rocket,
};
use std::sync::Arc;

use crate::types::{CIDInfo, DbConn, Web3Node};
use crate::{
    db,
    types::{config::IPFSNode, State},
};

pub async fn pin_chain_cids(
    provider: Web3Node,
    psql: Arc<DbConn>,
    nodes: Arc<Vec<IPFSNode>>,
    update_interval: u64,
) {
    loop {
        let bn = { provider.latest_block.clone().lock().unwrap().clone() };

        let bn = match bn {
            Some(v) => v,
            None => {
                error!("CHAIN '{}' - '{}' > latest block is 'None', will sleep for '{}' sec. and try again", 
                &provider.chain_name, &provider.chain_id, update_interval);
                tokio::time::sleep(tokio::time::Duration::from_secs(update_interval)).await;
                continue;
            }
        };

        let (cn, c_id) = (provider.chain_name.clone(), provider.chain_id.clone());
        match psql
            .run(move |client| {
                //update pinned cids valid block number
                let r = db::update_existing_cids_end_block(client, c_id, bn)?;

                info!(
                    "CHAIN '{}' - '{}' > UPDATED 'end block' number for pinned CIDs, total: '{}'",
                    &cn, c_id, r
                );

                //collect new cids to pin
                db::get_new_cids(client, c_id, bn)
            })
            .await
        {
            Ok(v) => {
                info!(
                    "CHAIN '{}' - '{}' > CIDs to pin, total: '{}'",
                    &provider.chain_name,
                    &provider.chain_id,
                    v.len()
                );
                for cid in v {
                    // let c = (Arc::new(cid)).clone();
                    pin_unpin_cid(psql.clone(), nodes.clone(), cid, true).await;
                }
            }
            Err(e) => {
                error!(
                    "{} - {} : Error getting new CIDs to Pin: {}",
                    &provider.chain_name, &provider.chain_id, e
                )
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(update_interval)).await;
    }
}

pub async fn pin_unpin_cid(
    psql: Arc<DbConn>,
    nodes: Arc<Vec<IPFSNode>>,
    cid_info: CIDInfo,
    pin: bool,
) {
    for node in &*nodes {
        let mut c = cid_info.clone();
        c.node_login = node.login.clone();
        c.node_pass = node.password.clone();
        if pin {
            // no break, need to pin to all nodes
            c.node = Option::Some(node.api_url.clone());
            pin_cid_to_node(psql.clone(), c, true).await;
        } else {
            if c.node.as_ref().unwrap().clone().eq(&node.api_url) {
                unpin_cid_from_node(psql.clone(), c).await;
                return;
            }
        }
    }
}

pub async fn retry_failed_cids(provider: Web3Node, psql: Arc<DbConn>, update_interval: u64) {
    loop {
        let bn = { provider.latest_block.clone().lock().unwrap().clone() };

        let bn = match bn {
            Some(v) => v,
            None => {
                error!("CHAIN '{}' - '{}' > latest block is 'None', will sleep for '{}' sec. and try again", 
                &provider.chain_name, &provider.chain_id, update_interval);
                tokio::time::sleep(tokio::time::Duration::from_secs(update_interval)).await;
                continue;
            }
        };
        let (cn, c_id) = (provider.chain_name.clone(), provider.chain_id.clone());
        match psql
            .run(move |client| {
                let r = db::delete_expired_failed_cids(client, c_id, bn)?;
                info!(
                    "CHAIN '{}' - '{}' > DELETED expired, failed CIDs, total: '{}'",
                    &cn, &c_id, r
                );

                db::get_failed_cids(client, c_id, bn)
            })
            .await
        {
            Ok(v) => {
                info!(
                    "CHAIN '{}' - '{}' > Got failed CIDs to pin, total: {}",
                    &provider.chain_name,
                    &provider.chain_id,
                    v.len()
                );
                for cid in v {
                    // let c = (Arc::new(cid)).clone();
                    // self.pin_unpin_cid(c).await;
                    pin_cid_to_node(psql.clone(), cid, false).await;
                }
            }
            Err(e) => {
                error!(
                    "CHAIN '{}' - '{}' > ERROR getting failed CIDs to Pin: {}",
                    &provider.chain_name, &provider.chain_id, e
                )
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(update_interval)).await;
    }
}

pub async fn unpin_cids(
    provider: Web3Node,
    psql: Arc<DbConn>,
    nodes: Arc<Vec<IPFSNode>>,
    update_interval: u64,
) {
    loop {
        let bn = { provider.latest_block.clone().lock().unwrap().clone() };

        let bn = match bn {
            Some(v) => v,
            None => {
                error!("CHAIN '{}' - '{}' > latest block is 'None', will sleep for '{}' sec. and try again", 
                &provider.chain_name, &provider.chain_id, update_interval);
                tokio::time::sleep(tokio::time::Duration::from_secs(update_interval)).await;
                continue;
            }
        };
        let (cn, c_id) = (provider.chain_name.clone(), provider.chain_id.clone());
        match psql
            .run(move |client| {
                let res = db::delete_multichain_expired_cids(client, c_id, bn)?;
                info!(
                    "CHAIN '{}' - '{}' > DELETED '{}' multi-chain expired CIDs",
                    cn, &c_id, res
                );

                db::get_single_chain_expired_cids(client, c_id, bn)
            })
            .await
        {
            Ok(v) => {
                info!(
                    "CHAIN '{}' - '{}' > CIDs to unpin, total: {}",
                    &provider.chain_name,
                    &provider.chain_id,
                    v.len()
                );
                for cid in v {
                    pin_unpin_cid(psql.clone(), nodes.clone(), cid, false).await;
                }
            }
            Err(e) => {
                error!(
                    "CHAIN '{}' - '{}' > Error getting new CIDs to unpin: {}",
                    &provider.chain_name, &provider.chain_id, e
                )
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(update_interval)).await;
    }
}

async fn add_failed_pin_to_db(
    psql: Arc<DbConn>,
    chain_id: i64,
    block: i64,
    cid: String,
    node: String,
) {
    psql.run(move |client| {
        match db::add_failed_pin(client, chain_id, &node, &cid, block) {
            Ok(_) => {
                warn!(
                    "CHAIN '{}' > FAILED to pin '{}' to NODE '{}' expiration block '{}'",
                    &chain_id, &cid, &node, &block
                )
            }
            Err(e) => {
                error!(
                    "CHAIN '{}' > ERROR inserting cid {} to failed_pins: {}",
                    &chain_id, &cid, e
                )
            }
        };
    })
    .await;
}

async fn pin_cid_to_node(psql: Arc<DbConn>, c: CIDInfo, store_failed: bool) {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let node = c.node.unwrap();
        let cid = c.cid.unwrap();
        let chain_id = c.chain_id.unwrap();
        let block = c.end_block.unwrap();
        match client
            .post(format!("{}/api/v0/pin/add?arg={}", &node, &cid))
            .send()
            .await
        {
            Ok(v) => {
                if !v.status().is_success() {
                    error!(
                        "CHAIN '{}' > ERROR pinning cid '{}' to node '{}'",
                        &chain_id, &cid, &node
                    );
                    if store_failed {
                        add_failed_pin_to_db(psql, chain_id, block, cid, node).await;
                    }
                    return;
                }

                let (n, c_id) = (node.clone(), cid.clone());
                match psql
                    .run(move |client| db::add_cid(client, chain_id, n, c_id, block))
                    .await
                {
                    Ok(_) => {
                        info!(
                            "CHAIN '{}' > PINNED '{}' to 'NODE' {} till block '{}'",
                            &chain_id, &cid, &node, &block
                        )
                    }
                    Err(e) => {
                        error!(
                            "CHAIN '{}' > ERROR inserting cid '{}' to pinned_cids: '{}'",
                            &chain_id, &cid, e
                        )
                    }
                };
            }

            Err(e) => {
                error!(
                    "CHAIN '{}' > ERROR pinning cid '{}' to node '{}' : '{}'",
                    &chain_id, &cid, &node, e
                );
                if store_failed {
                    add_failed_pin_to_db(psql, chain_id, block, cid, node).await;
                }
            }
        }
    });
}

async fn unpin_cid_from_node(psql: Arc<DbConn>, c: CIDInfo) {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let node = c.node.unwrap();
        let cid = c.cid.unwrap();
        let chain_id = c.chain_id.unwrap();
        let block = c.end_block.unwrap();
        match client
            .post(format!("{}/api/v0/pin/rm?arg={}", &node, &cid))
            .send()
            .await
        {
            Ok(v) => {
                if !v.status().is_success() {
                    error!("ERROR unpinning cid {} from node {}", &cid, &node);
                    return;
                }
                let (n, c_id) = (node.clone(), cid.clone());
                match psql
                    .run(move |client| db::delete_cid(client, chain_id, n, c_id, block))
                    .await
                {
                    Ok(_) => {
                        info!(
                            "CHAIN '{}' > UNPINNED '{}' from NODE '{}'",
                            &chain_id, &cid, &node
                        )
                    }
                    Err(e) => {
                        error!(
                            "CHAIN '{}' > ERROR deleting cid '{}' from pinned_cids: '{}'",
                            &chain_id, &cid, e
                        )
                    }
                };
            }

            Err(e) => {
                error!("ERROR unpinning cid {} from node {} : {}", &cid, &node, e);
            }
        }
    });
}

#[derive(Debug, Clone)]
pub struct IPFSService {
    pub retry_failed_cids_sec: u64,
    pub update_nodes_sec: u64,
}

#[rocket::async_trait]
impl Fairing for IPFSService {
    fn info(&self) -> Info {
        Info {
            name: "Run contract watcher service",
            kind: Kind::Liftoff,
        }
    }

    async fn on_liftoff(&self, rocket: &Rocket<Orbit>) {
        let db = Arc::new(DbConn::get_one(&rocket).await.expect("database mounted."));

        let state = rocket.state::<State>().unwrap();
        // let nodes = rocket.state::<Arc<Vec<IPFSNode>>>().unwrap();
        // let providers = rocket.state::<Arc<Vec<types::Web3Node>>>().unwrap().clone();
        // let shutdown = rocket.shutdown();

        for provider in &*state.providers.clone() {
            let (p, psql, n, ut) = (
                provider.clone(),
                db.clone(),
                state.nodes.clone(),
                self.update_nodes_sec,
            );
            tokio::spawn(async move { pin_chain_cids(p, psql, n, ut).await });
            // spawn failed pins retry
            let (p, psql, ut) = (
                provider.clone(),
                db.clone(),
                self.retry_failed_cids_sec.clone(),
            );
            tokio::spawn(async move { retry_failed_cids(p, psql, ut).await });
            // spawn unpin
            let (p, psql, n, ut) = (
                provider.clone(),
                db.clone(),
                state.nodes.clone(),
                self.update_nodes_sec,
            );
            tokio::spawn(async move { unpin_cids(p, psql, n, ut).await });
        }
    }
}
