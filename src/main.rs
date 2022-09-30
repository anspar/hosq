#[macro_use]
extern crate rocket;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use types::State;
mod db;
mod routes;
mod services;
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

    let providers_service = services::providers::Providers {};
    let providers_manage = providers_service
        .get_providers(conf.providers.unwrap())
        .await
        .unwrap();
    let providers_manage = Arc::new(providers_manage);

    let ipfs_watcher = services::ipfs_watcher::IPFSService {
        retry_failed_cids_sec: conf.retry_failed_cids_sec,
        update_nodes_sec: conf.update_nodes_sec,
    };

    let _ = rocket::build()
        .mount("/", routes![routes::proxy::ipfs])
        .mount(
            "/v0",
            routes![
                routes::proxy::upload,
                routes::handlers::get_cids,
                routes::handlers::get_providers,
                routes::handlers::get_provider,
                routes::handlers::is_pinned,
                routes::handlers::cid_info,
                routes::handlers::pin_cid,
                routes::handlers::monitoring,
            ],
        )
        .attach(types::DbConn::fairing())
        .attach(routes::cors::CORS)
        .attach(ipfs_watcher)
        .attach(services::contract_watcher::ContractService)
        .attach(providers_service)
        .manage(State {
            nodes,
            providers: providers_manage,
            admin_secret: conf.admin_secret,
            monitoring: Arc::new(Mutex::new(HashMap::new())),
        })
        .launch()
        .await;
    //add fairing to keep sockets alive
}
