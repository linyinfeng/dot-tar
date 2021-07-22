use bytes::Bytes;
use rocket::fairing::AdHoc;
use rocket::http::ContentType;
use rocket::response::Responder;
use rocket::routes;
use rocket::State;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io;
use std::path::PathBuf;

#[rocket::get("/<scheme>/<authority>/<path..>?<query..>")]
async fn index(
    config: &State<Config>,
    scheme: String,
    authority: String,
    path: PathBuf,
    query: BTreeMap<String, String>,
) -> Result<(ContentType, Vec<u8>), Error> {
    if !config.authority_allow_list.contains(&authority) {
        return Err(Error::Simple(format!(
            "the authority \"{}\" is not in allow list",
            authority
        )));
    }

    /* url construction */
    let url_str = format!("{}://{}", scheme, authority);
    let mut url = reqwest::Url::parse(&url_str)
        .map_err(|e| Error::Simple(format!("invalid url part: {}", e)))?;
    let path_str = path
        .to_str()
        .ok_or(Error::Simple("invalid path encoding".to_owned()))?;
    url.set_path(
        path_str
            .strip_suffix(".tar")
            .ok_or(Error::Simple("path must end with \".tar\"".to_owned()))?,
    );
    let query_vec: Vec<_> = query
        .into_iter()
        .map(|(l, r)| format!("{}={}", l, r))
        .collect();
    let query_str = query_vec.join("&");
    if !query_str.is_empty() {
        url.set_query(Some(&query_str));
    }

    log::info!("request url: {:?}", url);
    let bytes = reqwest::get(url)
        .await
        .map_err(reqwest_error)?
        .bytes()
        .await
        .map_err(reqwest_error)?;

    let tar = build_tar(bytes).await?;
    Ok((ContentType::TAR, tar))
}

async fn build_tar(b: Bytes) -> Result<Vec<u8>, Error> {
    let buffer = Vec::new();
    let mut ar = async_tar::Builder::new(buffer);

    let mut header = async_tar::Header::new_gnu();
    header.set_size(b.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();

    ar.append_data(&mut header, "data", &b[..])
        .await
        .map_err(Error::Io)?;

    ar.into_inner().await.map_err(Error::Io)
}

#[derive(Deserialize)]
struct Config {
    authority_allow_list: BTreeSet<String>,
}

#[derive(Responder, Debug)]
enum Error {
    #[response(status = 400)]
    Simple(String),
    #[response(status = 400)]
    Request(String),
    #[response(status = 400)]
    Io(io::Error),
}

fn reqwest_error(e: reqwest::Error) -> Error {
    Error::Request(format!("request error: {}", e))
}

#[rocket::main]
async fn main() {
    let rkt = rocket::build()
        .mount("/", routes![index])
        .attach(AdHoc::config::<Config>());
    match rkt.launch().await {
        Ok(()) => (),
        Err(e) => println!("{}", e),
    }
}
