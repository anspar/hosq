use std::str::FromStr;
use std::sync::Arc;

use crate::types::{self, DbConn, Web3Node};
use rocket::fairing::{Fairing, Info, Kind};
use rocket::futures::StreamExt;
use rocket::{tokio, Orbit, Rocket, Shutdown};
use web3::signing::keccak256;
use web3::types::{BlockNumber, FilterBuilder, H160, H256, U64};

use crate::db;

async fn watch_update_valid_block(provider: Web3Node, psql: Arc<DbConn>, r_off: Shutdown) {
    let web3 = provider.web3.clone();
    let chain_id = match web3.eth().chain_id().await {
        Ok(v) => v.as_u64() as i64,
        Err(e) => {
            error!("Error getting chain_id: {:?}", e);
            r_off.notify();
            return;
        }
    };

    let block: Result<i64, postgres::Error> = psql
        .run(move |client| {
            db::get_max_update_block(client, "event_update_valid_block".to_owned(), chain_id)
        })
        .await;

    let block = match block {
        Ok(v) => v,
        Err(e) => {
            error!(
                "{} - {} > '{}', will start watching from block {}",
                provider.chain_name, provider.chain_id, e, provider.start_block
            );
            provider.start_block
        }
    };

    let contarct_address = H160::from_str(&provider.contract_address).unwrap();
    let topic_price_update_hash =
        keccak256(&(b"UpdateValidBlock(address,uint256,uint256,string)"[..]));
    let topic_price_update = H256::from_slice(&topic_price_update_hash);
    let filter = FilterBuilder::default()
        .address(vec![contarct_address])
        .from_block(BlockNumber::Number(
            U64::from_dec_str(block.to_string().as_str()).unwrap(),
        ))
        .topics(Some(vec![topic_price_update]), None, None, None)
        .build();

    info!(
        "Starting watch_update_valid_block event listener for chain - '{}' from block - '{}'",
        &chain_id, &block
    );

    let logs = web3.eth().logs(filter.clone()).await.unwrap();
    for l in logs {
        // println!("{:?} - {:?}", l, l.data.0.len());
        db::update_valid_block(psql.clone(), l, chain_id, provider.provider_id)
            .await
            .unwrap();
    }
    // let web3 = Web3::new(&*web3);

    let e = web3.eth_subscribe().subscribe_logs(filter).await.unwrap();

    e.for_each(|event| async {
        let _ = match event {
            Ok(l) => {
                db::update_valid_block(psql.clone(), l, chain_id, provider.provider_id)
                    .await
                    .unwrap();
            }
            Err(e) => {
                error!("watch_update_valid_block data: Error parsing Log '{}'", e)
            }
        };
    })
    .await;
}
//     pub async fn watch_contract(self){

//         let me = self.clone();
//         tokio::spawn(async move { me.watch_update_valid_block().await });

//         // let (w2, cn1, off) = (web3.clone(), (&self).provider.chain_name.clone(), self.r_off.clone());
//         // tokio::spawn(async move {
//         //     loop{
//         //         let bn = match w2.eth().block_number().await{
//         //             Ok(v)=>v.as_u64(),
//         //             Err(e)=>{eprintln!("Error getting block number: {:?}", e); off.notify(); break}
//         //         };
//         //         println!("{} : socket is alive at block {}", cn1, bn);
//         //         tokio::time::sleep(tokio::time::Duration::from_secs(self.provider.update_interval_sec)).await;
//         //     }
//         // });
//     }
// }

#[derive(Debug, Clone)]
pub struct ContractService;

#[rocket::async_trait]
impl Fairing for ContractService {
    fn info(&self) -> Info {
        Info {
            name: "Run contract watcher service",
            kind: Kind::Liftoff,
        }
    }

    async fn on_liftoff(&self, rocket: &Rocket<Orbit>) {
        let db = Arc::new(DbConn::get_one(&rocket).await.expect("database mounted."));

        let shutdown = rocket.shutdown();
        let providers = rocket.state::<Arc<Vec<types::Web3Node>>>().unwrap().clone();

        for provider in &*providers {
            let (p, psql, off) = (provider.clone(), db.clone(), shutdown.clone());
            tokio::spawn(async move { watch_update_valid_block(p, psql, off).await });
        }
    }
}
