mod fixtures;
mod utils;

use diqwest::blocking::WithDigestAuth;
use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn no_auth(#[with(&["--auth", "/@user:pass", "-A"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 401);
    assert!(resp.headers().contains_key("www-authenticate"));
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[rstest]
fn auth(#[with(&["--auth", "/@user:pass", "-A"])] server: TestServer) -> Result<(), Error> {
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
fn auth_skip(#[with(&["--auth", "/@user:pass@*"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn auth_readonly(
    #[with(&["--auth", "/@user:pass@user2:pass2", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"GET", &url).send_with_digest_auth("user2", "pass2")?;
    assert_eq!(resp.status(), 200);
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .send_with_digest_auth("user2", "pass2")?;
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[rstest]
fn auth_nest(
    #[with(&["--auth", "/@user:pass@user2:pass2", "--auth", "/dira@user3:pass3", "-A"])]
    server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}dira/file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .send_with_digest_auth("user3", "pass3")?;
    assert_eq!(resp.status(), 201);
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .send_with_digest_auth("user", "pass")?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn auth_nest_share(
    #[with(&["--auth", "/@user:pass@*", "--auth", "/dira@user3:pass3", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn no_auth(#[with(&["--basic-auth", "--auth", "/@user:pass", "-A"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 401);
    assert!(resp.headers().contains_key("www-authenticate"));
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[rstest]
fn auth(#[with(&["--basic-auth", "--auth", "/@user:pass", "-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .send_with_basic_auth("user", "pass")?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn auth_skip(#[with(&["--basic-auth", "--auth", "/@user:pass@*"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn auth_readonly(
    #[with(&["--basic-auth", "--auth", "/@user:pass@user2:pass2", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"GET", &url).send_with_basic_auth("user2", "pass2")?;
    assert_eq!(resp.status(), 200);
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .send_with_basic_auth("user2", "pass2")?;
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[rstest]
fn auth_nest(
    #[with(&["--basic-auth", "--auth", "/@user:pass@user2:pass2", "--auth", "/dira@user3:pass3", "-A"])]
    server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}dira/file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .send_with_basic_auth("user3", "pass3")?;
    assert_eq!(resp.status(), 201);
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .send_with_basic_auth("user", "pass")?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn auth_nest_share(
    #[with(&["--basic-auth", "--auth", "/@user:pass@*", "--auth", "/dira@user3:pass3", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}
