mod fixtures;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn auth_upload(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn auth_delete(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn auth_get(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn skip_access_auth(server: TestServer) -> Result<(), Error> {
    Ok(())
}
