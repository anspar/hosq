#[macro_use]
extern crate rocket;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use std::env;
use std::sync::Arc;
use web3::Web3;
mod contract_watcher;
mod cors;
mod ipfs_watcher;
mod yaml_parser;
mod db;
mod types;
mod handlers;


#[rocket::main]
async fn main() {
    pretty_env_logger::init();
    let args: Vec<String> = env::args().collect();
    if args.len().ne(&2) {
        error!("Please specify the path to <config.yml> file");
        return;
    }

    let conf = yaml_parser::get_conf(&args[1]);
    //   let pre_release = conf.pre_release;
    let nodes = Arc::new(conf.ipfs_nodes.unwrap());
    let mut providers_manage = vec![];

    for provider in conf.providers.unwrap() {
        let transport = web3::transports::WebSocket::new(&(provider.provider))
            .await
            .unwrap();
        let socket = Arc::new(Web3::new(transport));
        let chain_id = match socket.eth().chain_id().await {
            Ok(v) => v.as_u64() as i64,
            Err(e) => {
                error!(
                    "Error getting chain_id for {}: {:?}",
                    &provider.chain_name, e
                );
                return;
            }
        };
        providers_manage.push(types::Web3Node {
            contract_address: provider.contract_address,
            chain_name: provider.chain_name,
            start_block: provider.start_block,
            block_time_sec: provider.block_time_sec,
            update_interval_sec: provider.update_interval_sec,
            provider_id: provider.provider_id,
            chain_id,
            batch_size: provider.batch_size,
            web3: socket,
        });
    }

    let providers_manage = Arc::new(providers_manage);

    let ipfs_watcher = ipfs_watcher::IPFSService {
        retry_failed_cids_sec: conf.retry_failed_cids_sec.unwrap(),
    };

    let _ = rocket::build()
        .mount("/", routes![handlers::upload_file, handlers::get_cids])
        .attach(types::DbConn::fairing())
        .attach(cors::CORS)
        .attach(ipfs_watcher)
        .attach(contract_watcher::ContractService)
        .manage(nodes)
        .manage(providers_manage)
        .launch()
        .await;
    //add fairing to keep sockets alive
}
