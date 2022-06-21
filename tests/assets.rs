mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn assets(server: TestServer) -> Result<(), Error> {
    let ver = env!("CARGO_PKG_VERSION");
    let resp = reqwest::blocking::get(server.url())?;
    let index_js = format!("/__dufs_v{}_index.js", ver);
    let index_css = format!("/__dufs_v{}_index.css", ver);
    let favicon_ico = format!("/__dufs_v{}_favicon.ico", ver);
    let text = resp.text()?;
    assert!(text.contains(&format!(r#"href="{}""#, index_css)));
    assert!(text.contains(&format!(r#"href="{}""#, favicon_ico)));
    assert!(text.contains(&format!(r#"src="{}""#, index_js)));
    Ok(())
}

#[rstest]
fn asset_js(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}_index.js",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/javascript"
    );
    Ok(())
}

#[rstest]
fn asset_css(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}_index.css",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "text/css");
    Ok(())
}

#[rstest]
fn asset_ico(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}_favicon.ico",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/x-icon");
    Ok(())
}
