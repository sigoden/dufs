use crate::auth::{generate_www_auth, valid_digest};
use crate::{Args, BoxResult};
use xml::escape::escape_str_pcdata;

use async_walkdir::WalkDir;
use async_zip::write::{EntryOptions, ZipFileWriter};
use async_zip::Compression;
use chrono::{TimeZone, Utc};
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
use std::net::{IpAddr, SocketAddr};
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
use uuid::Uuid;

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
    let args = Arc::new(args);
    let inner = Arc::new(InnerService::new(args.clone()));
    match args.tls.clone() {
        Some((certs, key)) => {
            let config = ServerConfig::builder()
                .with_safe_defaults()
                .with_no_client_auth()
                .with_single_cert(certs, key)?;
            let tls_acceptor = TlsAcceptor::from(Arc::new(config));
            let arc_acceptor = Arc::new(tls_acceptor);
            let listener = TcpListener::bind(&args.addr).await?;
            let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
            let incoming =
                hyper::server::accept::from_stream(incoming.filter_map(|socket| async {
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
            print_listening(&args.addr, &args.uri_prefix, true);
            server.await?;
        }
        None => {
            let server = hyper::Server::try_bind(&args.addr)?.serve(make_service_fn(move |_| {
                let inner = inner.clone();
                async move {
                    Ok::<_, Infallible>(service_fn(move |req| {
                        let inner = inner.clone();
                        inner.call(req)
                    }))
                }
            }));
            print_listening(&args.addr, &args.uri_prefix, false);
            server.await?;
        }
    }
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

        let mut res = match self.handle(req).await {
            Ok(res) => {
                info!(r#""{} {}" - {}"#, method, uri, res.status());
                res
            }
            Err(err) => {
                let mut res = Response::default();
                let status = StatusCode::INTERNAL_SERVER_ERROR;
                status!(res, status);
                error!(r#""{} {}" - {} {}"#, method, uri, status, err);
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

        let (is_miss, is_dir, is_file, size) = match fs::metadata(path).await.ok() {
            Some(meta) => (false, meta.is_dir(), meta.is_file(), meta.len()),
            None => (true, false, false, 0),
        };

        let allow_upload = self.args.allow_upload;
        let allow_delete = self.args.allow_delete;
        let render_index = self.args.render_index;
        let render_spa = self.args.render_spa;

        if !self.args.allow_symlink && !is_miss && !self.is_root_contained(path).await {
            status!(res, StatusCode::NOT_FOUND);
            return Ok(res);
        }

        let headers = req.headers();

        match req.method() {
            &Method::GET => {
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
                if !allow_upload || (!allow_delete && is_file && size > 0) {
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
            &Method::HEAD => {
                if is_miss {
                    status!(res, StatusCode::NOT_FOUND);
                } else {
                    status!(res, StatusCode::OK);
                }
            }
            method => match method.as_str() {
                "PROPFIND" => {
                    if is_dir {
                        self.handle_propfind_dir(path, headers, &mut res).await?;
                    } else if is_file {
                        self.handle_propfind_file(path, &mut res).await?;
                    } else {
                        status!(res, StatusCode::NOT_FOUND);
                    }
                }
                "PROPPATCH" => {
                    if is_file {
                        self.handle_proppatch(req_path, &mut res).await?;
                    } else {
                        status!(res, StatusCode::NOT_FOUND);
                    }
                }
                "MKCOL" if allow_upload && is_miss => self.handle_mkcol(path, &mut res).await?,
                "COPY" if allow_upload && !is_miss => {
                    self.handle_copy(path, headers, &mut res).await?
                }
                "MOVE" if allow_upload && allow_delete && !is_miss => {
                    self.handle_move(path, headers, &mut res).await?
                }
                "LOCK" => {
                    // Fake lock
                    if is_file {
                        self.handle_lock(req_path, &mut res).await?;
                    } else {
                        status!(res, StatusCode::NOT_FOUND);
                    }
                }
                "UNLOCK" => {
                    // Fake unlock
                    if is_miss {
                        status!(res, StatusCode::NOT_FOUND);
                    } else {
                        status!(res, StatusCode::OK);
                    }
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
            paths = match self.list_dir(path, path).await {
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
        let filename = path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| format!("Failed to get name of `{}`", path.display()))?;
        let path = path.to_owned();
        tokio::spawn(async move {
            if let Err(e) = zip_dir(&mut writer, &path).await {
                error!("Failed to zip {}, {}", path.display(), e);
            }
        });
        let stream = ReaderStream::new(reader);
        *res.body_mut() = Body::wrap_stream(stream);
        res.headers_mut().insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_str(&format!(
                "attachment; filename=\"{}.zip\"",
                encode_uri(filename),
            ))
            .unwrap(),
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
        res.headers_mut().insert(
            "Allow",
            "GET,HEAD,PUT,OPTIONS,DELETE,PROPFIND,COPY,MOVE"
                .parse()
                .unwrap(),
        );
        res.headers_mut().insert("DAV", "1".parse().unwrap());

        status!(res, StatusCode::NO_CONTENT);
    }

    async fn handle_propfind_dir(
        &self,
        path: &Path,
        headers: &HeaderMap<HeaderValue>,
        res: &mut Response,
    ) -> BoxResult<()> {
        let depth: u32 = match headers.get("depth") {
            Some(v) => match v.to_str().ok().and_then(|v| v.parse().ok()) {
                Some(v) => v,
                None => {
                    status!(res, StatusCode::BAD_REQUEST);
                    return Ok(());
                }
            },
            None => 0,
        };
        let mut paths = vec![self.to_pathitem(path, &self.args.path).await?.unwrap()];
        if depth > 0 {
            match self.list_dir(path, &self.args.path).await {
                Ok(child) => paths.extend(child),
                Err(_) => {
                    status!(res, StatusCode::FORBIDDEN);
                    return Ok(());
                }
            }
        }
        let output = paths
            .iter()
            .map(|v| v.to_dav_xml(self.args.uri_prefix.as_str()))
            .fold(String::new(), |mut acc, v| {
                acc.push_str(&v);
                acc
            });
        res_multistatus(res, &output);
        Ok(())
    }

    async fn handle_propfind_file(&self, path: &Path, res: &mut Response) -> BoxResult<()> {
        if let Some(pathitem) = self.to_pathitem(path, &self.args.path).await? {
            res_multistatus(res, &pathitem.to_dav_xml(self.args.uri_prefix.as_str()));
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

    async fn handle_lock(&self, req_path: &str, res: &mut Response) -> BoxResult<()> {
        let token = if self.args.auth.is_none() {
            Utc::now().timestamp().to_string()
        } else {
            format!("opaquelocktoken:{}", Uuid::new_v4())
        };

        res.headers_mut().insert(
            "content-type",
            "application/xml; charset=utf-8".parse().unwrap(),
        );
        res.headers_mut()
            .insert("lock-token", format!("<{}>", token).parse().unwrap());

        *res.body_mut() = Body::from(format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:prop xmlns:D="DAV:"><D:lockdiscovery><D:activelock>
<D:locktoken><D:href>{}</D:href></D:locktoken>
<D:lockroot><D:href>{}</D:href></D:lockroot>
</D:activelock></D:lockdiscovery></D:prop>"#,
            token, req_path
        ));
        Ok(())
    }

    async fn handle_proppatch(&self, req_path: &str, res: &mut Response) -> BoxResult<()> {
        let output = format!(
            r#"<D:response>
<D:href>{}</D:href>
<D:propstat>
<D:prop>
</D:prop>
<D:status>HTTP/1.1 403 Forbidden</D:status>
</D:propstat>
</D:response>"#,
            req_path
        );
        res_multistatus(res, &output);
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
        res.headers_mut()
            .typed_insert(ContentType::from(mime_guess::mime::TEXT_HTML_UTF_8));
        Ok(())
    }

    fn auth_guard(&self, req: &Request, res: &mut Response) -> bool {
        let method = req.method();
        let pass = {
            match &self.args.auth {
                None => true,
                Some((user, pass)) => match req.headers().get(AUTHORIZATION) {
                    Some(value) => {
                        valid_digest(value, method.as_str(), user.as_str(), pass.as_str()).is_some()
                    }
                    None => {
                        self.args.no_auth_access
                            && (method == Method::GET
                                || method == Method::OPTIONS
                                || method == Method::HEAD
                                || method.as_str() == "PROPFIND")
                    }
                },
            }
        };
        if !pass {
            let value = generate_www_auth(false);
            status!(res, StatusCode::UNAUTHORIZED);
            res.headers_mut()
                .insert(WWW_AUTHENTICATE, value.parse().unwrap());
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
        if self.args.path_prefix.is_empty() {
            Some(path)
        } else {
            path.strip_prefix(&self.args.path_prefix).ok()
        }
    }

    async fn list_dir(&self, entry_path: &Path, base_path: &Path) -> BoxResult<Vec<PathItem>> {
        let mut paths: Vec<PathItem> = vec![];
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
    pub fn to_dav_xml(&self, prefix: &str) -> String {
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
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
</D:response>"#,
                escape_str_pcdata(prefix),
                escape_str_pcdata(&self.name),
                escape_str_pcdata(&self.base_name),
                mtime
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
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
</D:response>"#,
                escape_str_pcdata(prefix),
                escape_str_pcdata(&self.name),
                escape_str_pcdata(&self.base_name),
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

fn res_multistatus(res: &mut Response, content: &str) {
    *res.status_mut() = StatusCode::MULTI_STATUS;
    res.headers_mut().insert(
        "content-type",
        "application/xml; charset=utf-8".parse().unwrap(),
    );
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

fn print_listening(addr: &SocketAddr, prefix: &str, tls: bool) {
    let prefix = encode_uri(prefix.trim_end_matches('/'));
    let addrs = retrieve_listening_addrs(addr);
    let protocol = if tls { "https" } else { "http" };
    if addrs.len() == 1 {
        eprintln!("Listening on {}://{}{}", protocol, addr, prefix);
    } else {
        eprintln!("Listening on:");
        for addr in addrs {
            eprintln!("  {}://{}{}", protocol, addr, prefix);
        }
        eprintln!();
    }
}

fn retrieve_listening_addrs(addr: &SocketAddr) -> Vec<SocketAddr> {
    let ip = addr.ip();
    let port = addr.port();
    if ip.is_unspecified() {
        if let Ok(interfaces) = get_if_addrs() {
            let mut ifaces: Vec<IpAddr> = interfaces
                .into_iter()
                .map(|v| v.ip())
                .filter(|v| {
                    if ip.is_ipv4() {
                        v.is_ipv4()
                    } else {
                        v.is_ipv6()
                    }
                })
                .collect();
            ifaces.sort();
            return ifaces
                .into_iter()
                .map(|v| SocketAddr::new(v, port))
                .collect();
        }
    }
    vec![addr.to_owned()]
}

fn encode_uri(v: &str) -> String {
    let parts: Vec<_> = v.split('/').map(urlencoding::encode).collect();
    parts.join("/")
}
