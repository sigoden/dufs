use crate::BoxResult;
use std::{borrow::Cow, path::Path};

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

pub fn try_get_file_name(path: &Path) -> BoxResult<&str> {
    path.file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| format!("Failed to get file name of `{}`", path.display()).into())
}

pub fn glob(source: &str, target: &str) -> bool {
    let ss: Vec<char> = source.chars().collect();
    let mut iter = target.chars();
    let mut i = 0;
    'outer: while i < ss.len() {
        let s = ss[i];
        match s {
            '*' => match ss.get(i + 1) {
                Some(s_next) => {
                    for t in iter.by_ref() {
                        if t == *s_next {
                            i += 2;
                            continue 'outer;
                        }
                    }
                    return false;
                }
                None => return true,
            },
            '?' => match iter.next() {
                Some(_) => {
                    i += 1;
                    continue;
                }
                None => return false,
            },
            _ => match iter.next() {
                Some(t) => {
                    if s == t {
                        i += 1;
                        continue;
                    }
                    return false;
                }
                None => return false,
            },
        }
    }
    iter.next().is_none()
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
    assert!(glob("*.log", ".log"));
    assert!(glob("*.log", "a.log"));
}
