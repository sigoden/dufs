mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn cors(#[with(&["--cors"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;

    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        "*"
    );
    assert_eq!(
        resp.headers().get("access-control-allow-headers").unwrap(),
        "range, content-type, accept, origin, www-authenticate"
    );

    Ok(())
}

#[rstest]
fn cors_options(#[with(&["--cors"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"OPTIONS", server.url()).send()?;

    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        "*"
    );
    assert_eq!(
        resp.headers().get("access-control-allow-headers").unwrap(),
        "range, content-type, accept, origin, www-authenticate"
    );

    Ok(())
}
