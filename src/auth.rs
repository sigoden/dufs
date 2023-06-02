use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose, Engine as _};
use headers::HeaderValue;
use hyper::Method;
use indexmap::IndexMap;
use lazy_static::lazy_static;
use md5::Context;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use uuid::Uuid;

use crate::utils::unix_now;

const REALM: &str = "DUFS";
const DIGEST_AUTH_TIMEOUT: u32 = 86400;

lazy_static! {
    static ref NONCESTARTHASH: Context = {
        let mut h = Context::new();
        h.consume(Uuid::new_v4().as_bytes());
        h.consume(std::process::id().to_be_bytes());
        h
    };
}

#[derive(Debug, Default)]
pub struct AccessControl {
    users: IndexMap<String, (String, AccessPaths)>,
    anony: Option<AccessPaths>,
}

impl AccessControl {
    pub fn new(raw_rules: &[&str]) -> Result<Self> {
        if raw_rules.is_empty() {
            return Ok(AccessControl {
                anony: Some(AccessPaths::new(AccessPerm::ReadWrite)),
                users: IndexMap::new(),
            });
        }

        let create_err = |v: &str| anyhow!("Invalid auth `{v}`");
        let mut anony = None;
        let mut anony_paths = vec![];
        let mut users = IndexMap::new();
        for rule in raw_rules {
            let (user, list) = rule.split_once('@').ok_or_else(|| create_err(rule))?;
            if user.is_empty() && anony.is_some() {
                bail!("Invalid auth, duplicate anonymous rules");
            }
            let mut paths = AccessPaths::default();
            for value in list.trim_matches(',').split(',') {
                let (path, perm) = match value.split_once(':') {
                    None => (value, AccessPerm::ReadOnly),
                    Some((path, "rw")) => (path, AccessPerm::ReadWrite),
                    _ => return Err(create_err(rule)),
                };
                if user.is_empty() {
                    anony_paths.push((path, perm));
                }
                paths.add(path, perm);
            }
            if user.is_empty() {
                anony = Some(paths);
            } else if let Some((user, pass)) = user.split_once(':') {
                if user.is_empty() || pass.is_empty() {
                    return Err(create_err(rule));
                }
                users.insert(user.to_string(), (pass.to_string(), paths));
            } else {
                return Err(create_err(rule));
            }
        }
        for (path, perm) in anony_paths {
            for (_, (_, paths)) in users.iter_mut() {
                paths.add(path, perm)
            }
        }
        Ok(Self { users, anony })
    }

    pub fn valid(&self) -> bool {
        !self.users.is_empty() || self.anony.is_some()
    }

    pub fn guard(
        &self,
        path: &str,
        method: &Method,
        authorization: Option<&HeaderValue>,
        auth_method: AuthMethod,
    ) -> (Option<String>, Option<AccessPaths>) {
        if let Some(authorization) = authorization {
            if let Some(user) = auth_method.get_user(authorization) {
                if let Some((pass, paths)) = self.users.get(&user) {
                    if method == Method::OPTIONS {
                        return (Some(user), Some(AccessPaths::new(AccessPerm::ReadOnly)));
                    }
                    if auth_method
                        .check(authorization, method.as_str(), &user, pass)
                        .is_some()
                    {
                        return (Some(user), paths.find(path, !is_readonly_method(method)));
                    } else {
                        return (None, None);
                    }
                }
            }
        }

        if method == Method::OPTIONS {
            return (None, Some(AccessPaths::new(AccessPerm::ReadOnly)));
        }

        if let Some(paths) = self.anony.as_ref() {
            return (None, paths.find(path, !is_readonly_method(method)));
        }

        (None, None)
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

    fn set_perm(&mut self, perm: AccessPerm) {
        if self.perm < perm {
            self.perm = perm
        }
    }

    pub fn add(&mut self, path: &str, perm: AccessPerm) {
        let path = path.trim_matches('/');
        if path.is_empty() {
            self.set_perm(perm);
        } else {
            let parts: Vec<&str> = path.split('/').collect();
            self.add_impl(&parts, perm);
        }
    }

    fn add_impl(&mut self, parts: &[&str], perm: AccessPerm) {
        let parts_len = parts.len();
        if parts_len == 0 {
            self.set_perm(perm);
            return;
        }
        let child = self.children.entry(parts[0].to_string()).or_default();
        child.add_impl(&parts[1..], perm)
    }

    pub fn find(&self, path: &str, writable: bool) -> Option<AccessPaths> {
        let parts: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|v| !v.is_empty())
            .collect();
        let target = self.find_impl(&parts, self.perm)?;
        if writable && !target.perm().readwrite() {
            return None;
        }
        Some(target)
    }

    fn find_impl(&self, parts: &[&str], perm: AccessPerm) -> Option<AccessPaths> {
        let perm = self.perm.max(perm);
        if parts.is_empty() {
            if perm.indexonly() {
                return Some(self.clone());
            } else {
                return Some(AccessPaths::new(perm));
            }
        }
        let child = match self.children.get(parts[0]) {
            Some(v) => v,
            None => {
                if perm.indexonly() {
                    return None;
                } else {
                    return Some(AccessPaths::new(perm));
                }
            }
        };
        child.find_impl(&parts[1..], perm)
    }

    pub fn child_paths(&self) -> Vec<&String> {
        self.children.keys().collect()
    }

    pub fn leaf_paths(&self, base: &Path) -> Vec<PathBuf> {
        if !self.perm().indexonly() {
            return vec![base.to_path_buf()];
        }
        let mut output = vec![];
        self.leaf_paths_impl(&mut output, base);
        output
    }

    fn leaf_paths_impl(&self, output: &mut Vec<PathBuf>, base: &Path) {
        for (name, child) in self.children.iter() {
            let base = base.join(name);
            if child.perm().indexonly() {
                child.leaf_paths_impl(output, &base);
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
    pub fn readwrite(&self) -> bool {
        self == &AccessPerm::ReadWrite
    }

    pub fn indexonly(&self) -> bool {
        self == &AccessPerm::IndexOnly
    }
}

fn is_readonly_method(method: &Method) -> bool {
    method == Method::GET
        || method == Method::OPTIONS
        || method == Method::HEAD
        || method.as_str() == "PROPFIND"
}

#[derive(Debug, Clone)]
pub enum AuthMethod {
    Basic,
    Digest,
}

impl AuthMethod {
    pub fn www_auth(&self, stale: bool) -> Result<String> {
        match self {
            AuthMethod::Basic => Ok(format!("Basic realm=\"{REALM}\"")),
            AuthMethod::Digest => {
                let str_stale = if stale { "stale=true," } else { "" };
                Ok(format!(
                    "Digest realm=\"{}\",nonce=\"{}\",{}qop=\"auth\"",
                    REALM,
                    create_nonce()?,
                    str_stale
                ))
            }
        }
    }

    pub fn get_user(&self, authorization: &HeaderValue) -> Option<String> {
        match self {
            AuthMethod::Basic => {
                let value: Vec<u8> = general_purpose::STANDARD
                    .decode(strip_prefix(authorization.as_bytes(), b"Basic ")?)
                    .ok()?;
                let parts: Vec<&str> = std::str::from_utf8(&value).ok()?.split(':').collect();
                Some(parts[0].to_string())
            }
            AuthMethod::Digest => {
                let digest_value = strip_prefix(authorization.as_bytes(), b"Digest ")?;
                let digest_map = to_headermap(digest_value).ok()?;
                digest_map
                    .get(b"username".as_ref())
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .map(|v| v.to_string())
            }
        }
    }

    fn check(
        &self,
        authorization: &HeaderValue,
        method: &str,
        auth_user: &str,
        auth_pass: &str,
    ) -> Option<()> {
        match self {
            AuthMethod::Basic => {
                let basic_value: Vec<u8> = general_purpose::STANDARD
                    .decode(strip_prefix(authorization.as_bytes(), b"Basic ")?)
                    .ok()?;
                let parts: Vec<&str> = std::str::from_utf8(&basic_value).ok()?.split(':').collect();

                if parts[0] != auth_user {
                    return None;
                }

                if parts[1] == auth_pass {
                    return Some(());
                }

                None
            }
            AuthMethod::Digest => {
                let digest_value = strip_prefix(authorization.as_bytes(), b"Digest ")?;
                let digest_map = to_headermap(digest_value).ok()?;
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
                    h.consume(format!("{}:{}:{}", auth_user, REALM, auth_pass).as_bytes());
                    let auth_pass = format!("{:x}", h.compute());

                    let mut ha = Context::new();
                    ha.consume(method);
                    ha.consume(b":");
                    if let Some(uri) = digest_map.get(b"uri".as_ref()) {
                        ha.consume(uri);
                    }
                    let ha = format!("{:x}", ha.compute());
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
                                format!("{:x}", c.compute())
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
                            format!("{:x}", c.compute())
                        }
                    };
                    if correct_response.as_bytes() == *user_response {
                        return Some(());
                    }
                }
                None
            }
        }
    }
}

/// Check if a nonce is still valid.
/// Return an error if it was never valid
fn validate_nonce(nonce: &[u8]) -> Result<bool> {
    if nonce.len() != 34 {
        bail!("invalid nonce");
    }
    //parse hex
    if let Ok(n) = std::str::from_utf8(nonce) {
        //get time
        if let Ok(secs_nonce) = u32::from_str_radix(&n[..8], 16) {
            //check time
            let now = unix_now()?;
            let secs_now = now.as_secs() as u32;

            if let Some(dur) = secs_now.checked_sub(secs_nonce) {
                //check hash
                let mut h = NONCESTARTHASH.clone();
                h.consume(secs_nonce.to_be_bytes());
                let h = format!("{:x}", h.compute());
                if h[..26] == n[8..34] {
                    return Ok(dur < DIGEST_AUTH_TIMEOUT);
                }
            }
        }
    }
    bail!("invalid nonce");
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
        while header[i] == b' ' {
            i += 1;
        }
        if a <= i || k <= 1 + a {
            //keys and values must contain one char
            return Err(());
        }
        let key = &header[i..a];
        let val = if header[1 + a] == b'"' && header[k - 1] == b'"' {
            //escaped
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
    let now = unix_now()?;
    let secs = now.as_secs() as u32;
    let mut h = NONCESTARTHASH.clone();
    h.consume(secs.to_be_bytes());

    let n = format!("{:08x}{:032x}", secs, h.compute());
    Ok(n[..34].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_paths() {
        let mut paths = AccessPaths::default();
        paths.add("/dir1", AccessPerm::ReadWrite);
        paths.add("/dir2/dir1", AccessPerm::ReadWrite);
        paths.add("/dir2/dir2", AccessPerm::ReadOnly);
        paths.add("/dir2/dir3/dir1", AccessPerm::ReadWrite);
        assert_eq!(
            paths.leaf_paths(Path::new("/tmp")),
            [
                "/tmp/dir1",
                "/tmp/dir2/dir1",
                "/tmp/dir2/dir2",
                "/tmp/dir2/dir3/dir1"
            ]
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>()
        );
        assert_eq!(
            paths
                .find("dir2", false)
                .map(|v| v.leaf_paths(Path::new("/tmp/dir2"))),
            Some(
                ["/tmp/dir2/dir1", "/tmp/dir2/dir2", "/tmp/dir2/dir3/dir1"]
                    .iter()
                    .map(PathBuf::from)
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(paths.find("dir2", true), None);
        assert!(paths.find("dir1/file", true).is_some());
    }

    #[test]
    fn test_access_paths_perm() {
        let mut paths = AccessPaths::default();
        assert_eq!(paths.perm(), AccessPerm::IndexOnly);
        paths.set_perm(AccessPerm::ReadOnly);
        assert_eq!(paths.perm(), AccessPerm::ReadOnly);
        paths.set_perm(AccessPerm::ReadWrite);
        assert_eq!(paths.perm(), AccessPerm::ReadWrite);
        paths.set_perm(AccessPerm::ReadOnly);
        assert_eq!(paths.perm(), AccessPerm::ReadWrite);
    }
}
