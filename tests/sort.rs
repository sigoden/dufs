mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

#[rstest]
fn ls_dir_sort_by_name(server: TestServer) -> Result<(), Error> {
    let url = server.url();
    let resp = reqwest::blocking::get(format!("{url}?sort=name&order=asc"))?;
    let paths1 = self::utils::retrieve_index_paths(&resp.text()?);
    let resp = reqwest::blocking::get(format!("{url}?sort=name&order=desc"))?;
    let mut paths2 = self::utils::retrieve_index_paths(&resp.text()?);
    paths2.reverse();
    assert_eq!(paths1, paths2);
    Ok(())
}

#[rstest]
fn search_dir_sort_by_name(server: TestServer) -> Result<(), Error> {
    let url = server.url();
    let resp = reqwest::blocking::get(format!("{url}?q=test.html&sort=name&order=asc"))?;
    let paths1 = self::utils::retrieve_index_paths(&resp.text()?);
    let resp = reqwest::blocking::get(format!("{url}?q=test.html&sort=name&order=desc"))?;
    let mut paths2 = self::utils::retrieve_index_paths(&resp.text()?);
    paths2.reverse();
    assert_eq!(paths1, paths2);
    Ok(())
}
