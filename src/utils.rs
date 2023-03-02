use anyhow::{anyhow, Result};
use std::{
    borrow::Cow,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn unix_now() -> Result<Duration> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| anyhow!("Invalid system time, {err}"))
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
