mod fixtures;

use fixtures::{port, server, tmpdir, wait_for_port, Error, TestServer};

use assert_cmd::prelude::*;
use assert_fs::fixture::TempDir;
use regex::Regex;
use rstest::rstest;
use std::io::Read;
use std::process::{Command, Stdio};

#[rstest]
#[case(&["-b", "20.205.243.166"])]
fn bind_fails(tmpdir: TempDir, port: u16, #[case] args: &[&str]) -> Result<(), Error> {
    Command::cargo_bin("dufs")?
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(args)
        .assert()
        .stderr(predicates::str::contains("Failed to bind"))
        .failure();

    Ok(())
}

#[rstest]
#[case(server(&[] as &[&str]), true, true)]
#[case(server(&["-b", "0.0.0.0"]), true, false)]
#[case(server(&["-b", "127.0.0.1", "-b", "::1"]), true, true)]
fn bind_ipv4_ipv6(
    #[case] server: TestServer,
    #[case] bind_ipv4: bool,
    #[case] bind_ipv6: bool,
) -> Result<(), Error> {
    assert_eq!(
        reqwest::blocking::get(format!("http://127.0.0.1:{}", server.port()).as_str()).is_ok(),
        bind_ipv4
    );
    assert_eq!(
        reqwest::blocking::get(format!("http://[::1]:{}", server.port()).as_str()).is_ok(),
        bind_ipv6
    );

    Ok(())
}

#[rstest]
#[case(&[] as &[&str])]
#[case(&["--path-prefix", "/prefix"])]
fn validate_printed_urls(tmpdir: TempDir, port: u16, #[case] args: &[&str]) -> Result<(), Error> {
    let mut child = Command::cargo_bin("dufs")?
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(args)
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(port);

    let stdout = child.stdout.as_mut().expect("Failed to get stdout");
    let mut buf = [0; 1000];
    let buf_len = stdout.read(&mut buf)?;
    let output = std::str::from_utf8(&buf[0..buf_len])?;
    let url_lines = output
        .lines()
        .take_while(|line| !line.is_empty()) /* non-empty lines */
        .collect::<Vec<_>>()
        .join("\n");

    let urls = Regex::new(r"http://[a-zA-Z0-9\.\[\]:/]+")
        .unwrap()
        .captures_iter(url_lines.as_str())
        .filter_map(|caps| caps.get(0).map(|v| v.as_str()))
        .collect::<Vec<_>>();

    assert!(!urls.is_empty());
    reqwest::blocking::get(urls[0])?.error_for_status()?;

    child.kill()?;

    Ok(())
}
