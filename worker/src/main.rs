#[macro_use]
extern crate rocket;

use std::{env};
// use types::watcher::PriceUpdate;
// use pretty_env_logger;
mod yaml_parser;
mod cors;
mod contract_watcher;
// use tokio_postgres::{Client, NoTls};
// mod db;
use rocket_sync_db_pools::{database, postgres};
#[database("pg")]
pub struct DbConn(postgres::Client);


#[get("/")]
async fn index() -> &'static str {
    "Hello, world!"
}

#[rocket::main]
async fn main() {
    // pretty_env_logger::init();
    let args: Vec<String> = env::args().collect();
    if args.len().ne(&2) {
        println!("Please specify the path to config.yml file");
        return;
    }

    let conf = yaml_parser::get_conf(&args[1]);
    //   let pre_release = conf.pre_release;
    let watcher = contract_watcher::ContractService::new(conf.providers.unwrap(), conf.ipfs_nodes.unwrap());
    //   let mut store = Store{config: Arc::new(conf.clone()), ipfs: Option::None};
    //   if !pre_release{
    //     let url: Uri = conf.ipfs.unwrap().parse().unwrap();
    //     store.ipfs = Option::Some(Arc::new( IpfsClient::build_with_base_uri(url)));
    //   }
    let _ = rocket::build()
        .mount("/", routes![index])
        .attach(DbConn::fairing())
        .attach(cors::CORS)
        .attach(watcher)
        .launch()
        .await;
}
