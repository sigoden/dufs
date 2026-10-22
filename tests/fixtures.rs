use assert_fs::fixture::TempDir;
use assert_fs::prelude::*;
use port_check::free_local_port;
use reqwest::Url;
use rstest::fixture;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[allow(dead_code)]
pub type Error = Box<dyn std::error::Error>;

// `free_local_port` has to release the port before dufs can bind it, so two
// tests can pick the same port in that window (see #726). Ports are therefore
// registered here from the moment they are picked until the server using them
// has exited, and picking a port skips the ones already registered. The list
// only covers a single test binary (fixtures.rs is compiled into each one),
// which is enough because `cargo test` runs test binaries one at a time.
#[allow(dead_code)]
static RESERVED_PORTS: Mutex<Vec<u16>> = Mutex::new(Vec::new());

#[allow(dead_code)]
pub const BIN_FILE: &str = "😀.bin";

/// File names for testing purpose
#[allow(dead_code)]
pub static FILES: &[&str] = &[
    "test.txt",
    "test.html",
    "index.html",
    #[cfg(not(target_os = "windows"))]
    "file\n1.txt",
    BIN_FILE,
];

/// Directory names for testing directory don't exist
#[allow(dead_code)]
pub static DIR_NO_FOUND: &str = "dir-no-found/";

/// Directory names for testing directory don't have index.html
#[allow(dead_code)]
pub static DIR_NO_INDEX: &str = "dir-no-index/";

/// Directory names for testing hidden
#[allow(dead_code)]
pub static DIR_GIT: &str = ".git/";

/// Directory names for testings assets override
#[allow(dead_code)]
pub static DIR_ASSETS: &str = "dir-assets/";

/// Directory names for testing purpose
#[allow(dead_code)]
pub static DIRECTORIES: &[&str] = &["dir1/", "dir2/", "dir3/", DIR_NO_INDEX, DIR_GIT, DIR_ASSETS];

/// Test fixture which creates a temporary directory with a few files and directories inside.
/// The directories also contain files.
#[fixture]
#[allow(dead_code)]
pub fn tmpdir() -> TempDir {
    let tmpdir = assert_fs::TempDir::new().expect("Couldn't create a temp dir for tests");
    for file in FILES {
        if *file == BIN_FILE {
            tmpdir.child(file).write_binary(b"bin\0\x00123").unwrap();
        } else {
            tmpdir
                .child(file)
                .write_str(&format!("This is {file}"))
                .unwrap();
        }
    }
    for directory in DIRECTORIES {
        if *directory == DIR_ASSETS {
            tmpdir
                .child(format!("{}{}", directory, "index.html"))
                .write_str("__ASSETS_PREFIX__index.js;<template id=\"index-data\">__INDEX_DATA__</template>")
                .unwrap();
        } else {
            for file in FILES {
                if *directory == DIR_NO_INDEX && *file == "index.html" {
                    continue;
                }
                if *file == BIN_FILE {
                    tmpdir
                        .child(format!("{directory}{file}"))
                        .write_binary(b"bin\0\x00123")
                        .unwrap();
                } else {
                    tmpdir
                        .child(format!("{directory}{file}"))
                        .write_str(&format!("This is {directory}{file}"))
                        .unwrap();
                }
            }
        }
    }
    tmpdir.child("dir4/hidden").touch().unwrap();
    tmpdir
        .child("content-types/bin.tar")
        .write_binary(b"\x7f\x45\x4c\x46\x02\x01\x00\x00")
        .unwrap();
    tmpdir
        .child("content-types/bin")
        .write_binary(b"\x7f\x45\x4c\x46\x02\x01\x00\x00")
        .unwrap();
    tmpdir
        .child("content-types/file-utf8.txt")
        .write_str("世界")
        .unwrap();
    tmpdir
        .child("content-types/file-gbk.txt")
        .write_binary(b"\xca\xc0\xbd\xe7")
        .unwrap();
    tmpdir
        .child("content-types/file")
        .write_str("世界")
        .unwrap();

    tmpdir
}

/// A port picked for a single test and kept out of circulation until dropped.
#[allow(dead_code)]
pub struct Port {
    num: u16,
}

impl Port {
    fn reserve() -> Port {
        let mut reserved = RESERVED_PORTS.lock().unwrap_or_else(|e| e.into_inner());
        for _ in 0..100 {
            let num = free_local_port().expect("Couldn't find a free local port");
            if !reserved.contains(&num) {
                reserved.push(num);
                return Port { num };
            }
        }
        panic!("Couldn't find a free local port");
    }
}

impl Drop for Port {
    fn drop(&mut self) {
        let mut reserved = RESERVED_PORTS.lock().unwrap_or_else(|e| e.into_inner());
        reserved.retain(|num| *num != self.num);
    }
}

impl std::fmt::Display for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.num.fmt(f)
    }
}

/// Pick a free port that no other test is using.
#[fixture]
#[allow(dead_code)]
pub fn port() -> Port {
    Port::reserve()
}

/// Run dufs as a server; Start with a temporary directory, a free port and some
/// optional arguments then wait for a while for the server setup to complete.
#[fixture]
#[allow(dead_code)]
pub fn server<I>(#[default(&[] as &[&str])] args: I) -> TestServer
where
    I: IntoIterator + Clone,
    I::Item: AsRef<std::ffi::OsStr>,
{
    let tmpdir = tmpdir();
    let port = port();
    let child = Command::new(assert_cmd::cargo::cargo_bin!())
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

    wait_for_port(&port);
    TestServer::new(port, tmpdir, child, is_tls)
}

/// Whether the host can actually use IPv6.
#[allow(dead_code)]
pub fn is_ipv6_available() -> bool {
    std::net::TcpListener::bind("[::]:0").is_ok() && std::net::TcpListener::bind("[::1]:0").is_ok()
}

/// Wait a max of 2s for the server on `port` to become reachable.
pub fn wait_for_port(port: &Port) {
    let start_wait = Instant::now();

    while !port_check::is_port_reachable(format!("localhost:{port}")) {
        sleep(Duration::from_millis(250));

        if start_wait.elapsed().as_secs() > 2 {
            panic!("timeout waiting for port {port}");
        }
    }
}

#[allow(dead_code)]
pub struct TestServer {
    port: Port,
    tmpdir: TempDir,
    child: Child,
    is_tls: bool,
}

#[allow(dead_code)]
impl TestServer {
    pub fn new(port: Port, tmpdir: TempDir, child: Child, is_tls: bool) -> Self {
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
        self.port.num
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.child.kill().expect("Couldn't kill test server");
        self.child.wait().unwrap();
    }
}
