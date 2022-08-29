use bytes::Bytes;
use futures::AsyncWriteExt;
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
use once_cell::sync::Lazy;
use maplit::btreemap;
use futures::AsyncWrite;
use async_compression::futures::write as encoders;

#[derive(Debug, Clone)]
enum ArchiveType {
    None,
    Tar
}

impl Default for ArchiveType {
    fn default() -> Self {
        Self::None
    }
}

impl ArchiveType {
    async fn archive(&self, b: Bytes) -> Result<Vec<u8>, Error> {
        match self {
            ArchiveType::None => Ok(b.to_vec()),
            ArchiveType::Tar => build_tar(b).await,
        }
    }
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

#[derive(Debug, Clone)]
enum CompressionType {
    None,
    Brotli,
    Bzip2,
    Gzip,
    Xz,
    Zstd
}

impl Default for CompressionType {
    fn default() -> Self {
        Self::None
    }
}

impl CompressionType {
    async fn compress(&self, b: Vec<u8>) -> Result<Vec<u8>, Error> {
        match self {
            CompressionType::None => Ok(b),
            algorithm => {
                let mut encoder = algorithm.encoder().unwrap();
                encoder.write_all(&b[..]).await.map_err(Error::Io)?;
                encoder.close().await.map_err(Error::Io)?;
                // can't move out of trait object
                // copy the buffer instead
                Ok(encoder.buffer().to_vec())
            }
        }
    }

    fn encoder(&self) -> Option<Box<dyn Encoder + Send + Unpin>> {
        use encoders::*;
        let buffer = Vec::new();

        match self {
            CompressionType::None => None,
            CompressionType::Brotli => Some(Box::new(BrotliEncoder::new(buffer))),
            CompressionType::Bzip2 => Some(Box::new(BzEncoder::new(buffer))),
            CompressionType::Gzip => Some(Box::new(GzipEncoder::new(buffer))),
            CompressionType::Xz => Some(Box::new(XzEncoder::new(buffer))),
            CompressionType::Zstd => Some(Box::new(ZstdEncoder::new(buffer))),
        }
    }
}

trait Encoder : AsyncWrite {
    fn buffer(&self) -> &[u8];
}

impl Encoder for encoders::BrotliEncoder<Vec<u8>> {
    fn buffer(&self) -> &[u8] { self.get_ref() }
}
impl Encoder for encoders::BzEncoder<Vec<u8>> {
    fn buffer(&self) -> &[u8] { self.get_ref() }
}
impl Encoder for encoders::GzipEncoder<Vec<u8>> {
    fn buffer(&self) -> &[u8] { self.get_ref() }
}
impl Encoder for encoders::XzEncoder<Vec<u8>> {
    fn buffer(&self) -> &[u8] { self.get_ref() }
}
impl Encoder for encoders::ZstdEncoder<Vec<u8>> {
    fn buffer(&self) -> &[u8] { self.get_ref() }
}

static ARCHIVE_TYPES: Lazy<BTreeMap<&'static str, ArchiveType>> = Lazy::new(|| btreemap!{
    ".tar" => ArchiveType::Tar,
});

static COMPRESSION_TYPES: Lazy<BTreeMap<&'static str, CompressionType>> = Lazy::new(|| btreemap!{
    ".br" => CompressionType::Brotli,
    ".bz2" => CompressionType::Bzip2,
    ".gz" => CompressionType::Gzip,
    ".xz" => CompressionType::Xz,
    ".zst" => CompressionType::Zstd,
});

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
    let (path_comp_striped, compression_type) = extract_suffix(path_str, &COMPRESSION_TYPES);
    let (url_path, archive_type) = extract_suffix(&path_comp_striped, &ARCHIVE_TYPES);
    url.set_path(&url_path);
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

    let archive = archive_type.archive(bytes).await?;
    let compressed = compression_type.compress(archive).await?;
    let content_type = content_type(archive_type, compression_type);
    Ok((content_type, compressed))
}

fn extract_suffix<'a, T: Default + Clone>(path: &'a str, map: &BTreeMap<&'static str, T>) -> (&'a str, T) {
    let mut result_path = None;
    let mut result_type = None;

    for (suffix, ty) in map.iter() {
        match path.strip_suffix(suffix) {
            Some(stripped) => {
                result_path = Some(stripped);
                result_type = Some(ty.clone())
            },
            None => continue
        }
    }

    (result_path.unwrap_or(path), result_type.unwrap_or(Default::default()))
}

fn content_type(a: ArchiveType, c: CompressionType) -> ContentType {
    match (a, c) {
        (ArchiveType::Tar, CompressionType::None) => ContentType::TAR,
        (_, CompressionType::Brotli) => ContentType::new("application", "x-brotli"),
        (_, CompressionType::Bzip2) => ContentType::new("application", "x-bzip2"),
        (_, CompressionType::Gzip) => ContentType::GZIP,
        (_, CompressionType::Xz) => ContentType::new("application", "x-xz"),
        (_, CompressionType::Zstd) => ContentType::new("application", "zstd"),
        (_, _) => ContentType::Binary
    }
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
    #[response(status = 500)]
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
        Ok(_r) => (),
        Err(e) => println!("{}", e),
    }
}
