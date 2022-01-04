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

#[derive(Debug, Clone)]
pub struct Contract{
    provider: Providers
}

impl Contract {
    async fn watch_update_valid_block(self, web3: Arc<Web3<WebSocket>>, psql: Arc<DbConn>) {
        println!("Starting watch_update_valid_block event listener");
        let me = self.clone();
        let block = psql.run(move|client|{
            let r = client.query_one("SELECT MAX(update_block) FROM event_update_valid_block", &[]).unwrap();
            let r: i64 = match r.try_get(0){
                Ok(v)=>v,
                Err(e)=>{eprintln!("{:?}", e); me.provider.start_block.unwrap()}
            };
            r
        }).await;
        // println!("{:?}", (*db).query_one("SELECT count(*) from nfts", &[]).await.unwrap());
    
        let contarct_address = H160::from_str((&self).provider.contract_address.as_ref().unwrap()).unwrap();
        let topic_price_update_hash =
            keccak256(&(b"UpdateValidBlock(address,uint256,uint256,uint256,string)"[..]));
        let topic_price_update = H256::from_slice(&topic_price_update_hash);
        let filter = FilterBuilder::default()
            .address(vec![contarct_address])
            .from_block(BlockNumber::Number(U64::from_dec_str(block.to_string().as_str()).unwrap()))
            .topics(Some(vec![topic_price_update]), None, None, None)
            .build();
    
        let logs = web3.eth().logs(filter.clone()).await.unwrap();
        for l in logs{
            // println!("{:?} - {:?}", l, l.data.0.len());
            db::update_valid_block(psql.clone(), l).await;
        }
        // let web3 = Web3::new(&*web3);
        
        let e = web3.eth_subscribe()
            .subscribe_logs(filter)
            .await
            .unwrap();
    
        e.for_each(|event| async {
            let _ = match event {
                Ok(l)=>{
                    db::update_valid_block(psql.clone(), l).await;
                }
                Err(e)=>{eprintln!("watch_update_valid_block data: Error parsing Log {:?}", e)}
            }; 
            
        })
        .await;
    }

    pub async fn watch_contract(self, db: Arc<DbConn>, rocket_off: rocket::Shutdown){
        let provider_url = (&self).provider.provider.as_ref().unwrap().to_owned();
        let transport =  web3::transports::WebSocket::new(&provider_url).await.unwrap();

        let web3 = Arc::new(Web3::new(transport));
        // let contarct_address = Arc::new(H160::from_str(&(&self).provider.contract_address.as_ref().unwrap()).unwrap());

        let (w1, db1, me) = (web3.clone(), db.clone(), self.clone());
        tokio::spawn(async move { me.watch_update_valid_block(w1, db1).await });
        let (w2, p) = (web3.clone(), provider_url.clone());
        tokio::spawn(async move {
            loop{
                let bn = match w2.eth().block_number().await{
                    Ok(v)=>v.as_u64(),
                    Err(e)=>{eprintln!("Error getting block number: {:?}", e); rocket_off.notify(); break}
                };
                println!("{} : alive at block {}", p, bn);
                tokio::time::sleep(tokio::time::Duration::from_secs(self.provider.block_time_sec.unwrap())).await;    
            }
        });
    }
}


#[derive(Debug, Clone)]
pub struct ContractService{
    providers: Vec<Providers>,
    ipfs_nodes: Vec<String>
}

impl ContractService {
    pub fn new(providers: Vec<Providers>, ipfs_nodes: Vec<String>)->Self{
        Self{
            providers,
            ipfs_nodes
        }
    } 
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
        // let server = assets::ImageServer { client: Client::new() };
        let db = Arc::new(DbConn::get_one(&rocket).await
                            .expect("database mounted."));
        // let s = Arc::new(self);
        // let contract = Contract{};
        // let s = Arc::new(*self);

        let shutdown = rocket.shutdown();
        // let mut lunch = rocket.launch();

        for provider in &self.providers{
            let db = db.clone();
            let t_c = Contract{provider: provider.clone()};
            // let provider = provider.clone();
            let r_off = shutdown.clone();
            tokio::spawn(async move { t_c.watch_contract(db, r_off).await});
        }
    }
}