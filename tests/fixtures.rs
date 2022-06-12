use assert_cmd::prelude::*;
use assert_fs::fixture::TempDir;
use assert_fs::prelude::*;
use port_check::free_local_port;
use reqwest::Url;
use rstest::fixture;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[allow(dead_code)]
pub type Error = Box<dyn std::error::Error>;

/// File names for testing purpose
#[allow(dead_code)]
pub static FILES: &[&str] = &[
    "test.txt",
    "test.html",
    "index.html",
    "test.mkv",
    #[cfg(not(windows))]
    "test \" \' & < >.csv",
    "😀.data",
    "⎙.mp4",
    "#[]{}()@!$&'`+,;= %20.test",
    #[cfg(unix)]
    ":?#[]{}<>()@!$&'`|*+,;= %20.test",
    #[cfg(not(windows))]
    "foo\\bar.test",
];

/// Directory names for testing purpose
#[allow(dead_code)]
pub static DIR_NO_INDEX: &str = "dir-no-index/";

/// Directory names for testing purpose
#[allow(dead_code)]
pub static DIRECTORIES: &[&str] = &["dira/", "dirb/", "dirc/", DIR_NO_INDEX];

/// Name of a deeply nested file
#[allow(dead_code)]
pub static DEEPLY_NESTED_FILE: &str = "very/deeply/nested/test.rs";

/// Test fixture which creates a temporary directory with a few files and directories inside.
/// The directories also contain files.
#[fixture]
#[allow(dead_code)]
pub fn tmpdir() -> TempDir {
    let tmpdir = assert_fs::TempDir::new().expect("Couldn't create a temp dir for tests");
    for file in FILES {
        tmpdir
            .child(file)
            .write_str(&format!("This is {}", file))
            .expect("Couldn't write to file");
    }
    for directory in DIRECTORIES {
        for file in FILES {
            if *directory == DIR_NO_INDEX {
                continue;
            }
            tmpdir
                .child(format!("{}{}", directory, file))
                .write_str(&format!("This is {}{}", directory, file))
                .expect("Couldn't write to file");
        }
    }

    tmpdir
        .child(&DEEPLY_NESTED_FILE)
        .write_str("File in a deeply nested directory.")
        .expect("Couldn't write to file");
    tmpdir
}

/// Get a free port.
#[fixture]
#[allow(dead_code)]
pub fn port() -> u16 {
    free_local_port().expect("Couldn't find a free local port")
}

/// Run miniserve as a server; Start with a temporary directory, a free port and some
/// optional arguments then wait for a while for the server setup to complete.
#[fixture]
#[allow(dead_code)]
pub fn server<I>(#[default(&[] as &[&str])] args: I) -> TestServer
where
    I: IntoIterator + Clone,
    I::Item: AsRef<std::ffi::OsStr>,
{
    let port = port();
    let tmpdir = tmpdir();
    let child = Command::cargo_bin("duf")
        .expect("Couldn't find test binary")
        .env("RUST_LOG", "false")
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(args.clone())
        .stdout(Stdio::null())
        .spawn()
        .expect("Couldn't run test binary");
    let is_tls = args
        .into_iter()
        .any(|x| x.as_ref().to_str().unwrap().contains("tls"));

    wait_for_port(port);
    TestServer::new(port, tmpdir, child, is_tls)
}

/// Same as `server()` but ignore stderr
#[fixture]
#[allow(dead_code)]
pub fn server_no_stderr<I>(#[default(&[] as &[&str])] args: I) -> TestServer
where
    I: IntoIterator + Clone,
    I::Item: AsRef<std::ffi::OsStr>,
{
    let port = port();
    let tmpdir = tmpdir();
    let child = Command::cargo_bin("duf")
        .expect("Couldn't find test binary")
        .env("RUST_LOG", "false")
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .args(args.clone())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Couldn't run test binary");
    let is_tls = args
        .into_iter()
        .any(|x| x.as_ref().to_str().unwrap().contains("tls"));

    wait_for_port(port);
    TestServer::new(port, tmpdir, child, is_tls)
}

/// Wait a max of 1s for the port to become available.
fn wait_for_port(port: u16) {
    let start_wait = Instant::now();

    while !port_check::is_port_reachable(format!("localhost:{}", port)) {
        sleep(Duration::from_millis(100));

        if start_wait.elapsed().as_secs() > 1 {
            panic!("timeout waiting for port {}", port);
        }
    }
}

#[allow(dead_code)]
pub struct TestServer {
    port: u16,
    tmpdir: TempDir,
    child: Child,
    is_tls: bool,
}

#[allow(dead_code)]
impl TestServer {
    pub fn new(port: u16, tmpdir: TempDir, child: Child, is_tls: bool) -> Self {
        Self {
            port,
            tmpdir,
            child,
            is_tls,
        }
    }

    pub fn url(&self) -> Url {
        let protocol = if self.is_tls { "https" } else { "http" };
        Url::parse(&format!("{}://localhost:{}", protocol, self.port)).unwrap()
    }

    pub fn path(&self) -> &std::path::Path {
        self.tmpdir.path()
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.child.kill().expect("Couldn't kill test server");
        self.child.wait().unwrap();
    }
}
