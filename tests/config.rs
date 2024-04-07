mod digest_auth_util;
mod fixtures;
mod utils;

use assert_cmd::prelude::*;
use assert_fs::TempDir;
use digest_auth_util::send_with_digest_auth;
use fixtures::{port, tmpdir, wait_for_port, Error};
use rstest::rstest;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[rstest]
fn use_config_file(tmpdir: TempDir, port: u16) -> Result<(), Error> {
    let config_path = get_config_path().display().to_string();
    let mut child = Command::cargo_bin("dufs")?
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(["--config", &config_path])
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(port);

    let url = format!("http://localhost:{port}/dufs/index.html");
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 401);

    let url = format!("http://localhost:{port}/dufs/index.html");
    let resp = send_with_digest_auth(fetch!(b"GET", &url), "user", "pass")?;
    assert_eq!(resp.text()?, "This is index.html");

    let url = format!("http://localhost:{port}/dufs?simple");
    let resp = send_with_digest_auth(fetch!(b"GET", &url), "user", "pass")?;
    let text: String = resp.text().unwrap();
    assert!(text.split('\n').any(|c| c == "dir1/"));
    assert!(!text.split('\n').any(|c| c == "dir3/"));
    assert!(!text.split('\n').any(|c| c == "test.txt"));

    let url = format!("http://localhost:{port}/dufs/dir1/upload.txt");
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body("Hello"), "user", "pass")?;
    assert_eq!(resp.status(), 201);

    child.kill()?;
    Ok(())
}

fn get_config_path() -> PathBuf {
    let mut path = std::env::current_dir().expect("Failed to get current directory");
    path.push("tests");
    path.push("data");
    path.push("config.yaml");
    path
}
