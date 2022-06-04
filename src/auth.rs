use headers::HeaderValue;
use lazy_static::lazy_static;
use md5::Context;
use uuid::Uuid;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::BoxResult;

const REALM: &str = "DUF";

lazy_static! {
    static ref NONCESTARTHASH: Context = {
        let mut h = Context::new();
        h.consume(Uuid::new_v4().as_bytes());
        h.consume(std::process::id().to_be_bytes());
        h
    };
}

pub fn generate_www_auth(stale: bool) -> String {
    let str_stale = if stale { "stale=true," } else { "" };
    format!(
        "Digest realm=\"{}\",nonce=\"{}\",{}qop=\"auth\",algorithm=\"MD5\"",
        REALM,
        create_nonce(),
        str_stale
    )
}

pub fn parse_auth(auth: &str) -> BoxResult<(String, String)> {
    let p: Vec<&str> = auth.trim().split(':').collect();
    let err = "Invalid auth value";
    if p.len() != 2 {
        return Err(err.into());
    }
    let user = p[0];
    let pass = p[1];
    let mut h = Context::new();
    h.consume(format!("{}:{}:{}", user, REALM, pass).as_bytes());
    Ok((user.to_owned(), format!("{:x}", h.compute())))
}

pub fn valid_digest(
    header_value: &HeaderValue,
    method: &str,
    auth_user: &str,
    auth_pass: &str,
) -> Option<()> {
    let digest_value = strip_prefix(header_value.as_bytes(), b"Digest ")?;
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
