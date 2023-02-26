mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn cors(#[with(&["--enable-cors"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        "*"
    );
    assert_eq!(
        resp.headers()
            .get("access-control-allow-credentials")
            .unwrap(),
        "true"
    );
    assert_eq!(
        resp.headers().get("access-control-allow-methods").unwrap(),
        "GET,HEAD,PUT,OPTIONS,DELETE,PROPFIND,COPY,MOVE"
    );
    assert_eq!(
        resp.headers().get("access-control-allow-headers").unwrap(),
        "Authorization,Destination,Range,Content-Type"
    );
    assert_eq!(
        resp.headers().get("access-control-expose-headers").unwrap(),
        "WWW-Authenticate,Content-Range,Accept-Ranges,Content-Disposition"
    );
    Ok(())
}
