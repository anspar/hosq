use std::{path::PathBuf, str::FromStr};

use futures_util::StreamExt;
use hyper::{body::HttpBody, header::HeaderName, http::HeaderValue};
use rand::{rngs::StdRng, Rng};
use rocket::{
    data::{Data, FromData, Outcome, ToByteUnit},
    http::Status,
    request::FromRequest,
    response::{
        status,
        stream::{ByteStream, ReaderStream},
    },
    serde::json::Json,
    tokio::{io::AsyncReadExt, select},
    Request,
};
use serde_json::Value;

pub struct ProxyUploadData {
    pub data: Value,
}
#[derive(Debug)]
pub enum ProxyError {
    ProxyFailed(String),
}

#[rocket::async_trait]
impl<'r> FromData<'r> for ProxyUploadData {
    type Error = ProxyError;

    async fn from_data(r: &'r Request<'_>, data: Data<'r>) -> Outcome<'r, Self> {
        let state = r.rocket().state::<crate::types::State>().unwrap();
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
            .method(hyper::Method::from_str(r.method().as_str()).unwrap())
            .uri(uri_string);
        let headers = proxy_req.headers_mut().unwrap();
        r.headers().clone().into_iter().for_each(|h| {
            headers.insert(
                HeaderName::from_str(h.name().as_str()).unwrap(),
                HeaderValue::from_str(&h.value().to_owned()).unwrap(),
            );
        });
        let (mut sender, body) = hyper::body::Body::channel();

        let mut data = data.open(100.mebibytes());
        let mut buffer = [0u8; 4096];

        select! {
            biased;
            res = web_client
            .request(proxy_req.body(body).unwrap()) => {
                println!(" ee \t\t{:?}", res);
                return Outcome::Success(Self {
                    data: serde_json::from_slice::<Value>(
                        &res.unwrap().body_mut()
                        .data()
                        .await
                        .unwrap()
                        .unwrap()
                    ).unwrap()
                })
            }

            _ = async {
                println!("\n\t\t Loop call\n");
                loop{
                    match data.read_exact(&mut buffer).await {
                        Ok(_) => {
                            println!("\t\tLooping {buffer:?}");
                                sender
                                    .send_data(hyper::body::Bytes::copy_from_slice(&buffer[..]))
                                    .await
                                    .unwrap();
                        }
                        Err(e) => {
                            error!(" gg {e}");
                            let mut last_bytes: usize = 4096 - 1;
                            for i in 1..4096 {
                                if buffer[4096 - i] > 0 {
                                    break;
                                }
                                last_bytes -= 1;
                            }
                            sender
                                .send_data(hyper::body::Bytes::copy_from_slice(&buffer[..last_bytes]))
                                .await
                                .unwrap();
                                drop(sender);
                            break;
                        }
                    }
                }
                loop{
                    rocket::tokio::time::sleep(rocket::tokio::time::Duration::from_secs(
                        10,
                        ))
                        .await;
                }
            } => {
                    println!("\n\t\t Loopend\n");
            }

        }

        Outcome::Failure((
            Status::InternalServerError,
            ProxyError::ProxyFailed("".to_owned()),
        ))
    }
}

#[post("/file/upload?<dir>", data = "<data>")]
pub async fn upload(dir: Option<bool>, data: ProxyUploadData) -> status::Custom<Json<Value>> {
    status::Custom(Status::Ok, Json(data.data))
}

#[derive(Debug)]
pub struct ProxyIpfsData {
    pub data: reqwest::Response,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ProxyIpfsData {
    // use responder to set headers, return stream from main handler
    type Error = ProxyError;

    async fn from_request(r: &'r Request<'_>) -> rocket::request::Outcome<Self, Self::Error> {
        let state = r.rocket().state::<crate::types::State>().unwrap();
        let node_index = if state.nodes.len() == 1 {
            0
        } else {
            let mut rng: StdRng = rand::SeedableRng::from_entropy();
            rng.gen_range(0..state.nodes.len() - 1)
        };
        let uri_string = format!(
            "{}/ipfs/{}",
            state.nodes[node_index].gateway,
            r.segments::<PathBuf>(1..).unwrap().to_str().unwrap()
        );
        println!("u\t\t{uri_string}");
        let web_client = reqwest::Client::new().get(uri_string);
        let mut ipfs_headers = reqwest::header::HeaderMap::new();
        r.headers().clone().into_iter().for_each(|h| {
            ipfs_headers.append(
                reqwest::header::HeaderName::from_str(h.name().as_str()).unwrap(),
                reqwest::header::HeaderValue::from_str(&h.value().to_owned()).unwrap(),
            );
        });

        let res = web_client.headers(ipfs_headers).send().await.unwrap();

        rocket::request::Outcome::Success(Self { data: res })
    }
}

impl<'r> rocket::response::Responder<'r, 'static> for ProxyIpfsData {
    fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'static> {
        let mut res = rocket::Response::build().finalize();
        // copy_response_headers(&self.data, &mut res);
        for header_name in self.data.headers().keys() {
            if let Some(header_value) = self.data.headers().get(header_name) {
                if let Ok(header_value) = String::from_utf8(header_value.as_bytes().to_vec()) {
                    res.set_header(rocket::http::Header::new(
                        header_name.to_string(),
                        header_value,
                    ));
                }
            }
        }

        let ss = ByteStream::from(
            self.data
                .bytes_stream()
                .filter_map(|i| async move { i.ok() }),
        );
        let ss = ss.0.map(std::io::Cursor::new);
        res.set_streamed_body(ReaderStream::from(ss));
        Ok(res)
    }
}

#[get("/ipfs/<cid..>")]
pub async fn ipfs(cid: PathBuf, ipfs_resp: ProxyIpfsData) -> ProxyIpfsData {
    // println!("{:?}", ipfs_resp.data);
    ipfs_resp
}
