mod fixtures;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn default_no_upload(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn default_no_delete(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn default_no_symlink(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn default_none_exist_dir(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn allow_upload(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn allow_upload_no_delete(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn allow_upload_no_override(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn allow_upload_none_exist_dir(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn allow_delete(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn allow_delete_no_upload(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn allow_uplad_delete_can_override(server: TestServer) -> Result<(), Error> {
    Ok(())
}

#[rstest]
fn allow_symlink(server: TestServer) -> Result<(), Error> {
    Ok(())
}
