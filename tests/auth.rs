mod fixtures;
mod utils;

use diqwest::blocking::WithDigestAuth;
use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn no_auth(#[with(&["--auth", "user:pass", "-A"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 401);
    assert!(resp.headers().contains_key("www-authenticate"));
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[rstest]
fn auth(#[with(&["--auth", "user:pass", "-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .send_with_digest_auth("user", "pass")?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn auth_skip_access(
    #[with(&["--auth", "user:pass", "--no-auth-access"])] server: TestServer,
) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 200);
    Ok(())
}
