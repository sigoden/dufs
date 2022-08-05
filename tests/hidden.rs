mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
#[case(server(&[] as &[&str]), true)]
#[case(server(&["--hidden", ".git,index.html"]), false)]
fn hidden_get_dir(#[case] server: TestServer, #[case] exist: bool) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert!(paths.contains("dira/"));
    assert_eq!(paths.contains(".git/"), exist);
    assert_eq!(paths.contains("index.html"), exist);
    Ok(())
}

#[rstest]
#[case(server(&[] as &[&str]), true)]
#[case(server(&["--hidden", "*.html"]), false)]
fn hidden_get_dir2(#[case] server: TestServer, #[case] exist: bool) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert!(paths.contains("dira/"));
    assert_eq!(paths.contains("index.html"), exist);
    assert_eq!(paths.contains("test.html"), exist);
    Ok(())
}

#[rstest]
#[case(server(&[] as &[&str]), true)]
#[case(server(&["--hidden", ".git,index.html"]), false)]
fn hidden_propfind_dir(#[case] server: TestServer, #[case] exist: bool) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", server.url()).send()?;
    assert_eq!(resp.status(), 207);
    let body = resp.text()?;
    assert!(body.contains("<D:href>/dira/</D:href>"));
    assert_eq!(body.contains("<D:href>/.git/</D:href>"), exist);
    assert_eq!(body.contains("<D:href>/index.html</D:href>"), exist);
    Ok(())
}

#[rstest]
#[case(server(&["--allow-search"] as &[&str]), true)]
#[case(server(&["--allow-search", "--hidden", ".git,test.html"]), false)]
fn hidden_search_dir(#[case] server: TestServer, #[case] exist: bool) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?q={}", server.url(), "test.html"))?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrieve_index_paths(&resp.text()?);
    for p in paths {
        assert_eq!(p.contains("test.html"), exist);
    }
    Ok(())
}
