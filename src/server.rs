use crate::{Args, BoxResult};

use async_walkdir::WalkDir;
use async_zip::read::seek::ZipFileReader;
use async_zip::write::{EntryOptions, ZipFileWriter};
use async_zip::Compression;
use futures::stream::StreamExt;
use futures::TryStreamExt;
use headers::{
    AccessControlAllowHeaders, AccessControlAllowOrigin, ContentRange, ContentType, ETag,
    HeaderMap, HeaderMapExt, IfModifiedSince, IfNoneMatch, IfRange, LastModified, Range,
};
use hyper::header::{
    HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_TYPE, ORIGIN, RANGE,
    WWW_AUTHENTICATE,
};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, StatusCode};
use percent_encoding::percent_decode;
use serde::Serialize;
use std::convert::Infallible;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWrite};
use tokio::{fs, io};
use tokio_util::codec::{BytesCodec, FramedRead};
use tokio_util::io::{ReaderStream, StreamReader};

type Request = hyper::Request<Body>;
type Response = hyper::Response<Body>;

const INDEX_HTML: &str = include_str!("assets/index.html");
const INDEX_CSS: &str = include_str!("assets/index.css");
const INDEX_JS: &str = include_str!("assets/index.js");
const BUF_SIZE: usize = 1024 * 16;

macro_rules! status {
    ($res:ident, $status:expr) => {
        *$res.status_mut() = $status;
        *$res.body_mut() = Body::from($status.canonical_reason().unwrap_or_default());
    };
}

pub async fn serve(args: Args) -> BoxResult<()> {
    let address = args.address()?;
    let inner = Arc::new(InnerService::new(args));
    let make_svc = make_service_fn(move |_| {
        let inner = inner.clone();
        async {
            Ok::<_, Infallible>(service_fn(move |req| {
                let inner = inner.clone();
                inner.call(req)
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
                status!(res, StatusCode::INTERNAL_SERVER_ERROR);
                error!(r#""{} {}" - {} {}"#, method, uri, res.status(), err);
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

        let path = req.uri().path();

        let pathname = match self.extract_path(path) {
            Some(v) => v,
            None => {
                status!(res, StatusCode::FORBIDDEN);
                return Ok(res);
            }
        };
        let pathname = pathname.as_path();

        let query = req.uri().query().unwrap_or_default();

        let meta = fs::metadata(pathname).await.ok();

        let is_miss = meta.is_none();
        let is_dir = meta.map(|v| v.is_dir()).unwrap_or_default();
        let is_file = !is_miss && !is_dir;

        let allow_upload = self.args.allow_upload;
        let allow_delete = self.args.allow_delete;

        if !self.args.allow_symlink && !is_miss && !self.is_root_contained(pathname).await {
            status!(res, StatusCode::NOT_FOUND);
            return Ok(res);
        }

        match *req.method() {
            Method::GET if is_dir && query == "zip" => {
                self.handle_zip_dir(pathname, &mut res).await?
            }
            Method::GET if is_dir && query.starts_with("q=") => {
                self.handle_query_dir(pathname, &query[3..], &mut res)
                    .await?
            }
            Method::GET if is_dir => self.handle_ls_dir(pathname, true, &mut res).await?,
            Method::GET if is_file => {
                self.handle_send_file(pathname, req.headers(), &mut res)
                    .await?
            }
            Method::GET if allow_upload && is_miss && path.ends_with('/') => {
                self.handle_ls_dir(pathname, false, &mut res).await?
            }
            Method::OPTIONS => {
                status!(res, StatusCode::NO_CONTENT);
            }
            Method::PUT if !allow_upload || (!allow_delete && is_file) => {
                status!(res, StatusCode::FORBIDDEN);
            }
            Method::PUT => self.handle_upload(pathname, req, &mut res).await?,
            Method::DELETE if !allow_delete => {
                status!(res, StatusCode::FORBIDDEN);
            }
            Method::DELETE if !is_miss => self.handle_delete(pathname, is_dir).await?,
            _ => {
                status!(res, StatusCode::NOT_FOUND);
            }
        }

        Ok(res)
    }

    async fn handle_upload(
        &self,
        path: &Path,
        mut req: Request,
        res: &mut Response,
    ) -> BoxResult<()> {
        let ensure_parent = match path.parent() {
            Some(parent) => match fs::metadata(parent).await {
                Ok(meta) => meta.is_dir(),
                Err(_) => {
                    fs::create_dir_all(parent).await?;
                    true
                }
            },
            None => false,
        };
        if !ensure_parent {
            status!(res, StatusCode::FORBIDDEN);
            return Ok(());
        }

        let mut file = fs::File::create(&path).await?;

        let body_with_io_error = req
            .body_mut()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err));

        let body_reader = StreamReader::new(body_with_io_error);

        futures::pin_mut!(body_reader);

        io::copy(&mut body_reader, &mut file).await?;

        let query = req.uri().query().unwrap_or_default();
        if query == "unzip" {
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
                    if let Some(parent) = entry_path.parent() {
                        if fs::symlink_metadata(parent).await.is_err() {
                            fs::create_dir_all(&parent).await?;
                        }
                    }
                    let mut outfile = fs::File::create(&entry_path).await?;
                    let mut reader = zip.entry_reader(i).await?;
                    io::copy(&mut reader, &mut outfile).await?;
                }
            }
            fs::remove_file(&path).await?;
        }

        Ok(())
    }

    async fn handle_delete(&self, path: &Path, is_dir: bool) -> BoxResult<()> {
        match is_dir {
            true => fs::remove_dir_all(path).await?,
            false => fs::remove_file(path).await?,
        }
        Ok(())
    }

    async fn handle_ls_dir(&self, path: &Path, exist: bool, res: &mut Response) -> BoxResult<()> {
        let mut paths: Vec<PathItem> = vec![];
        if exist {
            let mut rd = match fs::read_dir(path).await {
                Ok(rd) => rd,
                Err(_) => {
                    status!(res, StatusCode::FORBIDDEN);
                    return Ok(());
                }
            };
            while let Some(entry) = rd.next_entry().await? {
                let entry_path = entry.path();
                if let Ok(Some(item)) = self.to_pathitem(entry_path, path.to_path_buf()).await {
                    paths.push(item);
                }
            }
        }
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
                error!("Fail to zip {}, {}", path.display(), e.to_string());
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
                    None => self.args.no_auth_read && req.method() == Method::GET,
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

    fn extract_path(&self, path: &str) -> Option<PathBuf> {
        let decoded_path = percent_decode(path[1..].as_bytes()).decode_utf8().ok()?;
        let slashes_switched = if cfg!(windows) {
            decoded_path.replace('/', "\\")
        } else {
            decoded_path.into_owned()
        };
        Some(self.args.path.join(&slashes_switched))
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
        let name = normalize_path(rel_path);
        Ok(Some(PathItem {
            path_type,
            name,
            mtime,
            size,
        }))
    }
}

#[derive(Debug, Serialize, Eq, PartialEq, Ord, PartialOrd)]
struct IndexData {
    breadcrumb: String,
    paths: Vec<PathItem>,
    allow_upload: bool,
    allow_delete: bool,
}

#[derive(Debug, Serialize, Eq, PartialEq, Ord, PartialOrd)]
struct PathItem {
    path_type: PathType,
    name: String,
    mtime: u64,
    size: Option<u64>,
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

fn add_cors(res: &mut Response) {
    res.headers_mut()
        .typed_insert(AccessControlAllowOrigin::ANY);
    res.headers_mut().typed_insert(
        vec![RANGE, CONTENT_TYPE, ACCEPT, ORIGIN, WWW_AUTHENTICATE]
            .into_iter()
            .collect::<AccessControlAllowHeaders>(),
    );
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
