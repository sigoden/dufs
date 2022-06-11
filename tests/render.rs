mod fixtures;

use fixtures::{server, Error, TestServer, DIR_NO_INDEX};
use rstest::rstest;

#[rstest]
fn render_index(#[with(&["--render-index"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    let text = resp.text()?;
    assert_eq!(text, "This is index.html");
    Ok(())
}

#[rstest]
fn render_index_404(#[with(&["--render-index"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}/{}", server.url(), DIR_NO_INDEX))?;
    assert_eq!(resp.status(), 404);
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
fn render_spa_no_404(#[with(&["--render-spa"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}/{}", server.url(), DIR_NO_INDEX))?;
    let text = resp.text()?;
    assert_eq!(text, "This is index.html");
    Ok(())
}
