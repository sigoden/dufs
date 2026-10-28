mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

fn error_code(resp: &reqwest::blocking::Response) -> Option<&str> {
    resp.headers()
        .get("x-dufs-error-code")
        .and_then(|v| v.to_str().ok())
}

#[rstest]
fn invalid_path(#[with(&["--path-prefix", "/foo"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}bar", server.url())).send()?;
    assert_eq!(resp.status(), 400);
    assert_eq!(error_code(&resp), Some("invalid-path"));
    Ok(())
}

#[rstest]
fn invalid_destination(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"MOVE", format!("{}test.txt", server.url())).send()?;
    assert_eq!(resp.status(), 400);
    assert_eq!(error_code(&resp), Some("invalid-destination"));
    Ok(())
}

#[rstest]
fn invalid_depth(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}dir1/", server.url()))
        .header("Depth", "2")
        .send()?;
    assert_eq!(resp.status(), 400);
    assert_eq!(error_code(&resp), Some("invalid-depth"));
    Ok(())
}

#[rstest]
fn invalid_upload_offset(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.txt", server.url());
    let resp = fetch!(b"PATCH", &url)
        .header("X-Update-Range", "bogus")
        .body("abc")
        .send()?;
    assert_eq!(resp.status(), 400);
    assert_eq!(error_code(&resp), Some("invalid-upload-offset"));

    let resp = fetch!(b"PATCH", &url).body("abc").send()?;
    assert_eq!(resp.status(), 405);
    assert_eq!(error_code(&resp), Some("missing-upload-offset"));
    Ok(())
}

#[rstest]
fn upload_disabled(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PUT", format!("{}new.txt", server.url()))
        .body("abc")
        .send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), Some("upload-disabled"));
    Ok(())
}

#[rstest]
fn delete_disabled(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"DELETE", format!("{}test.txt", server.url())).send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), Some("delete-disabled"));
    Ok(())
}

#[rstest]
fn hash_disabled(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}test.txt?hash", server.url())).send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), Some("hash-disabled"));
    Ok(())
}

#[rstest]
fn put_directory(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PUT", format!("{}dir1/", server.url()))
        .body("abc")
        .send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), Some("is-a-directory"));
    Ok(())
}

#[rstest]
fn copy_directory(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"COPY", format!("{}dir1/", server.url()))
        .header("Destination", format!("{}copied", server.url()))
        .send()?;
    assert_eq!(resp.status(), 403);
    assert_eq!(error_code(&resp), Some("is-a-directory"));
    Ok(())
}

#[rstest]
fn mkcol_already_exists(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"MKCOL", format!("{}dir1/", server.url())).send()?;
    assert_eq!(resp.status(), 405);
    assert_eq!(error_code(&resp), Some("already-exists"));
    Ok(())
}

#[rstest]
fn method_not_allowed(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"FOO", server.url()).send()?;
    assert_eq!(resp.status(), 405);
    assert_eq!(error_code(&resp), Some("method-not-allowed"));
    Ok(())
}

#[rstest]
fn precondition_failed(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}test.txt", server.url()))
        .header("If-Match", "\"bogus\"")
        .send()?;
    assert_eq!(resp.status(), 412);
    assert_eq!(error_code(&resp), Some("precondition-failed"));
    Ok(())
}

#[rstest]
fn copy_destination_exists(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"COPY", format!("{}test.txt", server.url()))
        .header("Destination", format!("{}index.html", server.url()))
        .header("Overwrite", "F")
        .send()?;
    assert_eq!(resp.status(), 412);
    assert_eq!(error_code(&resp), Some("destination-exists"));
    Ok(())
}

#[rstest]
fn archive_disabled(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}?zip", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    assert_eq!(error_code(&resp), Some("archive-disabled"));
    Ok(())
}
