mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;

fn assert_dirs_first_and_reversed_within_groups(paths_asc: &[String], paths_desc: &[String]) {
    let dir_count = paths_asc.iter().filter(|v| v.ends_with('/')).count();
    // directories are grouped on top for both orders
    assert!(paths_asc[..dir_count].iter().all(|v| v.ends_with('/')));
    assert!(paths_desc[..dir_count].iter().all(|v| v.ends_with('/')));
    // within each group, desc is the reverse of asc
    let mut expected: Vec<String> = paths_asc[..dir_count].iter().rev().cloned().collect();
    expected.extend(paths_asc[dir_count..].iter().rev().cloned());
    assert_eq!(paths_desc, expected);
}

#[rstest]
fn ls_dir_sort_by_name(server: TestServer) -> Result<(), Error> {
    let url = server.url();
    let resp = reqwest::blocking::get(format!("{url}?sort=name&order=asc"))?;
    let paths1: Vec<String> = self::utils::retrieve_index_paths(&resp.text()?)
        .into_iter()
        .collect();
    let resp = reqwest::blocking::get(format!("{url}?sort=name&order=desc"))?;
    let paths2: Vec<String> = self::utils::retrieve_index_paths(&resp.text()?)
        .into_iter()
        .collect();
    assert_dirs_first_and_reversed_within_groups(&paths1, &paths2);
    Ok(())
}

#[rstest]
fn search_dir_sort_by_name(server: TestServer) -> Result<(), Error> {
    let url = server.url();
    let resp = reqwest::blocking::get(format!("{url}?q=test.html&sort=name&order=asc"))?;
    let paths1: Vec<String> = self::utils::retrieve_index_paths(&resp.text()?)
        .into_iter()
        .collect();
    let resp = reqwest::blocking::get(format!("{url}?q=test.html&sort=name&order=desc"))?;
    let paths2: Vec<String> = self::utils::retrieve_index_paths(&resp.text()?)
        .into_iter()
        .collect();
    assert_dirs_first_and_reversed_within_groups(&paths1, &paths2);
    Ok(())
}
