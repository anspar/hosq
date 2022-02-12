use super::db;
use crate::types::{self, DbConn, db::{PinnedCIDs, EventAddProviderResponse}, Web3Node};
use crate::yaml_parser::IPFSNode;
use postgres::Client;
use rand::Rng;
use rocket::http::Status;
use rocket::response::stream::{Event, EventStream};
use rocket::{
    data::{Data, ToByteUnit},
    response::{content::Json, status::Custom},
    State,
};
use serde_json::json;
use std::sync::Arc;

async fn get_block_number(chain_id: i64, providers: Arc<Vec<Web3Node>>) -> Option<(u64, u64)>{
    for provider in &**providers {
        if provider.chain_id == chain_id {
            // return Option::Some(provider.to_owned());
            match provider.web3.eth().block_number().await{
                Ok(v)=>{return Option::Some((v.as_u64(), provider.block_time_sec))}
                Err(e)=>{
                    error!(
                        "Error getting block number for {}: {:?}",
                        &provider.chain_name, e
                    );
                    return Option::None
                }
            }
        }
    }

    Option::None
}

#[post("/file/upload?<name>&<chain_id>&<address>", data = "<file>")]
pub async fn upload_file(
    name: String,
    file: Data<'_>,
    nodes: &State<Arc<Vec<IPFSNode>>>,
    providers: &State<Arc<Vec<Web3Node>>>,
    chain_id: i64,
    address: String,
    psql: DbConn,
) -> Result<EventStream![], Custom<Json<String>>> {
    let (update_block, b_time) = match get_block_number(chain_id, (**providers).clone()).await{
        Some(v) => v,
        None => {
            return Err(Custom(
                Status::BadRequest,
                Json("Internal Error, Unsupported chain".to_owned()),
            ))
        }
    };

    let end_block = update_block + (604800 / b_time); // 7 Days

    let rng = rand::thread_rng().gen_range(0..nodes.len());
    //todo: use async buffer don't store the file on memory.
    let bytes = file
        .open(100.mebibytes())
        .into_bytes()
        .await
        .unwrap()
        .map(|m| m).to_vec();
    
    let form = reqwest::multipart::Form::new()
    .part("path", reqwest::multipart::Part::stream(bytes).file_name(name));
    
    let req = reqwest::Client::new()
    .post(format!("{}/api/v0/add?progress=true&pin=false", nodes[rng].api_url))
    .header("Content-Disposition", "form-data")
    .multipart(form);

    let req = if let Some(login) = &nodes[rng].login {
        req.basic_auth(login, nodes[rng].password.as_ref())
    } else {
        req
    };

    let mut res = req.send().await.unwrap();
    if res.status().is_success() {
        return Ok(EventStream! {
            let mut chunk = types::IPFSAddResponse::default();
            while let Some(next) = res.chunk().await.unwrap() {
                chunk = serde_json::from_slice(&next.to_vec()[..]).unwrap();
                if let Some(_) = chunk.hash.clone(){
                    break;
                }
                // error!("{:?}", &chunk);
                yield Event::json(&chunk);
            }
            // info!("{:?}", chunk.hash);
            let cid = chunk.hash.as_ref().unwrap().clone();
            let result: Result<bool, postgres::Error> = psql.run(move|client|{
                if db::cid_exists(client, &cid)?{
                    return Ok(false);
                }
                db::add_valid_block(client, types::db::EventUpdateValidBlock{
                    chain_id,
                    cid,
                    donor: address,
                    update_block: update_block as i64,
                    end_block: end_block as i64,
                    manual_add: Option::Some(true),
                })?;
                Ok(true)
            }).await;
            match result{
                Ok(v)=>{
                    chunk.first_import = Option::Some(v);
                    yield Event::json(&chunk);
                }
                Err(e)=>{error!("Error Uploading {}", e); yield Event::data("Internal Error");}
            };

        });
    }else{
        error!("{:?}", res.status());
    }

    Err(Custom(
        Status::InternalServerError,
        Json("Internal Error :(".to_owned()),
    ))
}

#[get("/pinned_cids?<address>&<chain_id>")]
pub async fn get_cids(address: String, chain_id: i64, psql: DbConn, providers: &State<Arc<Vec<types::Web3Node>>>,) -> Custom<Option<Json<String>>>{
    let bn = match get_block_number(chain_id, (**providers).clone()).await{
        Some(v) => v.0,
        None => {
            return Custom(
                Status::BadRequest,
                Option::None,
            )
        }
    };
    match psql.run( move |client: &mut Client|{
        let res = client.query("
        SELECT euvb.cid, euvb.donor, min(euvb.update_block), max(euvb.end_block) as eb, 
                (SELECT count(pc.node) 
                FROM pinned_cids as pc 
                WHERE pc.chain_id=$1::BIGINT AND pc.cid=euvb.cid AND pc.end_block>=$3::BIGINT),
                (SELECT count(fc.node) 
                FROM failed_pins as fc 
                WHERE fc.chain_id=$1::BIGINT AND fc.cid=euvb.cid AND fc.end_block>=$3::BIGINT)
        FROM event_update_valid_block as euvb
        WHERE euvb.chain_id=$1::BIGINT AND euvb.donor=LOWER($2::TEXT) 
        GROUP BY euvb.cid, euvb.donor
        ORDER BY eb ASC LIMIT 100;
        ", &[&chain_id, &address, &(bn as i64)])?;

        Ok(res.into_iter().map(|r| PinnedCIDs{
            cid: r.get(0),
            donor: r.get(1),
            update_block: r.get(2),
            end_block: r.get(3),
            node_count: r.get(4),
            failed_node_count: r.get(5),
        }).collect())
    })
    .await{
        Ok::<Vec<PinnedCIDs>, postgres::Error>(v)=>Custom(
                    Status::Ok,
                    Option::Some(Json(json!(v).to_string()))
                ),
        Err(e)=>{
            error!("Error collecting pinned CIDs > {}", e);
            return Custom(
                Status::InternalServerError,
                Option::None
            )
        }
    }
}

#[get("/providers?<chain_id>")]
pub async fn get_providers(chain_id: i64, psql: DbConn) -> Custom<Option<Json<String>>>{
    match psql.run( move |client: &mut Client|{
        let res = client.query("
        SELECT provider_id, block_price_gwei, name, api_url
        FROM event_add_provider
        WHERE chain_id=$1::BIGINT 
        ORDER BY block_price_gwei ASC, name ASC 
        LIMIT 100;
        ", &[&chain_id])?;

        Ok(res.into_iter().map(|r| EventAddProviderResponse{
            provider_id: r.get(0),
            block_price_gwei: r.get(1),
            name: r.get(2),
            api_url: r.get(3),
            update_block: None            
        }).collect())
    })
    .await{
        Ok::<Vec<EventAddProviderResponse>, postgres::Error>(v)=>Custom(
                    Status::Ok,
                    Option::Some(Json(json!(v).to_string()))
                ),
        Err(e)=>{
            error!("Error collecting pinned CIDs > {}", e);
            return Custom(
                Status::InternalServerError,
                Option::None
            )
        }
    }
}

#[get("/provider?<chain_id>&<address>")]
pub async fn get_provider(chain_id: i64, address: String, psql: DbConn) -> Custom<Option<Json<String>>>{
    match psql.run( move |client: &mut Client|{
        let res = client.query("
        SELECT provider_id, block_price_gwei, name, api_url, update_block
        FROM event_add_provider
        WHERE chain_id=$1::BIGINT AND owner=LOWER($2::TEXT) 
        ORDER BY name ASC 
        LIMIT 100;
        ", &[&chain_id, &address])?;

        Ok(res.into_iter().map(|r| EventAddProviderResponse{
            provider_id: r.get(0),
            block_price_gwei: r.get(1),
            name: r.get(2),
            api_url: r.get(3),            
            update_block: r.get(4)            
        }).collect())
    })
    .await{
        Ok::<Vec<EventAddProviderResponse>, postgres::Error>(v)=>Custom(
                    Status::Ok,
                    Option::Some(Json(json!(v).to_string()))
                ),
        Err(e)=>{
            error!("Error collecting pinned CIDs > {}", e);
            return Custom(
                Status::InternalServerError,
                Option::None
            )
        }
    }
}
