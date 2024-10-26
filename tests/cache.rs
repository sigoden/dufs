mod fixtures;
mod utils;

use chrono::{DateTime, Duration};
use fixtures::{server, Error, TestServer};
use reqwest::header::{
    HeaderName, ETAG, IF_MATCH, IF_MODIFIED_SINCE, IF_NONE_MATCH, IF_UNMODIFIED_SINCE,
    LAST_MODIFIED,
};
use reqwest::StatusCode;
use rstest::rstest;

#[rstest]
#[case(IF_UNMODIFIED_SINCE, Duration::days(1), StatusCode::OK)]
#[case(IF_UNMODIFIED_SINCE, Duration::days(0), StatusCode::OK)]
#[case(IF_UNMODIFIED_SINCE, Duration::days(-1), StatusCode::PRECONDITION_FAILED)]
#[case(IF_MODIFIED_SINCE, Duration::days(1), StatusCode::NOT_MODIFIED)]
#[case(IF_MODIFIED_SINCE, Duration::days(0), StatusCode::NOT_MODIFIED)]
#[case(IF_MODIFIED_SINCE, Duration::days(-1), StatusCode::OK)]
fn get_file_with_if_modified_since_condition(
    #[case] header_condition: HeaderName,
    #[case] duration_after_file_modified: Duration,
    #[case] expected_code: StatusCode,
    server: TestServer,
) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}index.html", server.url())).send()?;

    let last_modified = resp
        .headers()
        .get(LAST_MODIFIED)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| DateTime::parse_from_rfc2822(s).ok())
        .expect("Received no valid last modified header");

    let req_modified_time = (last_modified + duration_after_file_modified)
        .format("%a, %d %b %Y %T GMT")
        .to_string();

    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header(header_condition, req_modified_time)
        .send()?;

    assert_eq!(resp.status(), expected_code);
    Ok(())
}

fn same_etag(etag: &str) -> String {
    etag.to_owned()
}

fn different_etag(etag: &str) -> String {
    format!("{}1234", etag)
}

#[rstest]
#[case(IF_MATCH, same_etag, StatusCode::OK)]
#[case(IF_MATCH, different_etag, StatusCode::PRECONDITION_FAILED)]
#[case(IF_NONE_MATCH, same_etag, StatusCode::NOT_MODIFIED)]
#[case(IF_NONE_MATCH, different_etag, StatusCode::OK)]
fn get_file_with_etag_match(
    #[case] header_condition: HeaderName,
    #[case] etag_modifier: fn(&str) -> String,
    #[case] expected_code: StatusCode,
    server: TestServer,
) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}index.html", server.url())).send()?;

    let etag = resp
        .headers()
        .get(ETAG)
        .and_then(|h| h.to_str().ok())
        .expect("Received no valid etag header");

    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header(header_condition, etag_modifier(etag))
        .send()?;

    assert_eq!(resp.status(), expected_code);
    Ok(())
}
