mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;
use serde_json::Value;
use std::process::Command;
use utils::retrieve_json;

fn retrieve_i18n(content: &str) -> Value {
    retrieve_json(content).unwrap().get("i18n").unwrap().clone()
}

#[rstest]
fn default_lang(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 200);
    let i18n = retrieve_i18n(&resp.text()?);
    assert_eq!(i18n["lang"], "en");
    assert_eq!(i18n["langs"].as_array().unwrap().len(), 2);
    assert_eq!(i18n["dicts"]["en"]["empty_folder"], "Empty folder");
    assert_eq!(i18n["dicts"]["zh-CN"]["empty_folder"], "空文件夹");
    Ok(())
}

#[rstest]
fn lang_option(#[with(&["--lang", "zh-CN"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 200);
    let i18n = retrieve_i18n(&resp.text()?);
    assert_eq!(i18n["lang"], "zh-CN");
    Ok(())
}

#[rstest]
fn lang_option_is_normalized(
    #[with(&["--lang", "zh_cn"])] server: TestServer,
) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 200);
    let i18n = retrieve_i18n(&resp.text()?);
    assert_eq!(i18n["lang"], "zh-CN");
    Ok(())
}

#[rstest]
fn lang_query_param(#[with(&["--lang", "zh-CN"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?lang=en", server.url()))?;
    assert_eq!(resp.status(), 200);
    let i18n = retrieve_i18n(&resp.text()?);
    assert_eq!(i18n["lang"], "en");
    Ok(())
}

#[rstest]
fn lang_cookie(server: TestServer) -> Result<(), Error> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(server.url())
        .header("cookie", "dufs_lang=zh-CN")
        .send()?;
    assert_eq!(resp.status(), 200);
    let i18n = retrieve_i18n(&resp.text()?);
    assert_eq!(i18n["lang"], "zh-CN");
    Ok(())
}

#[rstest]
fn lang_accept_language(#[with(&["--lang", "auto"])] server: TestServer) -> Result<(), Error> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(server.url())
        .header("accept-language", "zh-CN,zh;q=0.9,en;q=0.8")
        .send()?;
    assert_eq!(resp.status(), 200);
    let i18n = retrieve_i18n(&resp.text()?);
    assert_eq!(i18n["lang"], "zh-CN");
    Ok(())
}

#[rstest]
fn noscript_page(#[with(&["--lang", "zh-CN"])] server: TestServer) -> Result<(), Error> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(server.url())
        .header("user-agent", "curl/8.7.1")
        .send()?;
    assert_eq!(resp.status(), 200);
    let text = resp.text()?;
    assert!(text.contains("<title>目录索引 /</title>"));
    Ok(())
}

#[rstest]
fn invalid_lang_option() {
    let output = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg("--lang")
        .arg("fr")
        .output()
        .expect("Couldn't run test binary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Invalid language `fr`"), "{stderr}");
}
