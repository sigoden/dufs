mod fixtures;
mod utils;

use assert_cmd::prelude::*;
use assert_fs::fixture::TempDir;
use fixtures::{port, server, tmpdir, Error, TestServer};
use rstest::rstest;
use std::process::{Command, Stdio};

#[rstest]
fn path_prefix_index(#[with(&["--path-prefix", "xyz"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), "xyz"))?;
    assert_index_resp!(resp);
    Ok(())
}

#[rstest]
fn path_prefix_file(#[with(&["--path-prefix", "xyz"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}/index.html", server.url(), "xyz"))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is index.html");
    Ok(())
}

#[rstest]
fn path_prefix_propfind(
    #[with(&["--path-prefix", "xyz"])] server: TestServer,
) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}{}", server.url(), "xyz")).send()?;
    let text = resp.text()?;
    assert!(text.contains("<D:href>/xyz/</D:href>"));
    Ok(())
}

#[rstest]
#[case("index.html")]
fn serve_single_file(tmpdir: TempDir, port: u16, #[case] file: &str) -> Result<(), Error> {
    let mut child = Command::cargo_bin("duf")?
        .env("RUST_LOG", "false")
        .arg(tmpdir.path().join(file))
        .arg("-p")
        .arg(port.to_string())
        .stdout(Stdio::piped())
        .spawn()?;

    let resp = reqwest::blocking::get(format!("http://localhost:{}/index.html", port))?;
    assert_eq!(resp.text()?, "This is index.html");

    child.kill()?;
    Ok(())
}
