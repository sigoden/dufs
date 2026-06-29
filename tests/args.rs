//! Run file server with different args

mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn path_prefix_index(#[with(&["--path-prefix", "xyz"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), "xyz"))?;
    assert_resp_paths!(resp);
    Ok(())
}

#[rstest]
fn path_prefix_file(#[with(&["--path-prefix", "xyz"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}/index.html", server.url(), "xyz"))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is index.html");
    Ok(())
}

#[rstest]
fn path_prefix_reject_same_component(
    #[with(&["--path-prefix", "xyz"])] server: TestServer,
) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}xyzpublic.txt", server.url()))?;
    assert_eq!(resp.status(), 400);
    Ok(())
}

#[rstest]
fn path_prefix_reject_extra_component_text(
    #[with(&["--path-prefix", "xyz"])] server: TestServer,
) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}xyzevil/public.txt", server.url()))?;
    assert_eq!(resp.status(), 400);
    Ok(())
}

#[rstest]
fn path_prefix_propfind(
    #[with(&["--path-prefix", "xyz"])] server: TestServer,
) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", format!("{}{}", server.url(), "xyz")).send()?;
    let text = resp.text()?;
    assert!(text.contains("<D:href>/xyz/</D:href>"));
    Ok(())
}
