use std::sync::Arc;
use rocket::{fairing::{Fairing, Info, Kind}, tokio, Orbit, Rocket};
use web3::{Web3, transports::WebSocket};

use crate::{DbConn, yaml_parser::{Providers}};
use crate::yaml_parser::IPFSNode;

#[derive(Debug)]
struct EventUpdateValidBlock{
    pub chain_id: Option<i64>,
    pub cid: Option<String>, 
    pub donor: Option<String>,
    pub update_block: Option<i64>,
    pub end_block: Option<i64>,
    pub block_price_gwei: Option<i64>,
    pub ts: Option<chrono::NaiveDateTime>,
}

#[derive(Clone)]
struct IPFSWatcher{
    pub db: Arc<DbConn>,
    pub nodes: Vec<IPFSNode>,
    pub providers: Vec<Providers>,
    pub r_off: rocket::Shutdown
}

impl IPFSWatcher {
    pub async fn watch_nodes(&self){
        for provider in &self.providers{
            let chain_name = provider.chain_name.as_ref().unwrap().to_owned();
            let block_time = provider.block_time_sec.as_ref().unwrap().to_owned();
            let transport =  web3::transports::WebSocket::new(&(provider.provider.as_ref().unwrap())).await.unwrap();
            let web3 = Web3::new(transport);
            let me = self.clone();
            tokio::spawn(async move {me.pin_chain_cids(web3, chain_name, block_time).await});
            // spawn unpin tasks
            // spawn failed pins retry
        }
    }

    pub async fn pin_chain_cids(self, web3: Web3<WebSocket>, chain_name: String, block_time: u64){
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
            let cids_to_pin: Result<Vec<EventUpdateValidBlock>, postgres::Error> = self.db.run(move |client|{
                //update pinned cids valid block number
                let r = client.query("SELECT euvb.cid, euvb.donor, euvb.end_block, pc.node
                                                    FROM event_update_valid_block as euvb
                                                    LEFT JOIN pinned_cids pc ON euvb.chain_id=pc.chain_id
                                                    WHERE euvb.end_block>$1::BIGINT AND euvb.chain_id=$2::BIGINT
                                                            AND (
                                                                    pc.cid=euvb.cid 
                                                                    AND 
                                                                    pc.donor=euvb.donor 
                                                                    AND 
                                                                    pc.end_block<euvb.end_block
                                                                );", &[&bn, &chain_id])?;
                
                println!("{} - {} : Updating end block number for pinned CIDs, total: {} ...", &cn, &chain_id, r.len());
                for row in r{
                    let cid: String = row.get(0);
                    let donor: String = row.get(1);
                    let end_block: i64 = row.get(2);
                    let node: String = row.get(3);
                    client.execute("UPDATE event_update_valid_block
                                          SET end_block=$3::BIGINT
                                          WHERE cid=$1::TEXT AND donor=$2::TEXT AND chain_id=$4::BIGINT AND node=$5::TEXT", 
                                          &[&cid, &donor, &end_block, &chain_id, &node])?;
                }

                //collect new cids to pin
                let r = client.query("SELECT euvb.cid, euvb.donor, euvb.end_block
                                                    FROM event_update_valid_block as euvb
                                                    LEFT JOIN pinned_cids pc ON euvb.chain_id=pc.chain_id
                                                    WHERE euvb.end_block>$1::BIGINT AND euvb.chain_id=$2::BIGINT
                                                          AND (
                                                                (pc.cid IS NULL OR pc.cid!=euvb.cid) 
                                                                AND 
                                                                (pc.donor IS NULL OR pc.donor!=euvb.donor) 
                                                                AND 
                                                                (pc.end_block IS NULL OR pc.end_block!=euvb.end_block)
                                                            );", 
                                                          &[&bn, &chain_id])?;
                let mut rows = vec![];
                for row in r{
                    rows.push(EventUpdateValidBlock{
                        cid: row.get(0),
                        donor: row.get(1),
                        end_block: row.get(2),
                        chain_id: Option::Some(chain_id.clone()),
                        update_block: Option::None,
                        block_price_gwei: Option::None,
                        ts: Option::None
                    })
                }
                Ok(rows)
            }).await;

            match cids_to_pin{
                Ok(v)=>{
                    println!("Got CIDs to pin, total: {}", v.len());
                    // for every node pin the cid
                    todo!()
                }
                Err(e)=>{eprintln!("{} - {} : Error getting new CIDs to Pin: {}", &chain_name, &chain_id, e)}
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(block_time)).await;    
        }
    }
    
    pub async fn watch_failed_pins(&self){
    
    }
}

#[derive(Debug, Clone)]
pub struct IPFSService{
    pub nodes: Vec<IPFSNode>,
    pub providers: Vec<Providers>
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
            r_off: rocket.shutdown().clone()
        });

        let nw1 = node_watcher.clone();
        tokio::spawn(async move {nw1.watch_nodes().await});

        node_watcher.watch_failed_pins().await;
        // tokio::spawn(async move { watch_failed_pins(&nodes, db1,r_off).await});

        // watch_nodes(&self.nodes, db,shutdown).await;
    }
}