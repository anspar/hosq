use std::sync::Arc;
use rocket::{fairing::{Fairing, Info, Kind}, tokio, Orbit, Rocket};

use crate::{DbConn, yaml_parser::Providers};
use crate::yaml_parser::IPFSNode;

#[derive(Clone)]
struct IPFSWatcher{
    pub db: Arc<DbConn>,
    pub nodes: Vec<IPFSNode>,
    pub providers: Vec<Providers>,
    pub r_off: rocket::Shutdown
}

impl IPFSWatcher {
    pub async fn watch_nodes(&self){
            
    }
    
    pub async fn watch_failed_pins(&self){
    
    }
}

#[derive(Debug, Clone)]
pub struct IPFSService{
    pub nodes: Vec<IPFSNode>,
    pub providers: Vec<Providers>
}

#[rocket::async_trait]
impl Fairing for IPFSService {
    fn info(&self) -> Info {
        Info {
            name: "Run contract watcher service",
            kind: Kind::Liftoff,
        }
    }

    async fn on_liftoff(&self, rocket: &Rocket<Orbit>) {
        let db = Arc::new(DbConn::get_one(&rocket).await
                            .expect("database mounted."));

        // let shutdown = rocket.shutdown();
        
        // let (db1, nodes, r_off) = (db.clone(), (&self).nodes.clone(), shutdown.clone());
        let node_watcher = Arc::new(IPFSWatcher{
            db: db.clone(),
            nodes: (&self).nodes.clone(),
            providers: (&self).providers.clone(),
            r_off: rocket.shutdown().clone()
        });

        let nw1 = node_watcher.clone();
        tokio::spawn(async move {nw1.watch_nodes().await});

        node_watcher.watch_failed_pins().await;
        // tokio::spawn(async move { watch_failed_pins(&nodes, db1,r_off).await});

        // watch_nodes(&self.nodes, db,shutdown).await;
    }
}