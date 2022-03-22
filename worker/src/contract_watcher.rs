use std::error::Error;
use std::ops::Div;
use std::str::FromStr;
use std::sync::Arc;

use crate::types::db::{EventAddProvider, EventUpdateValidBlock};
use crate::types::errors::CustomError;
use crate::types::{DbConn, State};
use rocket::fairing::{Fairing, Info, Kind};
use rocket::{tokio, Orbit, Rocket};
use web3::signing::keccak256;
use web3::types::{BlockNumber, FilterBuilder, Log, H160, H256, U64};

use crate::db;

macro_rules! get_logs {
    ($name:expr => $provider:expr, $psql:expr, $filter:expr, $start_block:expr, $web3:expr => $f:expr) => {
        let mut start_block = $start_block;
        loop{
            let bn = {
                $provider.latest_block.clone().lock().unwrap().clone()
            };

            let bn = match bn {
                Some(v) => v,
                None => {
                    error!("CHAIN '{}' - '{}' > latest block is 'None', will sleep for '{}' sec. and try again > '{}'",
                    &$provider.chain_name, &$provider.chain_id, &$provider.log_update_sec, $name);
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        $provider.log_update_sec,
                    ))
                    .await;
                    continue;
                }
            };

            match $provider.skip_old{
                Some(v)=>{
                    if v && bn-start_block > $provider.batch_size{
                        start_block = bn-$provider.batch_size;
                    }
                }
                None=>{}
            };

            if start_block >= bn {
                info!("CHAIN '{}' - '{}' > Synced at '{}' waiting for latest block, will sleep for '{} sec.' > '{}'",
                &$provider.chain_name, &$provider.chain_id, &bn, &$provider.log_update_sec, $name);
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    $provider.log_update_sec,
                ))
                .await;
                continue;
            }

            info!("CHAIN '{}' - '{}' > Looking '{}' logs from block '{}'", &$provider.chain_name, $provider.chain_id, $name, $start_block);
            let filter = if bn-start_block>$provider.batch_size{
                let tf = $filter.clone().from_block(BlockNumber::Number(
                    U64::from_dec_str(start_block.to_string().as_str()).unwrap(),
                )).to_block(
                    BlockNumber::Number(
                        U64::from_dec_str((start_block+$provider.batch_size).to_string().as_str()).unwrap(),
                    )
                )
                .build();
                start_block = start_block+$provider.batch_size;
                tf
            } else {
                let tf = $filter.clone().from_block(BlockNumber::Number(
                    U64::from_dec_str(start_block.to_string().as_str()).unwrap(),
                )).to_block(
                    BlockNumber::Number(
                        U64::from_dec_str(bn.to_string().as_str()).unwrap(),
                    )
                )
                .build();
                start_block = bn;
                tf
            };
            let logs = $web3.eth().logs(filter).await.unwrap();
            for l in logs {
                $f($psql.clone(), l, $provider.chain_id, $provider.provider_id).await.unwrap();
            }
        }

    };
}

macro_rules! watch_event {
    ($db_name:expr, $topic:expr => $provider:expr, $psql:expr, $r_off:expr => $func:expr) => {
        let (provider, psql, r_off) = ($provider.clone(), $psql.clone(), $r_off.clone());
        tokio::spawn(async move {
            let web3 = {
                provider.web3.clone().lock().unwrap().clone()
            };
            let c_id = provider.chain_id.clone();
            let block = match psql.run(move |client| {
                    db::get_max_update_block(client, $db_name.to_owned(), c_id)
                })
                .await {
                Ok(v) => std::cmp::max(v, provider.start_block),
                Err(e) => {
                    error!(
                        "CHAIN '{}' - '{}' > '{}', will start watching '{}' from block {}",
                        provider.chain_name, provider.chain_id, e, $topic, provider.start_block
                    );
                    provider.start_block
                }
            };

            let contract_address = H160::from_str(&provider.contract_address).unwrap();
            let event = H256::from_slice(&keccak256($topic.as_bytes()));
            let filter = FilterBuilder::default()
                .address(vec![contract_address])
                .topics(Some(vec![event]), None, None, None);

            info!(
                "CHAIN '{}' - '{}' > Starting event listener for '{}' from block - '{}'",
                &provider.chain_name, &provider.chain_id, $topic, &block
            );

            get_logs!($topic => provider, psql, filter,
                                block, web3 => $func);

            // let e = web3.eth_subscribe().subscribe_logs(filter.build()).await.unwrap();

            // e.for_each(|event| async {
            //     let _ = match event {
            //         Ok(l) => {
            //             $func(psql.clone(), l, chain_id, provider.provider_id)
            //                 .await
            //                 .unwrap();
            //         }
            //         Err(e) => {
            //             error!("'{}' data: Error parsing Log '{}'", $topic, e)
            //         }
            //     };
            // })
            // .await;
        })
    };
}

pub async fn update_valid_block(
    psql: Arc<DbConn>,
    l: Log,
    chain_id: i64,
    cur_provider_id: i64,
) -> Result<(), Box<dyn Error>> {
    if l.data.0.len() < 96 {
        return Err(Box::new(CustomError::Inequality(format!(
            "update_valid_block: data len {:?} !>= 96",
            l.data.0.len()
        ))));
    }

    let dec_d = ethabi::decode(
        &[
            ethabi::ParamType::Address,
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::String,
        ],
        &l.data.0,
    )
    .unwrap();
    // let block: i64 = l.block_number.unwrap().low_u64().try_into().unwrap();
    let p_id = dec_d[2].clone().into_uint().unwrap().as_u64() as i64;
    if p_id != cur_provider_id {
        info!("CHAIN '{}' -> GOT 'update_valid_block' Event for provider '{}', I'am '{}', Not updating", chain_id, p_id, cur_provider_id);
        return Ok(());
    }

    let donor = format!("0x{}", dec_d[0].to_string());
    let update_block = (&l.block_number.unwrap()).as_u64() as i64;
    let end_block = dec_d[1].clone().into_uint().unwrap().as_u64() as i64;
    let cid = dec_d[3].to_string();

    info!(
        "CHAIN '{}' -> GOT 'update_valid_block' Event :: {:?}, {:?}, {:?}, {:?}, {:?} :: {:?}",
        &chain_id,
        &donor,
        &update_block,
        &end_block,
        &p_id,
        &cid,
        &l.data.0.len()
    );

    let res: Result<_, postgres::Error> = psql
        .run(move |client| {
            db::add_valid_block(
                client,
                EventUpdateValidBlock {
                    chain_id,
                    cid,
                    donor,
                    update_block,
                    end_block,
                    manual_add: Option::Some(false),
                },
            )
        })
        .await;

    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(Box::new(e)),
    }
}

pub async fn update_add_provider(
    psql: Arc<DbConn>,
    l: Log,
    chain_id: i64,
    _cur_provider_id: i64,
) -> Result<(), Box<dyn Error>> {
    if l.data.0.len() < 96 {
        return Err(Box::new(CustomError::Inequality(format!(
            "update_add_provider: data len {:?} !>= 96",
            l.data.0.len()
        ))));
    }

    let dec_d = ethabi::decode(
        &[
            ethabi::ParamType::Address,
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::String,
            ethabi::ParamType::String,
        ],
        &l.data.0,
    )
    .unwrap();

    let owner: String = format!("0x{}", dec_d[0]);
    let update_block = (&l.block_number.unwrap()).as_u64() as i64;
    let provider_id = dec_d[1].clone().into_uint().unwrap().as_u64() as i64;
    let block_price_gwei = dec_d[2]
        .clone()
        .into_uint()
        .unwrap()
        .div(ethabi::ethereum_types::U256::from_dec_str("1000000000").unwrap())
        .as_u64() as i64;

    // println!("bb {:?}, {}", dec_d, &l.data.0.len());
    let api_url = dec_d[3].to_string();
    let name = dec_d[4].to_string();

    info!(
        "CHAIN '{}' -> GOT 'update_add_provider' Event :: {}, {}, {}, {}, {} : {}",
        &chain_id, &owner, &update_block, &provider_id, &block_price_gwei, &api_url, name
    );
    // Ok(())
    let res: Result<_, postgres::Error> = psql
        .run(move |client| {
            db::add_provider(
                client,
                EventAddProvider {
                    chain_id,
                    update_block,
                    owner,
                    provider_id,
                    block_price_gwei,
                    api_url,
                    name,
                },
            )
        })
        .await;

    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(Box::new(e)),
    }
}

pub async fn update_provider_block_price(
    psql: Arc<DbConn>,
    l: Log,
    chain_id: i64,
    _cur_provider_id: i64,
) -> Result<(), Box<dyn Error>> {
    if l.data.0.len() < 64 {
        return Err(Box::new(CustomError::Inequality(format!(
            "update_provider_block_price: data len {:?} !>= 64",
            l.data.0.len()
        ))));
    }

    let dec_d = ethabi::decode(
        &[ethabi::ParamType::Uint(256), ethabi::ParamType::Uint(256)],
        &l.data.0,
    )
    .unwrap();

    let update_block = (&l.block_number.unwrap()).as_u64() as i64;
    let provider_id = dec_d[0].clone().into_uint().unwrap().as_u64() as i64;
    let block_price = dec_d[0]
        .clone()
        .into_uint()
        .unwrap()
        .div(ethabi::ethereum_types::U256::from_dec_str("1000000000").unwrap())
        .as_u64() as i64; //gwei

    info!(
        "CHAIN '{}' -> GOT 'update_provider_block_price' Event :: {:?}, {:?}, {:?} :: {:?}",
        &chain_id,
        &update_block,
        &block_price,
        &provider_id,
        &l.data.0.len()
    );

    let res: Result<_, postgres::Error> = psql
        .run(move |client| {
            db::update_provider_block_price(
                client,
                chain_id,
                update_block,
                provider_id,
                block_price,
            )
        })
        .await;

    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(Box::new(e)),
    }
}

pub async fn update_provider_api_url(
    psql: Arc<DbConn>,
    l: Log,
    chain_id: i64,
    _cur_provider_id: i64,
) -> Result<(), Box<dyn Error>> {
    if l.data.0.len() < 64 {
        return Err(Box::new(CustomError::Inequality(format!(
            "update_provider_api_url: data len {:?} !>= 64",
            l.data.0.len()
        ))));
    }

    let dec_d = ethabi::decode(
        &[ethabi::ParamType::Uint(256), ethabi::ParamType::String],
        &l.data.0,
    )
    .unwrap();

    let update_block = (&l.block_number.unwrap()).as_u64() as i64;
    let provider_id = dec_d[0].clone().into_uint().unwrap().as_u64() as i64;
    let api_url = dec_d[1].to_string();

    info!(
        "CHAIN '{}' -> GOT 'update_provider_api_url' Event :: {:?}, {:?}, {:?} :: {:?}",
        &chain_id,
        &update_block,
        &api_url,
        &provider_id,
        &l.data.0.len()
    );

    let res: Result<_, postgres::Error> = psql
        .run(move |client| {
            db::update_provider_api_url(client, chain_id, update_block, provider_id, api_url)
        })
        .await;

    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(Box::new(e)),
    }
}

pub async fn update_provider_owner(
    psql: Arc<DbConn>,
    l: Log,
    chain_id: i64,
    _cur_provider_id: i64,
) -> Result<(), Box<dyn Error>> {
    if l.data.0.len() < 64 {
        return Err(Box::new(CustomError::Inequality(format!(
            "update_provider_owner: data len {:?} !>= 64",
            l.data.0.len()
        ))));
    }

    let dec_d = ethabi::decode(
        &[ethabi::ParamType::Uint(256), ethabi::ParamType::String],
        &l.data.0,
    )
    .unwrap();

    let update_block = (&l.block_number.unwrap()).as_u64() as i64;
    let provider_id = dec_d[0].clone().into_uint().unwrap().as_u64() as i64;
    let owner = format!("0x{}", dec_d[1].to_string());

    info!(
        "CHAIN '{}' -> GOT 'update_provider_owner' Event :: {:?}, {:?}, {:?} :: {:?}",
        &chain_id,
        &update_block,
        &owner,
        &provider_id,
        &l.data.0.len()
    );

    let res: Result<_, postgres::Error> = psql
        .run(move |client| {
            db::update_provider_owner(client, chain_id, update_block, provider_id, owner)
        })
        .await;

    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(Box::new(e)),
    }
}

pub async fn update_provider_name(
    psql: Arc<DbConn>,
    l: Log,
    chain_id: i64,
    _cur_provider_id: i64,
) -> Result<(), Box<dyn Error>> {
    if l.data.0.len() < 64 {
        return Err(Box::new(CustomError::Inequality(format!(
            "update_provider_name: data len {:?} !>= 64",
            l.data.0.len()
        ))));
    }

    let dec_d = ethabi::decode(
        &[ethabi::ParamType::Uint(256), ethabi::ParamType::String],
        &l.data.0,
    )
    .unwrap();

    let update_block = (&l.block_number.unwrap()).as_u64() as i64;
    let provider_id = dec_d[0].clone().into_uint().unwrap().as_u64() as i64;
    let name = dec_d[1].to_string();

    info!(
        "CHAIN '{}' -> GOT 'update_provider_name' Event :: {:?}, {:?}, {:?} :: {:?}",
        &chain_id,
        &update_block,
        &name,
        &provider_id,
        &l.data.0.len()
    );

    let res: Result<_, postgres::Error> = psql
        .run(move |client| {
            db::update_provider_name(client, chain_id, update_block, provider_id, name)
        })
        .await;

    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(Box::new(e)),
    }
}

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
        let providers = rocket.state::<State>().unwrap().clone().providers.clone();

        for provider in &*providers {
            watch_event!("event_update_valid_block", "UpdateValidBlock(address,uint256,uint256,string)"
                            => provider, db, shutdown 
                            => update_valid_block);
            watch_event!("event_add_provider", "AddProvider(address,uint256,uint256,string,string)" 
                            => provider, db, shutdown 
                            => update_add_provider);
            watch_event!("event_add_provider", "UpdateProviderBlockPrice(uint256,uint256)" 
                            => provider, db, shutdown 
                            => update_provider_block_price);
            watch_event!("event_add_provider", "UpdateProviderApiUrl(uint256,string)" 
                            => provider, db, shutdown 
                            => update_provider_api_url);
            watch_event!("event_add_provider", "UpdateProviderAddress(uint256,address)" 
                            => provider, db, shutdown 
                            => update_provider_owner);
            watch_event!("event_add_provider", "UpdateProviderName(uint256,string)" 
                            => provider, db, shutdown 
                            => update_provider_name);
        }
    }
}
