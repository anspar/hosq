use rand::{rngs::StdRng, Rng};
use rocket::{response::Redirect, State};

use crate::types;

#[post("/file/upload?<dir>")]
pub async fn upload(state: &State<types::State>, dir: Option<bool>) -> Redirect {
    let node_index = if state.nodes.len() == 1 {
        0
    } else {
        let mut rng: StdRng = rand::SeedableRng::from_entropy();
        rng.gen_range(0..state.nodes.len() - 1)
    };
    let uri_string = match dir {
        Some(b) if b => format!(
            "{}/api/v0/add?progress=false&pin=false&wrap-with-directory=true&cid-version=1&silent=true",
            state.nodes[node_index].api_url
        ),
        _ => format!(
            "{}/api/v0/add?progress=false&pin=false&cid-version=1&quieter=true",
            state.nodes[node_index].api_url
        )
    };

    Redirect::permanent(uri_string)
}

#[post("/file/download?<cid>")]
pub async fn download(state: &State<types::State>, cid: String) -> Redirect {
    let node_index = if state.nodes.len() == 1 {
        0
    } else {
        let mut rng: StdRng = rand::SeedableRng::from_entropy();
        rng.gen_range(0..state.nodes.len() - 1)
    };
    let uri_string = format!("{}/api/v0/get?arg={}", state.nodes[node_index].api_url, cid);

    Redirect::permanent(uri_string)
}

#[get("/ipfs/<cid>")]
pub async fn ipfs(state: &State<types::State>, cid: String) -> Redirect {
    let node_index = if state.nodes.len() == 1 {
        0
    } else {
        let mut rng: StdRng = rand::SeedableRng::from_entropy();
        rng.gen_range(0..state.nodes.len() - 1)
    };

    let uri_string = format!("{}/ipfs{}", state.nodes[node_index].gateway, cid);

    Redirect::permanent(uri_string)
}
