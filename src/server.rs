#![allow(clippy::too_many_arguments)]

use crate::auth::{www_authenticate, AccessPaths, AccessPerm};
use crate::http_utils::{body_full, IncomingStream, LengthLimitedStream};
use crate::utils::{
    decode_uri, encode_uri, get_file_mtime_and_mode, get_file_name, glob, parse_range,
    try_get_file_name,
};
use crate::Args;

use anyhow::{anyhow, Result};
use async_zip::{tokio::write::ZipFileWriter, Compression, ZipDateTime, ZipEntryBuilder};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use bytes::Bytes;
use chrono::{LocalResult, TimeZone, Utc};
use futures_util::{pin_mut, TryStreamExt};
use headers::{
    AcceptRanges, AccessControlAllowCredentials, AccessControlAllowOrigin, CacheControl,
    ContentLength, ContentType, ETag, HeaderMap, HeaderMapExt, IfMatch, IfModifiedSince,
    IfNoneMatch, IfRange, IfUnmodifiedSince, LastModified, Range,
};
use http_body_util::{combinators::BoxBody, BodyExt, StreamBody};
use hyper::body::Frame;
use hyper::{
    body::Incoming,
    header::{
        HeaderValue, AUTHORIZATION, CONNECTION, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
        CONTENT_TYPE, RANGE,
    },
    Method, StatusCode, Uri,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::Metadata;
use std::io::SeekFrom;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{self, AtomicBool};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWrite};
use tokio::{fs, io};

use tokio_util::compat::FuturesAsyncWriteCompatExt;
use tokio_util::io::{ReaderStream, StreamReader};
use uuid::Uuid;
use walkdir::WalkDir;
use xml::escape::escape_str_pcdata;

pub type Request = hyper::Request<Incoming>;
pub type Response = hyper::Response<BoxBody<Bytes, anyhow::Error>>;

const INDEX_HTML: &str = include_str!("../assets/index.html");
const INDEX_CSS: &str = include_str!("../assets/index.css");
const INDEX_JS: &str = include_str!("../assets/index.js");
const FAVICON_ICO: &[u8] = include_bytes!("../assets/favicon.ico");
const INDEX_NAME: &str = "index.html";
const BUF_SIZE: usize = 65536;
const EDITABLE_TEXT_MAX_SIZE: u64 = 4194304; // 4M
const RESUMABLE_UPLOAD_MIN_SIZE: u64 = 20971520; // 20M
const HEALTH_CHECK_PATH: &str = "__dufs__/health";

pub struct Server {
    args: Args,
    assets_prefix: String,
    html: Cow<'static, str>,
    single_file_req_paths: Vec<String>,
    running: Arc<AtomicBool>,
}

impl Server {
    pub fn init(args: Args, running: Arc<AtomicBool>) -> Result<Self> {
        let assets_prefix = format!("__dufs_v{}__/", env!("CARGO_PKG_VERSION"));
        let single_file_req_paths = if args.path_is_file {
            vec![
                args.uri_prefix.to_string(),
                args.uri_prefix[0..args.uri_prefix.len() - 1].to_string(),
                encode_uri(&format!(
                    "{}{}",
                    &args.uri_prefix,
                    get_file_name(&args.serve_path)
                )),
            ]
        } else {
            vec![]
        };
        let html = match args.assets.as_ref() {
            Some(path) => Cow::Owned(std::fs::read_to_string(path.join("index.html"))?),
            None => Cow::Borrowed(INDEX_HTML),
        };
        Ok(Self {
            args,
            running,
            single_file_req_paths,
            assets_prefix,
            html,
        })
    }

    pub async fn call(
        self: Arc<Self>,
        req: Request,
        addr: Option<SocketAddr>,
    ) -> Result<Response, hyper::Error> {
        let uri = req.uri().clone();
        let assets_prefix = &self.assets_prefix;
        let enable_cors = self.args.enable_cors;
        let is_microsoft_webdav = req
            .headers()
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.starts_with("Microsoft-WebDAV-MiniRedir/"))
            .unwrap_or_default();
        let mut http_log_data = self.args.http_logger.data(&req);
        if let Some(addr) = addr {
            http_log_data.insert("remote_addr".to_string(), addr.ip().to_string());
        }

        let mut res = match self.clone().handle(req, is_microsoft_webdav).await {
            Ok(res) => {
                http_log_data.insert("status".to_string(), res.status().as_u16().to_string());
                if !uri.path().starts_with(assets_prefix) {
                    self.args.http_logger.log(&http_log_data, None);
                }
                res
            }
            Err(err) => {
                let mut res = Response::default();
                let status = StatusCode::INTERNAL_SERVER_ERROR;
                *res.status_mut() = status;
                http_log_data.insert("status".to_string(), status.as_u16().to_string());
                self.args
                    .http_logger
                    .log(&http_log_data, Some(err.to_string()));
                res
            }
        };

        if is_microsoft_webdav {
            // microsoft webdav requires this.
            res.headers_mut()
                .insert(CONNECTION, HeaderValue::from_static("close"));
        }
        if enable_cors {
            add_cors(&mut res);
        }
        Ok(res)
    }

    pub async fn handle(
        self: Arc<Self>,
        req: Request,
        is_microsoft_webdav: bool,
    ) -> Result<Response> {
        let mut res = Response::default();

        let req_path = req.uri().path();
        let headers = req.headers();
        let method = req.method().clone();

        let relative_path = match self.resolve_path(req_path) {
            Some(v) => v,
            None => {
                status_bad_request(&mut res, "Invalid Path");
                return Ok(res);
            }
        };

        if method == Method::GET
            && self
                .handle_internal(&relative_path, headers, &mut res)
                .await?
        {
            return Ok(res);
        }

        let authorization = headers.get(AUTHORIZATION);
        let guard =
            self.args
                .auth
                .guard(&relative_path, &method, authorization, is_microsoft_webdav);

        let (user, access_paths) = match guard {
            (None, None) => {
                self.auth_reject(&mut res)?;
                return Ok(res);
            }
            (Some(_), None) => {
                status_forbid(&mut res);
                return Ok(res);
            }
            (x, Some(y)) => (x, y),
        };

        let query = req.uri().query().unwrap_or_default();
        let query_params: HashMap<String, String> = form_urlencoded::parse(query.as_bytes())
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        if method.as_str() == "CHECKAUTH" {
            match user.clone() {
                Some(user) => {
                    *res.body_mut() = body_full(user);
                }
                None => self.auth_reject(&mut res)?,
            }
            return Ok(res);
        } else if method.as_str() == "LOGOUT" {
            self.auth_reject(&mut res)?;
            return Ok(res);
        }

        let head_only = method == Method::HEAD;

        if self.args.path_is_file {
            if self
                .single_file_req_paths
                .iter()
                .any(|v| v.as_str() == req_path)
            {
                self.handle_send_file(&self.args.serve_path, headers, head_only, &mut res)
                    .await?;
            } else {
                status_not_found(&mut res);
            }
            return Ok(res);
        }
        let path = match self.join_path(&relative_path) {
            Some(v) => v,
            None => {
                status_forbid(&mut res);
                return Ok(res);
            }
        };

        let path = path.as_path();

        let (is_miss, is_dir, is_file, size) = match fs::metadata(path).await.ok() {
            Some(meta) => (false, meta.is_dir(), meta.is_file(), meta.len()),
            None => (true, false, false, 0),
        };

        let allow_upload = self.args.allow_upload;
        let allow_delete = self.args.allow_delete;
        let allow_search = self.args.allow_search;
        let allow_archive = self.args.allow_archive;
        let render_index = self.args.render_index;
        let render_spa = self.args.render_spa;
        let render_try_index = self.args.render_try_index;

        if !self.args.allow_symlink && !is_miss && !self.is_root_contained(path).await {
            status_not_found(&mut res);
            return Ok(res);
        }

        match method {
            Method::GET | Method::HEAD => {
                if is_dir {
                    if render_try_index {
                        if allow_archive && has_query_flag(&query_params, "zip") {
                            if !allow_archive {
                                status_not_found(&mut res);
                                return Ok(res);
                            }
                            self.handle_zip_dir(path, head_only, access_paths, &mut res)
                                .await?;
                        } else if allow_search && query_params.contains_key("q") {
                            self.handle_search_dir(
                                path,
                                &query_params,
                                head_only,
                                user,
                                access_paths,
                                &mut res,
                            )
                            .await?;
                        } else {
                            self.handle_render_index(
                                path,
                                &query_params,
                                headers,
                                head_only,
                                user,
                                access_paths,
                                &mut res,
                            )
                            .await?;
                        }
                    } else if render_index || render_spa {
                        self.handle_render_index(
                            path,
                            &query_params,
                            headers,
                            head_only,
                            user,
                            access_paths,
                            &mut res,
                        )
                        .await?;
                    } else if has_query_flag(&query_params, "zip") {
                        if !allow_archive {
                            status_not_found(&mut res);
                            return Ok(res);
                        }
                        self.handle_zip_dir(path, head_only, access_paths, &mut res)
                            .await?;
                    } else if allow_search && query_params.contains_key("q") {
                        self.handle_search_dir(
                            path,
                            &query_params,
                            head_only,
                            user,
                            access_paths,
                            &mut res,
                        )
                        .await?;
                    } else {
                        self.handle_ls_dir(
                            path,
                            true,
                            &query_params,
                            head_only,
                            user,
                            access_paths,
                            &mut res,
                        )
                        .await?;
                    }
                } else if is_file {
                    if has_query_flag(&query_params, "edit") {
                        self.handle_edit_file(path, DataKind::Edit, head_only, user, &mut res)
                            .await?;
                    } else if has_query_flag(&query_params, "view") {
                        self.handle_edit_file(path, DataKind::View, head_only, user, &mut res)
                            .await?;
                    } else if has_query_flag(&query_params, "hash") {
                        self.handle_hash_file(path, head_only, &mut res).await?;
                    } else {
                        self.handle_send_file(path, headers, head_only, &mut res)
                            .await?;
                    }
                } else if render_spa {
                    self.handle_render_spa(path, headers, head_only, &mut res)
                        .await?;
                } else if allow_upload && req_path.ends_with('/') {
                    self.handle_ls_dir(
                        path,
                        false,
                        &query_params,
                        head_only,
                        user,
                        access_paths,
                        &mut res,
                    )
                    .await?;
                } else {
                    status_not_found(&mut res);
                }
            }
            Method::OPTIONS => {
                set_webdav_headers(&mut res);
            }
            Method::PUT => {
                if is_dir || !allow_upload || (!allow_delete && size > 0) {
                    status_forbid(&mut res);
                } else {
                    self.handle_upload(path, None, size, req, &mut res).await?;
                }
            }
            Method::PATCH => {
                if is_miss {
                    status_not_found(&mut res);
                } else if !allow_upload {
                    status_forbid(&mut res);
                } else {
                    let offset = match parse_upload_offset(headers, size) {
                        Ok(v) => v,
                        Err(err) => {
                            status_bad_request(&mut res, &err.to_string());
                            return Ok(res);
                        }
                    };
                    match offset {
                        Some(offset) => {
                            if offset < size && !allow_delete {
                                status_forbid(&mut res);
                            }
                            self.handle_upload(path, Some(offset), size, req, &mut res)
                                .await?;
                        }
                        None => {
                            *res.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
                        }
                    }
                }
            }
            Method::DELETE => {
                if !allow_delete {
                    status_forbid(&mut res);
                } else if !is_miss {
                    self.handle_delete(path, is_dir, &mut res).await?
                } else {
                    status_not_found(&mut res);
                }
            }
            method => match method.as_str() {
                "PROPFIND" => {
                    if is_dir {
                        let access_paths =
                            if access_paths.perm().indexonly() && authorization.is_none() {
                                // see https://github.com/sigoden/dufs/issues/229
                                AccessPaths::new(AccessPerm::ReadOnly)
                            } else {
                                access_paths
                            };
                        self.handle_propfind_dir(path, headers, access_paths, &mut res)
                            .await?;
                    } else if is_file {
                        self.handle_propfind_file(path, &mut res).await?;
                    } else {
                        status_not_found(&mut res);
                    }
                }
                "PROPPATCH" => {
                    if is_file {
                        self.handle_proppatch(req_path, &mut res).await?;
                    } else {
                        status_not_found(&mut res);
                    }
                }
                "MKCOL" => {
                    if !allow_upload {
                        status_forbid(&mut res);
                    } else if !is_miss {
                        *res.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
                        *res.body_mut() = body_full("Already exists");
                    } else {
                        self.handle_mkcol(path, &mut res).await?;
                    }
                }
                "COPY" => {
                    if !allow_upload {
                        status_forbid(&mut res);
                    } else if is_miss {
                        status_not_found(&mut res);
                    } else {
                        self.handle_copy(path, &req, &mut res).await?
                    }
                }
                "MOVE" => {
                    if !allow_upload || !allow_delete {
                        status_forbid(&mut res);
                    } else if is_miss {
                        status_not_found(&mut res);
                    } else {
                        self.handle_move(path, &req, &mut res).await?
                    }
                }
                "LOCK" => {
                    // Fake lock
                    if is_file {
                        let has_auth = authorization.is_some();
                        self.handle_lock(req_path, has_auth, &mut res).await?;
                    } else {
                        status_not_found(&mut res);
                    }
                }
                "UNLOCK" => {
                    // Fake unlock
                    if is_miss {
                        status_not_found(&mut res);
                    }
                }
                _ => {
                    *res.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
                }
            },
        }
        Ok(res)
    }

    async fn handle_upload(
        &self,
        path: &Path,
        upload_offset: Option<u64>,
        size: u64,
        req: Request,
        res: &mut Response,
    ) -> Result<()> {
        ensure_path_parent(path).await?;
        let (mut file, status) = match upload_offset {
            None => (fs::File::create(path).await?, StatusCode::CREATED),
            Some(offset) if offset == size => (
                fs::OpenOptions::new().append(true).open(path).await?,
                StatusCode::NO_CONTENT,
            ),
            Some(offset) => {
                let mut file = fs::OpenOptions::new().write(true).open(path).await?;
                file.seek(SeekFrom::Start(offset)).await?;
                (file, StatusCode::NO_CONTENT)
            }
        };
        let stream = IncomingStream::new(req.into_body());

        let body_with_io_error = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
        let body_reader = StreamReader::new(body_with_io_error);

        pin_mut!(body_reader);

        let ret = io::copy(&mut body_reader, &mut file).await;
        let size = fs::metadata(path)
            .await
            .map(|v| v.len())
            .unwrap_or_default();
        if ret.is_err() {
            if upload_offset.is_none() && size < RESUMABLE_UPLOAD_MIN_SIZE {
                let _ = tokio::fs::remove_file(&path).await;
            }
            ret?;
        }

        *res.status_mut() = status;

        Ok(())
    }

    async fn handle_delete(&self, path: &Path, is_dir: bool, res: &mut Response) -> Result<()> {
        match is_dir {
            true => fs::remove_dir_all(path).await?,
            false => fs::remove_file(path).await?,
        }

        status_no_content(res);
        Ok(())
    }

    async fn handle_ls_dir(
        &self,
        path: &Path,
        exist: bool,
        query_params: &HashMap<String, String>,
        head_only: bool,
        user: Option<String>,
        access_paths: AccessPaths,
        res: &mut Response,
    ) -> Result<()> {
        let mut paths = vec![];
        if exist {
            paths = match self.list_dir(path, path, access_paths.clone()).await {
                Ok(paths) => paths,
                Err(_) => {
                    status_forbid(res);
                    return Ok(());
                }
            }
        };
        self.send_index(
            path,
            paths,
            exist,
            query_params,
            head_only,
            user,
            access_paths,
            res,
        )
    }

    async fn handle_search_dir(
        &self,
        path: &Path,
        query_params: &HashMap<String, String>,
        head_only: bool,
        user: Option<String>,
        access_paths: AccessPaths,
        res: &mut Response,
    ) -> Result<()> {
        let mut paths: Vec<PathItem> = vec![];
        let search = query_params
            .get("q")
            .ok_or_else(|| anyhow!("invalid q"))?
            .to_lowercase();
        if search.is_empty() {
            return self
                .handle_ls_dir(path, true, query_params, head_only, user, access_paths, res)
                .await;
        } else {
            let path_buf = path.to_path_buf();
            let hidden = Arc::new(self.args.hidden.to_vec());
            let hidden = hidden.clone();
            let running = self.running.clone();
            let access_paths = access_paths.clone();
            let search_paths = tokio::task::spawn_blocking(move || {
                let mut paths: Vec<PathBuf> = vec![];
                for dir in access_paths.entry_paths(&path_buf) {
                    let mut it = WalkDir::new(&dir).into_iter();
                    it.next();
                    while let Some(Ok(entry)) = it.next() {
                        if !running.load(atomic::Ordering::SeqCst) {
                            break;
                        }
                        let entry_path = entry.path();
                        let base_name = get_file_name(entry_path);
                        let file_type = entry.file_type();
                        let mut is_dir_type: bool = file_type.is_dir();
                        if file_type.is_symlink() {
                            match std::fs::symlink_metadata(entry_path) {
                                Ok(meta) => {
                                    is_dir_type = meta.is_dir();
                                }
                                Err(_) => {
                                    continue;
                                }
                            }
                        }
                        if is_hidden(&hidden, base_name, is_dir_type) {
                            if file_type.is_dir() {
                                it.skip_current_dir();
                            }
                            continue;
                        }
                        if !base_name.to_lowercase().contains(&search) {
                            continue;
                        }
                        paths.push(entry_path.to_path_buf());
                    }
                }
                paths
            })
            .await?;
            for search_path in search_paths.into_iter() {
                if let Ok(Some(item)) = self.to_pathitem(search_path, path.to_path_buf()).await {
                    paths.push(item);
                }
            }
        }
        self.send_index(
            path,
            paths,
            true,
            query_params,
            head_only,
            user,
            access_paths,
            res,
        )
    }

    async fn handle_zip_dir(
        &self,
        path: &Path,
        head_only: bool,
        access_paths: AccessPaths,
        res: &mut Response,
    ) -> Result<()> {
        let (mut writer, reader) = tokio::io::duplex(BUF_SIZE);
        let filename = try_get_file_name(path)?;
        set_content_disposition(res, false, &format!("{}.zip", filename))?;
        res.headers_mut()
            .insert("content-type", HeaderValue::from_static("application/zip"));
        if head_only {
            return Ok(());
        }
        let path = path.to_owned();
        let hidden = self.args.hidden.clone();
        let running = self.running.clone();
        let compression = self.args.compress.to_compression();
        tokio::spawn(async move {
            if let Err(e) = zip_dir(
                &mut writer,
                &path,
                access_paths,
                &hidden,
                compression,
                running,
            )
            .await
            {
                error!("Failed to zip {}, {}", path.display(), e);
            }
        });
        let reader_stream = ReaderStream::with_capacity(reader, BUF_SIZE);
        let stream_body = StreamBody::new(
            reader_stream
                .map_ok(Frame::data)
                .map_err(|err| anyhow!("{err}")),
        );
        let boxed_body = stream_body.boxed();
        *res.body_mut() = boxed_body;
        Ok(())
    }

    async fn handle_render_index(
        &self,
        path: &Path,
        query_params: &HashMap<String, String>,
        headers: &HeaderMap<HeaderValue>,
        head_only: bool,
        user: Option<String>,
        access_paths: AccessPaths,
        res: &mut Response,
    ) -> Result<()> {
        let index_path = path.join(INDEX_NAME);
        if fs::metadata(&index_path)
            .await
            .ok()
            .map(|v| v.is_file())
            .unwrap_or_default()
        {
            self.handle_send_file(&index_path, headers, head_only, res)
                .await?;
        } else if self.args.render_try_index {
            self.handle_ls_dir(path, true, query_params, head_only, user, access_paths, res)
                .await?;
        } else {
            status_not_found(res)
        }
        Ok(())
    }

    async fn handle_render_spa(
        &self,
        path: &Path,
        headers: &HeaderMap<HeaderValue>,
        head_only: bool,
        res: &mut Response,
    ) -> Result<()> {
        if path.extension().is_none() {
            let path = self.args.serve_path.join(INDEX_NAME);
            self.handle_send_file(&path, headers, head_only, res)
                .await?;
        } else {
            status_not_found(res)
        }
        Ok(())
    }

    async fn handle_internal(
        &self,
        req_path: &str,
        headers: &HeaderMap<HeaderValue>,
        res: &mut Response,
    ) -> Result<bool> {
        if let Some(name) = req_path.strip_prefix(&self.assets_prefix) {
            match self.args.assets.as_ref() {
                Some(assets_path) => {
                    let path = assets_path.join(name);
                    if path.exists() {
                        self.handle_send_file(&path, headers, false, res).await?;
                    } else {
                        status_not_found(res);
                        return Ok(true);
                    }
                }
                None => match name {
                    "index.js" => {
                        *res.body_mut() = body_full(INDEX_JS);
                        res.headers_mut().insert(
                            "content-type",
                            HeaderValue::from_static("application/javascript; charset=UTF-8"),
                        );
                    }
                    "index.css" => {
                        *res.body_mut() = body_full(INDEX_CSS);
                        res.headers_mut().insert(
                            "content-type",
                            HeaderValue::from_static("text/css; charset=UTF-8"),
                        );
                    }
                    "favicon.ico" => {
                        *res.body_mut() = body_full(FAVICON_ICO);
                        res.headers_mut()
                            .insert("content-type", HeaderValue::from_static("image/x-icon"));
                    }
                    _ => {
                        status_not_found(res);
                    }
                },
            }
            res.headers_mut().insert(
                "cache-control",
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
            res.headers_mut().insert(
                "x-content-type-options",
                HeaderValue::from_static("nosniff"),
            );
            Ok(true)
        } else if req_path == HEALTH_CHECK_PATH {
            res.headers_mut()
                .typed_insert(ContentType::from(mime_guess::mime::APPLICATION_JSON));

            *res.body_mut() = body_full(r#"{"status":"OK"}"#);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn handle_send_file(
        &self,
        path: &Path,
        headers: &HeaderMap<HeaderValue>,
        head_only: bool,
        res: &mut Response,
    ) -> Result<()> {
        let (file, meta) = tokio::join!(fs::File::open(path), fs::metadata(path),);
        let (mut file, meta) = (file?, meta?);
        let size = meta.len();
        let mut use_range = true;
        if let Some((etag, last_modified)) = extract_cache_headers(&meta) {
            if let Some(if_unmodified_since) = headers.typed_get::<IfUnmodifiedSince>() {
                if !if_unmodified_since.precondition_passes(last_modified.into()) {
                    *res.status_mut() = StatusCode::PRECONDITION_FAILED;
                    return Ok(());
                }
            }
            if let Some(if_match) = headers.typed_get::<IfMatch>() {
                if !if_match.precondition_passes(&etag) {
                    *res.status_mut() = StatusCode::PRECONDITION_FAILED;
                    return Ok(());
                }
            }
            if let Some(if_modified_since) = headers.typed_get::<IfModifiedSince>() {
                if !if_modified_since.is_modified(last_modified.into()) {
                    *res.status_mut() = StatusCode::NOT_MODIFIED;
                    return Ok(());
                }
            }
            if let Some(if_none_match) = headers.typed_get::<IfNoneMatch>() {
                if !if_none_match.precondition_passes(&etag) {
                    *res.status_mut() = StatusCode::NOT_MODIFIED;
                    return Ok(());
                }
            }

            res.headers_mut().typed_insert(last_modified);
            res.headers_mut().typed_insert(etag.clone());

            if headers.typed_get::<Range>().is_some() {
                use_range = headers
                    .typed_get::<IfRange>()
                    .map(|if_range| !if_range.is_modified(Some(&etag), Some(&last_modified)))
                    // Always be fresh if there is no validators
                    .unwrap_or(true);
            } else {
                use_range = false;
            }
        }

        let range = if use_range {
            headers.get(RANGE).map(|range| {
                range
                    .to_str()
                    .ok()
                    .and_then(|range| parse_range(range, size))
            })
        } else {
            None
        };

        res.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_str(&get_content_type(path).await?)?,
        );

        let filename = try_get_file_name(path)?;
        set_content_disposition(res, true, filename)?;

        res.headers_mut().typed_insert(AcceptRanges::bytes());

        if let Some(range) = range {
            if let Some((start, end)) = range {
                file.seek(SeekFrom::Start(start)).await?;
                let range_size = end - start + 1;
                *res.status_mut() = StatusCode::PARTIAL_CONTENT;
                let content_range = format!("bytes {}-{}/{}", start, end, size);
                res.headers_mut()
                    .insert(CONTENT_RANGE, content_range.parse()?);
                res.headers_mut()
                    .insert(CONTENT_LENGTH, format!("{range_size}").parse()?);
                if head_only {
                    return Ok(());
                }

                let stream_body = StreamBody::new(
                    LengthLimitedStream::new(file, range_size as usize)
                        .map_ok(Frame::data)
                        .map_err(|err| anyhow!("{err}")),
                );
                let boxed_body = stream_body.boxed();
                *res.body_mut() = boxed_body;
            } else {
                *res.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
                res.headers_mut()
                    .insert(CONTENT_RANGE, format!("bytes */{size}").parse()?);
            }
        } else {
            res.headers_mut()
                .insert(CONTENT_LENGTH, format!("{size}").parse()?);
            if head_only {
                return Ok(());
            }

            let reader_stream = ReaderStream::with_capacity(file, BUF_SIZE);
            let stream_body = StreamBody::new(
                reader_stream
                    .map_ok(Frame::data)
                    .map_err(|err| anyhow!("{err}")),
            );
            let boxed_body = stream_body.boxed();
            *res.body_mut() = boxed_body;
        }
        Ok(())
    }

    async fn handle_edit_file(
        &self,
        path: &Path,
        kind: DataKind,
        head_only: bool,
        user: Option<String>,
        res: &mut Response,
    ) -> Result<()> {
        let (file, meta) = tokio::join!(fs::File::open(path), fs::metadata(path),);
        let (file, meta) = (file?, meta?);
        let href = format!(
            "/{}",
            normalize_path(path.strip_prefix(&self.args.serve_path)?)
        );
        let mut buffer: Vec<u8> = vec![];
        file.take(1024).read_to_end(&mut buffer).await?;
        let editable =
            meta.len() <= EDITABLE_TEXT_MAX_SIZE && content_inspector::inspect(&buffer).is_text();
        let data = EditData {
            href,
            kind,
            uri_prefix: self.args.uri_prefix.clone(),
            allow_upload: self.args.allow_upload,
            allow_delete: self.args.allow_delete,
            auth: self.args.auth.exist(),
            user,
            editable,
        };
        res.headers_mut()
            .typed_insert(ContentType::from(mime_guess::mime::TEXT_HTML_UTF_8));
        let index_data = STANDARD.encode(serde_json::to_string(&data)?);
        let output = self
            .html
            .replace(
                "__ASSETS_PREFIX__",
                &format!("{}{}", self.args.uri_prefix, self.assets_prefix),
            )
            .replace("__INDEX_DATA__", &index_data);
        res.headers_mut()
            .typed_insert(ContentLength(output.as_bytes().len() as u64));
        if head_only {
            return Ok(());
        }
        *res.body_mut() = body_full(output);
        Ok(())
    }

    async fn handle_hash_file(
        &self,
        path: &Path,
        head_only: bool,
        res: &mut Response,
    ) -> Result<()> {
        let output = sha256_file(path).await?;
        res.headers_mut()
            .typed_insert(ContentType::from(mime_guess::mime::TEXT_HTML_UTF_8));
        res.headers_mut()
            .typed_insert(ContentLength(output.as_bytes().len() as u64));
        if head_only {
            return Ok(());
        }
        *res.body_mut() = body_full(output);
        Ok(())
    }

    async fn handle_propfind_dir(
        &self,
        path: &Path,
        headers: &HeaderMap<HeaderValue>,
        access_paths: AccessPaths,
        res: &mut Response,
    ) -> Result<()> {
        let depth: u32 = match headers.get("depth") {
            Some(v) => match v.to_str().ok().and_then(|v| v.parse().ok()) {
                Some(0) => 0,
                Some(1) => 1,
                _ => {
                    status_bad_request(res, "Invalid depth: only 0 and 1 are allowed.");
                    return Ok(());
                }
            },
            None => 1,
        };
        let mut paths = match self.to_pathitem(path, &self.args.serve_path).await? {
            Some(v) => vec![v],
            None => vec![],
        };
        if depth == 1 {
            match self
                .list_dir(path, &self.args.serve_path, access_paths)
                .await
            {
                Ok(child) => paths.extend(child),
                Err(_) => {
                    status_forbid(res);
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

    async fn handle_propfind_file(&self, path: &Path, res: &mut Response) -> Result<()> {
        if let Some(pathitem) = self.to_pathitem(path, &self.args.serve_path).await? {
            res_multistatus(res, &pathitem.to_dav_xml(self.args.uri_prefix.as_str()));
        } else {
            status_not_found(res);
        }
        Ok(())
    }

    async fn handle_mkcol(&self, path: &Path, res: &mut Response) -> Result<()> {
        fs::create_dir_all(path).await?;
        *res.status_mut() = StatusCode::CREATED;
        Ok(())
    }

    async fn handle_copy(&self, path: &Path, req: &Request, res: &mut Response) -> Result<()> {
        let dest = match self.extract_dest(req, res) {
            Some(dest) => dest,
            None => {
                return Ok(());
            }
        };

        let meta = fs::symlink_metadata(path).await?;
        if meta.is_dir() {
            status_forbid(res);
            return Ok(());
        }

        ensure_path_parent(&dest).await?;

        fs::copy(path, &dest).await?;

        status_no_content(res);
        Ok(())
    }

    async fn handle_move(&self, path: &Path, req: &Request, res: &mut Response) -> Result<()> {
        let dest = match self.extract_dest(req, res) {
            Some(dest) => dest,
            None => {
                return Ok(());
            }
        };

        ensure_path_parent(&dest).await?;

        fs::rename(path, &dest).await?;

        status_no_content(res);
        Ok(())
    }

    async fn handle_lock(&self, req_path: &str, auth: bool, res: &mut Response) -> Result<()> {
        let token = if auth {
            format!("opaquelocktoken:{}", Uuid::new_v4())
        } else {
            Utc::now().timestamp().to_string()
        };

        res.headers_mut().insert(
            "content-type",
            HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        res.headers_mut()
            .insert("lock-token", format!("<{token}>").parse()?);

        *res.body_mut() = body_full(format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<D:prop xmlns:D="DAV:"><D:lockdiscovery><D:activelock>
<D:locktoken><D:href>{token}</D:href></D:locktoken>
<D:lockroot><D:href>{req_path}</D:href></D:lockroot>
</D:activelock></D:lockdiscovery></D:prop>"#
        ));
        Ok(())
    }

    async fn handle_proppatch(&self, req_path: &str, res: &mut Response) -> Result<()> {
        let output = format!(
            r#"<D:response>
<D:href>{req_path}</D:href>
<D:propstat>
<D:prop>
</D:prop>
<D:status>HTTP/1.1 403 Forbidden</D:status>
</D:propstat>
</D:response>"#
        );
        res_multistatus(res, &output);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn send_index(
        &self,
        path: &Path,
        mut paths: Vec<PathItem>,
        exist: bool,
        query_params: &HashMap<String, String>,
        head_only: bool,
        user: Option<String>,
        access_paths: AccessPaths,
        res: &mut Response,
    ) -> Result<()> {
        if let Some(sort) = query_params.get("sort") {
            if sort == "name" {
                paths.sort_by(|v1, v2| v1.sort_by_name(v2))
            } else if sort == "mtime" {
                paths.sort_by(|v1, v2| v1.sort_by_mtime(v2))
            } else if sort == "size" {
                paths.sort_by(|v1, v2| v1.sort_by_size(v2))
            }
            if query_params
                .get("order")
                .map(|v| v == "desc")
                .unwrap_or_default()
            {
                paths.reverse()
            }
        } else {
            paths.sort_by(|v1, v2| v1.sort_by_name(v2))
        }
        if has_query_flag(query_params, "simple") {
            let output = paths
                .into_iter()
                .map(|v| {
                    if v.is_dir() {
                        format!("{}/\n", v.name)
                    } else {
                        format!("{}\n", v.name)
                    }
                })
                .collect::<Vec<String>>()
                .join("");
            res.headers_mut()
                .typed_insert(ContentType::from(mime_guess::mime::TEXT_HTML_UTF_8));
            res.headers_mut()
                .typed_insert(ContentLength(output.as_bytes().len() as u64));
            *res.body_mut() = body_full(output);
            if head_only {
                return Ok(());
            }
            return Ok(());
        }
        let href = format!(
            "/{}",
            normalize_path(path.strip_prefix(&self.args.serve_path)?)
        );
        let readwrite = access_paths.perm().readwrite();
        let data = IndexData {
            kind: DataKind::Index,
            href,
            uri_prefix: self.args.uri_prefix.clone(),
            allow_upload: self.args.allow_upload && readwrite,
            allow_delete: self.args.allow_delete && readwrite,
            allow_search: self.args.allow_search,
            allow_archive: self.args.allow_archive,
            dir_exists: exist,
            auth: self.args.auth.exist(),
            user,
            paths,
        };
        let output = if has_query_flag(query_params, "json") {
            res.headers_mut()
                .typed_insert(ContentType::from(mime_guess::mime::APPLICATION_JSON));
            serde_json::to_string_pretty(&data)?
        } else {
            res.headers_mut()
                .typed_insert(ContentType::from(mime_guess::mime::TEXT_HTML_UTF_8));

            let index_data = STANDARD.encode(serde_json::to_string(&data)?);
            self.html
                .replace(
                    "__ASSETS_PREFIX__",
                    &format!("{}{}", self.args.uri_prefix, self.assets_prefix),
                )
                .replace("__INDEX_DATA__", &index_data)
        };
        res.headers_mut()
            .typed_insert(ContentLength(output.as_bytes().len() as u64));
        res.headers_mut()
            .typed_insert(CacheControl::new().with_no_cache());
        res.headers_mut().insert(
            "x-content-type-options",
            HeaderValue::from_static("nosniff"),
        );
        if head_only {
            return Ok(());
        }
        *res.body_mut() = body_full(output);
        Ok(())
    }

    fn auth_reject(&self, res: &mut Response) -> Result<()> {
        set_webdav_headers(res);

        www_authenticate(res, &self.args)?;
        *res.status_mut() = StatusCode::UNAUTHORIZED;
        Ok(())
    }

    async fn is_root_contained(&self, path: &Path) -> bool {
        fs::canonicalize(path)
            .await
            .ok()
            .map(|v| v.starts_with(&self.args.serve_path))
            .unwrap_or_default()
    }

    fn extract_dest(&self, req: &Request, res: &mut Response) -> Option<PathBuf> {
        let headers = req.headers();
        let dest_path = match self
            .extract_destination_header(headers)
            .and_then(|dest| self.resolve_path(&dest))
        {
            Some(dest) => dest,
            None => {
                status_bad_request(res, "Invalid Destination");
                return None;
            }
        };

        let authorization = headers.get(AUTHORIZATION);
        let guard = self
            .args
            .auth
            .guard(&dest_path, req.method(), authorization, false);

        match guard {
            (_, Some(_)) => {}
            _ => {
                status_forbid(res);
                return None;
            }
        };

        let dest = match self.join_path(&dest_path) {
            Some(dest) => dest,
            None => {
                *res.status_mut() = StatusCode::BAD_REQUEST;
                return None;
            }
        };

        Some(dest)
    }

    fn extract_destination_header(&self, headers: &HeaderMap<HeaderValue>) -> Option<String> {
        let dest = headers.get("Destination")?.to_str().ok()?;
        let uri: Uri = dest.parse().ok()?;
        Some(uri.path().to_string())
    }

    fn resolve_path(&self, path: &str) -> Option<String> {
        let path = decode_uri(path)?;
        let path = path.trim_matches('/');
        let mut parts = vec![];
        for comp in Path::new(path).components() {
            if let Component::Normal(v) = comp {
                let v = v.to_string_lossy();
                if cfg!(windows) {
                    let chars: Vec<char> = v.chars().collect();
                    if chars.len() == 2 && chars[1] == ':' && chars[0].is_ascii_alphabetic() {
                        return None;
                    }
                }
                parts.push(v);
            } else {
                return None;
            }
        }
        let new_path = parts.join("/");
        let path_prefix = self.args.path_prefix.as_str();
        if path_prefix.is_empty() {
            return Some(new_path);
        }
        new_path
            .strip_prefix(path_prefix.trim_start_matches('/'))
            .map(|v| v.trim_matches('/').to_string())
    }

    fn join_path(&self, path: &str) -> Option<PathBuf> {
        if path.is_empty() {
            return Some(self.args.serve_path.clone());
        }
        let path = if cfg!(windows) {
            path.replace('/', "\\")
        } else {
            path.to_string()
        };
        Some(self.args.serve_path.join(path))
    }

    async fn list_dir(
        &self,
        entry_path: &Path,
        base_path: &Path,
        access_paths: AccessPaths,
    ) -> Result<Vec<PathItem>> {
        let mut paths: Vec<PathItem> = vec![];
        if access_paths.perm().indexonly() {
            for name in access_paths.child_names() {
                let entry_path = entry_path.join(name);
                self.add_pathitem(&mut paths, base_path, &entry_path).await;
            }
        } else {
            let mut rd = fs::read_dir(entry_path).await?;
            while let Ok(Some(entry)) = rd.next_entry().await {
                let entry_path = entry.path();
                self.add_pathitem(&mut paths, base_path, &entry_path).await;
            }
        }
        Ok(paths)
    }

    async fn add_pathitem(&self, paths: &mut Vec<PathItem>, base_path: &Path, entry_path: &Path) {
        let base_name = get_file_name(entry_path);
        if let Ok(Some(item)) = self.to_pathitem(entry_path, base_path).await {
            if is_hidden(&self.args.hidden, base_name, item.is_dir()) {
                return;
            }
            paths.push(item);
        }
    }

    async fn to_pathitem<P: AsRef<Path>>(&self, path: P, base_path: P) -> Result<Option<PathItem>> {
        let path = path.as_ref();
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
            PathType::Dir | PathType::SymlinkDir => {
                let mut count = 0;
                let mut entries = tokio::fs::read_dir(&path).await?;
                while entries.next_entry().await?.is_some() {
                    count += 1;
                }
                count
            }
            PathType::File | PathType::SymlinkFile => meta.len(),
        };
        let rel_path = path.strip_prefix(base_path)?;
        let name = normalize_path(rel_path);
        Ok(Some(PathItem {
            path_type,
            name,
            mtime,
            size,
        }))
    }
}

#[derive(Debug, Serialize, PartialEq)]
enum DataKind {
    Index,
    Edit,
    View,
}

#[derive(Debug, Serialize)]
struct IndexData {
    href: String,
    kind: DataKind,
    uri_prefix: String,
    allow_upload: bool,
    allow_delete: bool,
    allow_search: bool,
    allow_archive: bool,
    dir_exists: bool,
    auth: bool,
    user: Option<String>,
    paths: Vec<PathItem>,
}

#[derive(Debug, Serialize)]
struct EditData {
    href: String,
    kind: DataKind,
    uri_prefix: String,
    allow_upload: bool,
    allow_delete: bool,
    auth: bool,
    user: Option<String>,
    editable: bool,
}

#[derive(Debug, Serialize, Eq, PartialEq, Ord, PartialOrd)]
struct PathItem {
    path_type: PathType,
    name: String,
    mtime: u64,
    size: u64,
}

impl PathItem {
    pub fn is_dir(&self) -> bool {
        self.path_type == PathType::Dir || self.path_type == PathType::SymlinkDir
    }

    pub fn to_dav_xml(&self, prefix: &str) -> String {
        let mtime = match Utc.timestamp_millis_opt(self.mtime as i64) {
            LocalResult::Single(v) => format!("{}", v.format("%a, %d %b %Y %H:%M:%S GMT")),
            _ => String::new(),
        };
        let mut href = encode_uri(&format!("{}{}", prefix, &self.name));
        if self.is_dir() && !href.ends_with('/') {
            href.push('/');
        }
        let displayname = escape_str_pcdata(self.base_name());
        match self.path_type {
            PathType::Dir | PathType::SymlinkDir => format!(
                r#"<D:response>
<D:href>{href}</D:href>
<D:propstat>
<D:prop>
<D:displayname>{displayname}</D:displayname>
<D:getlastmodified>{mtime}</D:getlastmodified>
<D:resourcetype><D:collection/></D:resourcetype>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
</D:response>"#
            ),
            PathType::File | PathType::SymlinkFile => format!(
                r#"<D:response>
<D:href>{href}</D:href>
<D:propstat>
<D:prop>
<D:displayname>{displayname}</D:displayname>
<D:getcontentlength>{}</D:getcontentlength>
<D:getlastmodified>{mtime}</D:getlastmodified>
<D:resourcetype></D:resourcetype>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
</D:response>"#,
                self.size
            ),
        }
    }

    pub fn base_name(&self) -> &str {
        self.name.split('/').last().unwrap_or_default()
    }

    pub fn sort_by_name(&self, other: &Self) -> Ordering {
        match self.path_type.cmp(&other.path_type) {
            Ordering::Equal => {
                alphanumeric_sort::compare_str(self.name.to_lowercase(), other.name.to_lowercase())
            }
            v => v,
        }
    }

    pub fn sort_by_mtime(&self, other: &Self) -> Ordering {
        match self.path_type.cmp(&other.path_type) {
            Ordering::Equal => self.mtime.cmp(&other.mtime),
            v => v,
        }
    }

    pub fn sort_by_size(&self, other: &Self) -> Ordering {
        match self.path_type.cmp(&other.path_type) {
            Ordering::Equal => self.size.cmp(&other.size),
            v => v,
        }
    }
}

#[derive(Debug, Serialize, Eq, PartialEq)]
enum PathType {
    Dir,
    SymlinkDir,
    File,
    SymlinkFile,
}

impl Ord for PathType {
    fn cmp(&self, other: &Self) -> Ordering {
        let to_value = |t: &Self| -> u8 {
            if matches!(t, Self::Dir | Self::SymlinkDir) {
                0
            } else {
                1
            }
        };
        to_value(self).cmp(&to_value(other))
    }
}
impl PartialOrd for PathType {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn to_timestamp(time: &SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
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

async fn ensure_path_parent(path: &Path) -> Result<()> {
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
    res.headers_mut()
        .typed_insert(AccessControlAllowCredentials);
    res.headers_mut().insert(
        "Access-Control-Allow-Methods",
        HeaderValue::from_static("*"),
    );
    res.headers_mut().insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("Authorization,*"),
    );
    res.headers_mut().insert(
        "Access-Control-Expose-Headers",
        HeaderValue::from_static("Authorization,*"),
    );
}

fn res_multistatus(res: &mut Response, content: &str) {
    *res.status_mut() = StatusCode::MULTI_STATUS;
    res.headers_mut().insert(
        "content-type",
        HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    *res.body_mut() = body_full(format!(
        r#"<?xml version="1.0" encoding="utf-8" ?>
<D:multistatus xmlns:D="DAV:">
{content}
</D:multistatus>"#,
    ));
}

async fn zip_dir<W: AsyncWrite + Unpin>(
    writer: &mut W,
    dir: &Path,
    access_paths: AccessPaths,
    hidden: &[String],
    compression: Compression,
    running: Arc<AtomicBool>,
) -> Result<()> {
    let mut writer = ZipFileWriter::with_tokio(writer);
    let hidden = Arc::new(hidden.to_vec());
    let dir_clone = dir.to_path_buf();
    let zip_paths = tokio::task::spawn_blocking(move || {
        let mut paths: Vec<PathBuf> = vec![];
        for dir in access_paths.entry_paths(&dir_clone) {
            let mut it = WalkDir::new(&dir).into_iter();
            it.next();
            while let Some(Ok(entry)) = it.next() {
                if !running.load(atomic::Ordering::SeqCst) {
                    break;
                }
                let entry_path = entry.path();
                let base_name = get_file_name(entry_path);
                let file_type = entry.file_type();
                let mut is_dir_type: bool = file_type.is_dir();
                if file_type.is_symlink() {
                    match std::fs::symlink_metadata(entry_path) {
                        Ok(meta) => {
                            is_dir_type = meta.is_dir();
                        }
                        Err(_) => {
                            continue;
                        }
                    }
                }
                if is_hidden(&hidden, base_name, is_dir_type) {
                    if file_type.is_dir() {
                        it.skip_current_dir();
                    }
                    continue;
                }
                if entry.path().symlink_metadata().is_err() {
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }
                paths.push(entry_path.to_path_buf());
            }
        }
        paths
    })
    .await?;
    for zip_path in zip_paths.into_iter() {
        let filename = match zip_path.strip_prefix(dir).ok().and_then(|v| v.to_str()) {
            Some(v) => v,
            None => continue,
        };
        let (datetime, mode) = get_file_mtime_and_mode(&zip_path).await?;
        let builder = ZipEntryBuilder::new(filename.into(), compression)
            .unix_permissions(mode)
            .last_modification_date(ZipDateTime::from_chrono(&datetime));
        let mut file = File::open(&zip_path).await?;
        let mut file_writer = writer.write_entry_stream(builder).await?.compat_write();
        io::copy(&mut file, &mut file_writer).await?;
        file_writer.into_inner().close().await?;
    }
    writer.close().await?;
    Ok(())
}

fn extract_cache_headers(meta: &Metadata) -> Option<(ETag, LastModified)> {
    let mtime = meta.modified().ok()?;
    let timestamp = to_timestamp(&mtime);
    let size = meta.len();
    let etag = format!(r#""{timestamp}-{size}""#).parse::<ETag>().ok()?;
    let last_modified = LastModified::from(mtime);
    Some((etag, last_modified))
}

fn status_forbid(res: &mut Response) {
    *res.status_mut() = StatusCode::FORBIDDEN;
    *res.body_mut() = body_full("Forbidden");
}

fn status_not_found(res: &mut Response) {
    *res.status_mut() = StatusCode::NOT_FOUND;
    *res.body_mut() = body_full("Not Found");
}

fn status_no_content(res: &mut Response) {
    *res.status_mut() = StatusCode::NO_CONTENT;
}

fn status_bad_request(res: &mut Response, body: &str) {
    *res.status_mut() = StatusCode::BAD_REQUEST;
    if !body.is_empty() {
        *res.body_mut() = body_full(body.to_string());
    }
}

fn set_content_disposition(res: &mut Response, inline: bool, filename: &str) -> Result<()> {
    let kind = if inline { "inline" } else { "attachment" };
    let filename: String = filename
        .chars()
        .map(|ch| {
            if ch.is_ascii_control() && ch != '\t' {
                ' '
            } else {
                ch
            }
        })
        .collect();
    let value = if filename.is_ascii() {
        HeaderValue::from_str(&format!("{kind}; filename=\"{}\"", filename,))?
    } else {
        HeaderValue::from_str(&format!(
            "{kind}; filename=\"{}\"; filename*=UTF-8''{}",
            filename,
            encode_uri(&filename),
        ))?
    };
    res.headers_mut().insert(CONTENT_DISPOSITION, value);
    Ok(())
}

fn is_hidden(hidden: &[String], file_name: &str, is_dir_type: bool) -> bool {
    hidden.iter().any(|v| {
        if is_dir_type {
            if let Some(x) = v.strip_suffix('/') {
                return glob(x, file_name);
            }
        }
        glob(v, file_name)
    })
}

fn set_webdav_headers(res: &mut Response) {
    res.headers_mut().insert(
        "Allow",
        HeaderValue::from_static(
            "GET,HEAD,PUT,OPTIONS,DELETE,PATCH,PROPFIND,COPY,MOVE,CHECKAUTH,LOGOUT",
        ),
    );
    res.headers_mut()
        .insert("DAV", HeaderValue::from_static("1, 2, 3"));
}

async fn get_content_type(path: &Path) -> Result<String> {
    let mut buffer: Vec<u8> = vec![];
    fs::File::open(path)
        .await?
        .take(1024)
        .read_to_end(&mut buffer)
        .await?;
    let mime = mime_guess::from_path(path).first();
    let is_text = content_inspector::inspect(&buffer).is_text();
    let content_type = if is_text {
        let mut detector = chardetng::EncodingDetector::new();
        detector.feed(&buffer, buffer.len() < 1024);
        let (enc, confident) = detector.guess_assess(None, true);
        let charset = if confident {
            format!("; charset={}", enc.name())
        } else {
            "".into()
        };
        match mime {
            Some(m) => format!("{m}{charset}"),
            None => format!("text/plain{charset}"),
        }
    } else {
        match mime {
            Some(m) => m.to_string(),
            None => "application/octet-stream".into(),
        }
    };
    Ok(content_type)
}

fn parse_upload_offset(headers: &HeaderMap<HeaderValue>, size: u64) -> Result<Option<u64>> {
    let value = match headers.get("x-update-range") {
        Some(v) => v,
        None => return Ok(None),
    };
    let err = || anyhow!("Invalid X-Update-Range Header");
    let value = value.to_str().map_err(|_| err())?;
    if value == "append" {
        return Ok(Some(size));
    }
    let (start, _) = parse_range(value, size).ok_or_else(err)?;
    Ok(Some(start))
}

async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

fn has_query_flag(query_params: &HashMap<String, String>, name: &str) -> bool {
    query_params
        .get(name)
        .map(|v| v.is_empty())
        .unwrap_or_default()
}
