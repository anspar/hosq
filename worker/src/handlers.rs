use super::db;
use crate::types;
use crate::yaml_parser::IPFSNode;
use rand::Rng;
use rocket::http::Status;
use rocket::response::stream::{Event, EventStream};
use rocket::{
    data::{Data, ToByteUnit},
    response::{content::Json, status::Custom},
    State,
};
use std::sync::Arc;

#[post("/file/upload?<name>&<chain_id>&<address>", data = "<file>")]
pub async fn upload_file(
    name: String,
    file: Data<'_>,
    nodes: &State<Arc<Vec<IPFSNode>>>,
    providers: &State<Arc<Vec<types::Web3Node>>>,
    chain_id: i64,
    address: String,
    psql: types::DbConn,
) -> Result<EventStream![], Custom<Json<String>>> {
    let mut web3_node = Option::None;
    for provider in &***providers.clone() {
        if provider.chain_id == chain_id {
            web3_node = Option::Some(provider);
            break;
        }
    }

    let web3_node = match web3_node {
        Some(v) => v,
        None => {
            return Err(Custom(
                Status::BadRequest,
                Json("Internal Error".to_owned()),
            ))
        }
    };

    let (update_block, end_block) = match web3_node.web3.clone().eth().block_number().await {
        Ok(v) => {
            let blocks_from_time_sec = 604800 / web3_node.block_time_sec; // 7 days
            (
                v.as_u64() as i64,
                (v.as_u64() + blocks_from_time_sec) as i64,
            )
        }
        Err(e) => {
            error!(
                "Error getting block number for {}: {:?}",
                &web3_node.chain_name, e
            );
            return Err(Custom(
                Status::InternalServerError,
                Json("Internal Error".to_owned()),
            ));
        }
    };

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
    .post(format!("{}/api/v0/add?progress=true", nodes[rng].api_url))
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
            let result: Result<(), postgres::Error> = psql.run(move|client|{
                // todo check if cid exists, don't insert if does.
                db::add_valid_block(client, types::db::EventUpdateValidBlock{
                    chain_id,
                    cid,
                    donor: address,
                    update_block,
                    end_block,
                    manual_add: Option::Some(true),
                })
            }).await;
            match result{
                Ok(_)=>{yield Event::json(&chunk);}
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
