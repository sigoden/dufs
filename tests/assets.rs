mod fixtures;
mod utils;

use assert_cmd::prelude::*;
use assert_fs::fixture::TempDir;
use fixtures::{port, server, tmpdir, wait_for_port, Error, TestServer, DIR_ASSETS};
use rstest::rstest;
use std::process::{Command, Stdio};

#[rstest]
fn assets(server: TestServer) -> Result<(), Error> {
    let ver = env!("CARGO_PKG_VERSION");
    let resp = reqwest::blocking::get(server.url())?;
    let index_js = format!("/__dufs_v{ver}__/index.js");
    let index_css = format!("/__dufs_v{ver}__/index.css");
    let favicon_ico = format!("/__dufs_v{ver}__/favicon.ico");
    let text = resp.text()?;
    println!("{text}");
    assert!(text.contains(&format!(r#"href="{index_css}""#)));
    assert!(text.contains(&format!(r#"href="{favicon_ico}""#)));
    assert!(text.contains(&format!(r#"src="{index_js}""#)));
    Ok(())
}

#[rstest]
fn asset_js(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}__/index.js",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/javascript; charset=UTF-8"
    );
    Ok(())
}

#[rstest]
fn asset_css(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}__/index.css",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/css; charset=UTF-8"
    );
    Ok(())
}

#[rstest]
fn asset_ico(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}__/favicon.ico",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/x-icon");
    Ok(())
}

#[rstest]
fn assets_with_prefix(#[with(&["--path-prefix", "xyz"])] server: TestServer) -> Result<(), Error> {
    let ver = env!("CARGO_PKG_VERSION");
    let resp = reqwest::blocking::get(format!("{}xyz/", server.url()))?;
    let index_js = format!("/xyz/__dufs_v{ver}__/index.js");
    let index_css = format!("/xyz/__dufs_v{ver}__/index.css");
    let favicon_ico = format!("/xyz/__dufs_v{ver}__/favicon.ico");
    let text = resp.text()?;
    assert!(text.contains(&format!(r#"href="{index_css}""#)));
    assert!(text.contains(&format!(r#"href="{favicon_ico}""#)));
    assert!(text.contains(&format!(r#"src="{index_js}""#)));
    Ok(())
}

#[rstest]
fn asset_js_with_prefix(
    #[with(&["--path-prefix", "xyz"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!(
        "{}xyz/__dufs_v{}__/index.js",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/javascript; charset=UTF-8"
    );
    Ok(())
}

#[rstest]
fn assets_override(tmpdir: TempDir, port: u16) -> Result<(), Error> {
    let mut child = Command::cargo_bin("dufs")?
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .arg("--assets")
        .arg(tmpdir.join(DIR_ASSETS))
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(port);

    let url = format!("http://localhost:{port}");
    let resp = reqwest::blocking::get(&url)?;
    assert!(resp.text()?.starts_with(&format!(
        "/__dufs_v{}__/index.js;<template id=\"index-data\">",
        env!("CARGO_PKG_VERSION")
    )));
    let resp = reqwest::blocking::get(&url)?;
    assert_resp_paths!(resp);

    child.kill()?;
    Ok(())
}
