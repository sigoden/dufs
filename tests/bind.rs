mod fixtures;

use fixtures::{port, server, tmpdir, wait_for_port, Error, Port, TestServer};

use assert_cmd::prelude::*;
use assert_fs::fixture::TempDir;
use regex::Regex;
use rstest::rstest;
use std::io::Read;
use std::process::{Command, Stdio};

#[rstest]
#[case(&["-b", "20.205.243.166"])]
fn bind_fails(tmpdir: TempDir, port: Port, #[case] args: &[&str]) -> Result<(), Error> {
    Command::new(assert_cmd::cargo::cargo_bin!())
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
fn bind_fails_when_port_in_use(server: TestServer, tmpdir: TempDir) -> Result<(), Error> {
    // `server` already holds the port; a second instance must not bind it.
    Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(server.port().to_string())
        .assert()
        .stderr(predicates::str::contains("Failed to bind"))
        .failure();

    // And the first instance keeps serving requests.
    reqwest::blocking::get(server.url())?.error_for_status()?;

    Ok(())
}

#[rstest]
#[case(&[] as &[&str], true, true)]
#[case(&["-b", "0.0.0.0"], true, false)]
#[case(&["-b", "127.0.0.1", "-b", "::1"], true, true)]
fn bind_ipv4_ipv6(
    #[case] args: &[&str],
    #[case] bind_ipv4: bool,
    #[case] bind_ipv6: bool,
) -> Result<(), Error> {
    let ipv6_available = fixtures::is_ipv6_available();
    let explicit_ipv6 = args
        .iter()
        .filter_map(|arg| arg.parse::<std::net::IpAddr>().ok())
        .any(|ip| ip.is_ipv6());
    if explicit_ipv6 && !ipv6_available {
        // An explicitly requested IPv6 address cannot be bound on this host.
        return Ok(());
    }

    let server = server(args);
    assert_eq!(
        reqwest::blocking::get(format!("http://127.0.0.1:{}", server.port()).as_str()).is_ok(),
        bind_ipv4
    );
    assert_eq!(
        reqwest::blocking::get(format!("http://[::1]:{}", server.port()).as_str()).is_ok(),
        bind_ipv6 && ipv6_available
    );

    Ok(())
}

#[rstest]
#[case(&[] as &[&str])]
#[case(&["--path-prefix", "/prefix"])]
fn validate_printed_urls(tmpdir: TempDir, port: Port, #[case] args: &[&str]) -> Result<(), Error> {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(args)
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(&port);

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

/// Send a minimal HTTP/1.1 request over `stream` and return its status line.
#[cfg(unix)]
fn raw_http_status_line<S>(stream: &mut S) -> Result<String, Error>
where
    S: std::io::Read + std::io::Write,
{
    stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp)?;
    Ok(resp.lines().next().unwrap_or_default().to_string())
}

#[cfg(unix)]
#[rstest]
fn bind_unix_socket(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    let sock_path = tmpdir.path().join("dufs.sock");
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .arg("-b")
        .arg(&sock_path)
        .stdout(Stdio::piped())
        .spawn()?;

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut stream = loop {
        match UnixStream::connect(&sock_path) {
            Ok(stream) => break stream,
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(err) => {
                child.kill()?;
                return Err(format!("failed to connect to {}: {err}", sock_path.display()).into());
            }
        }
    };
    assert_eq!(raw_http_status_line(&mut stream)?, "HTTP/1.1 200 OK");

    child.kill()?;
    Ok(())
}

/// A `@name` bind address selects a Linux abstract socket. Abstract sockets are
/// not unlinked before binding, so a duplicated address makes the second bind
/// fail with `EADDRINUSE` instead of silently going through.
#[cfg(any(target_os = "linux", target_os = "android"))]
#[rstest]
fn bind_abstract_unix_socket(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixStream};
    use std::time::{Duration, Instant};

    let name = format!("dufs-test-{}-{port}", std::process::id());
    let addr = SocketAddr::from_abstract_name(name.as_bytes())?;

    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .arg("-b")
        .arg(format!("@{name}"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut stream = loop {
        match UnixStream::connect_addr(&addr) {
            Ok(stream) => break stream,
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(err) => {
                child.kill()?;
                return Err(
                    format!("failed to connect to abstract socket `@{name}`: {err}").into(),
                );
            }
        }
    };
    assert_eq!(raw_http_status_line(&mut stream)?, "HTTP/1.1 200 OK");

    child.kill()?;
    Ok(())
}
