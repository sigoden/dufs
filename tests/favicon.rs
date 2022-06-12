mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn default_favicon(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}favicon.ico", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/x-icon");
    Ok(())
}

#[rstest]
fn exist_favicon(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}favicon.ico", server.url());
    let data = b"abc";
    let resp = fetch!(b"PUT", &url).body(data.to_vec()).send()?;
    assert_eq!(resp.status(), 201);
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes()?, data.to_vec());
    Ok(())
}
