use std::{io::ErrorKind, path::PathBuf, str::FromStr};

use anyhow::anyhow;
use hyper::{
    self,
    body::{HttpBody, Sender},
    header::HeaderName,
    http::HeaderValue,
    Body,
};
use rand::{rngs::StdRng, Rng};
use reqwest::Response;
use rocket::{
    data::ToByteUnit,
    tokio::{io::AsyncWrite, join},
    Data, Request,
};
use serde_json::Value;

struct ProxySynchronizer {
    sender: Option<Sender>,
}

impl ProxySynchronizer {
    pub fn new() -> Self {
        Self { sender: None }
    }

    pub fn get_body(&mut self) -> Body {
        let (sender, body) = hyper::body::Body::channel();
        self.sender = Some(sender);
        body
    }
}

impl AsyncWrite for ProxySynchronizer {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        // println!("\tpw: {buf:?}");
        let sender = self.sender.as_mut().unwrap();
        match sender.poll_ready(cx) {
            std::task::Poll::Pending => {
                return std::task::Poll::Pending;
            }
            std::task::Poll::Ready(v) if v.is_err() => {
                return std::task::Poll::Ready(Err(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    format!("failed to poll: {v:?}"),
                )));
            }
            _ => {}
        }

        match sender.try_send_data(hyper::body::Bytes::copy_from_slice(&buf[..])) {
            Ok(_) => std::task::Poll::Ready(Ok(buf.len())),
            Err(e) => std::task::Poll::Ready(Err(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                format!("failed to send: {e:?}"),
            ))),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
}

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
    let mut ps = ProxySynchronizer::new();
    let data = data.open(100.mebibytes());
    let (proxy_req, data_in) = join!(web_client.request(proxy_req.body(ps.get_body())?), async {
        data.stream_to(ps).await?;
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
        &match r.segments::<PathBuf>(1..) {
            Ok(v) => v,
            Err(e) => return Err(anyhow!("{e:?}")),
        }
        .display()
    );
    // println!("\t\t{uri_string}");
    let web_client = reqwest::Client::new().get(uri_string);
    let mut ipfs_headers = reqwest::header::HeaderMap::new();
    for h in r.headers().clone().into_iter() {
        ipfs_headers.append(
            reqwest::header::HeaderName::from_str(h.name().as_str())?,
            reqwest::header::HeaderValue::from_str(&h.value().to_owned())?,
        );
    }

    Ok(web_client.headers(ipfs_headers).send().await?)
}
