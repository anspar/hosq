#[macro_use]
extern crate rocket;

use std::env;
// use types::watcher::PriceUpdate;
// use pretty_env_logger;
mod contract_watcher;
mod cors;
mod yaml_parser;
mod ipfs_watcher;
// use tokio_postgres::{Client, NoTls};
// mod db;
use rocket_sync_db_pools::{database, postgres};
#[database("pg")]
#[derive(Debug)]
pub struct DbConn(postgres::Client);

#[get("/")]
async fn index() -> &'static str {
    "Hello, world!"
}

#[rocket::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len().ne(&2) {
        println!("Please specify the path to config.yml file");
        return;
    }

    let conf = yaml_parser::get_conf(&args[1]);
    //   let pre_release = conf.pre_release;
    let providers = conf.providers.unwrap();
    
    let contract_watcher = contract_watcher::ContractService {
        providers: providers.clone(),
    };

    let ipfs_watcher = ipfs_watcher::IPFSService{
        providers: providers,
        nodes: conf.ipfs_nodes.unwrap()
    };

    let _ = rocket::build()
        .mount("/", routes![index])
        .attach(DbConn::fairing())
        .attach(cors::CORS)
        .attach(ipfs_watcher)
        .attach(contract_watcher)
        .launch()
        .await;
}
