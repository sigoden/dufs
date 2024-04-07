mod digest_auth_util;
mod fixtures;
mod utils;

use digest_auth_util::send_with_digest_auth;
use fixtures::{port, tmpdir, wait_for_port, Error};

use assert_cmd::prelude::*;
use assert_fs::fixture::TempDir;
use rstest::rstest;
use std::io::Read;
use std::process::{Command, Stdio};

#[rstest]
#[case(&["-a", "user:pass@/:rw", "--log-format", "$remote_user"], false)]
#[case(&["-a", "user:pass@/:rw", "--log-format", "$remote_user"], true)]
fn log_remote_user(
    tmpdir: TempDir,
    port: u16,
    #[case] args: &[&str],
    #[case] is_basic: bool,
) -> Result<(), Error> {
    let mut child = Command::cargo_bin("dufs")?
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(args)
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(port);

    let stdout = child.stdout.as_mut().expect("Failed to get stdout");

    let req_builder = fetch!(b"GET", &format!("http://localhost:{port}"));

    let resp = if is_basic {
        req_builder.basic_auth("user", Some("pass")).send()?
    } else {
        send_with_digest_auth(req_builder, "user", "pass")?
    };

    assert_eq!(resp.status(), 200);

    let mut buf = [0; 2048];
    let buf_len = stdout.read(&mut buf)?;
    let output = std::str::from_utf8(&buf[0..buf_len])?;

    assert!(output.lines().last().unwrap().ends_with("user"));

    child.kill()?;
    Ok(())
}

#[rstest]
#[case(&["--log-format", ""])]
fn no_log(tmpdir: TempDir, port: u16, #[case] args: &[&str]) -> Result<(), Error> {
    let mut child = Command::cargo_bin("dufs")?
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(args)
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(port);

    let stdout = child.stdout.as_mut().expect("Failed to get stdout");

    let resp = fetch!(b"GET", &format!("http://localhost:{port}")).send()?;
    assert_eq!(resp.status(), 200);

    let mut buf = [0; 2048];
    let buf_len = stdout.read(&mut buf)?;
    let output = std::str::from_utf8(&buf[0..buf_len])?;

    assert_eq!(output.lines().last().unwrap(), "");

    child.kill()?;
    Ok(())
}
