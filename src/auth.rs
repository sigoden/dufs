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
};
use uuid::Uuid;

const REALM: &str = "DUFS";
const DIGEST_AUTH_TIMEOUT: u32 = 60 * 60 * 24 * 7; // 7 days
const TOKEN_EXPIRATION: u64 = 1000 * 60 * 60 * 24 * 3; // 3 days

lazy_static! {
    static ref NONCESTARTHASH: Context = {
        let mut h = Context::new();
        h.consume(Uuid::new_v4().as_bytes());
        h.consume(std::process::id().to_be_bytes());
        h
    };
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

    /// Returns the guarded `(user, access_paths, via_token)`. `via_token` is
    /// true only when the identity was established by a share token rather than
    /// primary credentials or anonymous access. `root` is the served directory,
    /// used to resolve filesystem aliases of configured names.
    pub async fn guard(
        &self,
        path: &str,
        method: &Method,
        authorization: Option<&HeaderValue>,
        token: Option<&String>,
        guard_options: bool,
        root: &Path,
    ) -> (Option<String>, Option<AccessPaths>, bool) {
        if self.empty {
            return (None, Some(AccessPaths::new(AccessPerm::ReadWrite)), false);
        }

        if method == Method::GET {
            if let Some(token) = token {
                if let Ok((user, ap)) = self.verify_token(token, path) {
                    return (Some(user), ap.guard(path, method, Some(root)).await, true);
                }
            }
        }

        if let Some(authorization) = authorization {
            if let Some(user) = get_auth_user(authorization) {
                if let Some((pass, ap)) = self.users.get(&user) {
                    if check_auth(authorization, method.as_str(), &user, pass).is_some() {
                        if method == Method::OPTIONS {
                            return (
                                Some(user),
                                Some(AccessPaths::new(AccessPerm::ReadOnly)),
                                false,
                            );
                        }
                        return (Some(user), ap.guard(path, method, Some(root)).await, false);
                    }
                }
            }

            return (None, None, false);
        }

        if !guard_options && method == Method::OPTIONS {
            return (None, Some(AccessPaths::new(AccessPerm::ReadOnly)), false);
        }

        if let Some(ap) = self.anonymous.as_ref() {
            return (None, ap.guard(path, method, Some(root)).await, false);
        }

        (None, None, false)
    }

    pub fn generate_token(&self, path: &str, user: &str) -> Result<String> {
        let (pass, _) = self
            .users
            .get(user)
            .ok_or_else(|| anyhow!("Not found user '{user}'"))?;
        let exp = unix_now().as_millis() as u64 + TOKEN_EXPIRATION;
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
            self.add(path, perm);
        }
        Some(())
    }

    pub async fn guard(&self, path: &str, method: &Method, root: Option<&Path>) -> Option<Self> {
        let target = self.find(path, root).await?;
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
    /// `root` is the on-disk directory the relative `path` resolves against. It
    /// is used to recognize filesystem aliases of configured names: the backing
    /// filesystem may resolve several different spellings (case variants, NTFS
    /// 8.3 short names, NTFS data streams, ...) to the same object. Matching such
    /// a request against the canonical rule instead of falling back to the parent
    /// permission keeps a restrictive child rule enforceable. Pass `None` for
    /// pure string matching.
    pub async fn find(&self, path: &str, root: Option<&Path>) -> Option<AccessPaths> {
        let parts: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|v| !v.is_empty())
            .collect();
        let mut perm = self.perm;
        let mut paths = self;
        for (i, name) in parts.iter().enumerate() {
            let name = *name;
            if !paths.perm.indexonly() {
                perm = paths.perm;
            }
            let child = match paths.children.get(name) {
                Some(child) => child,
                None => match paths.find_alias(name, &parts[..i], root).await {
                    Some(child) => child,
                    None => {
                        return if perm.indexonly() {
                            None
                        } else {
                            Some(AccessPaths::new(perm))
                        }
                    }
                },
            };
            paths = child;
        }
        if !paths.perm.indexonly() {
            perm = paths.perm;
        }
        if perm.indexonly() {
            Some(paths.clone())
        } else {
            Some(AccessPaths::new(perm))
        }
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

pub fn www_authenticate(res: &mut Response, args: &Args) -> Result<()> {
    if args.auth.use_hashed_password {
        let basic = HeaderValue::from_str(&format!("Basic realm=\"{REALM}\""))?;
        res.headers_mut().insert(WWW_AUTHENTICATE, basic);
    } else {
        let nonce = create_nonce()?;
        let digest = HeaderValue::from_str(&format!(
            "Digest realm=\"{REALM}\", nonce=\"{nonce}\", qop=\"auth\""
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

pub fn check_auth(
    authorization: &HeaderValue,
    method: &str,
    auth_user: &str,
    auth_pass: &str,
) -> Option<()> {
    if let Some(value) = strip_prefix(authorization.as_bytes(), b"Basic ") {
        let value: Vec<u8> = STANDARD.decode(value).ok()?;
        let (user, pass) = std::str::from_utf8(&value).ok()?.split_once(':')?;

        if user != auth_user {
            return None;
        }

        if auth_pass.starts_with("$6$") {
            if sha_crypt::ShaCrypt::SHA512
                .verify_password(pass.as_bytes(), auth_pass)
                .is_ok()
            {
                return Some(());
            }
        } else if pass == auth_pass {
            return Some(());
        }

        None
    } else if let Some(value) = strip_prefix(authorization.as_bytes(), b"Digest ") {
        let digest_map = to_headermap(value).ok()?;
        if let (Some(username), Some(nonce), Some(user_response)) = (
            digest_map
                .get(b"username".as_ref())
                .and_then(|b| std::str::from_utf8(b).ok()),
            digest_map.get(b"nonce".as_ref()),
            digest_map.get(b"response".as_ref()),
        ) {
            match validate_nonce(nonce) {
                Ok(true) => {}
                _ => return None,
            }
            if auth_user != username {
                return None;
            }

            let mut h = Context::new();
            h.consume(format!("{auth_user}:{REALM}:{auth_pass}").as_bytes());
            let auth_pass = format!("{:x}", h.finalize());

            let mut ha = Context::new();
            ha.consume(method);
            ha.consume(b":");
            if let Some(uri) = digest_map.get(b"uri".as_ref()) {
                ha.consume(uri);
            }
            let ha = format!("{:x}", ha.finalize());
            let mut correct_response = None;
            if let Some(qop) = digest_map.get(b"qop".as_ref()) {
                if qop == &b"auth".as_ref() || qop == &b"auth-int".as_ref() {
                    correct_response = Some({
                        let mut c = Context::new();
                        c.consume(&auth_pass);
                        c.consume(b":");
                        c.consume(nonce);
                        c.consume(b":");
                        if let Some(nc) = digest_map.get(b"nc".as_ref()) {
                            c.consume(nc);
                        }
                        c.consume(b":");
                        if let Some(cnonce) = digest_map.get(b"cnonce".as_ref()) {
                            c.consume(cnonce);
                        }
                        c.consume(b":");
                        c.consume(qop);
                        c.consume(b":");
                        c.consume(&*ha);
                        format!("{:x}", c.finalize())
                    });
                }
            }
            let correct_response = match correct_response {
                Some(r) => r,
                None => {
                    let mut c = Context::new();
                    c.consume(&auth_pass);
                    c.consume(b":");
                    c.consume(nonce);
                    c.consume(b":");
                    c.consume(&*ha);
                    format!("{:x}", c.finalize())
                }
            };
            if correct_response.as_bytes() == *user_response {
                return Some(());
            }
        }
        None
    } else {
        None
    }
}

fn derive_secret_key(user: &str, pass: &str) -> SigningKey {
    let mut hasher = Sha256::new();
    hasher.update(format!("{user}:{pass}").as_bytes());
    let hash = hasher.finalize();
    SigningKey::from_bytes(&hash.into())
}

/// Check if a nonce is still valid.
/// Return an error if it was never valid
fn validate_nonce(nonce: &[u8]) -> Result<bool> {
    if nonce.len() != 34 || !nonce.iter().all(u8::is_ascii_hexdigit) {
        bail!("invalid nonce");
    }
    //parse hex
    let n = std::str::from_utf8(nonce)?;
    //get time
    if let Ok(secs_nonce) = u32::from_str_radix(&n[..8], 16) {
        //check time
        let now = unix_now();
        let secs_now = now.as_secs() as u32;

        if let Some(dur) = secs_now.checked_sub(secs_nonce) {
            //check hash
            let mut h = NONCESTARTHASH.clone();
            h.consume(secs_nonce.to_be_bytes());
            let h = format!("{:x}", h.finalize());
            if h[..26] == n[8..34] {
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
    let now = unix_now();
    let secs = now.as_secs() as u32;
    let mut h = NONCESTARTHASH.clone();
    h.consume(secs.to_be_bytes());

    let n = format!("{:08x}{:032x}", secs, h.finalize());
    Ok(n[..34].to_string())
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
        nonce.extend_from_slice(&[b'A'; 25]);
        assert_eq!(nonce.len(), 34);
        assert!(validate_nonce(&nonce).is_err());

        assert!(validate_nonce(b"short").is_err());
        assert!(validate_nonce(&[0xff; 34]).is_err());

        // a freshly created nonce still validates
        let nonce = create_nonce().unwrap();
        assert!(matches!(validate_nonce(nonce.as_bytes()), Ok(true)));
    }
}
