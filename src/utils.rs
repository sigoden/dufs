use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
#[cfg(feature = "tls")]
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::{
    borrow::Cow,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn unix_now() -> Result<Duration> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .with_context(|| "Invalid system time")
}

pub fn encode_uri(v: &str) -> String {
    let parts: Vec<_> = v.split('/').map(urlencoding::encode).collect();
    parts.join("/")
}

pub fn decode_uri(v: &str) -> Option<Cow<str>> {
    percent_encoding::percent_decode(v.as_bytes())
        .decode_utf8()
        .ok()
}

pub fn get_file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
}

#[cfg(unix)]
pub async fn get_file_mtime_and_mode(path: &Path) -> Result<(DateTime<Utc>, u16)> {
    use std::os::unix::prelude::MetadataExt;
    let meta = tokio::fs::metadata(path).await?;
    let datetime: DateTime<Utc> = meta.modified()?.into();
    Ok((datetime, meta.mode() as u16))
}

#[cfg(not(unix))]
pub async fn get_file_mtime_and_mode(path: &Path) -> Result<(DateTime<Utc>, u16)> {
    let meta = tokio::fs::metadata(&path).await?;
    let datetime: DateTime<Utc> = meta.modified()?.into();
    Ok((datetime, 0o644))
}

pub fn try_get_file_name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| anyhow!("Failed to get file name of `{}`", path.display()))
}

pub fn glob(pattern: &str, target: &str) -> bool {
    let pat = match ::glob::Pattern::new(pattern) {
        Ok(pat) => pat,
        Err(_) => return false,
    };
    pat.matches(target)
}

// Load public certificate from file.
#[cfg(feature = "tls")]
pub fn load_certs<T: AsRef<Path>>(filename: T) -> Result<Vec<CertificateDer<'static>>> {
    // Open certificate file.
    let cert_file = std::fs::File::open(filename.as_ref())
        .with_context(|| format!("Failed to access `{}`", filename.as_ref().display()))?;
    let mut reader = std::io::BufReader::new(cert_file);

    // Load and return certificate.
    let mut certs = vec![];
    for cert in rustls_pemfile::certs(&mut reader) {
        let cert = cert.with_context(|| "Failed to load certificate")?;
        certs.push(cert)
    }
    if certs.is_empty() {
        anyhow::bail!("No supported certificate in file");
    }
    Ok(certs)
}

// Load private key from file.
#[cfg(feature = "tls")]
pub fn load_private_key<T: AsRef<Path>>(filename: T) -> Result<PrivateKeyDer<'static>> {
    let key_file = std::fs::File::open(filename.as_ref())
        .with_context(|| format!("Failed to access `{}`", filename.as_ref().display()))?;
    let mut reader = std::io::BufReader::new(key_file);

    // Load and return a single private key.
    for key in rustls_pemfile::read_all(&mut reader) {
        let key = key.with_context(|| "There was a problem with reading private key")?;
        match key {
            rustls_pemfile::Item::Pkcs1Key(key) => return Ok(PrivateKeyDer::Pkcs1(key)),
            rustls_pemfile::Item::Pkcs8Key(key) => return Ok(PrivateKeyDer::Pkcs8(key)),
            rustls_pemfile::Item::Sec1Key(key) => return Ok(PrivateKeyDer::Sec1(key)),
            _ => {}
        }
    }
    anyhow::bail!("No supported private key in file");
}

#[test]
fn test_glob_key() {
    assert!(glob("", ""));
    assert!(glob(".*", ".git"));
    assert!(glob("abc", "abc"));
    assert!(glob("a*c", "abc"));
    assert!(glob("a?c", "abc"));
    assert!(glob("a*c", "abbc"));
    assert!(glob("*c", "abc"));
    assert!(glob("a*", "abc"));
    assert!(glob("?c", "bc"));
    assert!(glob("a?", "ab"));
    assert!(!glob("abc", "adc"));
    assert!(!glob("abc", "abcd"));
    assert!(!glob("a?c", "abbc"));
    assert!(!glob("*.log", "log"));
    assert!(glob("*.abc-cba", "xyz.abc-cba"));
    assert!(glob("*.abc-cba", "123.xyz.abc-cba"));
    assert!(glob("*.log", ".log"));
    assert!(glob("*.log", "a.log"));
    assert!(glob("*/", "abc/"));
    assert!(!glob("*/", "abc"));
}
