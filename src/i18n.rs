//! Internationalization.
//!
//! Translations are stored in `assets/i18n/<lang>.json` and are shared by the
//! server rendered pages and the web ui. To add a language, drop a json file
//! in that folder and list it in [`LANGS`].

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The language used when the request expresses no preference.
pub const DEFAULT_LANG: &str = "en";

/// The language used for the keys missing from a translation.
pub const FALLBACK_LANG: &str = "en";

/// The supported languages.
pub const LANGS: &[&str] = &[FALLBACK_LANG, "zh-CN"];

/// The cookie that stores the language chosen by the user.
pub const COOKIE_NAME: &str = "dufs_lang";

const EN_JSON: &str = include_str!("../assets/i18n/en.json");
const ZH_CN_JSON: &str = include_str!("../assets/i18n/zh-CN.json");

/// A translation dictionary.
pub type Dict = HashMap<String, String>;

/// The translations sent to the web ui.
#[derive(Debug, Serialize)]
pub struct I18nData {
    /// The language of the page.
    pub lang: &'static str,
    /// The supported languages.
    pub langs: Vec<&'static str>,
    /// The dictionaries of every supported language.
    pub dicts: &'static HashMap<String, Dict>,
}

fn dicts() -> &'static HashMap<String, Dict> {
    static DICTS: OnceLock<HashMap<String, Dict>> = OnceLock::new();
    DICTS.get_or_init(|| {
        let mut dicts = HashMap::new();
        for (lang, content) in [(FALLBACK_LANG, EN_JSON), ("zh-CN", ZH_CN_JSON)] {
            let value: Value = serde_json::from_str(content).expect("invalid translation file");
            let dict = value
                .as_object()
                .expect("translation file must be a json object")
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                .collect();
            dicts.insert(lang.to_string(), dict);
        }
        dicts
    })
}

/// The translations sent to the web ui.
pub fn data(lang: &str) -> I18nData {
    I18nData {
        lang: normalize(lang).unwrap_or(DEFAULT_LANG),
        langs: LANGS.to_vec(),
        dicts: dicts(),
    }
}

/// Normalize a language tag into one of the supported languages.
///
/// `zh-cn` and `zh_CN` are accepted, a bare `zh` resolves to `zh-CN`.
pub fn normalize(lang: &str) -> Option<&'static str> {
    let lang = lang.trim();
    if lang.is_empty() {
        return None;
    }
    let lang = lang.to_ascii_lowercase().replace('_', "-");
    for supported in LANGS {
        if supported.eq_ignore_ascii_case(&lang) {
            return Some(supported);
        }
    }
    match lang.split('-').next().unwrap_or_default() {
        "en" => LANGS.iter().find(|v| **v == FALLBACK_LANG).copied(),
        "zh" => LANGS.iter().find(|v| **v == "zh-CN").copied(),
        _ => None,
    }
}

/// Resolve the language of a request.
///
/// The `lang` query parameter takes precedence over the `dufs_lang` cookie,
/// which takes precedence over the `Accept-Language` header. The
/// `default_lang` is used when the request expresses no preference, it may
/// also be `auto` to follow the `Accept-Language` header.
pub fn resolve(
    default_lang: &str,
    query_lang: Option<&str>,
    cookie_lang: Option<&str>,
    accept_language: Option<&str>,
) -> &'static str {
    if let Some(lang) = query_lang.and_then(normalize) {
        return lang;
    }
    if let Some(lang) = cookie_lang.and_then(normalize) {
        return lang;
    }
    if default_lang.eq_ignore_ascii_case("auto") {
        if let Some(lang) = accept_language.and_then(match_accept_language) {
            return lang;
        }
        return LANGS
            .iter()
            .find(|v| **v == DEFAULT_LANG)
            .copied()
            .unwrap_or(FALLBACK_LANG);
    }
    normalize(default_lang)
        .or_else(|| LANGS.iter().find(|v| **v == DEFAULT_LANG).copied())
        .unwrap_or(FALLBACK_LANG)
}

/// Extract the language stored in the `dufs_lang` cookie.
pub fn cookie_lang(raw: Option<&str>) -> Option<&str> {
    let raw = raw?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            if key.trim() == COOKIE_NAME {
                return Some(value.trim());
            }
        }
    }
    None
}

/// Pick the best supported language of an `Accept-Language` header.
fn match_accept_language(header: &str) -> Option<&'static str> {
    let mut candidates: Vec<(f32, usize, &str)> = vec![];
    for (idx, part) in header.split(',').enumerate() {
        let mut parts = part.split(';');
        let tag = parts.next().unwrap_or_default().trim();
        let mut quality = 1.0f32;
        for param in parts {
            let param = param.trim();
            if let Some(value) = param.strip_prefix("q=") {
                quality = value.parse().unwrap_or(1.0);
            }
        }
        if !tag.is_empty() && quality > 0.0 {
            candidates.push((quality, idx, tag));
        }
    }
    candidates.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    for (_, _, tag) in candidates {
        if let Some(lang) = normalize(tag) {
            return Some(lang);
        }
    }
    None
}

/// Translate a key.
pub fn t(lang: &str, key: &str) -> String {
    let dicts = dicts();
    dicts
        .get(lang)
        .and_then(|v| v.get(key))
        .or_else(|| dicts.get(FALLBACK_LANG).and_then(|v| v.get(key)))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize() {
        assert_eq!(normalize("en"), Some("en"));
        assert_eq!(normalize("en-US"), Some("en"));
        assert_eq!(normalize("zh"), Some("zh-CN"));
        assert_eq!(normalize("zh-CN"), Some("zh-CN"));
        assert_eq!(normalize("zh_cn"), Some("zh-CN"));
        assert_eq!(normalize("fr"), None);
        assert_eq!(normalize(""), None);
    }

    #[test]
    fn test_resolve() {
        assert_eq!(resolve("en", Some("zh-CN"), None, None), "zh-CN");
        assert_eq!(resolve("en", None, Some("zh-CN"), None), "zh-CN");
        assert_eq!(resolve("en", None, None, Some("zh-CN")), "en");
        assert_eq!(resolve("auto", None, None, Some("zh-CN,zh;q=0.9")), "zh-CN");
        assert_eq!(resolve("auto", None, None, Some("en-US,en;q=0.9")), "en");
    }

    #[test]
    fn test_cookie_lang() {
        assert_eq!(cookie_lang(Some("dufs_lang=zh-CN; path=/")), Some("zh-CN"));
        assert_eq!(cookie_lang(Some("other=1; dufs_lang=en")), Some("en"));
        assert_eq!(cookie_lang(Some("other=1")), None);
        assert_eq!(cookie_lang(None), None);
    }

    #[test]
    fn test_translate() {
        assert_eq!(t("zh-CN", "empty_folder"), "空文件夹");
        assert_eq!(t("en", "empty_folder"), "Empty folder");
        assert_eq!(t("zh-CN", "unknown_key"), "unknown_key");
    }
}
