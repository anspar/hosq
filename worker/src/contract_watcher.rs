use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;

use crate::types::db::EventUpdateValidBlock;
use crate::types::errors::CustomError;
use crate::types::{self, DbConn, Web3Node};
use rocket::fairing::{Fairing, Info, Kind};
use rocket::futures::StreamExt;
use rocket::{tokio, Orbit, Rocket, Shutdown};
use serde::de::EnumAccess;
use web3::signing::keccak256;
use web3::types::{BlockNumber, FilterBuilder, H160, H256, U64, Log, U256};

use crate::db;

fn abi_slice_to_string(b: &[u8])->Result<String, CustomError>{
    // println!("{:?}", b);
    // let offset = U256::from_big_endian(&b[..32]).as_u64();
    if b.len()<=64 {return Err(CustomError::InvalidAbiString)}
    let mut length = U256::from_big_endian(&b[32..64]).as_u64();
    let mut s = "".to_owned();
    for c in &b[64..]{
        if length==0 {break}
        if *c==0 {continue} 
        s = format!("{}{}", s, &(*c as char));
        length-=1;
    }
    Ok(s)
}

pub async fn update_valid_block(psql: Arc<DbConn>, l: Log, chain_id: i64, provider_id: i64) -> Result<(),  Box<dyn Error>> {
    if l.data.0.len()<96{
        error!("update_valid_block: data len {:?} !>= 96", l.data.0.len());
        return Err(Box::new(CustomError::Inequality))
    }

    // let block: i64 = l.block_number.unwrap().low_u64().try_into().unwrap();
    let p_id = U256::from_big_endian(&l.data.0[64..96]).as_u64() as i64;
    if p_id!=provider_id{return Err(Box::new(CustomError::Inequality))}

    let donor = format!("{:?}", H160::from_slice(&l.data.0[12..32]));
    let update_block = (&l.block_number.unwrap()).as_u64() as i64;
    let end_block = U256::from_big_endian(&l.data.0[32..64]).as_u64() as i64;
    let cid = abi_slice_to_string(&l.data.0[96..])?;
   
    info!("{} -> GOT 'update_valid_block' Event :: {:?}, {:?}, {:?}, {:?}, {:?} :: {:?}", &chain_id, &donor, &update_block, &end_block, &p_id, &cid, &l.data.0.len());

    let res: Result<_, postgres::Error> = psql.run(move|client|{
        db::add_valid_block(client, EventUpdateValidBlock{
           chain_id,
           cid,
           donor,
           update_block,
           end_block,
           manual_add: Option::Some(false)
        })
    }).await;

    match res {
        Ok(_)=>Ok(()),
        Err(e)=>Err(Box::new(e))
    }
}

async fn update_valid_block_old_logs(provider: Web3Node, psql: Arc<DbConn>, r_off: Shutdown, chain_id: i64, start_block: i64, filter: FilterBuilder){
    tokio::spawn(async move{
        let bn = match provider.web3.eth().block_number().await {
            Ok(v) => v.as_u64() as i64,
            Err(e) => {
                error!(
                    "Error getting block number for {}: {:?}",
                    &provider.chain_name, e
                );
                r_off.notify();
                return;
            }
        };
        let mut start_block = start_block;
        let mut end_loop = false;
        loop{
            info!("CHAIN '{}'- '{}' > GETTING old logs from block '{}'", &provider.chain_name, &chain_id, &start_block);
            let f = if bn-start_block>provider.batch_size{
                let tf = filter.clone().from_block(BlockNumber::Number(
                    U64::from_dec_str(start_block.to_string().as_str()).unwrap(),
                )).to_block(
                    BlockNumber::Number(
                        U64::from_dec_str((start_block+provider.batch_size).to_string().as_str()).unwrap(),
                    )
                )
                .build();
                start_block = start_block+provider.batch_size;
                tf
            } else {
                end_loop = true;
                filter.clone().from_block(BlockNumber::Number(
                    U64::from_dec_str(start_block.to_string().as_str()).unwrap(),
                )).to_block(
                    BlockNumber::Number(
                        U64::from_dec_str(bn.to_string().as_str()).unwrap(),
                    )
                )
                .build()
            };
            let logs = provider.web3.eth().logs(f).await.unwrap();
            for l in logs {
                // println!("{:?} - {:?}", l, l.data.0.len());
                update_valid_block(psql.clone(), l, chain_id, provider.provider_id)
                    .await
                    .unwrap();
            }
            if end_loop{break}
        }
    });
}

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

    let block = match psql.run(move |client| {
            db::get_max_update_block(client, "event_update_valid_block".to_owned(), chain_id)
        })
        .await {
        Ok(v) => v,
        Err(e) => {
            error!(
                "CHAIN '{}' - '{}' > '{}', will start watching 'event_update_valid_block' from block {}",
                provider.chain_name, provider.chain_id, e, provider.start_block
            );
            provider.start_block
        }
    };

    let contarct_address = H160::from_str(&provider.contract_address).unwrap();

    let event = H256::from_slice(&keccak256(&(b"UpdateValidBlock(address,uint256,uint256,string)"[..])));
    let filter = FilterBuilder::default()
        .address(vec![contarct_address])
        // .from_block(BlockNumber::Number(
        //     U64::from_dec_str(block.to_string().as_str()).unwrap(),
        // ))
        .topics(Some(vec![event]), None, None, None);

    info!(
        "CHAIN '{}' - '{}' > Starting 'watch_update_valid_block' event listener from block - '{}'",
        &provider.chain_name, &chain_id, &block
    );

    let (p, f, db1) = (provider.clone(), filter.clone(), psql.clone());
    update_valid_block_old_logs(p, db1, r_off, chain_id, block, f).await;

    let e = web3.eth_subscribe().subscribe_logs(filter.build()).await.unwrap();

    e.for_each(|event| async {
        let _ = match event {
            Ok(l) => {
                update_valid_block(psql.clone(), l, chain_id, provider.provider_id)
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

async fn watch_add_provider(provider: Web3Node, psql: Arc<DbConn>, r_off: Shutdown) {
    let web3 = provider.web3.clone();
    let chain_id = match web3.eth().chain_id().await {
        Ok(v) => v.as_u64() as i64,
        Err(e) => {
            error!("Error getting chain_id: {:?}", e);
            r_off.notify();
            return;
        }
    };

    let block = match psql.run(move |client| {
            db::get_max_update_block(client, "event_add_provider".to_owned(), chain_id)
        }).await {
        Ok(v) => v,
        Err(e) => {
            error!(
                "CHAIN '{}' - {} > '{}', will start watching 'event_add_provider' from block {}",
                provider.chain_name, provider.chain_id, e, provider.start_block
            );
            provider.start_block
        }
    };

    let contarct_address = H160::from_str(&provider.contract_address).unwrap();
    let events = vec![H256::from_slice(&keccak256(&(b"AddProvider(address,uint256,uint256,string)"[..]))),
                                H256::from_slice(&keccak256(&(b"UpdateProviderBlockPrice(uint256,uint256)"[..]))),
                                H256::from_slice(&keccak256(&(b"UpdateProviderApiUrl(uint256,string)"[..]))),
                                H256::from_slice(&keccak256(&(b"UpdateProviderAddress(uint256,address)"[..])))];
    let filter = FilterBuilder::default()
        .address(vec![contarct_address])
        .from_block(BlockNumber::Number(
            U64::from_dec_str(block.to_string().as_str()).unwrap(),
        ))
        .topics(Some(events.clone()), None, None, None)
        .build();

    info!(
        "CHAIN '{}' - '{}' > Starting 'watch_add_provider' event listener from block - '{}'",
        &provider.chain_name, &chain_id, &block
    );

    let logs = web3.eth().logs(filter.clone()).await.unwrap();
    for l in logs {
        // let d = web3::helpers::decode(l.data.variant());
        // let d: types::AddProvider = serde_yaml::f(l.data);
        info!("{:?} : {:?} : {:?}", l.data, l.topics, events);
        // let d: StorageProvider = l.data.into(); 
        // println!("{:?} - {:?}", l, l.data.0.len());
        // db::update_valid_block(psql.clone(), l, chain_id, provider.provider_id)
        //     .await
        //     .unwrap();
    }
    // let web3 = Web3::new(&*web3);

    let e = web3.eth_subscribe().subscribe_logs(filter).await.unwrap();

    e.for_each(|event| async {
        let _ = match event {
            Ok(l) => {
                info!("{:?} : {:?} : {:?}", l.data, l.topics, events);
                // db::update_valid_block(psql.clone(), l, chain_id, provider.provider_id)
                //     .await
                //     .unwrap();
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
            let (p, psql, off) = (provider.clone(), db.clone(), shutdown.clone());
            tokio::spawn(async move { watch_add_provider(p, psql, off).await });
        }
    }
}
