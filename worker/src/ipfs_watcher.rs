use std::sync::Arc;
use rocket::{fairing::{Fairing, Info, Kind}, tokio, Orbit, Rocket};
use web3::{Web3, transports::WebSocket};

use crate::{DbConn, yaml_parser::{Providers}};
use crate::yaml_parser::IPFSNode;

#[derive(Debug, Clone)]
struct CIDInfo{
    pub chain_id: Option<i64>,
    pub cid: Option<String>, 
    pub end_block: Option<i64>,
    pub node: Option<String>, // used for failed pin service
    pub node_login: Option<String>, // used for failed pin service
    pub node_pass: Option<String> // used for failed pin service
}

#[derive(Clone)]
struct IPFSWatcher{
    pub db: Arc<DbConn>,
    pub nodes: Arc<Vec<IPFSNode>>,
    pub providers: Arc<Vec<Providers>>,
    pub r_off: rocket::Shutdown,
    pub retry_failed_cids_sec: u64
}

impl IPFSWatcher {
    pub async fn watch_nodes(&self){
        for provider in &*self.providers{
            let chain_name = provider.chain_name.to_owned();
            let update_time = provider.update_interval_sec;
            let transport =  web3::transports::WebSocket::new(&(provider.provider)).await.unwrap();
            let socket = Arc::new(Web3::new(transport));   
            let (me, web3, cn, un) = (self.clone(), socket.clone(), chain_name.clone(), update_time.clone());
            tokio::spawn(async move {me.pin_chain_cids(web3, cn, un).await});
            // spawn failed pins retry
            let (me, web3, cn) = (self.clone(), socket.clone(), chain_name.clone());
            tokio::spawn(async move {me.retry_failed_cids(web3, cn).await});
            // spawn unpin
            let (me, web3) = (self.clone(), socket.clone());
            tokio::spawn(async move {me.unpin_cids(web3, chain_name, update_time).await});

        }
    }

    pub async fn pin_chain_cids(self, web3: Arc<Web3<WebSocket>>, chain_name: String, update_time: u64){
        let chain_id = match web3.eth().chain_id().await{
            Ok(v)=>v.as_u64() as i64,
            Err(e)=>{eprintln!("Error getting chain_id for {}: {:?}", &chain_name, e); self.r_off.notify(); return}
        };
        
        loop{
            let bn = match web3.eth().block_number().await{
                Ok(v)=>v.as_u64() as i64,
                Err(e)=>{eprintln!("Error getting block number for {}: {:?}", &chain_name, e); self.r_off.notify(); break}
            };
            let cn = chain_name.clone();
            let cids_to_pin: Result<Vec<CIDInfo>, postgres::Error> = self.db.run(move |client|{
                //update pinned cids valid block number
                let r = client.query("SELECT euvb.cid, euvb.end_block, pc.node
                                                    FROM event_update_valid_block as euvb
                                                    INNER JOIN pinned_cids pc ON euvb.chain_id=pc.chain_id AND pc.cid=euvb.cid AND pc.end_block<euvb.end_block
                                                    WHERE euvb.end_block>$1::BIGINT AND euvb.chain_id=$2::BIGINT;", 
                                                    &[&bn, &chain_id])?; // get pinned cids with updated block number
                
                println!("{} - {} : Updating end block number for pinned CIDs, total: {} ...", &cn, &chain_id, r.len());
                for row in r{
                    let cid: String = row.get(0);
                    let end_block: i64 = row.get(1);
                    let node: String = row.get(2);
                    client.execute("UPDATE pinned_cids
                                          SET end_block=$1::BIGINT
                                          WHERE cid=$2::TEXT AND chain_id=$3::BIGINT AND node=$4::TEXT", 
                                          &[&end_block, &cid, &chain_id, &node])?;
                }

                //collect new cids to pin
                let r = client.query("SELECT euvb.cid, MAX(euvb.end_block)
                                                    FROM event_update_valid_block as euvb
                                                    LEFT JOIN pinned_cids pc ON euvb.chain_id=pc.chain_id AND euvb.cid=pc.cid
                                                    WHERE euvb.end_block>$1::BIGINT AND euvb.chain_id=$2::BIGINT AND pc.cid IS NULL
                                                    GROUP BY euvb.cid;", 
                                                          &[&bn, &chain_id])?;
                let mut rows = vec![];
                for row in r{
                    rows.push(CIDInfo{
                        chain_id: Option::Some(chain_id.clone()),
                        cid: row.get(0),
                        end_block: row.get(1),
                        node: Option::None,
                        node_login: Option::None,
                        node_pass: Option::None
                    })
                }
                Ok(rows)
            }).await;

            match cids_to_pin{
                Ok(v)=>{
                    println!("Got CIDs to pin, total: {}", v.len());
                    for cid in v{
                        // let c = (Arc::new(cid)).clone();
                        self.pin_cid_to_ipfs_nodes(cid, true).await;
                    }
                }
                Err(e)=>{eprintln!("{} - {} : Error getting new CIDs to Pin: {}", &chain_name, &chain_id, e)}
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(update_time)).await;    
        }
    }

    pub async fn pin_cid_to_ipfs_nodes(&self, cid_info: CIDInfo, pin: bool){
        for node in &*self.nodes{
            let mut c = cid_info.clone();
            c.node_login = node.login.clone();
            c.node_pass = node.password.clone();
            if pin{
                c.node = Option::Some(node.api_url.clone());
                pin_cid_to_node(self.db.clone(), c, true).await;
            }else{
                if c.node.as_ref().unwrap().clone().eq(&node.api_url){
                    unpin_cid_from_node(self.db.clone(), c).await;
                    return;
                }
            }
        }
    }
    
    pub async fn retry_failed_cids(self, web3: Arc<Web3<WebSocket>>, chain_name: String){
        let chain_id = match web3.eth().chain_id().await{
            Ok(v)=>v.as_u64() as i64,
            Err(e)=>{eprintln!("Error getting chain_id for {}: {:?}", &chain_name, e); self.r_off.notify(); return}
        };

        loop{
            let bn = match web3.eth().block_number().await{
                Ok(v)=>v.as_u64() as i64,
                Err(e)=>{eprintln!("Error getting block number for {}: {:?}", &chain_name, e); self.r_off.notify(); break}
            };
            let res: Result<Vec<CIDInfo>, postgres::Error> = self.db.run(move |client|{
                client.execute("DELETE FROM failed_pins WHERE end_block<=$1::BIGINT AND chain_id=$2::BIGINT", 
                &[&bn, &chain_id])?;

                let r = client.query("SELECT node, cid, end_block
                                                    FROM failed_pins
                                                    WHERE end_block>$1::BIGINT AND chain_id=$2::BIGINT", 
                                                          &[&bn, &chain_id])?;
                let mut rows = vec![];
                for row in r{
                    rows.push(CIDInfo{
                        chain_id: Option::Some(chain_id.clone()),
                        node: row.get(0),
                        cid: row.get(1),
                        end_block: row.get(2),
                        node_login: Option::None,
                        node_pass: Option::None
                    })
                }
                Ok(rows)
            }).await;
            
            match res{
                Ok(v)=>{
                    println!("Failed CIDs: Got CIDs to pin, total: {}", v.len());
                    for cid in v{
                        // let c = (Arc::new(cid)).clone();
                        // self.pin_cid_to_ipfs_nodes(c).await;
                        pin_cid_to_node(self.db.clone(),cid, false).await;
                    }
                }
                Err(e)=>{eprintln!("Failed CIDs: {} - {} : Error getting new CIDs to Pin: {}", &chain_name, &chain_id, e)}
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(self.retry_failed_cids_sec)).await;    
        }
    }
    
    pub async fn unpin_cids(self, web3: Arc<Web3<WebSocket>>, chain_name: String, update_time: u64){
        let chain_id = match web3.eth().chain_id().await{
            Ok(v)=>v.as_u64() as i64,
            Err(e)=>{eprintln!("Error getting chain_id for {}: {:?}", &chain_name, e); self.r_off.notify(); return}
        };
        
        loop{
            let bn = match web3.eth().block_number().await{
                Ok(v)=>v.as_u64() as i64,
                Err(e)=>{eprintln!("Error getting block number for {}: {:?}", &chain_name, e); self.r_off.notify(); break}
            };
            let cids_to_unpin: Result<Vec<CIDInfo>, postgres::Error> = self.db.run(move |client|{
                let res = client.execute("DELETE FROM pinned_cids AS p1
                                                    USING pinned_cids AS p2
                                                    WHERE p1.chain_id=$1::BIGINT AND p1.end_block<=$2::BIGINT AND p1.chain_id!=p2.chain_id AND p1.cid=p2.cid;",
                                     &[&chain_id, &bn])?;
                println!("DELETED '{}' expired CIDs for this '{}' chain", res, &chain_id);

                let res = client.query("SELECT p1.cid, p1.end_block, p1.node From pinned_cids AS p1
                                                        INNER JOIN pinned_cids p2 ON p1.chain_id!=p2.chain_id AND p1.cid!=p2.cid
                                                        WHERE p1.chain_id=$1::BIGINT AND p1.end_block<=$2::BIGINT
                                                        GROUP BY p1.chain_id, p1.node, p1.cid, p1.end_block", 
                                    &[&chain_id, &bn])?;
                
                let mut v = vec![];
                for r in res{
                    v.push(CIDInfo{
                        chain_id: Option::Some(chain_id.clone()),
                        cid: r.get(0),
                        end_block: r.get(1),
                        node: r.get(2),
                        node_login: Option::None,
                        node_pass: Option::None
                    });
                }
                Ok(v)
            }).await;

            match cids_to_unpin{
                Ok(v)=>{
                    println!("Got CIDs to unpin, total: {}", v.len());
                    for cid in v{
                        // let c = (Arc::new(cid)).clone();
                        self.pin_cid_to_ipfs_nodes(cid, false).await;
                    }
                }
                Err(e)=>{eprintln!("{} - {} : Error getting new CIDs to unpin: {}", &chain_name, &chain_id, e)}
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(update_time)).await;    
        }
    }
}


async fn add_failed_pin_to_db(db: Arc<DbConn>, chain_id: i64, block: i64, cid: String, node: String){
    db.run(move |client|{
        match client.execute("INSERT INTO failed_pins (chain_id, node, cid, end_block)
                                    VALUES ($1::BIGINT, $2::TEXT, $3::TEXT, $4::BIGINT)
                                    ON CONFLICT (chain_id, node, cid, end_block) DO NOTHING", 
                &[&chain_id, &node, &cid, &block]){
                    Ok(_)=>{println!("failed_pins TABLE ADD {} for NODE {} on CHAIN with ID {} till block {}", &cid, &node, &chain_id, &block)}
                    Err(e)=>{eprintln!("ERROR inserting cid {} to failed_pins: {}", &cid, e)}
                };
    }).await;
}


async fn pin_cid_to_node(db: Arc<DbConn>, c: CIDInfo, store_failed: bool){
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let node = c.node.unwrap();
        let cid = c.cid.unwrap();
        let chain_id = c.chain_id.unwrap();
        let block = c.end_block.unwrap();
        match client.post(format!("{}/api/v0/pin/add?arg={}", &node, &cid))
        .send()
        .await{
            Ok(v)=>{
                if !v.status().is_success(){
                    eprintln!("ERROR pinning cid {} to node {}", &cid, &node);
                    if store_failed{
                        add_failed_pin_to_db(db, chain_id, block, cid, node).await;
                    }
                    return;
                }

                db.run(move |client|{
                    match client.execute("INSERT INTO pinned_cids (chain_id, node, cid, end_block)
                                          VALUES ($1::BIGINT, $2::TEXT, $3::TEXT, $4::BIGINT)", 
                            &[&chain_id, &node, &cid, &block]){
                                Ok(_)=>{println!("PINNED {} to NODE {} on CHAIN with ID {} till block {}", &cid, &node, &chain_id, &block)}
                                Err(e)=>{eprintln!("ERROR inserting cid {} to pinned_cids: {}", &cid, e)}
                            };
                }).await; 
            }

            Err(e)=>{
                eprintln!("ERROR pinning cid {} to node {} : {}", &cid, &node, e);
                if store_failed{
                    add_failed_pin_to_db(db, chain_id, block, cid, node).await;
                }
            }
        }
    });
}

async fn unpin_cid_from_node(db: Arc<DbConn>, c: CIDInfo){
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let node = c.node.unwrap();
        let cid = c.cid.unwrap();
        let chain_id = c.chain_id.unwrap();
        let block = c.end_block.unwrap();
        match client.post(format!("{}/api/v0/pin/rm?arg={}", &node, &cid))
        .send()
        .await{
            Ok(v)=>{
                if !v.status().is_success(){
                    eprintln!("ERROR unpinning cid {} from node {}", &cid, &node);
                    return;
                }

                db.run(move |client|{
                    match client.execute("DELETE FROM pinned_cids
                                                WHERE chain_id=$1::BIGINT AND node=$2::TEXT AND cid=$3::TEXT AND end_block=$4::BIGINT);", 
                                        &[&chain_id, &node, &cid, &block]){
                                Ok(_)=>{println!("UNPINNED {} from NODE {} on CHAIN with ID {}", &cid, &node, &chain_id)}
                                Err(e)=>{eprintln!("ERROR deleting cid {} from pinned_cids: {}", &cid, e)}
                            };
                }).await; 
            }

            Err(e)=>{
                eprintln!("ERROR unpinning cid {} from node {} : {}", &cid, &node, e);
            }
        }
    });
}

#[derive(Debug, Clone)]
pub struct IPFSService{
    pub nodes: Arc<Vec<IPFSNode>>,
    pub providers: Arc<Vec<Providers>>,
    pub retry_failed_cids_sec: u64
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
        let db = Arc::new(DbConn::get_one(&rocket).await
                            .expect("database mounted."));

        // let shutdown = rocket.shutdown();
        
        // let (db1, nodes, r_off) = (db.clone(), (&self).nodes.clone(), shutdown.clone());
        let node_watcher = Arc::new(IPFSWatcher{
            db: db.clone(),
            nodes: (&self).nodes.clone(),
            providers: (&self).providers.clone(),
            r_off: rocket.shutdown().clone(),
            retry_failed_cids_sec: (&self).retry_failed_cids_sec.clone()
        });

        let nw1 = node_watcher.clone();
        tokio::spawn(async move {nw1.watch_nodes().await});

        // node_watcher.watch_failed_pins().await;

        // spawn unpin tasks


        // tokio::spawn(async move { watch_failed_pins(&nodes, db1,r_off).await});

        // watch_nodes(&self.nodes, db,shutdown).await;
    }
}