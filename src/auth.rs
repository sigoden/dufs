use headers::HeaderValue;
use hyper::Method;
use lazy_static::lazy_static;
use md5::Context;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

use crate::utils::encode_uri;
use crate::BoxResult;

const REALM: &str = "DUFS";

lazy_static! {
    static ref NONCESTARTHASH: Context = {
        let mut h = Context::new();
        h.consume(Uuid::new_v4().as_bytes());
        h.consume(std::process::id().to_be_bytes());
        h
    };
}

#[derive(Debug, Clone)]
pub struct AccessControl {
    rules: HashMap<String, PathControl>,
}

#[derive(Debug, Clone)]
pub struct PathControl {
    readwrite: Account,
    readonly: Option<Account>,
    share: bool,
}

impl AccessControl {
    pub fn new(raw_rules: &[&str], uri_prefix: &str) -> BoxResult<Self> {
        let mut rules = HashMap::default();
        if raw_rules.is_empty() {
            return Ok(Self { rules });
        }
        for rule in raw_rules {
            let parts: Vec<&str> = rule.split('@').collect();
            let create_err = || format!("Invalid auth `{}`", rule).into();
            match parts.as_slice() {
                [path, readwrite] => {
                    let control = PathControl {
                        readwrite: Account::new(readwrite).ok_or_else(create_err)?,
                        readonly: None,
                        share: false,
                    };
                    rules.insert(sanitize_path(path, uri_prefix), control);
                }
                [path, readwrite, readonly] => {
                    let (readonly, share) = if *readonly == "*" {
                        (None, true)
                    } else {
                        (Some(Account::new(readonly).ok_or_else(create_err)?), false)
                    };
                    let control = PathControl {
                        readwrite: Account::new(readwrite).ok_or_else(create_err)?,
                        readonly,
                        share,
                    };
                    rules.insert(sanitize_path(path, uri_prefix), control);
                }
                _ => return Err(create_err()),
            }
        }
        Ok(Self { rules })
    }

    pub fn guard(
        &self,
        path: &str,
        method: &Method,
        authorization: Option<&HeaderValue>,
        basic_auth: bool,
    ) -> GuardType {
        if self.rules.is_empty() {
            return GuardType::ReadWrite;
        }
        let mut controls = vec![];
        for path in walk_path(path) {
            if let Some(control) = self.rules.get(path) {
                controls.push(control);
                if let Some(authorization) = authorization {
                    let Account { user, pass } = &control.readwrite;
                    if basic_auth {
                        if valid_basic_auth(authorization, user, pass).is_some() {
                            return GuardType::ReadWrite;
                        }
                    } else {
                        if valid_digest(authorization, method.as_str(), user, pass).is_some() {
                            return GuardType::ReadWrite;
                        }
                    }
                }
            }
        }
        if is_readonly_method(method) {
            for control in controls.into_iter() {
                if control.share {
                    return GuardType::ReadOnly;
                }
                if let Some(authorization) = authorization {
                    if let Some(Account { user, pass }) = &control.readonly {
                        if valid_digest(authorization, method.as_str(), user, pass).is_some() {
                            return GuardType::ReadOnly;
                        }
                    }
                }
            }
        }
        GuardType::Reject
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GuardType {
    Reject,
    ReadWrite,
    ReadOnly,
}

impl GuardType {
    pub fn is_reject(&self) -> bool {
        *self == GuardType::Reject
    }
}

fn sanitize_path(path: &str, uri_prefix: &str) -> String {
    encode_uri(&format!("{}{}", uri_prefix, path.trim_matches('/')))
}

fn walk_path(path: &str) -> impl Iterator<Item = &str> {
    let mut idx = 0;
    path.split('/').enumerate().map(move |(i, part)| {
        let end = if i == 0 { 1 } else { idx + part.len() + i };
        let value = &path[..end];
        idx += part.len();
        value
    })
}

fn is_readonly_method(method: &Method) -> bool {
    method == Method::GET
        || method == Method::OPTIONS
        || method == Method::HEAD
        || method.as_str() == "PROPFIND"
}

#[derive(Debug, Clone)]
struct Account {
    user: String,
    pass: String,
}

impl Account {
    fn new(data: &str) -> Option<Self> {
        let p: Vec<&str> = data.trim().split(':').collect();
        if p.len() != 2 {
            return None;
        }
        let user = p[0];
        let pass = p[1];
        let mut h = Context::new();
        h.consume(format!("{}:{}:{}", user, REALM, pass).as_bytes());
        Some(Account {
            user: user.to_owned(),
            pass: format!("{:x}", h.compute()),
        })
    }
}

pub fn generate_www_auth(stale: bool, basic_auth: bool) -> String {
    if basic_auth {        
        format!("Basic realm=\"{}\"", REALM)
    } else {
        let str_stale = if stale { "stale=true," } else { "" };
        format!(
            "Digest realm=\"{}\",nonce=\"{}\",{}qop=\"auth\"",
            REALM,
            create_nonce(),
            str_stale
        )
    }
}

pub fn valid_basic_auth(
    authorization: &HeaderValue,
    auth_user: &str,
    auth_pass: &str,
) -> Option<()> {
    let value: Vec<u8> = base64::decode(strip_prefix(authorization.as_bytes(), b"Basic ").unwrap()).unwrap();
    let parts: Vec<&str> = std::str::from_utf8(&value).unwrap().split(":").collect();

    if parts[0] != auth_user {
        return None;
    }

    let mut h = Context::new();
    h.consume(format!("{}:{}:{}", parts[0], REALM, parts[1]).as_bytes());

    let http_pass = format!("{:x}", h.compute());

    if http_pass == auth_pass {
        return Some(());
    }

    return None;
}

pub fn valid_digest(
    authorization: &HeaderValue,
    method: &str,
    auth_user: &str,
    auth_pass: &str,
) -> Option<()> {
    let digest_value = strip_prefix(authorization.as_bytes(), b"Digest ")?;
    let user_vals = to_headermap(digest_value).ok()?;
    if let (Some(username), Some(nonce), Some(user_response)) = (
        user_vals
            .get(b"username".as_ref())
            .and_then(|b| std::str::from_utf8(*b).ok()),
        user_vals.get(b"nonce".as_ref()),
        user_vals.get(b"response".as_ref()),
    ) {
        match validate_nonce(nonce) {
            Ok(true) => {}
            _ => return None,
        }
        if auth_user != username {
            return None;
        }
        let mut ha = Context::new();
        ha.consume(method);
        ha.consume(b":");
        if let Some(uri) = user_vals.get(b"uri".as_ref()) {
            ha.consume(uri);
        }
        let ha = format!("{:x}", ha.compute());
        let mut correct_response = None;
        if let Some(qop) = user_vals.get(b"qop".as_ref()) {
            if qop == &b"auth".as_ref() || qop == &b"auth-int".as_ref() {
                correct_response = Some({
                    let mut c = Context::new();
                    c.consume(&auth_pass);
                    c.consume(b":");
                    c.consume(nonce);
                    c.consume(b":");
                    if let Some(nc) = user_vals.get(b"nc".as_ref()) {
                        c.consume(nc);
                    }
                    c.consume(b":");
                    if let Some(cnonce) = user_vals.get(b"cnonce".as_ref()) {
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
            // grant access
            return Some(());
        }
    }
    None
}

/// Check if a nonce is still valid.
/// Return an error if it was never valid
fn validate_nonce(nonce: &[u8]) -> Result<bool, ()> {
    if nonce.len() != 34 {
        return Err(());
    }
    //parse hex
    if let Ok(n) = std::str::from_utf8(nonce) {
        //get time
        if let Ok(secs_nonce) = u32::from_str_radix(&n[..8], 16) {
            //check time
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let secs_now = now.as_secs() as u32;

            if let Some(dur) = secs_now.checked_sub(secs_nonce) {
                //check hash
                let mut h = NONCESTARTHASH.clone();
                h.consume(secs_nonce.to_be_bytes());
                let h = format!("{:x}", h.compute());
                if h[..26] == n[8..34] {
                    return Ok(dur < 300); // from the last 5min
                                          //Authentication-Info ?
                }
            }
        }
    }
    Err(())
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
    let mut asign = Vec::new();
    let mut i: usize = 0;
    let mut esc = false;
    for c in header {
        match (c, esc) {
            (b'=', false) => asign.push(i),
            (b',', false) => sep.push(i),
            (b'"', false) => esc = true,
            (b'"', true) => esc = false,
            _ => {}
        }
        i += 1;
    }
    sep.push(i); // same len for both Vecs

    i = 0;
    let mut ret = HashMap::new();
    for (&k, &a) in sep.iter().zip(asign.iter()) {
        while header[i] == b' ' {
            i += 1;
        }
        if a <= i || k <= 1 + a {
            //keys and vals must contain one char
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

fn create_nonce() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let secs = now.as_secs() as u32;
    let mut h = NONCESTARTHASH.clone();
    h.consume(secs.to_be_bytes());

    let n = format!("{:08x}{:032x}", secs, h.compute());
    n[..34].to_string()
}
