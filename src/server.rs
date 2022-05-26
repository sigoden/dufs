use crate::{Args, BoxResult};

use futures::TryStreamExt;
use hyper::header::HeaderValue;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, StatusCode};
use percent_encoding::percent_decode;
use serde::Serialize;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::{fs, io};
use tokio_util::codec::{BytesCodec, FramedRead};
use tokio_util::io::StreamReader;

type Request = hyper::Request<Body>;
type Response = hyper::Response<Body>;

macro_rules! status_code {
    ($status:expr) => {
        hyper::Response::builder()
            .status($status)
            .body($status.canonical_reason().unwrap_or_default().into())
            .unwrap()
    };
}

const INDEX_HTML: &str = include_str!("index.html");
const INDEX_CSS: &str = include_str!("index.css");

pub async fn serve(args: Args) -> BoxResult<()> {
    let address = args.address()?;
    let inner = Arc::new(InnerService::new(args));
    let make_svc = make_service_fn(move |_| {
        let inner = inner.clone();
        async {
            Ok::<_, Infallible>(service_fn(move |req| {
                let inner = inner.clone();
                inner.handle(req)
            }))
        }
    });

    let server = hyper::Server::try_bind(&address)?.serve(make_svc);
    let address = server.local_addr();
    eprintln!("Files served on http://{}", address);
    server.await?;

    Ok(())
}

struct InnerService {
    args: Args,
}

impl InnerService {
    pub fn new(args: Args) -> Self {
        Self { args }
    }

    pub async fn handle(self: Arc<Self>, req: Request) -> Result<Response, hyper::Error> {
        if !self.auth_guard(&req).unwrap_or_default() {
            let mut res = status_code!(StatusCode::UNAUTHORIZED);
            res.headers_mut()
                .insert("WWW-Authenticate", HeaderValue::from_static("Basic"));
            return Ok(res);
        }

        let res = if req.method() == Method::GET {
            self.handle_static(req).await
        } else if req.method() == Method::PUT {
            if self.args.readonly {
                return Ok(status_code!(StatusCode::FORBIDDEN));
            }
            self.handle_upload(req).await
        } else if req.method() == Method::DELETE {
            self.handle_delete(req).await
        } else {
            return Ok(status_code!(StatusCode::NOT_FOUND));
        };
        Ok(res.unwrap_or_else(|_| status_code!(StatusCode::INTERNAL_SERVER_ERROR)))
    }

    async fn handle_static(&self, req: Request) -> BoxResult<Response> {
        let path = match self.get_file_path(req.uri().path())? {
            Some(path) => path,
            None => return Ok(status_code!(StatusCode::FORBIDDEN)),
        };
        match fs::metadata(&path).await {
            Ok(meta) => {
                if meta.is_dir() {
                    self.handle_send_dir(path.as_path()).await
                } else {
                    self.handle_send_file(path.as_path()).await
                }
            }
            Err(_) => return Ok(status_code!(StatusCode::NOT_FOUND)),
        }
    }

    async fn handle_upload(&self, mut req: Request) -> BoxResult<Response> {
        let path = match self.get_file_path(req.uri().path())? {
            Some(path) => path,
            None => return Ok(status_code!(StatusCode::FORBIDDEN)),
        };

        if !fs::metadata(&path.parent().unwrap()).await?.is_dir() {
            return Ok(status_code!(StatusCode::FORBIDDEN));
        }

        let mut file = fs::File::create(path).await?;

        let body_with_io_error = req
            .body_mut()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err));

        let body_reader = StreamReader::new(body_with_io_error);

        futures::pin_mut!(body_reader);

        io::copy(&mut body_reader, &mut file).await?;

        return Ok(status_code!(StatusCode::OK));
    }

    async fn handle_delete(&self, req: Request) -> BoxResult<Response> {
        let path = match self.get_file_path(req.uri().path())? {
            Some(path) => path,
            None => return Ok(status_code!(StatusCode::FORBIDDEN)),
        };

        let meta = fs::metadata(&path).await?;
        if meta.is_file() {
            fs::remove_file(path).await?;
        } else {
            fs::remove_dir_all(path).await?;
        }
        Ok(status_code!(StatusCode::OK))
    }

    async fn handle_send_dir(&self, path: &Path) -> BoxResult<Response> {
        let base_path = &self.args.path;
        let mut rd = fs::read_dir(path).await?;
        let mut paths: Vec<PathItem> = vec![];
        while let Some(entry) = rd.next_entry().await? {
            let entry_path = entry.path();
            let rel_path = entry_path.strip_prefix(base_path).unwrap();
            let meta = fs::metadata(&entry_path).await?;
            let s_meta = fs::symlink_metadata(&entry_path).await?;
            let is_dir = meta.is_dir();
            let is_symlink = s_meta.file_type().is_symlink();
            let path_type = match (is_symlink, is_dir) {
                (true, true) => PathType::SymlinkDir,
                (false, true) => PathType::Dir,
                (true, false) => PathType::SymlinkFile,
                (false, false) => PathType::File,
            };
            let mtime = meta
                .modified()?
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|v| v.as_millis() as u64);
            let size = match path_type {
                PathType::Dir | PathType::SymlinkDir => None,
                PathType::File | PathType::SymlinkFile => Some(meta.len()),
            };
            let name = rel_path
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default()
                .to_owned();
            paths.push(PathItem {
                path_type,
                name,
                path: format!("/{}", normalize_path(rel_path)),
                mtime,
                size,
            })
        }

        paths.sort_unstable();
        let breadcrumb = self.get_breadcrumb(path);
        let data = SendDirData {
            breadcrumb,
            paths,
            readonly: self.args.readonly,
        };
        let data = serde_json::to_string(&data).unwrap();

        let mut output =
            INDEX_HTML.replace("__STYLE__", &format!("<style>\n{}</style>", INDEX_CSS));
        output = output.replace("__DATA__", &data);

        Ok(hyper::Response::builder().body(output.into()).unwrap())
    }

    async fn handle_send_file(&self, path: &Path) -> BoxResult<Response> {
        let file = fs::File::open(path).await?;
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);
        Ok(Response::new(body))
    }

    fn auth_guard(&self, req: &Request) -> BoxResult<bool> {
        if let Some(auth) = &self.args.auth {
            if let Some(value) = req.headers().get("Authorization") {
                let value = value.to_str()?;
                let value = if value.contains("Basic ") {
                    &value[6..]
                } else {
                    return Ok(false);
                };
                let value = base64::decode(value)?;
                let value = std::str::from_utf8(&value)?;
                return Ok(value == auth);
            } else {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn get_breadcrumb(&self, path: &Path) -> String {
        let path = match self.args.path.parent() {
            Some(p) => path.strip_prefix(p).unwrap(),
            None => path,
        };
        normalize_path(path)
    }

    fn get_file_path(&self, path: &str) -> BoxResult<Option<PathBuf>> {
        let decoded_path = percent_decode(path[1..].as_bytes()).decode_utf8()?;
        let slashes_switched = if cfg!(windows) {
            decoded_path.replace('/', "\\")
        } else {
            decoded_path.into_owned()
        };
        let path = self.args.path.join(&slashes_switched);
        if path.starts_with(&self.args.path) {
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Serialize, Eq, PartialEq, Ord, PartialOrd)]
struct SendDirData {
    breadcrumb: String,
    paths: Vec<PathItem>,
    readonly: bool,
}

#[derive(Debug, Serialize, Eq, PartialEq, Ord, PartialOrd)]
struct PathItem {
    path_type: PathType,
    name: String,
    path: String,
    mtime: Option<u64>,
    size: Option<u64>,
}

#[derive(Debug, Serialize, Eq, PartialEq, Ord, PartialOrd)]
enum PathType {
    Dir,
    SymlinkDir,
    File,
    SymlinkFile,
}

fn normalize_path<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref().to_str().unwrap_or_default();
    if cfg!(windows) {
        path.replace('\\', "/")
    } else {
        path.to_string()
    }
}
