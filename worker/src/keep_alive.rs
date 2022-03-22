use std::sync::Arc;

use rocket::{
    fairing::{Fairing, Info, Kind},
    tokio, Orbit, Rocket,
};

use crate::types::{State, Web3Node};

#[derive(Debug, Clone)]
pub struct KeepProvidersAlive;

#[rocket::async_trait]
impl Fairing for KeepProvidersAlive {
    fn info(&self) -> Info {
        Info {
            name: "Run contract watcher service",
            kind: Kind::Liftoff,
        }
    }

    async fn on_liftoff(&self, rocket: &Rocket<Orbit>) {
        let providers = rocket.state::<State>().unwrap().providers.clone();
        let shutdown = rocket.shutdown();

        for provider in &*providers {
            let keep_alive = provider.keep_alive.unwrap_or_else(|| false);
            if keep_alive {
                let (p, r_off) = (provider.clone(), shutdown.clone());
                tokio::spawn(async move {
                    loop {
                        let web3 = { p.web3.clone().lock().unwrap().clone() };
                        let bn = match web3.eth().block_number().await {
                            Ok(v) => v.as_u64() as i64,
                            Err(e) => {
                                error!("Error getting block number for {}: {:?}", &p.chain_name, e);
                                r_off.notify();
                                return;
                            }
                        };

                        info!(
                            "CHAIN '{}' - '{}' > Socket is alive at block '{}'",
                            p.chain_name, p.chain_id, bn
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(p.update_interval_sec))
                            .await;
                    }
                });
            }
        }
    }
}
