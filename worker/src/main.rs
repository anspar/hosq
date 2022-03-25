#[macro_use]
extern crate rocket;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use std::env;
use std::sync::Arc;
use types::State;
mod contract_watcher;
mod cors;
mod db;
mod handlers;
mod ipfs_watcher;
mod providers;
mod types;
mod yaml_parser;

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

    let providers_service = providers::Providers {};
    let providers_manage = providers_service
        .get_providers(conf.providers.unwrap())
        .await
        .unwrap();
    let providers_manage = Arc::new(providers_manage);

    let ipfs_watcher = ipfs_watcher::IPFSService {
        retry_failed_cids_sec: conf.retry_failed_cids_sec,
        update_nodes_sec: conf.update_nodes_sec,
    };

    let _ = rocket::build()
        .mount(
            "/",
            routes![
                handlers::upload_file,
                handlers::get_cids,
                handlers::get_providers,
                handlers::get_provider,
                handlers::is_pinned,
                handlers::cid_info,
                handlers::pin_cid
            ],
        )
        .attach(types::DbConn::fairing())
        .attach(cors::CORS)
        .attach(ipfs_watcher)
        .attach(contract_watcher::ContractService)
        .attach(providers_service)
        .manage(State {
            nodes,
            providers: providers_manage,
            admin_secret: conf.admin_secret,
        })
        .launch()
        .await;
    //add fairing to keep sockets alive
}
