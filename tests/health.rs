mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

const HEALTH_CHECK_PATH: &str = "__dufs__/health";
const HEALTH_CHECK_RESPONSE: &str = r#"{"status":"OK"}"#;

#[rstest]
fn normal_health(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{HEALTH_CHECK_PATH}", server.url()))?;
    assert_eq!(resp.text()?, HEALTH_CHECK_RESPONSE);
    Ok(())
}

#[rstest]
fn auth_health(
    #[with(&["--auth", "user:pass@/:rw", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{HEALTH_CHECK_PATH}", server.url()))?;
    assert_eq!(resp.text()?, HEALTH_CHECK_RESPONSE);
    Ok(())
}

#[rstest]
fn path_prefix_health(#[with(&["--path-prefix", "xyz"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}xyz/{HEALTH_CHECK_PATH}", server.url()))?;
    assert_eq!(resp.text()?, HEALTH_CHECK_RESPONSE);
    Ok(())
}
