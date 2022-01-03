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
pub struct Contract;

impl Contract {
    async fn watch_update_valid_block(self, web3: Arc<Web3<WebSocket>>, contarct_address: Arc<H160>, psql: Arc<DbConn>, start_block: i64) {
        println!("Starting watch_update_valid_block event listener");
        let block = psql.run(move|client|{
            let r = client.query_one("SELECT MAX(update_block) FROM event_update_valid_block", &[]).unwrap();
            let r: i64 = match r.try_get(0){
                Ok(v)=>v,
                Err(e)=>{eprintln!("{:?}", e); start_block}
            };
            r
        }).await;
        // println!("{:?}", (*db).query_one("SELECT count(*) from nfts", &[]).await.unwrap());
    
        let contarct_address = *contarct_address;
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

    pub async fn watch_contract(self, provider: &str, contract_address: &str, start_block: i64, db: Arc<DbConn>){
        let transport =  web3::transports::WebSocket::new(provider).await.unwrap();

        let web3 = Arc::new(Web3::new(transport));
        let contarct_address = Arc::new(H160::from_str(contract_address).unwrap());

        let (w1, c1,db1) = (web3.clone(), contarct_address.clone(), db.clone());
        tokio::spawn(async move { self.watch_update_valid_block(w1, c1, db1, start_block).await });
    }
}


#[derive(Debug, Clone)]
pub struct ContractService{
    providers: Vec<Providers>,
    ipfs_nodes: Vec<String>,
}

impl ContractService {
    pub fn new(providers: Vec<Providers>, ipfs_nodes: Vec<String>)->Self{
        Self{
            providers,
            ipfs_nodes,
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

        // let mut shutdown = rocket.shutdown();

        for provider in &self.providers{
            let db = db.clone();
            let t_c = Contract{};
            let provider = provider.clone();
            tokio::spawn(async move { t_c.watch_contract(&provider.provider.unwrap(),
                                                    &provider.contract_address.unwrap(),
                                                        provider.start_block.unwrap(), db).await});
        }
    }
}