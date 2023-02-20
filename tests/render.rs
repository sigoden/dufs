mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer, BIN_FILE, DIR_NO_FOUND, DIR_NO_INDEX, FILES};
use rstest::rstest;

#[rstest]
fn render_index(#[with(&["--render-index"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    let text = resp.text()?;
    assert_eq!(text, "This is index.html");
    Ok(())
}

#[rstest]
fn render_index2(#[with(&["--render-index"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), DIR_NO_INDEX))?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn render_try_index(#[with(&["--render-try-index"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    let text = resp.text()?;
    assert_eq!(text, "This is index.html");
    Ok(())
}

#[rstest]
fn render_try_index2(#[with(&["--render-try-index"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), DIR_NO_INDEX))?;
    let files: Vec<&str> = FILES
        .iter()
        .filter(|v| **v != "index.html")
        .cloned()
        .collect();
    assert_resp_paths!(resp, files);
    Ok(())
}

#[rstest]
fn render_try_index3(
    #[with(&["--render-try-index", "--allow-archive"])] server: TestServer,
) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}?zip", server.url(), DIR_NO_INDEX))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/zip"
    );
    Ok(())
}

#[rstest]
#[case(server(&["--render-try-index"] as &[&str]), false)]
#[case(server(&["--render-try-index", "--allow-search"] as &[&str]), true)]
fn render_try_index4(#[case] server: TestServer, #[case] searched: bool) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}?q={}", server.url(), DIR_NO_INDEX, BIN_FILE))?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert_eq!(paths.iter().all(|v| v.contains(BIN_FILE)), searched);
    Ok(())
}

#[rstest]
fn render_spa(#[with(&["--render-spa"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    let text = resp.text()?;
    assert_eq!(text, "This is index.html");
    Ok(())
}

#[rstest]
fn render_spa2(#[with(&["--render-spa"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), DIR_NO_FOUND))?;
    let text = resp.text()?;
    assert_eq!(text, "This is index.html");
    Ok(())
}
