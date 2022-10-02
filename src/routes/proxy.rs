use std::path::PathBuf;

use futures_util::StreamExt;
use rocket::{
    data::{Data, FromData, Outcome},
    http::Status,
    request::FromRequest,
    response::{
        status,
        stream::{ByteStream, ReaderStream},
    },
    serde::json::Json,
    Request,
};
use serde_json::Value;

use crate::utils::proxy::{upload_to_ipfs, get_from_ipfs};

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
        match upload_to_ipfs(r, data).await {
            Ok(v) => Outcome::Success(Self { data: v }),
            Err(e) =>{
                error!("Uploading to IPFS: {e}");
                Outcome::Failure((Status::InternalServerError, ProxyError::ProxyFailed("Failed to upload to IPFS".to_owned())))
            }
        }
    }
}

/// Upload file(s) to IPFS node.
///  
/// Use multipart form for payload where key=file and value=blob
/// 
/// Wrap the files in dir with query `?dir=true` 
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
        match get_from_ipfs(r).await{
            Ok(v)=>rocket::request::Outcome::Success(Self { data: v }),
            Err(e)=>{
                error!("Error get ipfs: {e}");
                rocket::request::Outcome::Failure((Status::InternalServerError, ProxyError::ProxyFailed("Failed to get data from IPFS".to_owned())))
            }
        } 
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

/// Retrieve data from IPFS node
#[get("/ipfs/<cid..>")]
pub async fn ipfs(cid: PathBuf, ipfs_resp: ProxyIpfsData) -> ProxyIpfsData {
    // println!("{:?}", ipfs_resp.data);
    ipfs_resp
}
