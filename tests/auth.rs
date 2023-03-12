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
fn auth_skip_on_options_method(
    #[with(&["--auth", "/@user:pass"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"OPTIONS", &url).send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn auth_check(
    #[with(&["--auth", "/@user:pass@user2:pass2", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"WRITEABLE", &url).send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"WRITEABLE", &url).send_with_digest_auth("user2", "pass2")?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"WRITEABLE", &url).send_with_digest_auth("user", "pass")?;
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
    #[with(&["--auth", "/@user:pass@user2:pass2", "--auth", "/dir1@user3:pass3", "-A"])]
    server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}dir1/file1", server.url());
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
    #[with(&["--auth", "/@user:pass@*", "--auth", "/dir1@user3:pass3", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
#[case(server(&["--auth", "/@user:pass", "--auth-method", "basic", "-A"]), "user", "pass")]
#[case(server(&["--auth", "/@u1:p1", "--auth-method", "basic", "-A"]), "u1", "p1")]
fn auth_basic(
    #[case] server: TestServer,
    #[case] user: &str,
    #[case] pass: &str,
) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .basic_auth(user, Some(pass))
        .send()?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn auth_webdav_move(
    #[with(&["--auth", "/@user:pass@*", "--auth", "/dir1@user3:pass3", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let origin_url = format!("{}dir1/test.html", server.url());
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"MOVE", &origin_url)
        .header("Destination", &new_url)
        .send_with_digest_auth("user3", "pass3")?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn auth_webdav_copy(
    #[with(&["--auth", "/@user:pass@*", "--auth", "/dir1@user3:pass3", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let origin_url = format!("{}dir1/test.html", server.url());
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"COPY", &origin_url)
        .header("Destination", &new_url)
        .send_with_digest_auth("user3", "pass3")?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn auth_path_prefix(
    #[with(&["--auth", "/@user:pass", "--path-prefix", "xyz", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}xyz/index.html", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"GET", &url).send_with_digest_auth("user", "pass")?;
    assert_eq!(resp.status(), 200);
    Ok(())
}
