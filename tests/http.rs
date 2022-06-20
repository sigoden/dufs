mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn get_dir(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_index_resp!(resp);
    Ok(())
}

#[rstest]
fn head_dir(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", server.url()).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn get_dir_404(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}404/", server.url()))?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn head_dir_404(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}404/", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn get_dir_zip(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?zip", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/zip"
    );
    assert!(resp.headers().contains_key("content-disposition"));
    Ok(())
}

#[rstest]
fn head_dir_zip(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}?zip", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/zip"
    );
    assert!(resp.headers().contains_key("content-disposition"));
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn get_dir_search(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?q={}", server.url(), "test.html"))?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrive_index_paths(&resp.text()?);
    assert!(!paths.is_empty());
    for p in paths {
        assert!(p.contains(&"test.html"));
    }
    Ok(())
}

#[rstest]
fn get_dir_search2(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?q={}", server.url(), "😀.data"))?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrive_index_paths(&resp.text()?);
    assert!(!paths.is_empty());
    for p in paths {
        assert!(p.contains(&"😀.data"));
    }
    Ok(())
}

#[rstest]
fn head_dir_search(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}?q={}", server.url(), "test.html")).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn get_file(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}index.html", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "text/html");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert!(resp.headers().contains_key("etag"));
    assert!(resp.headers().contains_key("last-modified"));
    assert!(resp.headers().contains_key("content-length"));
    assert_eq!(resp.text()?, "This is index.html");
    Ok(())
}

#[rstest]
fn head_file(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}index.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "text/html");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert!(resp.headers().contains_key("content-disposition"));
    assert!(resp.headers().contains_key("etag"));
    assert!(resp.headers().contains_key("last-modified"));
    assert!(resp.headers().contains_key("content-length"));
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn get_file_404(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}404", server.url()))?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn head_file_404(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}404", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn options_dir(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"OPTIONS", format!("{}index.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("allow").unwrap(),
        "GET,HEAD,PUT,OPTIONS,DELETE,PROPFIND,COPY,MOVE"
    );
    assert_eq!(resp.headers().get("dav").unwrap(), "1,2");
    Ok(())
}

#[rstest]
fn put_file(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 201);
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn put_file_create_dir(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}xyz/file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 201);
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn put_file_conflict_dir(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}dira", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn delete_file(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.html", server.url());
    let resp = fetch!(b"DELETE", &url).send()?;
    assert_eq!(resp.status(), 204);
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn delete_file_404(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"DELETE", format!("{}file1", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}
