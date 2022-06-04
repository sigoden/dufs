use crate::{Args, BoxResult};

use async_walkdir::WalkDir;
use async_zip::read::seek::ZipFileReader;
use async_zip::write::{EntryOptions, ZipFileWriter};
use async_zip::Compression;
use chrono::{Local, TimeZone, Utc};
use futures::stream::StreamExt;
use futures::TryStreamExt;
use get_if_addrs::get_if_addrs;
use headers::{
    AcceptRanges, AccessControlAllowHeaders, AccessControlAllowOrigin, ContentLength, ContentRange,
    ContentType, ETag, HeaderMap, HeaderMapExt, IfModifiedSince, IfNoneMatch, IfRange,
    LastModified, Range,
};
use hyper::header::{
    HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_TYPE, ORIGIN, RANGE,
    WWW_AUTHENTICATE,
};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, StatusCode, Uri};
use percent_encoding::percent_decode;
use rustls::ServerConfig;
use serde::Serialize;
use std::convert::Infallible;
use std::fs::Metadata;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWrite};
use tokio::net::TcpListener;
use tokio::{fs, io};
use tokio_rustls::TlsAcceptor;
use tokio_util::codec::{BytesCodec, FramedRead};
use tokio_util::io::{ReaderStream, StreamReader};

type Request = hyper::Request<Body>;
type Response = hyper::Response<Body>;

const INDEX_HTML: &str = include_str!("../assets/index.html");
const INDEX_CSS: &str = include_str!("../assets/index.css");
const INDEX_JS: &str = include_str!("../assets/index.js");
const INDEX_NAME: &str = "index.html";
const BUF_SIZE: usize = 1024 * 16;

macro_rules! status {
    ($res:ident, $status:expr) => {
        *$res.status_mut() = $status;
        *$res.body_mut() = Body::from($status.canonical_reason().unwrap_or_default());
    };
}

pub async fn serve(args: Args) -> BoxResult<()> {
    match args.tls.as_ref() {
        Some(_) => serve_https(args).await,
        None => serve_http(args).await,
    }
}

pub async fn serve_https(args: Args) -> BoxResult<()> {
    let args = Arc::new(args);
    let socket_addr = args.address()?;
    let (certs, key) = args.tls.clone().unwrap();
    let inner = Arc::new(InnerService::new(args.clone()));
    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    let tls_acceptor = TlsAcceptor::from(Arc::new(config));
    let arc_acceptor = Arc::new(tls_acceptor);
    let listener = TcpListener::bind(&socket_addr).await?;
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    let incoming = hyper::server::accept::from_stream(incoming.filter_map(|socket| async {
        match socket {
            Ok(stream) => match arc_acceptor.clone().accept(stream).await {
                Ok(val) => Some(Ok::<_, Infallible>(val)),
                Err(_) => None,
            },
            Err(_) => None,
        }
    }));
    let server = hyper::Server::builder(incoming).serve(make_service_fn(move |_| {
        let inner = inner.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let inner = inner.clone();
                inner.call(req)
            }))
        }
    }));
    print_listening(args.address.as_str(), args.port, true);
    let graceful = server.with_graceful_shutdown(shutdown_signal());
    graceful.await?;
    Ok(())
}

pub async fn serve_http(args: Args) -> BoxResult<()> {
    let args = Arc::new(args);
    let socket_addr = args.address()?;
    let inner = Arc::new(InnerService::new(args.clone()));
    let server = hyper::Server::try_bind(&socket_addr)?.serve(make_service_fn(move |_| {
        let inner = inner.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let inner = inner.clone();
                inner.call(req)
            }))
        }
    }));
    print_listening(args.address.as_str(), args.port, false);
    let graceful = server.with_graceful_shutdown(shutdown_signal());
    graceful.await?;
    Ok(())
}

struct InnerService {
    args: Arc<Args>,
}

impl InnerService {
    pub fn new(args: Arc<Args>) -> Self {
        Self { args }
    }

    pub async fn call(self: Arc<Self>, req: Request) -> Result<Response, hyper::Error> {
        let method = req.method().clone();
        let uri = req.uri().clone();
        let cors = self.args.cors;

        let timestamp = Local::now().format("%d/%b/%Y %H:%M:%S");
        let mut res = match self.handle(req).await {
            Ok(res) => {
                println!(r#"[{}] "{} {}" - {}"#, timestamp, method, uri, res.status());
                res
            }
            Err(err) => {
                let mut res = Response::default();
                let status = StatusCode::INTERNAL_SERVER_ERROR;
                status!(res, status);
                eprintln!(
                    r#"[{}] "{} {}" - {} {}"#,
                    timestamp, method, uri, status, err
                );
                res
            }
        };

        if cors {
            add_cors(&mut res);
        }
        Ok(res)
    }

    pub async fn handle(self: Arc<Self>, req: Request) -> BoxResult<Response> {
        let mut res = Response::default();

        if !self.auth_guard(&req, &mut res) {
            return Ok(res);
        }

        let req_path = req.uri().path();

        let path = match self.extract_path(req_path) {
            Some(v) => v,
            None => {
                status!(res, StatusCode::FORBIDDEN);
                return Ok(res);
            }
        };
        let path = path.as_path();

        let query = req.uri().query().unwrap_or_default();

        let meta = fs::metadata(path).await.ok();

        let is_miss = meta.is_none();
        let is_dir = meta.map(|v| v.is_dir()).unwrap_or_default();
        let is_file = !is_miss && !is_dir;

        let allow_upload = self.args.allow_upload;
        let allow_delete = self.args.allow_delete;
        let render_index = self.args.render_index;
        let render_spa = self.args.render_spa;

        if !self.args.allow_symlink && !is_miss && !self.is_root_contained(path).await {
            status!(res, StatusCode::NOT_FOUND);
            return Ok(res);
        }

        match req.method() {
            &Method::GET => {
                let headers = req.headers();
                if is_dir {
                    if render_index || render_spa {
                        self.handle_render_index(path, headers, &mut res).await?;
                    } else if query == "zip" {
                        self.handle_zip_dir(path, &mut res).await?;
                    } else if query.starts_with("q=") {
                        self.handle_query_dir(path, &query[3..], &mut res).await?;
                    } else {
                        self.handle_ls_dir(path, true, &mut res).await?;
                    }
                } else if is_file {
                    self.handle_send_file(path, headers, &mut res).await?;
                } else if render_spa {
                    self.handle_render_spa(path, headers, &mut res).await?;
                } else if allow_upload && req_path.ends_with('/') {
                    self.handle_ls_dir(path, false, &mut res).await?;
                } else {
                    status!(res, StatusCode::NOT_FOUND);
                }
            }
            &Method::OPTIONS => {
                self.handle_method_options(&mut res);
            }
            &Method::PUT => {
                if !allow_upload || (!allow_delete && is_file) {
                    status!(res, StatusCode::FORBIDDEN);
                } else {
                    self.handle_upload(path, req, &mut res).await?;
                }
            }
            &Method::DELETE => {
                if !allow_delete {
                    status!(res, StatusCode::FORBIDDEN);
                } else if !is_miss {
                    self.handle_delete(path, is_dir, &mut res).await?
                } else {
                    status!(res, StatusCode::NOT_FOUND);
                }
            }
            method => match method.as_str() {
                "PROPFIND" => {
                    if is_dir {
                        self.handle_propfind_dir(path, &mut res).await?;
                    } else if is_file {
                        self.handle_propfind_file(path, &mut res).await?;
                    } else {
                        status!(res, StatusCode::NOT_FOUND);
                    }
                }
                "MKCOL" if allow_upload && is_miss => self.handle_mkcol(path, &mut res).await?,
                "COPY" if allow_upload && !is_miss => {
                    self.handle_copy(path, req.headers(), &mut res).await?
                }
                "MOVE" if allow_upload && allow_delete && !is_miss => {
                    self.handle_move(path, req.headers(), &mut res).await?
                }
                _ => {
                    status!(res, StatusCode::METHOD_NOT_ALLOWED);
                }
            },
        }
        Ok(res)
    }

    async fn handle_upload(
        &self,
        path: &Path,
        mut req: Request,
        res: &mut Response,
    ) -> BoxResult<()> {
        ensure_path_parent(path).await?;

        let mut file = fs::File::create(&path).await?;

        let body_with_io_error = req
            .body_mut()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err));

        let body_reader = StreamReader::new(body_with_io_error);

        futures::pin_mut!(body_reader);

        io::copy(&mut body_reader, &mut file).await?;

        let query = req.uri().query().unwrap_or_default();
        if query == "unzip" {
            if let Err(e) = self.unzip_file(path).await {
                eprintln!("Failed to unzip {}, {}", path.display(), e);
                status!(res, StatusCode::BAD_REQUEST);
            }
            fs::remove_file(&path).await?;
        }

        status!(res, StatusCode::CREATED);
        Ok(())
    }

    async fn handle_delete(&self, path: &Path, is_dir: bool, res: &mut Response) -> BoxResult<()> {
        match is_dir {
            true => fs::remove_dir_all(path).await?,
            false => fs::remove_file(path).await?,
        }

        status!(res, StatusCode::NO_CONTENT);
        Ok(())
    }

    async fn handle_ls_dir(&self, path: &Path, exist: bool, res: &mut Response) -> BoxResult<()> {
        let mut paths = vec![];
        if exist {
            paths = match self.list_dir(path, path, false).await {
                Ok(paths) => paths,
                Err(_) => {
                    status!(res, StatusCode::FORBIDDEN);
                    return Ok(());
                }
            }
        };
        self.send_index(path, paths, res)
    }

    async fn handle_query_dir(
        &self,
        path: &Path,
        query: &str,
        res: &mut Response,
    ) -> BoxResult<()> {
        let mut paths: Vec<PathItem> = vec![];
        let mut walkdir = WalkDir::new(path);
        while let Some(entry) = walkdir.next().await {
            if let Ok(entry) = entry {
                if !entry
                    .file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&query.to_lowercase())
                {
                    continue;
                }
                if fs::symlink_metadata(entry.path()).await.is_err() {
                    continue;
                }
                if let Ok(Some(item)) = self.to_pathitem(entry.path(), path.to_path_buf()).await {
                    paths.push(item);
                }
            }
        }
        self.send_index(path, paths, res)
    }

    async fn handle_zip_dir(&self, path: &Path, res: &mut Response) -> BoxResult<()> {
        let (mut writer, reader) = tokio::io::duplex(BUF_SIZE);
        let filename = path.file_name().unwrap().to_str().unwrap();
        let path = path.to_owned();
        tokio::spawn(async move {
            if let Err(e) = zip_dir(&mut writer, &path).await {
                eprintln!("Failed to zip {}, {}", path.display(), e);
            }
        });
        let stream = ReaderStream::new(reader);
        *res.body_mut() = Body::wrap_stream(stream);
        res.headers_mut().insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_str(&format!("attachment; filename=\"{}.zip\"", filename,)).unwrap(),
        );
        Ok(())
    }

    async fn handle_render_index(
        &self,
        path: &Path,
        headers: &HeaderMap<HeaderValue>,
        res: &mut Response,
    ) -> BoxResult<()> {
        let path = path.join(INDEX_NAME);
        if fs::metadata(&path)
            .await
            .ok()
            .map(|v| v.is_file())
            .unwrap_or_default()
        {
            self.handle_send_file(&path, headers, res).await?;
        } else {
            status!(res, StatusCode::NOT_FOUND);
        }
        Ok(())
    }

    async fn handle_render_spa(
        &self,
        path: &Path,
        headers: &HeaderMap<HeaderValue>,
        res: &mut Response,
    ) -> BoxResult<()> {
        if path.extension().is_none() {
            let path = self.args.path.join(INDEX_NAME);
            self.handle_send_file(&path, headers, res).await?;
        } else {
            status!(res, StatusCode::NOT_FOUND);
        }
        Ok(())
    }

    async fn handle_send_file(
        &self,
        path: &Path,
        headers: &HeaderMap<HeaderValue>,
        res: &mut Response,
    ) -> BoxResult<()> {
        let (file, meta) = tokio::join!(fs::File::open(path), fs::metadata(path),);
        let (mut file, meta) = (file?, meta?);
        let mut maybe_range = true;
        if let Some((etag, last_modified)) = extract_cache_headers(&meta) {
            let cached = {
                if let Some(if_none_match) = headers.typed_get::<IfNoneMatch>() {
                    !if_none_match.precondition_passes(&etag)
                } else if let Some(if_modified_since) = headers.typed_get::<IfModifiedSince>() {
                    !if_modified_since.is_modified(last_modified.into())
                } else {
                    false
                }
            };
            res.headers_mut().typed_insert(last_modified);
            res.headers_mut().typed_insert(etag.clone());
            if cached {
                status!(res, StatusCode::NOT_MODIFIED);
                return Ok(());
            }
            if headers.typed_get::<Range>().is_some() {
                maybe_range = headers
                    .typed_get::<IfRange>()
                    .map(|if_range| !if_range.is_modified(Some(&etag), Some(&last_modified)))
                    // Always be fresh if there is no validators
                    .unwrap_or(true);
            } else {
                maybe_range = false;
            }
        }
        let file_range = if maybe_range {
            if let Some(content_range) = headers
                .typed_get::<Range>()
                .and_then(|range| to_content_range(&range, meta.len()))
            {
                res.headers_mut().typed_insert(content_range.clone());
                *res.status_mut() = StatusCode::PARTIAL_CONTENT;
                content_range.bytes_range()
            } else {
                None
            }
        } else {
            None
        };
        if let Some(mime) = mime_guess::from_path(&path).first() {
            res.headers_mut().typed_insert(ContentType::from(mime));
        }
        let body = if let Some((begin, end)) = file_range {
            file.seek(io::SeekFrom::Start(begin)).await?;
            let stream = FramedRead::new(file.take(end - begin + 1), BytesCodec::new());
            Body::wrap_stream(stream)
        } else {
            let stream = FramedRead::new(file, BytesCodec::new());
            Body::wrap_stream(stream)
        };
        *res.body_mut() = body;
        res.headers_mut().typed_insert(AcceptRanges::bytes());
        res.headers_mut()
            .typed_insert(ContentLength(meta.len() as u64));

        Ok(())
    }

    fn handle_method_options(&self, res: &mut Response) {
        let allow_upload = self.args.allow_upload;
        let allow_delete = self.args.allow_delete;
        let mut methods = vec!["GET", "PROPFIND", "OPTIONS"];
        if allow_upload {
            methods.extend(["PUT", "COPY", "MKCOL"]);
        }
        if allow_delete {
            methods.push("DELETE");
        }
        if allow_upload && allow_delete {
            methods.push("COPY");
        }
        let value = methods.join(",").parse().unwrap();
        res.headers_mut().insert("Allow", value);
        res.headers_mut().insert("DAV", "1".parse().unwrap());

        status!(res, StatusCode::NO_CONTENT);
    }

    async fn handle_propfind_dir(&self, path: &Path, res: &mut Response) -> BoxResult<()> {
        let paths = match self.list_dir(path, &self.args.path, true).await {
            Ok(paths) => paths,
            Err(_) => {
                status!(res, StatusCode::FORBIDDEN);
                return Ok(());
            }
        };
        let output = paths
            .iter()
            .map(|v| v.xml(self.args.path_prefix.as_ref()))
            .fold(String::new(), |mut acc, v| {
                acc.push_str(&v);
                acc
            });
        res_propfind(res, &output);
        Ok(())
    }

    async fn handle_propfind_file(&self, path: &Path, res: &mut Response) -> BoxResult<()> {
        if let Some(pathitem) = self.to_pathitem(path, &self.args.path).await? {
            res_propfind(res, &pathitem.xml(self.args.path_prefix.as_ref()));
        } else {
            status!(res, StatusCode::NOT_FOUND);
        }
        Ok(())
    }

    async fn handle_mkcol(&self, path: &Path, res: &mut Response) -> BoxResult<()> {
        fs::create_dir_all(path).await?;
        status!(res, StatusCode::CREATED);
        Ok(())
    }

    async fn handle_copy(
        &self,
        path: &Path,
        headers: &HeaderMap<HeaderValue>,
        res: &mut Response,
    ) -> BoxResult<()> {
        let dest = match self.extract_dest(headers) {
            Some(dest) => dest,
            None => {
                status!(res, StatusCode::BAD_REQUEST);
                return Ok(());
            }
        };

        let meta = fs::symlink_metadata(path).await?;
        if meta.is_dir() {
            status!(res, StatusCode::BAD_REQUEST);
            return Ok(());
        }

        ensure_path_parent(&dest).await?;

        fs::copy(path, &dest).await?;

        status!(res, StatusCode::NO_CONTENT);
        Ok(())
    }

    async fn handle_move(
        &self,
        path: &Path,
        headers: &HeaderMap<HeaderValue>,
        res: &mut Response,
    ) -> BoxResult<()> {
        let dest = match self.extract_dest(headers) {
            Some(dest) => dest,
            None => {
                status!(res, StatusCode::BAD_REQUEST);
                return Ok(());
            }
        };

        ensure_path_parent(&dest).await?;

        fs::rename(path, &dest).await?;

        status!(res, StatusCode::NO_CONTENT);
        Ok(())
    }

    fn send_index(
        &self,
        path: &Path,
        mut paths: Vec<PathItem>,
        res: &mut Response,
    ) -> BoxResult<()> {
        paths.sort_unstable();
        let rel_path = match self.args.path.parent() {
            Some(p) => path.strip_prefix(p).unwrap(),
            None => path,
        };
        let data = IndexData {
            breadcrumb: normalize_path(rel_path),
            paths,
            allow_upload: self.args.allow_upload,
            allow_delete: self.args.allow_delete,
        };
        let data = serde_json::to_string(&data).unwrap();
        let output = INDEX_HTML.replace(
            "__SLOT__",
            &format!(
                r#"
<title>Files in {}/ - Duf</title>
<style>{}</style>
<script>var DATA = {}; {}</script>
"#,
                rel_path.display(),
                INDEX_CSS,
                data,
                INDEX_JS
            ),
        );
        *res.body_mut() = output.into();

        Ok(())
    }

    fn auth_guard(&self, req: &Request, res: &mut Response) -> bool {
        let pass = {
            match &self.args.auth {
                None => true,
                Some(auth) => match req.headers().get(AUTHORIZATION) {
                    Some(value) => match value.to_str().ok().map(|v| {
                        let mut it = v.split(' ');
                        (it.next(), it.next())
                    }) {
                        Some((Some("Basic"), Some(tail))) => base64::decode(tail)
                            .ok()
                            .and_then(|v| String::from_utf8(v).ok())
                            .map(|v| v.as_str() == auth)
                            .unwrap_or_default(),
                        _ => false,
                    },
                    None => self.args.no_auth_access && req.method() == Method::GET,
                },
            }
        };
        if !pass {
            status!(res, StatusCode::UNAUTHORIZED);
            res.headers_mut()
                .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Basic"));
        }
        pass
    }

    async fn is_root_contained(&self, path: &Path) -> bool {
        fs::canonicalize(path)
            .await
            .ok()
            .map(|v| v.starts_with(&self.args.path))
            .unwrap_or_default()
    }

    async fn unzip_file(&self, path: &Path) -> BoxResult<()> {
        let root = path.parent().unwrap();
        let mut zip = ZipFileReader::new(File::open(&path).await?).await?;
        for i in 0..zip.entries().len() {
            let entry = &zip.entries()[i];
            let entry_name = entry.name();
            let entry_path = root.join(entry_name);
            if entry_name.ends_with('/') {
                fs::create_dir_all(entry_path).await?;
            } else {
                if !self.args.allow_delete && fs::metadata(&entry_path).await.is_ok() {
                    continue;
                }
                ensure_path_parent(&entry_path).await?;
                let mut outfile = fs::File::create(&entry_path).await?;
                let mut reader = zip.entry_reader(i).await?;
                io::copy(&mut reader, &mut outfile).await?;
            }
        }
        Ok(())
    }

    fn extract_dest(&self, headers: &HeaderMap<HeaderValue>) -> Option<PathBuf> {
        let dest = headers.get("Destination")?.to_str().ok()?;
        let uri: Uri = dest.parse().ok()?;
        self.extract_path(uri.path())
    }

    fn extract_path(&self, path: &str) -> Option<PathBuf> {
        let decoded_path = percent_decode(path[1..].as_bytes()).decode_utf8().ok()?;
        let slashes_switched = if cfg!(windows) {
            decoded_path.replace('/', "\\")
        } else {
            decoded_path.into_owned()
        };
        let stripped_path = match self.strip_path_prefix(&slashes_switched) {
            Some(path) => path,
            None => return None,
        };
        Some(self.args.path.join(&stripped_path))
    }

    fn strip_path_prefix<'a, P: AsRef<Path>>(&self, path: &'a P) -> Option<&'a Path> {
        let path = path.as_ref();
        match self.args.path_prefix.as_deref() {
            Some(prefix) => {
                let prefix = prefix.trim_start_matches('/');
                path.strip_prefix(prefix).ok()
            }
            None => Some(path),
        }
    }

    async fn list_dir(
        &self,
        entry_path: &Path,
        base_path: &Path,
        include_entry: bool,
    ) -> BoxResult<Vec<PathItem>> {
        let mut paths: Vec<PathItem> = vec![];
        if include_entry {
            paths.push(self.to_pathitem(entry_path, base_path).await?.unwrap())
        }
        let mut rd = fs::read_dir(entry_path).await?;
        while let Ok(Some(entry)) = rd.next_entry().await {
            let entry_path = entry.path();
            if let Ok(Some(item)) = self.to_pathitem(entry_path.as_path(), base_path).await {
                paths.push(item);
            }
        }
        Ok(paths)
    }

    async fn to_pathitem<P: AsRef<Path>>(
        &self,
        path: P,
        base_path: P,
    ) -> BoxResult<Option<PathItem>> {
        let path = path.as_ref();
        let rel_path = path.strip_prefix(base_path).unwrap();
        let (meta, meta2) = tokio::join!(fs::metadata(&path), fs::symlink_metadata(&path));
        let (meta, meta2) = (meta?, meta2?);
        let is_symlink = meta2.is_symlink();
        if !self.args.allow_symlink && is_symlink && !self.is_root_contained(path).await {
            return Ok(None);
        }
        let is_dir = meta.is_dir();
        let path_type = match (is_symlink, is_dir) {
            (true, true) => PathType::SymlinkDir,
            (false, true) => PathType::Dir,
            (true, false) => PathType::SymlinkFile,
            (false, false) => PathType::File,
        };
        let mtime = to_timestamp(&meta.modified()?);
        let size = match path_type {
            PathType::Dir | PathType::SymlinkDir => None,
            PathType::File | PathType::SymlinkFile => Some(meta.len()),
        };
        let base_name = rel_path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("/")
            .to_owned();
        let name = normalize_path(rel_path);
        Ok(Some(PathItem {
            path_type,
            base_name,
            name,
            mtime,
            size,
        }))
    }
}

#[derive(Debug, Serialize)]
struct IndexData {
    breadcrumb: String,
    paths: Vec<PathItem>,
    allow_upload: bool,
    allow_delete: bool,
}

#[derive(Debug, Serialize, Eq, PartialEq, Ord, PartialOrd)]
struct PathItem {
    path_type: PathType,
    base_name: String,
    name: String,
    mtime: u64,
    size: Option<u64>,
}

impl PathItem {
    pub fn xml(&self, prefix: Option<&String>) -> String {
        let prefix = match prefix {
            Some(value) => format!("/{}/", value),
            None => "/".to_owned(),
        };
        let mtime = Utc.timestamp_millis(self.mtime as i64).to_rfc2822();
        match self.path_type {
            PathType::Dir | PathType::SymlinkDir => format!(
                r#"<D:response>
<D:href>{}{}</D:href>
<D:propstat>
<D:prop>
<D:displayname>{}</D:displayname>
<D:getlastmodified>{}</D:getlastmodified>
<D:resourcetype><D:collection/></D:resourcetype>
<D:lockdiscovery/>
<D:supportedlock>
</D:supportedlock>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
</D:response>"#,
                prefix, self.name, self.base_name, mtime
            ),
            PathType::File | PathType::SymlinkFile => format!(
                r#"<D:response>
<D:href>{}{}</D:href>
<D:propstat>
<D:prop>
<D:displayname>{}</D:displayname>
<D:getcontentlength>{}</D:getcontentlength>
<D:getlastmodified>{}</D:getlastmodified>
<D:resourcetype></D:resourcetype>
<D:lockdiscovery/>
<D:supportedlock>
</D:supportedlock>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
</D:response>"#,
                prefix,
                self.name,
                self.base_name,
                self.size.unwrap_or_default(),
                mtime
            ),
        }
    }
}

#[derive(Debug, Serialize, Eq, PartialEq, Ord, PartialOrd)]
enum PathType {
    Dir,
    SymlinkDir,
    File,
    SymlinkFile,
}

fn to_timestamp(time: &SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn normalize_path<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref().to_str().unwrap_or_default();
    if cfg!(windows) {
        path.replace('\\', "/")
    } else {
        path.to_string()
    }
}

async fn ensure_path_parent(path: &Path) -> BoxResult<()> {
    if let Some(parent) = path.parent() {
        if fs::symlink_metadata(parent).await.is_err() {
            fs::create_dir_all(&parent).await?;
        }
    }
    Ok(())
}

fn add_cors(res: &mut Response) {
    res.headers_mut()
        .typed_insert(AccessControlAllowOrigin::ANY);
    res.headers_mut().typed_insert(
        vec![RANGE, CONTENT_TYPE, ACCEPT, ORIGIN, WWW_AUTHENTICATE]
            .into_iter()
            .collect::<AccessControlAllowHeaders>(),
    );
}

fn res_propfind(res: &mut Response, content: &str) {
    *res.status_mut() = StatusCode::MULTI_STATUS;
    *res.body_mut() = Body::from(format!(
        r#"<?xml version="1.0" encoding="utf-8" ?>
<D:multistatus xmlns:D="DAV:">
{}
</D:multistatus>"#,
        content,
    ));
}

async fn zip_dir<W: AsyncWrite + Unpin>(writer: &mut W, dir: &Path) -> BoxResult<()> {
    let mut writer = ZipFileWriter::new(writer);
    let mut walkdir = WalkDir::new(dir);
    while let Some(entry) = walkdir.next().await {
        if let Ok(entry) = entry {
            let entry_path = entry.path();
            let meta = match fs::symlink_metadata(entry.path()).await {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            if !meta.is_file() {
                continue;
            }
            let filename = match entry_path.strip_prefix(dir).ok().and_then(|v| v.to_str()) {
                Some(v) => v,
                None => continue,
            };
            let entry_options = EntryOptions::new(filename.to_owned(), Compression::Deflate);
            let mut file = File::open(&entry_path).await?;
            let mut file_writer = writer.write_entry_stream(entry_options).await?;
            io::copy(&mut file, &mut file_writer).await?;
            file_writer.close().await?;
        }
    }
    writer.close().await?;
    Ok(())
}

fn extract_cache_headers(meta: &Metadata) -> Option<(ETag, LastModified)> {
    let mtime = meta.modified().ok()?;
    let timestamp = to_timestamp(&mtime);
    let size = meta.len();
    let etag = format!(r#""{}-{}""#, timestamp, size)
        .parse::<ETag>()
        .unwrap();
    let last_modified = LastModified::from(mtime);
    Some((etag, last_modified))
}

fn to_content_range(range: &Range, complete_length: u64) -> Option<ContentRange> {
    use core::ops::Bound::{Included, Unbounded};
    let mut iter = range.iter();
    let bounds = iter.next();

    if iter.next().is_some() {
        // Found multiple byte-range-spec. Drop.
        return None;
    }

    bounds.and_then(|b| match b {
        (Included(start), Included(end)) if start <= end && start < complete_length => {
            ContentRange::bytes(
                start..=end.min(complete_length.saturating_sub(1)),
                complete_length,
            )
            .ok()
        }
        (Included(start), Unbounded) if start < complete_length => {
            ContentRange::bytes(start.., complete_length).ok()
        }
        (Unbounded, Included(end)) if end > 0 => {
            ContentRange::bytes(complete_length.saturating_sub(end).., complete_length).ok()
        }
        _ => None,
    })
}

fn print_listening(address: &str, port: u16, tls: bool) {
    let addrs = retrive_listening_addrs(address);
    let protocol = if tls { "https" } else { "http" };
    if addrs.len() == 1 {
        eprintln!("Listening on {}://{}:{}", protocol, addrs[0], port);
    } else {
        eprintln!("Listening on:");
        for addr in addrs {
            eprintln!("  {}://{}:{}", protocol, addr, port);
        }
        eprintln!();
    }
}

fn retrive_listening_addrs(address: &str) -> Vec<String> {
    if address == "0.0.0.0" {
        if let Ok(interfaces) = get_if_addrs() {
            let mut ifaces: Vec<IpAddr> = interfaces
                .into_iter()
                .map(|v| v.ip())
                .filter(|v| v.is_ipv4())
                .collect();
            ifaces.sort();
            return ifaces.into_iter().map(|v| v.to_string()).collect();
        }
    }
    vec![address.to_owned()]
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler")
}
