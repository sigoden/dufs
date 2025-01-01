use crate::{args::Args, server::Response, utils::unix_now};

use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use headers::HeaderValue;
use hyper::{header::WWW_AUTHENTICATE, Method};
use indexmap::IndexMap;
use lazy_static::lazy_static;
use md5::Context;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const REALM: &str = "DUFS";
const DIGEST_AUTH_TIMEOUT: u32 = 604800; // 7 days

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
    use_hashed_password: bool,
    users: IndexMap<String, (String, AccessPaths)>,
    anonymous: Option<AccessPaths>,
}

impl Default for AccessControl {
    fn default() -> Self {
        AccessControl {
            use_hashed_password: false,
            users: IndexMap::new(),
            anonymous: Some(AccessPaths::new(AccessPerm::ReadWrite)),
        }
    }
}

impl AccessControl {
    pub fn new(raw_rules: &[&str]) -> Result<Self> {
        if raw_rules.is_empty() {
            return Ok(Default::default());
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
            if let Some(paths) = annoy_paths {
                access_paths.merge(paths);
            }
            if pass.starts_with("$6$") {
                use_hashed_password = true;
            }
            users.insert(user.to_string(), (pass.to_string(), access_paths));
        }

        Ok(Self {
            use_hashed_password,
            users,
            anonymous,
        })
    }

    pub fn exist(&self) -> bool {
        !self.users.is_empty()
    }

    pub fn guard(
        &self,
        path: &str,
        method: &Method,
        authorization: Option<&HeaderValue>,
        guard_options: bool,
    ) -> (Option<String>, Option<AccessPaths>) {
        if self.users.is_empty() {
            return (None, Some(AccessPaths::new(AccessPerm::ReadWrite)));
        }
        if let Some(authorization) = authorization {
            if let Some(user) = get_auth_user(authorization) {
                if let Some((pass, ap)) = self.users.get(&user) {
                    if method == Method::OPTIONS {
                        return (Some(user), Some(AccessPaths::new(AccessPerm::ReadOnly)));
                    }
                    if check_auth(authorization, method.as_str(), &user, pass).is_some() {
                        return (Some(user), ap.guard(path, method));
                    }
                }
            }

            return (None, None);
        }

        if !guard_options && method == Method::OPTIONS {
            return (None, Some(AccessPaths::new(AccessPerm::ReadOnly)));
        }

        if let Some(ap) = self.anonymous.as_ref() {
            return (None, ap.guard(path, method));
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

    pub fn set_perm(&mut self, perm: AccessPerm) {
        if self.perm < perm {
            self.perm = perm;
            self.recursively_purge_children(perm);
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

    pub fn guard(&self, path: &str, method: &Method) -> Option<Self> {
        let target = self.find(path)?;
        if !is_readonly_method(method) && !target.perm().readwrite() {
            return None;
        }
        Some(target)
    }

    fn recursively_purge_children(&mut self, perm: AccessPerm) {
        self.children.retain(|_, child| {
            if child.perm <= perm {
                false
            } else {
                child.recursively_purge_children(perm);
                true
            }
        });
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
        let parts_len = parts.len();
        if parts_len == 0 {
            self.set_perm(perm);
            return;
        }
        if self.perm >= perm {
            return;
        }
        let child = self.children.entry(parts[0].to_string()).or_default();
        child.add_impl(&parts[1..], perm)
    }

    pub fn find(&self, path: &str) -> Option<AccessPaths> {
        let parts: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|v| !v.is_empty())
            .collect();
        self.find_impl(&parts, self.perm)
    }

    fn find_impl(&self, parts: &[&str], perm: AccessPerm) -> Option<AccessPaths> {
        let perm = if !self.perm.indexonly() {
            self.perm
        } else {
            perm
        };
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
        let basic = HeaderValue::from_str(&format!("Basic realm=\"{}\"", REALM))?;
        res.headers_mut().insert(WWW_AUTHENTICATE, basic);
    } else {
        let nonce = create_nonce()?;
        let digest = HeaderValue::from_str(&format!(
            "Digest realm=\"{}\", nonce=\"{}\", qop=\"auth\"",
            REALM, nonce
        ))?;
        let basic = HeaderValue::from_str(&format!("Basic realm=\"{}\"", REALM))?;
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
            if let Ok(()) = sha_crypt::sha512_check(pass, auth_pass) {
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
    } else {
        None
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

    #[test]
    fn test_access_paths() {
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
                .find("dir2")
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
            paths.find("dir1/file"),
            Some(AccessPaths::new(AccessPerm::ReadWrite))
        );
        assert_eq!(
            paths.find("dir2/dir21/file"),
            Some(AccessPaths::new(AccessPerm::ReadWrite))
        );
        assert_eq!(
            paths.find("dir2/dir21/dir211/file"),
            Some(AccessPaths::new(AccessPerm::ReadWrite))
        );
        assert_eq!(
            paths.find("dir2/dir22/file"),
            Some(AccessPaths::new(AccessPerm::ReadOnly))
        );
        assert_eq!(
            paths.find("dir2/dir22/dir221/file"),
            Some(AccessPaths::new(AccessPerm::ReadWrite))
        );
        assert_eq!(paths.find("dir2/dir23/file"), None);
        assert_eq!(
            paths.find("dir2/dir23//dir231/file"),
            Some(AccessPaths::new(AccessPerm::ReadWrite))
        );
    }
}
