use serde_json::Value;
use std::collections::HashSet;

#[macro_export]
macro_rules! assert_resp_paths {
    ($resp:ident) => {
        assert_resp_paths!($resp, self::fixtures::FILES)
    };
    ($resp:ident, $files:expr) => {
        assert_eq!($resp.status(), 200);
        let body = $resp.text()?;
        let paths = self::utils::retrieve_index_paths(&body);
        assert!(!paths.is_empty());
        for file in $files {
            assert!(paths.contains(&file.to_string()));
        }
    };
}

#[macro_export]
macro_rules! fetch {
    ($method:literal, $url:expr) => {
        reqwest::blocking::Client::new().request(hyper::Method::from_bytes($method)?, $url)
    };
}

#[allow(dead_code)]
pub fn retrieve_index_paths(index: &str) -> HashSet<String> {
    retrieve_index_paths_impl(index).unwrap_or_default()
}

#[allow(dead_code)]
pub fn encode_uri(v: &str) -> String {
    let parts: Vec<_> = v.split('/').map(urlencoding::encode).collect();
    parts.join("/")
}

fn retrieve_index_paths_impl(index: &str) -> Option<HashSet<String>> {
    let lines: Vec<&str> = index.lines().collect();
    let line = lines.iter().find(|v| v.contains("DATA ="))?;
    let value: Value = line[7..].parse().ok()?;
    let paths = value
        .get("paths")?
        .as_array()?
        .iter()
        .flat_map(|v| {
            let name = v.get("name")?.as_str()?;
            let path_type = v.get("path_type")?.as_str()?;
            if path_type.ends_with("Dir") {
                Some(format!("{}/", name))
            } else {
                Some(name.to_owned())
            }
        })
        .collect();
    Some(paths)
}
