mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer, FILES};
use rstest::rstest;
use xml::escape::escape_str_pcdata;

#[rstest]
fn propfind_dir(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}dira", server.url())).send()?;
    assert_eq!(resp.status(), 207);
    let body = resp.text()?;
    assert!(body.contains("<D:href>/dira</D:href>"));
    assert!(body.contains("<D:displayname>dira</D:displayname>"));
    for f in FILES {
        assert!(body.contains(&format!("<D:href>/dira/{}</D:href>", utils::encode_uri(f))));
        assert!(body.contains(&format!(
            "<D:displayname>{}</D:displayname>",
            escape_str_pcdata(f)
        )));
    }
    Ok(())
}

#[rstest]
fn propfind_dir_depth0(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}dira", server.url()))
        .header("depth", "0")
        .send()?;
    assert_eq!(resp.status(), 207);
    let body = resp.text()?;
    assert!(body.contains("<D:href>/dira</D:href>"));
    assert!(body.contains("<D:displayname>dira</D:displayname>"));
    assert_eq!(
        body.lines()
            .filter(|v| *v == "<D:status>HTTP/1.1 200 OK</D:status>")
            .count(),
        1
    );
    Ok(())
}

#[rstest]
fn propfind_404(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}404", server.url())).send()?;
    assert_eq!(resp.status(), 404);
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
fn copy_file_404(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"COPY", format!("{}404", server.url()))
        .header("Destination", &new_url)
        .send()?;
    assert_eq!(resp.status(), 405);
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
fn move_file_404(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let new_url = format!("{}test2.html", server.url());
    let resp = fetch!(b"MOVE", format!("{}404", server.url()))
        .header("Destination", &new_url)
        .send()?;
    assert_eq!(resp.status(), 405);
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
