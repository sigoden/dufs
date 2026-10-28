use crate::{args::Args, server::Response, utils::unix_now};

use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{ed25519::signature::Signer, Signature, SigningKey};
use headers::HeaderValue;
use hyper::{header::WWW_AUTHENTICATE, Method};
use indexmap::IndexMap;
use lazy_static::lazy_static;
use md5::Context;
use sha2::{Digest, Sha256};
use sha_crypt::PasswordVerifier;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use uuid::Uuid;

const REALM: &str = "DUFS";
/// How long a Digest nonce is accepted. Expired nonces are answered with
/// `stale=true` so clients can retry transparently with a fresh nonce.
const DIGEST_AUTH_TIMEOUT: u32 = 60 * 60; // 1 hour
/// Upper bound on the number of tracked `(nonce, user)` replay counters. Only
/// authenticated clients can add entries, and expired entries are pruned, but
/// the cap keeps a long-running server's memory use predictable.
const MAX_NONCE_COUNTS: usize = 10_000;
const TOKEN_EXPIRATION: u64 = 1000 * 60 * 60 * 24 * 3; // 3 days
/// Upper bound on the depth of an auth rule path. The permission tree is built
/// and traversed recursively, so an absurdly deep rule would otherwise risk a
/// stack overflow (which aborts the process under `panic = "abort"`).
const MAX_ACCESS_PATH_DEPTH: usize = 128;

lazy_static! {
    static ref NONCESTARTHASH: Context = {
        let mut h = Context::new();
        h.consume(Uuid::new_v4().as_bytes());
        h.consume(std::process::id().to_be_bytes());
        h
    };
    /// Highest `nc` seen for each `(nonce, user)` pair, together with the unix
    /// time at which the entry can be dropped. Digest replay protection relies
    /// on this table: a nonce may be used many times (with increasing counts),
    /// but the same count must never be accepted twice.
    static ref NONCE_COUNTS: Mutex<HashMap<(String, String), (u32, u64)>> =
        Mutex::new(HashMap::new());
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccessControl {
    empty: bool,
    use_hashed_password: bool,
    users: IndexMap<String, (String, AccessPaths)>,
    anonymous: Option<AccessPaths>,
}

impl Default for AccessControl {
    fn default() -> Self {
        AccessControl {
            empty: true,
            use_hashed_password: false,
            users: IndexMap::new(),
            anonymous: Some(AccessPaths::new(AccessPerm::ReadWrite)),
        }
    }
}

impl AccessControl {
    pub fn new(raw_rules: &[&str]) -> Result<Self> {
        if raw_rules.is_empty() {
            return Ok(Self::default());
        }
        let new_raw_rules = split_rules(raw_rules);
        let mut use_hashed_password = false;
        let mut annoy_paths = None;
        let mut account_paths_pairs = vec![];
        for rule in &new_raw_rules {
            let (account, paths) =
                split_account_paths(rule).ok_or_else(|| anyhow!("Invalid auth `{rule}`"))?;
            if account.is_empty() {
                if annoy_paths.is_some() {
                    bail!("Invalid auth, no duplicate anonymous rules");
                }
                annoy_paths = Some(paths)
            } else if let Some((user, pass)) = account.split_once(':') {
                if user.is_empty() || pass.is_empty() {
                    bail!("Invalid auth `{rule}`");
                }
                account_paths_pairs.push((user, pass, paths));
            }
        }
        let mut anonymous = None;
        if let Some(paths) = annoy_paths {
            let mut access_paths = AccessPaths::default();
            access_paths
                .merge(paths)
                .ok_or_else(|| anyhow!("Invalid auth value `@{paths}"))?;
            anonymous = Some(access_paths);
        }
        let mut users = IndexMap::new();
        for (user, pass, paths) in account_paths_pairs.into_iter() {
            let mut access_paths = AccessPaths::default();
            access_paths
                .merge(paths)
                .ok_or_else(|| anyhow!("Invalid auth value `{user}:{pass}@{paths}"))?;
            if let Some(anon_ap) = &anonymous {
                let orig_user = access_paths.clone();
                access_paths.absorb_anon(
                    anon_ap,
                    &orig_user,
                    AccessPerm::IndexOnly,
                    AccessPerm::IndexOnly,
                );
            }
            if pass.starts_with("$6$") {
                use_hashed_password = true;
            }
            users.insert(user.to_string(), (pass.to_string(), access_paths));
        }

        Ok(Self {
            empty: false,
            use_hashed_password,
            users,
            anonymous,
        })
    }

    pub fn has_users(&self) -> bool {
        !self.users.is_empty()
    }

    /// Returns the guarded `(user, access_paths, via_token, stale)`. `via_token`
    /// is true only when the identity was established by a share token rather
    /// than primary credentials or anonymous access. `stale` is true when the
    /// credentials were valid but the Digest nonce has expired, so the caller
    /// should answer with a `stale=true` challenge instead of a plain 401.
    /// `request_target` is the raw request target from the request line (path
    /// and query), used to bind Digest credentials to the request they were
    /// computed for. `root` is the served directory, used to resolve filesystem
    /// aliases of configured names.
    #[allow(clippy::too_many_arguments)]
    pub async fn guard(
        &self,
        path: &str,
        method: &Method,
        request_target: &str,
        authorization: Option<&HeaderValue>,
        token: Option<&String>,
        guard_options: bool,
        root: &Path,
    ) -> (Option<String>, Option<AccessPaths>, bool, bool) {
        if self.empty {
            return (
                None,
                Some(AccessPaths::new(AccessPerm::ReadWrite)),
                false,
                false,
            );
        }

        if method == Method::GET {
            if let Some(token) = token {
                if let Ok((user, ap)) = self.verify_token(token, path) {
                    return (
                        Some(user),
                        ap.guard(path, method, Some(root)).await,
                        true,
                        false,
                    );
                }
            }
        }

        if let Some(authorization) = authorization {
            if let Some(user) = get_auth_user(authorization) {
                if let Some((pass, ap)) = self.users.get(&user) {
                    match check_auth(authorization, method.as_str(), request_target, &user, pass) {
                        AuthResult::Valid => {
                            if method == Method::OPTIONS {
                                return (
                                    Some(user),
                                    Some(AccessPaths::new(AccessPerm::ReadOnly)),
                                    false,
                                    false,
                                );
                            }
                            return (
                                Some(user),
                                ap.guard(path, method, Some(root)).await,
                                false,
                                false,
                            );
                        }
                        AuthResult::Stale => return (None, None, false, true),
                        AuthResult::Invalid => {}
                    }
                }
            }

            return (None, None, false, false);
        }

        if !guard_options && method == Method::OPTIONS {
            return (
                None,
                Some(AccessPaths::new(AccessPerm::ReadOnly)),
                false,
                false,
            );
        }

        if let Some(ap) = self.anonymous.as_ref() {
            return (None, ap.guard(path, method, Some(root)).await, false, false);
        }

        (None, None, false, false)
    }

    /// Whether `user` may read `path` according to the complete rule tree.
    ///
    /// Unlike [`Self::guard`], the check is not scoped to the request path. It
    /// is used for files that are served implicitly, such as the index file
    /// rendered for a directory request or used as the SPA fallback. A share
    /// token's path scope is intentionally not re-applied here: the token
    /// scopes the requested route, while the index file is part of that
    /// route's response.
    pub async fn can_read(&self, user: Option<&str>, path: &str, root: &Path) -> bool {
        if self.empty {
            return true;
        }
        let access_paths = match user {
            Some(user) => match self.users.get(user) {
                Some((_, access_paths)) => access_paths,
                None => return false,
            },
            None => match self.anonymous.as_ref() {
                Some(access_paths) => access_paths,
                None => return false,
            },
        };
        access_paths
            .guard(path, &Method::GET, Some(root))
            .await
            .is_some_and(|access_paths| !access_paths.perm().indexonly())
    }

    pub fn generate_token(&self, path: &str, user: &str) -> Result<String> {
        let (pass, _) = self
            .users
            .get(user)
            .ok_or_else(|| anyhow!("Not found user '{user}'"))?;
        let exp = (unix_now().as_millis() as u64).saturating_add(TOKEN_EXPIRATION);
        let message = format!("{path}:{exp}");
        let signing_key = derive_secret_key(user, pass);
        let sig = signing_key.sign(message.as_bytes()).to_bytes();

        let mut raw = Vec::with_capacity(64 + 8 + user.len());
        raw.extend_from_slice(&sig);
        raw.extend_from_slice(&exp.to_be_bytes());
        raw.extend_from_slice(user.as_bytes());

        Ok(hex::encode(raw))
    }

    fn verify_token<'a>(&'a self, token: &str, path: &str) -> Result<(String, &'a AccessPaths)> {
        let raw = hex::decode(token)?;

        if raw.len() < 72 {
            bail!("Invalid token");
        }

        let sig_bytes = &raw[..64];
        let exp_bytes = &raw[64..72];
        let user_bytes = &raw[72..];

        let exp = u64::from_be_bytes(exp_bytes.try_into()?);
        if unix_now().as_millis() as u64 > exp {
            bail!("Token expired");
        }

        let user = std::str::from_utf8(user_bytes)?;
        let (pass, ap) = self
            .users
            .get(user)
            .ok_or_else(|| anyhow!("Not found user '{user}'"))?;

        let sig = Signature::from_bytes(&<[u8; 64]>::try_from(sig_bytes)?);

        let message = format!("{path}:{exp}");
        derive_secret_key(user, pass).verify(message.as_bytes(), &sig)?;
        Ok((user.to_string(), ap))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AccessPaths {
    perm: AccessPerm,
    children: IndexMap<String, AccessPaths>,
}

impl AccessPaths {
    pub fn new(perm: AccessPerm) -> Self {
        Self {
            perm,
            ..Default::default()
        }
    }

    pub fn perm(&self) -> AccessPerm {
        self.perm
    }

    pub fn set_perm(&mut self, perm: AccessPerm) {
        if !perm.indexonly() {
            self.perm = perm;
        }
    }

    pub fn merge(&mut self, paths: &str) -> Option<()> {
        for item in paths.trim_matches(',').split(',') {
            let (path, perm) = match item.split_once(':') {
                None => (item, AccessPerm::ReadOnly),
                Some((path, "ro")) => (path, AccessPerm::ReadOnly),
                Some((path, "rw")) => (path, AccessPerm::ReadWrite),
                _ => return None,
            };
            let trimmed = path.trim_matches('/');
            if !trimmed.is_empty() && trimmed.split('/').count() > MAX_ACCESS_PATH_DEPTH {
                return None;
            }
            self.add(path, perm);
        }
        Some(())
    }

    /// Guard `path` for `method` and return the resolved permission subtree.
    ///
    /// The result keeps the configured children of the matched node (instead of
    /// only its permission) so recursive operations can call
    /// [`Self::has_stricter_descendant`] to find rules the request path alone
    /// does not reveal.
    pub async fn guard(&self, path: &str, method: &Method, root: Option<&Path>) -> Option<Self> {
        let target = match self.resolve(path, root).await {
            Ok((node, perm)) => {
                let mut target = node.clone();
                // An index-only node has no permission of its own; its
                // effective permission is the one inherited from its parent.
                if !perm.indexonly() {
                    target.perm = perm;
                }
                target
            }
            Err(perm) => {
                if perm.indexonly() {
                    return None;
                }
                AccessPaths::new(perm)
            }
        };
        if !is_readonly_method(method) && !target.perm().readwrite() {
            return None;
        }
        Some(target)
    }

    fn add(&mut self, path: &str, perm: AccessPerm) {
        let path = path.trim_matches('/');
        if path.is_empty() {
            self.set_perm(perm);
        } else {
            let parts: Vec<&str> = path.split('/').collect();
            self.add_impl(&parts, perm);
        }
    }

    fn add_impl(&mut self, parts: &[&str], perm: AccessPerm) {
        if parts.is_empty() {
            self.perm = perm;
            return;
        }
        let child = self.children.entry(parts[0].to_string()).or_default();
        child.add_impl(&parts[1..], perm)
    }

    /// Merge anonymous `AccessPaths` into `self` (a user's paths) with "higher perm wins" semantics.
    /// `orig_user` is a snapshot of `self` before any anonymous merging begins, used so that
    /// the user's own effective perm is measured against the pre-merge state.
    fn absorb_anon(
        &mut self,
        anon: &AccessPaths,
        orig_user: &AccessPaths,
        user_inherited: AccessPerm,
        anon_inherited: AccessPerm,
    ) {
        let anon_eff = if !anon.perm.indexonly() {
            anon.perm
        } else {
            anon_inherited
        };
        let orig_user_eff = if !orig_user.perm.indexonly() {
            orig_user.perm
        } else {
            user_inherited
        };

        let combined = std::cmp::max(anon_eff, orig_user_eff);
        if !combined.indexonly() && combined > self.perm {
            self.perm = combined;
        }

        let default_ap = AccessPaths::default();
        for (name, anon_child) in &anon.children {
            let orig_user_child = orig_user.children.get(name).unwrap_or(&default_ap);
            let user_child = self.children.entry(name.clone()).or_default();
            user_child.absorb_anon(anon_child, orig_user_child, orig_user_eff, anon_eff);
        }
    }

    /// Resolve `path` against the permission tree.
    ///
    /// This drops the configured children of the matched node and is used by
    /// unit tests only; production callers use [`Self::guard`], which keeps
    /// them so recursive operations can inspect the subtree.
    ///
    /// `root` is the on-disk directory the relative `path` resolves against. It
    /// is used to recognize filesystem aliases of configured names: the backing
    /// filesystem may resolve several different spellings (case variants, NTFS
    /// 8.3 short names, NTFS data streams, ...) to the same object. Matching such
    /// a request against the canonical rule instead of falling back to the parent
    /// permission keeps a restrictive child rule enforceable. Pass `None` for
    /// pure string matching.
    #[cfg(test)]
    pub async fn find(&self, path: &str, root: Option<&Path>) -> Option<AccessPaths> {
        match self.resolve(path, root).await {
            Ok((node, perm)) => {
                if perm.indexonly() {
                    Some(node.clone())
                } else {
                    Some(AccessPaths::new(perm))
                }
            }
            Err(perm) => {
                if perm.indexonly() {
                    None
                } else {
                    Some(AccessPaths::new(perm))
                }
            }
        }
    }

    /// Resolve an entry below this (guarded) path and return its effective
    /// permission as well as whether it protects a stricter descendant rule.
    ///
    /// `prefix` is this node's path relative to `root`, used to recognize
    /// filesystem aliases of configured names. Pass `None` for `root` in pure
    /// string matching tests.
    pub async fn entry_access(
        &self,
        path: &str,
        prefix: &[&str],
        root: Option<&Path>,
    ) -> EntryAccess {
        // Without configured children every entry inherits this node's
        // permission, so the common no-auth case needs no per-entry lookup.
        if self.children.is_empty() {
            return EntryAccess {
                perm: self.perm,
                restricted: false,
            };
        }
        match self.resolve_from(path, root, prefix).await {
            Ok((node, perm)) => EntryAccess {
                perm,
                restricted: perm.readwrite()
                    && node.has_stricter_child(perm, AccessPerm::ReadWrite),
            },
            Err(perm) => EntryAccess {
                perm,
                restricted: false,
            },
        }
    }

    /// Resolve `path` to its configured node and effective permission.
    ///
    /// `Ok((node, perm))` means every path component matched a configured rule
    /// (or a filesystem alias of one). `Err(perm)` means a component has no
    /// rule, so the path falls back to the inherited `perm`.
    async fn resolve<'a>(
        &'a self,
        path: &str,
        root: Option<&Path>,
    ) -> Result<(&'a AccessPaths, AccessPerm), AccessPerm> {
        self.resolve_from(path, root, &[]).await
    }

    /// Resolve `path` relative to this node with `prefix` locating it under
    /// `root` for filesystem alias matching.
    async fn resolve_from<'a>(
        &'a self,
        path: &str,
        root: Option<&Path>,
        prefix: &[&str],
    ) -> Result<(&'a AccessPaths, AccessPerm), AccessPerm> {
        let parts: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|v| !v.is_empty())
            .collect();
        let mut perm = self.perm;
        let mut paths = self;
        let mut base: Vec<&str> = prefix.to_vec();
        for name in parts {
            if !paths.perm.indexonly() {
                perm = paths.perm;
            }
            let child = match paths.children.get(name) {
                Some(child) => child,
                None => match paths.find_alias(name, &base, root).await {
                    Some(child) => child,
                    None => return Err(perm),
                },
            };
            paths = child;
            base.push(name);
        }
        if !paths.perm.indexonly() {
            perm = paths.perm;
        }
        Ok((paths, perm))
    }

    /// Whether the subtree rooted at this (guarded) path contains a rule more
    /// restrictive than `required`.
    ///
    /// [`Self::guard`] only resolves the permission of the request path itself,
    /// but deleting or moving a directory affects every descendant. Without
    /// this check an account with `:rw` on an ancestor could remove or relocate
    /// a child that is explicitly `:ro`, because the recursive filesystem
    /// operation never re-evaluates the child rule.
    pub fn has_stricter_descendant(&self, required: AccessPerm) -> bool {
        self.has_stricter_child(self.perm, required)
    }

    fn has_stricter_child(&self, inherited: AccessPerm, required: AccessPerm) -> bool {
        self.children.values().any(|child| {
            let perm = if child.perm.indexonly() {
                inherited
            } else {
                child.perm
            };
            perm < required || child.has_stricter_child(perm, required)
        })
    }

    /// Find a configured child whose name the backing filesystem resolves to
    /// the same object as `name` under `prefix` (a path relative to `root`).
    async fn find_alias<'a>(
        &'a self,
        name: &str,
        prefix: &[&str],
        root: Option<&Path>,
    ) -> Option<&'a AccessPaths> {
        let root = root?;
        // Cheap pre-filter: without a plausible alias there is no need to touch
        // the filesystem at all, so precise requests stay on the fast path.
        if !self.children.keys().any(|key| could_alias(name, key)) {
            return None;
        }
        let name_ident = path_identity(root, prefix, name).await?;
        // The identity exposes the on-disk spelling of `name`, so children that
        // could not spell it can be skipped without a lookup of their own, which
        // bounds the number of lookups however many children a rule has.
        let real_name = name_ident.file_name().and_then(|v| v.to_str());
        for (key, child) in &self.children {
            if real_name.is_some_and(|real| !could_alias(real, key)) {
                continue;
            }
            if path_identity(root, prefix, key)
                .await
                .is_some_and(|v| v == name_ident)
            {
                return Some(child);
            }
        }
        None
    }

    pub fn child_names(&self) -> Vec<&String> {
        self.children.keys().collect()
    }

    pub fn entry_paths(&self, base: &Path) -> Vec<PathBuf> {
        if !self.perm().indexonly() {
            return vec![base.to_path_buf()];
        }
        let mut output = vec![];
        self.entry_paths_impl(&mut output, base);
        output
    }

    fn entry_paths_impl(&self, output: &mut Vec<PathBuf>, base: &Path) {
        for (name, child) in self.children.iter() {
            let base = base.join(name);
            if child.perm().indexonly() {
                child.entry_paths_impl(output, &base);
            } else {
                output.push(base)
            }
        }
    }
}

/// Effective permission of an entry below a guarded path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryAccess {
    pub perm: AccessPerm,
    /// Whether the subtree rooted at the entry contains a rule stricter than
    /// read-write. For directories this blocks recursive delete and move.
    pub restricted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum AccessPerm {
    #[default]
    IndexOnly,
    ReadOnly,
    ReadWrite,
}

impl AccessPerm {
    pub fn indexonly(&self) -> bool {
        self == &AccessPerm::IndexOnly
    }

    pub fn readwrite(&self) -> bool {
        self == &AccessPerm::ReadWrite
    }
}

pub fn www_authenticate(res: &mut Response, args: &Args, stale: bool) -> Result<()> {
    if args.auth.use_hashed_password {
        let basic = HeaderValue::from_str(&format!("Basic realm=\"{REALM}\""))?;
        res.headers_mut().insert(WWW_AUTHENTICATE, basic);
    } else {
        let nonce = create_nonce()?;
        let stale = if stale { ", stale=true" } else { "" };
        let digest = HeaderValue::from_str(&format!(
            "Digest realm=\"{REALM}\", nonce=\"{nonce}\", qop=\"auth\"{stale}"
        ))?;
        let basic = HeaderValue::from_str(&format!("Basic realm=\"{REALM}\""))?;
        res.headers_mut().append(WWW_AUTHENTICATE, digest);
        res.headers_mut().append(WWW_AUTHENTICATE, basic);
    }
    Ok(())
}

pub fn get_auth_user(authorization: &HeaderValue) -> Option<String> {
    if let Some(value) = strip_prefix(authorization.as_bytes(), b"Basic ") {
        let value: Vec<u8> = STANDARD.decode(value).ok()?;
        let parts: Vec<&str> = std::str::from_utf8(&value).ok()?.split(':').collect();
        Some(parts[0].to_string())
    } else if let Some(value) = strip_prefix(authorization.as_bytes(), b"Digest ") {
        let digest_map = to_headermap(value).ok()?;
        let username = digest_map.get(b"username".as_ref())?;
        std::str::from_utf8(username).map(|v| v.to_string()).ok()
    } else {
        None
    }
}

/// Result of checking an `Authorization` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthResult {
    /// Credentials are valid for this request.
    Valid,
    /// Credentials are valid, but the Digest nonce has expired. The client
    /// should retry with a fresh nonce without re-prompting the user.
    Stale,
    /// Credentials are missing, malformed, forged, or replayed.
    Invalid,
}

pub fn check_auth(
    authorization: &HeaderValue,
    method: &str,
    request_target: &str,
    auth_user: &str,
    auth_pass: &str,
) -> AuthResult {
    if let Some(value) = strip_prefix(authorization.as_bytes(), b"Basic ") {
        let Some(value) = STANDARD.decode(value).ok() else {
            return AuthResult::Invalid;
        };
        let Some((user, pass)) = std::str::from_utf8(&value)
            .ok()
            .and_then(|v| v.split_once(':'))
        else {
            return AuthResult::Invalid;
        };

        if user != auth_user {
            return AuthResult::Invalid;
        }

        if auth_pass.starts_with("$6$") {
            if sha_crypt::ShaCrypt::SHA512
                .verify_password(pass.as_bytes(), auth_pass)
                .is_ok()
            {
                return AuthResult::Valid;
            }
        } else if pass == auth_pass {
            return AuthResult::Valid;
        }

        AuthResult::Invalid
    } else if let Some(value) = strip_prefix(authorization.as_bytes(), b"Digest ") {
        check_digest_auth(value, method, request_target, auth_user, auth_pass)
            .unwrap_or(AuthResult::Invalid)
    } else {
        AuthResult::Invalid
    }
}

/// Verify a Digest `Authorization` header. Returns `None` for malformed input;
/// `check_auth` maps that to [`AuthResult::Invalid`].
fn check_digest_auth(
    value: &[u8],
    method: &str,
    request_target: &str,
    auth_user: &str,
    auth_pass: &str,
) -> Option<AuthResult> {
    let digest_map = to_headermap(value).ok()?;
    let username = digest_map
        .get(b"username".as_ref())
        .and_then(|b| std::str::from_utf8(b).ok())?;
    let nonce = digest_map.get(b"nonce".as_ref())?;
    let user_response = digest_map.get(b"response".as_ref())?;
    let uri = digest_map.get(b"uri".as_ref())?;

    // RFC 7616 §3.4.6: the `uri` directive must designate the resource
    // actually being requested. Without this check a captured header would
    // authenticate a request for any other path that uses the same method.
    if std::str::from_utf8(uri).ok()? != request_target {
        return Some(AuthResult::Invalid);
    }
    if auth_user != username {
        return Some(AuthResult::Invalid);
    }

    // `Ok(false)` means the nonce is authentic but expired. The response is
    // still verified below so an expired nonce can be answered with
    // `stale=true` instead of forcing the client to prompt the user again.
    let nonce_valid = match validate_nonce(nonce) {
        Ok(v) => v,
        Err(_) => return Some(AuthResult::Invalid),
    };

    let mut h = Context::new();
    h.consume(format!("{auth_user}:{REALM}:{auth_pass}").as_bytes());
    let auth_pass = format!("{:x}", h.finalize());

    let mut ha = Context::new();
    ha.consume(method);
    ha.consume(b":");
    ha.consume(uri);
    let ha = format!("{:x}", ha.finalize());

    let qop = *digest_map.get(b"qop".as_ref())?;
    if qop != b"auth" && qop != b"auth-int" {
        return Some(AuthResult::Invalid);
    }
    // `nc` and `cnonce` are mandatory for qop=auth (RFC 7616 §3.4.1) and are
    // what let the server detect replays below.
    let nc = digest_map.get(b"nc".as_ref())?;
    let cnonce = digest_map.get(b"cnonce".as_ref())?;

    let mut c = Context::new();
    c.consume(&auth_pass);
    c.consume(b":");
    c.consume(nonce);
    c.consume(b":");
    c.consume(nc);
    c.consume(b":");
    c.consume(cnonce);
    c.consume(b":");
    c.consume(qop);
    c.consume(b":");
    c.consume(&*ha);
    let correct_response = format!("{:x}", c.finalize());

    if correct_response.as_bytes() != *user_response {
        return Some(AuthResult::Invalid);
    }

    if !nonce_valid {
        // The digest is valid but the nonce is too old. A compliant client
        // retries with the fresh nonce and does not re-prompt the user. Do not
        // record `nc` for a dead nonce.
        return Some(AuthResult::Stale);
    }

    // Only record the nonce count once the response has been verified, so
    // unauthenticated clients cannot grow the table.
    if !record_nonce_count(std::str::from_utf8(nonce).ok()?, auth_user, nc) {
        return Some(AuthResult::Invalid);
    }
    Some(AuthResult::Valid)
}

/// Remember the highest `nc` seen for `(nonce, user)` and reject values that
/// were already used. Digest `nc` is an 8-digit hexadecimal counter that must
/// strictly increase for a given nonce; seeing the same value twice means the
/// Authorization header was replayed (RFC 7616 §3.2.2). Returns `false` for
/// malformed or replayed counts.
fn record_nonce_count(nonce: &str, user: &str, nc: &[u8]) -> bool {
    if nc.len() != 8 || !nc.iter().all(u8::is_ascii_hexdigit) {
        return false;
    }
    let Ok(nc) = std::str::from_utf8(nc) else {
        return false;
    };
    let Ok(nc) = u32::from_str_radix(nc, 16) else {
        return false;
    };

    let now = unix_now().as_secs();
    let expires = now.saturating_add(DIGEST_AUTH_TIMEOUT as u64);
    let mut nonce_counts = NONCE_COUNTS.lock().unwrap();
    if nonce_counts.len() >= MAX_NONCE_COUNTS {
        nonce_counts.retain(|_, (_, expires)| *expires > now);
        // `retain` cannot make room if every tracked nonce is still fresh;
        // dropping all state is preferable to unbounded memory growth.
        if nonce_counts.len() >= MAX_NONCE_COUNTS {
            nonce_counts.clear();
        }
    }
    let key = (nonce.to_string(), user.to_string());
    match nonce_counts.get_mut(&key) {
        Some((count, _)) => {
            if nc <= *count {
                return false;
            }
            *count = nc;
            true
        }
        None => {
            nonce_counts.insert(key, (nc, expires));
            true
        }
    }
}

fn derive_secret_key(user: &str, pass: &str) -> SigningKey {
    let mut hasher = Sha256::new();
    hasher.update(format!("{user}:{pass}").as_bytes());
    let hash = hasher.finalize();
    SigningKey::from_bytes(&hash.into())
}

/// Layout of a digest nonce: 8 hex digits of time, 16 hex digits of randomness
/// and 26 hex digits of HMAC. The random part keeps every challenge unique,
/// which matters because replay counters are tracked per nonce while clients
/// restart `nc` at 1 for each new challenge.
const NONCE_TIME_LEN: usize = 8;
const NONCE_RANDOM_LEN: usize = 16;
const NONCE_HASH_LEN: usize = 26;
const NONCE_HASH_OFFSET: usize = NONCE_TIME_LEN + NONCE_RANDOM_LEN;
const NONCE_LEN: usize = NONCE_HASH_OFFSET + NONCE_HASH_LEN;

/// Check if a nonce is still valid.
/// Return an error if it was never valid
fn validate_nonce(nonce: &[u8]) -> Result<bool> {
    if nonce.len() != NONCE_LEN || !nonce.iter().all(u8::is_ascii_hexdigit) {
        bail!("invalid nonce");
    }
    //parse hex
    let n = std::str::from_utf8(nonce)?;
    //get time and randomness
    if let (Ok(secs_nonce), Ok(random)) = (
        u32::from_str_radix(&n[..NONCE_TIME_LEN], 16),
        u64::from_str_radix(&n[NONCE_TIME_LEN..NONCE_HASH_OFFSET], 16),
    ) {
        //check time
        let now = unix_now();
        let secs_now = now.as_secs() as u32;

        if let Some(dur) = secs_now.checked_sub(secs_nonce) {
            //check hash
            let mut h = NONCESTARTHASH.clone();
            h.consume(secs_nonce.to_be_bytes());
            h.consume(random.to_be_bytes());
            let h = format!("{:x}", h.finalize());
            if h[..NONCE_HASH_LEN] == n[NONCE_HASH_OFFSET..] {
                return Ok(dur < DIGEST_AUTH_TIMEOUT);
            }
        }
    }
    bail!("invalid nonce");
}

/// Resolve the on-disk identity of `root/<prefix...>/<name>`.
async fn path_identity(root: &Path, prefix: &[&str], name: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    for part in prefix {
        path.push(part);
    }
    path.push(name);
    tokio::fs::canonicalize(path).await.ok()
}

/// Whether `name` could be a filesystem alias of the configured child `key`.
fn could_alias(name: &str, key: &str) -> bool {
    if !is_plain_name(name) || !is_plain_name(key) {
        return true;
    }
    name.eq_ignore_ascii_case(key)
}

fn is_plain_name(name: &str) -> bool {
    name.is_ascii()
        && !name.contains(':')
        && !name.contains('~')
        && !name.ends_with('.')
        && !name.ends_with(' ')
}

fn is_readonly_method(method: &Method) -> bool {
    method == Method::GET
        || method == Method::OPTIONS
        || method == Method::HEAD
        || method.as_str() == "PROPFIND"
        || method.as_str() == "CHECKAUTH"
        || method.as_str() == "LOGOUT"
}

fn strip_prefix<'a>(search: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    let l = prefix.len();
    if search.len() < l {
        return None;
    }
    if &search[..l] == prefix {
        Some(&search[l..])
    } else {
        None
    }
}

fn to_headermap(header: &[u8]) -> Result<HashMap<&[u8], &[u8]>, ()> {
    let mut sep = Vec::new();
    let mut assign = Vec::new();
    let mut i: usize = 0;
    let mut esc = false;
    for c in header {
        match (c, esc) {
            (b'=', false) => assign.push(i),
            (b',', false) => sep.push(i),
            (b'"', false) => esc = true,
            (b'"', true) => esc = false,
            _ => {}
        }
        i += 1;
    }
    sep.push(i);

    i = 0;
    let mut ret = HashMap::new();
    for (&k, &a) in sep.iter().zip(assign.iter()) {
        while header.get(i) == Some(&b' ') {
            i += 1;
        }
        if a <= i || k <= 1 + a || k > header.len() {
            //keys and values must contain one char
            return Err(());
        }
        let key = &header[i..a];
        let val = if header[1 + a] == b'"' && header[k - 1] == b'"' {
            //escaped
            if 2 + a > k - 1 {
                return Err(());
            }
            &header[2 + a..k - 1]
        } else {
            //not escaped
            &header[1 + a..k]
        };
        i = 1 + k;
        ret.insert(key, val);
    }
    Ok(ret)
}

fn create_nonce() -> Result<String> {
    let secs = unix_now().as_secs() as u32;
    let mut random_bytes = [0u8; 8];
    random_bytes.copy_from_slice(&Uuid::new_v4().as_bytes()[..8]);
    let random = u64::from_be_bytes(random_bytes);
    let mut h = NONCESTARTHASH.clone();
    h.consume(secs.to_be_bytes());
    h.consume(random.to_be_bytes());

    let n = format!("{:08x}{:016x}{:032x}", secs, random, h.finalize());
    Ok(n[..NONCE_LEN].to_string())
}

fn split_account_paths(s: &str) -> Option<(&str, &str)> {
    let i = s.find("@/")?;
    Some((&s[0..i], &s[i + 1..]))
}

fn split_rules(rules: &[&str]) -> Vec<String> {
    let mut output = vec![];
    for rule in rules {
        let parts: Vec<&str> = rule.split('|').collect();
        let mut rules_list = vec![];
        let mut concated_part = String::new();
        for (i, part) in parts.iter().enumerate() {
            if part.contains("@/") {
                concated_part.push_str(part);
                let mut concated_part_tmp = String::new();
                std::mem::swap(&mut concated_part_tmp, &mut concated_part);
                rules_list.push(concated_part_tmp);
                continue;
            }
            concated_part.push_str(part);
            if i < parts.len() - 1 {
                concated_part.push('|');
            }
        }
        if !concated_part.is_empty() {
            rules_list.push(concated_part)
        }
        output.extend(rules_list);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_account_paths() {
        assert_eq!(
            split_account_paths("user:pass@/:rw"),
            Some(("user:pass", "/:rw"))
        );
        assert_eq!(
            split_account_paths("user:pass@@/:rw"),
            Some(("user:pass@", "/:rw"))
        );
        assert_eq!(
            split_account_paths("user:pass@1@/:rw"),
            Some(("user:pass@1", "/:rw"))
        );
    }

    #[test]
    fn test_compact_split_rules() {
        assert_eq!(
            split_rules(&["user1:pass1@/:rw|user2:pass2@/:rw"]),
            ["user1:pass1@/:rw", "user2:pass2@/:rw"]
        );
        assert_eq!(
            split_rules(&["user1:pa|ss1@/:rw|user2:pa|ss2@/:rw"]),
            ["user1:pa|ss1@/:rw", "user2:pa|ss2@/:rw"]
        );
        assert_eq!(
            split_rules(&["user1:pa|ss1@/:rw|@/"]),
            ["user1:pa|ss1@/:rw", "@/"]
        );
    }

    #[tokio::test]
    async fn test_access_paths() {
        let mut paths = AccessPaths::default();
        paths.add("/dir1", AccessPerm::ReadWrite);
        paths.add("/dir2/dir21", AccessPerm::ReadWrite);
        paths.add("/dir2/dir21/dir211", AccessPerm::ReadOnly);
        paths.add("/dir2/dir22", AccessPerm::ReadOnly);
        paths.add("/dir2/dir22/dir221", AccessPerm::ReadWrite);
        paths.add("/dir2/dir23/dir231", AccessPerm::ReadWrite);
        assert_eq!(
            paths.entry_paths(Path::new("/tmp")),
            [
                "/tmp/dir1",
                "/tmp/dir2/dir21",
                "/tmp/dir2/dir22",
                "/tmp/dir2/dir23/dir231",
            ]
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>()
        );
        assert_eq!(
            paths
                .find("dir2", None)
                .await
                .map(|v| v.entry_paths(Path::new("/tmp/dir2"))),
            Some(
                [
                    "/tmp/dir2/dir21",
                    "/tmp/dir2/dir22",
                    "/tmp/dir2/dir23/dir231"
                ]
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>()
            )
        );
        assert_eq!(
            paths.find("dir1/file", None).await,
            Some(AccessPaths::new(AccessPerm::ReadWrite))
        );
        assert_eq!(
            paths.find("dir2/dir21/file", None).await,
            Some(AccessPaths::new(AccessPerm::ReadWrite))
        );
        assert_eq!(
            paths.find("dir2/dir21/dir211/file", None).await,
            Some(AccessPaths::new(AccessPerm::ReadOnly))
        );
        assert_eq!(
            paths.find("dir2/dir22/file", None).await,
            Some(AccessPaths::new(AccessPerm::ReadOnly))
        );
        assert_eq!(
            paths.find("dir2/dir22/dir221/file", None).await,
            Some(AccessPaths::new(AccessPerm::ReadWrite))
        );
        assert_eq!(paths.find("dir2/dir23/file", None).await, None);
        assert_eq!(
            paths.find("dir2/dir23//dir231/file", None).await,
            Some(AccessPaths::new(AccessPerm::ReadWrite))
        );
    }

    #[tokio::test]
    async fn test_has_stricter_descendant() {
        let mut paths = AccessPaths::default();
        paths.add("/", AccessPerm::ReadWrite);
        paths.add("/dir1", AccessPerm::ReadWrite);
        paths.add("/dir1/protected", AccessPerm::ReadOnly);
        paths.add("/dir1/open", AccessPerm::ReadWrite);
        paths.add("/dir2/deep/protected", AccessPerm::ReadOnly);

        let guarded = paths.guard("", &Method::DELETE, None).await.unwrap();
        // A stricter rule below a writable path must be found.
        assert!(guarded.has_stricter_descendant(AccessPerm::ReadWrite));

        let guarded = paths.guard("dir1", &Method::DELETE, None).await.unwrap();
        assert!(guarded.has_stricter_descendant(AccessPerm::ReadWrite));

        // Intermediate index-only nodes inherit the writable permission.
        let guarded = paths.guard("dir2", &Method::DELETE, None).await.unwrap();
        assert!(guarded.has_stricter_descendant(AccessPerm::ReadWrite));

        // Nothing stricter below a fully writable path.
        let guarded = paths
            .guard("dir1/open", &Method::DELETE, None)
            .await
            .unwrap();
        assert!(!guarded.has_stricter_descendant(AccessPerm::ReadWrite));

        // A path without configured rules has nothing to protect.
        let guarded = paths.guard("unknown", &Method::DELETE, None).await.unwrap();
        assert!(!guarded.has_stricter_descendant(AccessPerm::ReadWrite));

        // Read-only requests are not affected by read-only descendants.
        let guarded = paths.guard("dir1", &Method::GET, None).await.unwrap();
        assert!(!guarded.has_stricter_descendant(AccessPerm::ReadOnly));
    }

    #[tokio::test]
    async fn test_entry_access() {
        let mut paths = AccessPaths::default();
        paths.add("/", AccessPerm::ReadWrite);
        paths.add("/dir1", AccessPerm::ReadOnly);
        paths.add("/dir2/protected", AccessPerm::ReadOnly);
        paths.add("/dir3/open", AccessPerm::ReadWrite);
        paths.add("/dir4", AccessPerm::ReadOnly);
        paths.add("/dir4/open", AccessPerm::ReadWrite);

        assert_eq!(
            paths.entry_access("dir1", &[], None).await,
            EntryAccess {
                perm: AccessPerm::ReadOnly,
                restricted: false
            }
        );
        // A writable directory with a read-only descendant cannot be removed.
        assert_eq!(
            paths.entry_access("dir2", &[], None).await,
            EntryAccess {
                perm: AccessPerm::ReadWrite,
                restricted: true
            }
        );
        // Intermediate index-only nodes inherit their parent's permission.
        assert_eq!(
            paths.entry_access("dir3", &[], None).await,
            EntryAccess {
                perm: AccessPerm::ReadWrite,
                restricted: false
            }
        );
        assert_eq!(
            paths.entry_access("unknown", &[], None).await,
            EntryAccess {
                perm: AccessPerm::ReadWrite,
                restricted: false
            }
        );

        // Direct children resolve from the guarded node, including a writable
        // child below a read-only parent.
        let guarded = paths.guard("dir2", &Method::GET, None).await.unwrap();
        assert_eq!(
            guarded.entry_access("protected", &["dir2"], None).await,
            EntryAccess {
                perm: AccessPerm::ReadOnly,
                restricted: false
            }
        );
        let guarded = paths.guard("dir4", &Method::GET, None).await.unwrap();
        assert_eq!(
            guarded.entry_access("open", &["dir4"], None).await,
            EntryAccess {
                perm: AccessPerm::ReadWrite,
                restricted: false
            }
        );
    }

    #[test]
    fn test_could_alias() {
        // case variants and alias-shaped names need the filesystem to decide
        assert!(could_alias("TEST.TXT", "test.txt"));
        assert!(could_alias("Dir1", "dir1"));
        assert!(could_alias("test.txt::$DATA", "test.txt"));
        assert!(could_alias("test.txt.", "test.txt"));
        assert!(could_alias("test.txt ", "test.txt"));
        assert!(could_alias("SUPERS~1", "supersecretstorage"));
        assert!(could_alias("supersecretstorage", "SUPERS~1"));
        assert!(could_alias("café", "cafe\u{301}"));
        // plain ASCII names that differ by more than case never alias
        assert!(!could_alias("other.txt", "test.txt"));
        assert!(!could_alias("test.txt.bak", "test.txt"));
    }

    #[tokio::test]
    async fn test_find_alias() {
        use assert_fs::prelude::*;
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("dir1/test.txt").write_str("x").unwrap();
        dir.child("dir1/other.txt").write_str("x").unwrap();
        dir.child("Protected/file.txt").write_str("x").unwrap();

        let mut paths = AccessPaths::default();
        paths.add("/", AccessPerm::ReadWrite);
        paths.add("/dir1/test.txt", AccessPerm::ReadOnly);
        paths.add("/Protected", AccessPerm::ReadOnly);

        let root = Some(dir.path());
        // exact spellings
        assert_eq!(
            paths.find("dir1/test.txt", root).await,
            Some(AccessPaths::new(AccessPerm::ReadOnly))
        );
        // an unrelated sibling still inherits the parent permission
        assert_eq!(
            paths.find("dir1/other.txt", root).await,
            Some(AccessPaths::new(AccessPerm::ReadWrite))
        );

        // an alias of a configured name resolves to the configured rule when the
        // backing filesystem itself folds the names together
        let folds_case = dir.path().join("DIR1").is_dir();
        let protected = if folds_case {
            Some(AccessPaths::new(AccessPerm::ReadOnly))
        } else {
            Some(AccessPaths::new(AccessPerm::ReadWrite))
        };
        assert_eq!(paths.find("dir1/TEST.TXT", root).await, protected);
        assert_eq!(paths.find("DIR1/test.txt", root).await, protected);
        assert_eq!(paths.find("PROTECTED/file.txt", root).await, protected);
    }

    #[test]
    fn test_to_headermap() {
        let map = to_headermap(br#"username="user", realm="DUFS", nonce="abc""#).unwrap();
        assert_eq!(map.get(b"username".as_ref()), Some(&b"user".as_ref()));
        assert_eq!(map.get(b"realm".as_ref()), Some(&b"DUFS".as_ref()));
        assert_eq!(map.get(b"nonce".as_ref()), Some(&b"abc".as_ref()));
    }

    #[test]
    fn test_to_headermap_malformed_no_panic() {
        for header in [
            &b"a=\""[..],
            b"a==,",
            b"a=",
            b"=,",
            b"a=\"b",
            b"a=\"\"",
            b"a=\"b\",",
            b"a=\"b\" , c",
            b" , =, \"",
            b"",
            b",",
            b"\"",
        ] {
            // must not panic; the caller treats Err as invalid credentials
            let _ = to_headermap(header);
        }
    }

    #[test]
    fn test_validate_nonce_no_panic() {
        // a multi-byte UTF-8 char straddling the byte-8 boundary must not panic
        let mut nonce = b"0000000".to_vec();
        nonce.extend_from_slice("é".as_bytes());
        nonce.extend_from_slice(&[b'A'; NONCE_LEN - 9]);
        assert_eq!(nonce.len(), NONCE_LEN);
        assert!(validate_nonce(&nonce).is_err());

        assert!(validate_nonce(b"short").is_err());
        assert!(validate_nonce(&[0xff; NONCE_LEN]).is_err());

        // a freshly created nonce still validates
        let nonce = create_nonce().unwrap();
        assert!(matches!(validate_nonce(nonce.as_bytes()), Ok(true)));

        // nonces minted back to back must differ, even within the same second
        assert_ne!(nonce, create_nonce().unwrap());
    }

    /// Mint a valid nonce whose timestamp is `offset` seconds in the past.
    /// Tests use distinct offsets so they do not share replay state.
    fn nonce_with_offset(offset: u32) -> String {
        let secs = (unix_now().as_secs() as u32).saturating_sub(offset);
        let random = offset as u64 + 1;
        let mut h = NONCESTARTHASH.clone();
        h.consume(secs.to_be_bytes());
        h.consume(random.to_be_bytes());
        let n = format!("{:08x}{:016x}{:032x}", secs, random, h.finalize());
        n[..NONCE_LEN].to_string()
    }

    fn digest_authorization(
        user: &str,
        pass: &str,
        nonce: &str,
        uri: &str,
        nc: &str,
    ) -> HeaderValue {
        let mut ha1 = Context::new();
        ha1.consume(format!("{user}:{REALM}:{pass}").as_bytes());
        let ha1 = format!("{:x}", ha1.finalize());
        let mut ha2 = Context::new();
        ha2.consume(b"GET:");
        ha2.consume(uri.as_bytes());
        let ha2 = format!("{:x}", ha2.finalize());
        let cnonce = "0123456789abcdef";
        let mut resp = Context::new();
        resp.consume(ha1.as_bytes());
        resp.consume(b":");
        resp.consume(nonce.as_bytes());
        resp.consume(b":");
        resp.consume(nc.as_bytes());
        resp.consume(b":");
        resp.consume(cnonce.as_bytes());
        resp.consume(b":auth:");
        resp.consume(ha2.as_bytes());
        let response = format!("{:x}", resp.finalize());
        HeaderValue::from_str(&format!(
            "Digest username=\"{user}\", realm=\"{REALM}\", nonce=\"{nonce}\", uri=\"{uri}\", response=\"{response}\", qop=auth, nc={nc}, cnonce=\"{cnonce}\""
        ))
        .unwrap()
    }

    #[test]
    fn test_digest_binds_uri_to_request_target() {
        let user = "uri-user";
        let nonce = nonce_with_offset(10);
        let auth = digest_authorization(user, "pass", &nonce, "/public/hello.txt", "00000001");
        assert_eq!(
            check_auth(&auth, "GET", "/public/hello.txt", user, "pass"),
            AuthResult::Valid
        );
        // the same header must not authenticate a different resource
        assert_eq!(
            check_auth(&auth, "GET", "/private/secret.txt", user, "pass"),
            AuthResult::Invalid
        );
    }

    #[test]
    fn test_digest_binds_query_to_request_target() {
        let user = "query-user";
        let nonce = nonce_with_offset(20);
        let auth = digest_authorization(user, "pass", &nonce, "/dir/?simple", "00000001");
        assert_eq!(
            check_auth(&auth, "GET", "/dir/?simple", user, "pass"),
            AuthResult::Valid
        );
        // query flags such as ?zip must be covered by the digest too
        assert_eq!(
            check_auth(&auth, "GET", "/dir/?zip", user, "pass"),
            AuthResult::Invalid
        );
    }

    #[test]
    fn test_digest_replay_is_rejected() {
        let user = "replay-user";
        let nonce = nonce_with_offset(30);
        let auth = digest_authorization(user, "pass", &nonce, "/file.txt", "00000001");
        assert_eq!(
            check_auth(&auth, "GET", "/file.txt", user, "pass"),
            AuthResult::Valid
        );
        // replaying the exact same header must fail
        assert_eq!(
            check_auth(&auth, "GET", "/file.txt", user, "pass"),
            AuthResult::Invalid
        );
        // a higher nonce count is accepted
        let next = digest_authorization(user, "pass", &nonce, "/file.txt", "00000002");
        assert_eq!(
            check_auth(&next, "GET", "/file.txt", user, "pass"),
            AuthResult::Valid
        );
        // and going backwards is a replay
        let older = digest_authorization(user, "pass", &nonce, "/file.txt", "00000001");
        assert_eq!(
            check_auth(&older, "GET", "/file.txt", user, "pass"),
            AuthResult::Invalid
        );
        // malformed / non-8-digit counts are rejected
        let malformed = digest_authorization(user, "pass", &nonce, "/file.txt", "000003");
        assert_eq!(
            check_auth(&malformed, "GET", "/file.txt", user, "pass"),
            AuthResult::Invalid
        );
    }

    #[test]
    fn test_digest_nonce_count_is_per_user() {
        let nonce = nonce_with_offset(40);
        let alice = digest_authorization("alice", "pass", &nonce, "/file.txt", "00000001");
        let bob = digest_authorization("bob", "pass", &nonce, "/file.txt", "00000001");
        assert_eq!(
            check_auth(&alice, "GET", "/file.txt", "alice", "pass"),
            AuthResult::Valid
        );
        assert_eq!(
            check_auth(&bob, "GET", "/file.txt", "bob", "pass"),
            AuthResult::Valid
        );
    }

    #[test]
    fn test_digest_expired_nonce_is_stale() {
        let user = "stale-user";
        let nonce = nonce_with_offset(DIGEST_AUTH_TIMEOUT + 60);
        let auth = digest_authorization(user, "pass", &nonce, "/file.txt", "00000001");
        assert_eq!(
            check_auth(&auth, "GET", "/file.txt", user, "pass"),
            AuthResult::Stale
        );

        // Only a valid digest gets the stale hint.
        let wrong_pass = digest_authorization(user, "wrong", &nonce, "/file.txt", "00000001");
        assert_eq!(
            check_auth(&wrong_pass, "GET", "/file.txt", user, "pass"),
            AuthResult::Invalid
        );
        let wrong_uri = digest_authorization(user, "pass", &nonce, "/other.txt", "00000001");
        assert_eq!(
            check_auth(&wrong_uri, "GET", "/file.txt", user, "pass"),
            AuthResult::Invalid
        );

        // A forged nonce must never be reported as stale.
        let mut tampered = nonce.into_bytes();
        let last = tampered.last_mut().unwrap();
        *last = if *last == b'0' { b'1' } else { b'0' };
        let tampered = std::str::from_utf8(&tampered).unwrap().to_string();
        let forged = digest_authorization(user, "pass", &tampered, "/file.txt", "00000001");
        assert_eq!(
            check_auth(&forged, "GET", "/file.txt", user, "pass"),
            AuthResult::Invalid
        );
    }

    #[test]
    fn test_www_authenticate_stale_flag() {
        let args = Args::default();

        let mut res = Response::default();
        www_authenticate(&mut res, &args, true).unwrap();
        let challenges: Vec<&str> = res
            .headers()
            .get_all(WWW_AUTHENTICATE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();
        assert!(challenges.iter().any(|v| v.contains("stale=true")));

        let mut res = Response::default();
        www_authenticate(&mut res, &args, false).unwrap();
        let challenges: Vec<&str> = res
            .headers()
            .get_all(WWW_AUTHENTICATE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();
        assert!(challenges.iter().all(|v| !v.contains("stale=true")));
    }
}
