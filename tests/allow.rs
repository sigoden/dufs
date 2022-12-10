mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn default_not_allow_upload(server: TestServer) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn default_not_allow_delete(server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.html", server.url());
    let resp = fetch!(b"DELETE", &url).send()?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn default_not_allow_archive(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?zip", server.url()))?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn default_not_exist_dir(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}404/", server.url()))?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn allow_upload_not_exist_dir(
    #[with(&["--allow-upload"])] server: TestServer,
) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}404/", server.url()))?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn allow_upload_no_override(#[with(&["--allow-upload"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn allow_delete_no_override(#[with(&["--allow-delete"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn allow_upload_delete_can_override(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn allow_search(#[with(&["--allow-search"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?q={}", server.url(), "test.html"))?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert!(!paths.is_empty());
    for p in paths {
        assert!(p.contains("test.html"));
    }
    Ok(())
}

#[rstest]
fn allow_archive(#[with(&["--allow-archive"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?zip", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/zip"
    );
    assert!(resp.headers().contains_key("content-disposition"));
    Ok(())
}
