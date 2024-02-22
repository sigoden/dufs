//! Run file server with different args

mod fixtures;
mod utils;

use assert_cmd::prelude::*;
use assert_fs::fixture::TempDir;
use fixtures::{port, tmpdir, wait_for_port, Error};
use rstest::rstest;
use std::process::{Command, Stdio};

#[rstest]
#[case("index.html")]
fn single_file(tmpdir: TempDir, port: u16, #[case] file: &str) -> Result<(), Error> {
    let mut child = Command::cargo_bin("dufs")?
        .arg(tmpdir.path().join(file))
        .arg("-p")
        .arg(port.to_string())
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(port);

    let resp = reqwest::blocking::get(format!("http://localhost:{port}"))?;
    assert_eq!(resp.text()?, "This is index.html");
    let resp = reqwest::blocking::get(format!("http://localhost:{port}/"))?;
    assert_eq!(resp.text()?, "This is index.html");
    let resp = reqwest::blocking::get(format!("http://localhost:{port}/index.html"))?;
    assert_eq!(resp.text()?, "This is index.html");

    child.kill()?;
    Ok(())
}

#[rstest]
#[case("index.html")]
fn path_prefix_single_file(tmpdir: TempDir, port: u16, #[case] file: &str) -> Result<(), Error> {
    let mut child = Command::cargo_bin("dufs")?
        .arg(tmpdir.path().join(file))
        .arg("-p")
        .arg(port.to_string())
        .arg("--path-prefix")
        .arg("xyz")
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(port);

    let resp = reqwest::blocking::get(format!("http://localhost:{port}/xyz"))?;
    assert_eq!(resp.text()?, "This is index.html");
    let resp = reqwest::blocking::get(format!("http://localhost:{port}/xyz/"))?;
    assert_eq!(resp.text()?, "This is index.html");
    let resp = reqwest::blocking::get(format!("http://localhost:{port}/xyz/index.html"))?;
    assert_eq!(resp.text()?, "This is index.html");
    let resp = reqwest::blocking::get(format!("http://localhost:{port}"))?;
    assert_eq!(resp.status(), 400);

    child.kill()?;
    Ok(())
}
