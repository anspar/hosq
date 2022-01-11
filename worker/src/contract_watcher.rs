use std::str::FromStr;
use std::sync::Arc;

use rocket::futures::StreamExt;
use rocket::{Rocket, Orbit, tokio};
use rocket::fairing::{Fairing, Info, Kind};
use web3::Web3;
use web3::signing::keccak256;
use web3::transports::WebSocket;
use web3::types::{H160, H256, FilterBuilder, BlockNumber, U64};
use crate::DbConn;
use crate::yaml_parser::Providers;

#[path="./db.rs"]
mod db;

#[derive(Clone)]
pub struct Contract{
    provider: Providers,
    db: Arc<DbConn>,
    r_off: rocket::Shutdown
}

impl Contract {
    async fn watch_update_valid_block(self, web3: Arc<Web3<WebSocket>>) {
        let chain_id = match web3.eth().chain_id().await{
            Ok(v)=>v.as_u64(),
            Err(e)=>{eprintln!("Error getting chain_id: {:?}", e); return}
        };

        let me = self.clone();
        let block = self.db.run(move|client|{
            let r = client.query_one("SELECT MAX(update_block) FROM event_update_valid_block WHERE chain_id=$1", &[&(chain_id as i64)])
                                .unwrap();
            let r: i64 = match r.try_get(0){
                Ok(v)=>v,
                Err(e)=>{let b = me.provider.start_block.unwrap(); eprintln!("{:?}\nNo block data in db, will start watching from block {}", e, &b); b}
            };
            r
        }).await;
        // println!("{:?}", (*db).query_one("SELECT count(*) from nfts", &[]).await.unwrap());
    
        let contarct_address = H160::from_str((&self).provider.contract_address.as_ref().unwrap()).unwrap();
        let topic_price_update_hash =
            keccak256(&(b"UpdateValidBlock(address,uint256,uint256,string)"[..]));
        let topic_price_update = H256::from_slice(&topic_price_update_hash);
        let filter = FilterBuilder::default()
            .address(vec![contarct_address])
            .from_block(BlockNumber::Number(U64::from_dec_str(block.to_string().as_str()).unwrap()))
            .topics(Some(vec![topic_price_update]), None, None, None)
            .build();

        
        println!("Starting watch_update_valid_block event listener for chain - {} from block - {}", &chain_id, &block);
        
        let logs = web3.eth().logs(filter.clone()).await.unwrap();
        for l in logs{
            // println!("{:?} - {:?}", l, l.data.0.len());
            db::update_valid_block((&self).db.clone(), l, chain_id).await;
        }
        // let web3 = Web3::new(&*web3);
        
        let e = web3.eth_subscribe()
            .subscribe_logs(filter)
            .await
            .unwrap();
    
        e.for_each(|event| async {
            let _ = match event {
                Ok(l)=>{
                    db::update_valid_block((&self).db.clone(), l, chain_id).await;
                }
                Err(e)=>{eprintln!("watch_update_valid_block data: Error parsing Log {:?}", e)}
            }; 
            
        })
        .await;
    }

    pub async fn watch_contract(self){
        let provider_url = (&self).provider.provider.as_ref().unwrap().to_owned();
        let transport =  web3::transports::WebSocket::new(&provider_url).await.unwrap();

        let web3 = Arc::new(Web3::new(transport));
        // let contarct_address = Arc::new(H160::from_str(&(&self).provider.contract_address.as_ref().unwrap()).unwrap());

        let (w1, me) = (web3.clone(), self.clone());
        tokio::spawn(async move { me.watch_update_valid_block(w1).await });

        let (w2, cn1, off) = (web3.clone(), 
                                                               (&self).provider.chain_name.as_ref().unwrap().clone(), self.r_off.clone());                                                            
        tokio::spawn(async move {
            loop{
                let bn = match w2.eth().block_number().await{
                    Ok(v)=>v.as_u64(),
                    Err(e)=>{eprintln!("Error getting block number: {:?}", e); off.notify(); break}
                };
                println!("{} : socket is alive at block {}", cn1, bn);
                tokio::time::sleep(tokio::time::Duration::from_secs(self.provider.update_interval_sec.unwrap())).await;    
            }
        });
    }
}


#[derive(Debug, Clone)]
pub struct ContractService{
    pub providers: Vec<Providers>
}

#[rocket::async_trait]
impl Fairing for ContractService {
    fn info(&self) -> Info {
        Info {
            name: "Run contract watcher service",
            kind: Kind::Liftoff,
        }
    }

    async fn on_liftoff(&self, rocket: &Rocket<Orbit>) {
        let db = Arc::new(DbConn::get_one(&rocket).await
                            .expect("database mounted."));

        let shutdown = rocket.shutdown();
        // let mut lunch = rocket.launch();

        for provider in &self.providers{
            // let db = db.clone();
            // let r_off = shutdown.clone();
            let t_c = Contract{provider: provider.clone(), db: db.clone(), r_off: shutdown.clone()};
            tokio::spawn(async move { t_c.watch_contract().await});
        }
    }
}