mod fixtures;
mod utils;

use chrono::{DateTime, Duration};
use fixtures::{server, Error, TestServer, FILES};
use reqwest::header::{ETAG, IF_MATCH, IF_NONE_MATCH, IF_UNMODIFIED_SINCE, LAST_MODIFIED};
use rstest::rstest;
use xml::escape::escape_str_pcdata;

fn file_etag(server: &TestServer, name: &str) -> Result<String, Error> {
    let resp = fetch!(b"HEAD", format!("{}{}", server.url(), name)).send()?;
    Ok(resp
        .headers()
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .expect("Received no valid etag header")
        .to_owned())
}

fn file_last_modified(server: &TestServer, name: &str) -> Result<String, Error> {
    let resp = fetch!(b"HEAD", format!("{}{}", server.url(), name)).send()?;
    Ok(resp
        .headers()
        .get(LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .expect("Received no valid last modified header")
        .to_owned())
}

#[rstest]
fn propfind_dir(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}dir1", server.url())).send()?;
    assert_eq!(resp.status(), 207);
    let body = resp.text()?;
    assert!(body.contains("<D:href>/dir1/</D:href>"));
    assert!(body.contains("<D:displayname>dir1</D:displayname>"));
    for f in FILES {
        assert!(body.contains(&format!("<D:href>/dir1/{}</D:href>", utils::encode_uri(f))));
        assert!(body.contains(&format!(
            "<D:displayname>{}</D:displayname>",
            escape_str_pcdata(f)
        )));
    }
    Ok(())
}

#[rstest]
fn propfind_dir_depth0(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}dir1", server.url()))
        .header("depth", "0")
        .send()?;
    assert_eq!(resp.status(), 207);
    let body = resp.text()?;
    assert!(body.contains("<D:href>/dir1/</D:href>"));
    assert!(body.contains("<D:displayname>dir1</D:displayname>"));
    assert_eq!(
        body.lines()
            .filter(|v| *v == "<D:status>HTTP/1.1 200 OK</D:status>")
            .count(),
        1
    );
    Ok(())
}

#[rstest]
fn propfind_dir_depth2(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}dir1", server.url()))
        .header("depth", "2")
        .send()?;
    assert_eq!(resp.status(), 400);
    let body = resp.text()?;
    assert_eq!(body, "Invalid depth: only 0 and 1 are allowed.");
    Ok(())
}

#[rstest]
fn propfind_dir_depth_infinity(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}dir1", server.url()))
        .header("depth", "infinity")
        .send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("application/xml; charset=utf-8")
    );
    let body = resp.text()?;
    assert!(body.contains("<D:propfind-finite-depth/>"));
    Ok(())
}

#[rstest]
fn propfind_404(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}404", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn propfind_double_slash(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", server.url()).send()?;
    assert_eq!(resp.status(), 207);
    Ok(())
}

#[rstest]
fn propfind_file(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 207);
    let body = resp.text()?;
    assert!(body.contains("<D:href>/test.html</D:href>"));
    assert!(body.contains("<D:displayname>test.html</D:displayname>"));
    assert_eq!(
        body.lines()
            .filter(|v| *v == "<D:status>HTTP/1.1 200 OK</D:status>")
            .count(),
        1
    );
    Ok(())
}

#[rstest]
fn proppatch_file(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPPATCH", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 207);
    let body = resp.text()?;
    assert!(body.contains("<D:href>/test.html</D:href>"));
    Ok(())
}

#[rstest]
fn proppatch_404(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPPATCH", format!("{}404", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn mkcol_dir(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"MKCOL", format!("{}newdir", server.url())).send()?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn mkcol_not_allow_upload(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"MKCOL", format!("{}newdir", server.url())).send()?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn mkcol_already_exists(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"MKCOL", format!("{}dir1", server.url())).send()?;
    assert_eq!(resp.status(), 405);
    Ok(())
}

#[rstest]
fn copy_file(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"COPY", format!("{}test.html", server.url()))
        .header("Destination", &new_url)
        .send()?;
    assert_eq!(resp.status(), 204);
    let resp = reqwest::blocking::get(new_url)?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn copy_not_allow_upload(server: TestServer) -> Result<(), Error> {
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"COPY", format!("{}test.html", server.url()))
        .header("Destination", &new_url)
        .send()?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn copy_no_overwrite_without_delete(
    #[with(&["--allow-upload"])] server: TestServer,
) -> Result<(), Error> {
    let dest_url = format!("{}test.html", server.url());
    let resp = fetch!(b"COPY", format!("{}test.txt", server.url()))
        .header("Destination", &dest_url)
        .send()?;
    assert_eq!(resp.status(), 403);
    let resp = reqwest::blocking::get(dest_url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is test.html");
    Ok(())
}

#[rstest]
fn copy_overwrite_false(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let dest_url = format!("{}test.html", server.url());
    let resp = fetch!(b"COPY", format!("{}test.txt", server.url()))
        .header("Destination", &dest_url)
        .header("Overwrite", "F")
        .send()?;
    assert_eq!(resp.status(), 412);
    let resp = reqwest::blocking::get(dest_url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is test.html");
    Ok(())
}

#[rstest]
fn copy_file_404(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"COPY", format!("{}404", server.url()))
        .header("Destination", &new_url)
        .send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn move_file(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let origin_url = format!("{}test.html", server.url());
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"MOVE", &origin_url)
        .header("Destination", &new_url)
        .send()?;
    assert_eq!(resp.status(), 204);
    let resp = reqwest::blocking::get(new_url)?;
    assert_eq!(resp.status(), 200);
    let resp = reqwest::blocking::get(origin_url)?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn move_not_allow_upload(#[with(&["--allow-delete"])] server: TestServer) -> Result<(), Error> {
    let origin_url = format!("{}test.html", server.url());
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"MOVE", &origin_url)
        .header("Destination", &new_url)
        .send()?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn move_not_allow_delete(#[with(&["--allow-upload"])] server: TestServer) -> Result<(), Error> {
    let origin_url = format!("{}test.html", server.url());
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"MOVE", &origin_url)
        .header("Destination", &new_url)
        .send()?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn move_overwrite_false(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let dest_url = format!("{}index.html", server.url());
    let resp = fetch!(b"MOVE", format!("{}test.txt", server.url()))
        .header("Destination", &dest_url)
        .header("Overwrite", "F")
        .send()?;
    assert_eq!(resp.status(), 412);
    let resp = reqwest::blocking::get(&dest_url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is index.html");
    let resp = reqwest::blocking::get(format!("{}test.txt", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is test.txt");
    Ok(())
}

#[rstest]
fn move_overwrite_false_missing_destination(
    #[with(&["-A"])] server: TestServer,
) -> Result<(), Error> {
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"MOVE", format!("{}test.html", server.url()))
        .header("Destination", &new_url)
        .header("Overwrite", "F")
        .send()?;
    assert_eq!(resp.status(), 204);
    let resp = reqwest::blocking::get(new_url)?;
    assert_eq!(resp.status(), 200);
    let resp = reqwest::blocking::get(format!("{}test.html", server.url()))?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn move_file_404(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"MOVE", format!("{}404", server.url()))
        .header("Destination", &new_url)
        .send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn lock_file(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    assert!(body.contains("<D:href>/test.html</D:href>"));
    Ok(())
}

#[rstest]
fn lock_file_404(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}404", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn unlock_file(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn unlock_file_404(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}404", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn put_if_match(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt", server.url());
    let etag = file_etag(&server, "test.txt")?;

    let resp = fetch!(b"PUT", &url)
        .header(IF_MATCH, "\"stale\"")
        .body("changed")
        .send()?;
    assert_eq!(resp.status(), 412);

    let resp = fetch!(b"PUT", &url)
        .header(IF_MATCH, etag.as_str())
        .body("changed")
        .send()?;
    assert_eq!(resp.status(), 201);
    let resp = reqwest::blocking::get(&url)?;
    assert_eq!(resp.text()?, "changed");
    Ok(())
}

#[rstest]
fn put_if_none_match(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt", server.url());

    let resp = fetch!(b"PUT", &url)
        .header(IF_NONE_MATCH, "*")
        .body("changed")
        .send()?;
    assert_eq!(resp.status(), 412);

    let new_url = format!("{}new.txt", server.url());
    let resp = fetch!(b"PUT", &new_url)
        .header(IF_NONE_MATCH, "*")
        .body("new")
        .send()?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn put_if_unmodified_since(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt", server.url());
    let last_modified = file_last_modified(&server, "test.txt")?;
    let modified_at = DateTime::parse_from_rfc2822(&last_modified)?;
    let stale = (modified_at - Duration::days(1))
        .format("%a, %d %b %Y %T GMT")
        .to_string();

    let resp = fetch!(b"PUT", &url)
        .header(IF_UNMODIFIED_SINCE, stale)
        .body("changed")
        .send()?;
    assert_eq!(resp.status(), 412);

    let resp = fetch!(b"PUT", &url)
        .header(IF_UNMODIFIED_SINCE, last_modified)
        .body("changed")
        .send()?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn delete_if_match(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt", server.url());
    let etag = file_etag(&server, "test.txt")?;

    let resp = fetch!(b"DELETE", &url)
        .header(IF_MATCH, "\"stale\"")
        .send()?;
    assert_eq!(resp.status(), 412);
    let resp = reqwest::blocking::get(&url)?;
    assert_eq!(resp.status(), 200);

    let resp = fetch!(b"DELETE", &url)
        .header(IF_MATCH, etag.as_str())
        .send()?;
    assert_eq!(resp.status(), 204);
    Ok(())
}

#[rstest]
fn copy_if_match(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt", server.url());
    let new_url = format!("{}test2.txt", server.url());
    let etag = file_etag(&server, "test.txt")?;

    let resp = fetch!(b"COPY", &url)
        .header("Destination", &new_url)
        .header(IF_MATCH, "\"stale\"")
        .send()?;
    assert_eq!(resp.status(), 412);
    let resp = reqwest::blocking::get(&new_url)?;
    assert_eq!(resp.status(), 404);

    let resp = fetch!(b"COPY", &url)
        .header("Destination", &new_url)
        .header(IF_MATCH, etag.as_str())
        .send()?;
    assert_eq!(resp.status(), 204);
    Ok(())
}

#[rstest]
fn move_if_match(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt", server.url());
    let new_url = format!("{}test2.txt", server.url());
    let etag = file_etag(&server, "test.txt")?;

    let resp = fetch!(b"MOVE", &url)
        .header("Destination", &new_url)
        .header(IF_MATCH, "\"stale\"")
        .send()?;
    assert_eq!(resp.status(), 412);
    let resp = reqwest::blocking::get(&url)?;
    assert_eq!(resp.status(), 200);

    let resp = fetch!(b"MOVE", &url)
        .header("Destination", &new_url)
        .header(IF_MATCH, etag.as_str())
        .send()?;
    assert_eq!(resp.status(), 204);
    Ok(())
}

#[rstest]
fn mkcol_if_match_missing(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"MKCOL", format!("{}newdir", server.url()))
        .header(IF_MATCH, "*")
        .send()?;
    assert_eq!(resp.status(), 412);
    Ok(())
}

#[rstest]
fn mkcol_if_none_match_missing(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"MKCOL", format!("{}newdir", server.url()))
        .header(IF_NONE_MATCH, "*")
        .send()?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn propfind_if_none_match(server: TestServer) -> Result<(), Error> {
    let etag = file_etag(&server, "test.html")?;
    let resp = fetch!(b"PROPFIND", format!("{}test.html", server.url()))
        .header(IF_NONE_MATCH, etag.as_str())
        .send()?;
    assert_eq!(resp.status(), 412);
    Ok(())
}

#[rstest]
fn proppatch_if_match(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPPATCH", format!("{}test.html", server.url()))
        .header(IF_MATCH, "\"stale\"")
        .send()?;
    assert_eq!(resp.status(), 412);
    Ok(())
}

#[rstest]
fn patch_if_match(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let etag = file_etag(&server, "test.txt")?;
    let resp = fetch!(b"PATCH", format!("{}test.txt", server.url()))
        .header("X-Update-Range", "bytes=0-")
        .header(IF_MATCH, etag.as_str())
        .body("changed")
        .send()?;
    assert_eq!(resp.status(), 204);
    Ok(())
}
