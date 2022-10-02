use std::{io::ErrorKind, path::PathBuf, str::FromStr};

use anyhow::anyhow;
use hyper::{self, body::HttpBody, header::HeaderName, http::HeaderValue};
use rand::{rngs::StdRng, Rng};
use reqwest::Response;
use rocket::{
    data::ToByteUnit,
    tokio::{io::AsyncReadExt, join},
    Data, Request,
};
use serde_json::Value;

pub async fn upload_to_ipfs(r: &Request<'_>, data: Data<'_>) -> Result<Value, anyhow::Error> {
    let state = match r.rocket().state::<crate::types::State>() {
        Some(v) => v,
        None => return Err(anyhow!("Error Getting rocket state")),
    };
    let node_index = if state.nodes.len() == 1 {
        0
    } else {
        let mut rng: StdRng = rand::SeedableRng::from_entropy();
        rng.gen_range(0..state.nodes.len() - 1)
    };
    let uri_string = match r.query_value::<bool>("dir") {
            Some(b) if Ok(true)==b => format!(
                "{}/api/v0/add?progress=false&pin=false&wrap-with-directory=true&cid-version=1&silent=true",
                state.nodes[node_index].api_url
            ),
            _ => format!(
                "{}/api/v0/add?progress=false&pin=false&cid-version=1&quieter=true",
                state.nodes[node_index].api_url
            )
        };

    let web_client = hyper::Client::new();
    let mut proxy_req = hyper::Request::builder()
        .method(hyper::Method::from_str(r.method().as_str())?)
        .uri(uri_string);
    let headers = match proxy_req.headers_mut() {
        Some(v) => v,
        None => return Err(anyhow!("Error getting mut ref to proxy headers")),
    };
    for h in r.headers().clone().into_iter() {
        headers.insert(
            HeaderName::from_str(h.name().as_str())?,
            HeaderValue::from_str(&h.value().to_owned())?,
        );
    }
    let (mut sender, body) = hyper::body::Body::channel();

    let mut data = data.open(100.mebibytes());
    let mut buffer = [0u8; 4096];

    let (proxy_req, data_in) = join!(web_client.request(proxy_req.body(body)?), async {
        loop {
            match data.read_exact(&mut buffer).await {
                Ok(_) => {
                    sender
                        .send_data(hyper::body::Bytes::copy_from_slice(&buffer[..]))
                        .await?;
                }
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                    // error!("{e}");
                    let mut last_bytes: usize = 4096 - 1;
                    for i in 1..4096 {
                        if buffer[4096 - i] > 0 {
                            break;
                        }
                        last_bytes -= 1;
                    }
                    sender
                        .send_data(hyper::body::Bytes::copy_from_slice(&buffer[..last_bytes]))
                        .await?;
                    drop(sender);
                    break;
                }
                Err(e) => return Err(anyhow!("Error Reading bytes from stream: {e}")),
            }
        }
        Ok::<(), anyhow::Error>(())
    });

    data_in?;

    Ok(serde_json::from_slice::<Value>(
        &match proxy_req?.body_mut().data().await {
            Some(v) => v?,
            None => return Err(anyhow!("Error getting IPFS response body")),
        },
    )?)
}

pub async fn get_from_ipfs(r: &Request<'_>) -> Result<Response, anyhow::Error> {
    let state = match r.rocket().state::<crate::types::State>() {
        Some(v) => v,
        None => return Err(anyhow!("Error Getting rocket state")),
    };
    let node_index = if state.nodes.len() == 1 {
        0
    } else {
        let mut rng: StdRng = rand::SeedableRng::from_entropy();
        rng.gen_range(0..state.nodes.len() - 1)
    };
    let uri_string = format!(
        "{}/ipfs/{}",
        state.nodes[node_index].gateway,
        &match r.segments::<PathBuf>(1..){
            Ok(v)=> v,
            Err(e)=> return Err(anyhow!("{e:?}"))
        }.display()
    );
    println!("u\t\t{uri_string}");
    let web_client = reqwest::Client::new().get(uri_string);
    let mut ipfs_headers = reqwest::header::HeaderMap::new();
    for h in r.headers().clone().into_iter(){
        ipfs_headers.append(
            reqwest::header::HeaderName::from_str(h.name().as_str())?,
            reqwest::header::HeaderValue::from_str(&h.value().to_owned())?,
        );
    }

    Ok(web_client.headers(ipfs_headers).send().await?)
}
