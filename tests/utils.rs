use indexmap::IndexSet;
use serde_json::Value;

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
        reqwest::blocking::Client::new().request(reqwest::Method::from_bytes($method)?, $url)
    };
}

#[allow(dead_code)]
pub fn retrieve_index_paths(content: &str) -> IndexSet<String> {
    let value = retrive_json(content).unwrap();
    let paths = value
        .get("paths")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|v| {
            let name = v.get("name")?.as_str()?;
            let path_type = v.get("path_type")?.as_str()?;
            if path_type.ends_with("Dir") {
                Some(format!("{name}/"))
            } else {
                Some(name.to_owned())
            }
        })
        .collect();
    paths
}

#[allow(dead_code)]
pub fn retrive_edit_file(content: &str) -> Option<bool> {
    let value = retrive_json(content)?;
    let value = value.get("editable").unwrap();
    Some(value.as_bool().unwrap())
}

#[allow(dead_code)]
pub fn encode_uri(v: &str) -> String {
    let parts: Vec<_> = v.split('/').map(urlencoding::encode).collect();
    parts.join("/")
}

#[allow(dead_code)]
pub fn retrive_json(content: &str) -> Option<Value> {
    let lines: Vec<&str> = content.lines().collect();
    let line = lines.iter().find(|v| v.contains("DATA ="))?;
    let line_col = line.find("DATA =").unwrap() + 6;
    let value: Value = line[line_col..].parse().unwrap();
    Some(value)
}
