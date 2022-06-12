mod fixtures;
mod utils;

use assert_cmd::Command;
use fixtures::{server, Error, TestServer};
use predicates::str::contains;
use reqwest::blocking::ClientBuilder;
use rstest::rstest;

/// Can start the server with TLS and receive encrypted responses.
#[rstest]
#[case(server(&[
        "--tls-cert", "tests/data/cert.pem",
        "--tls-key", "tests/data/key_pkcs8.pem",
]))]
#[case(server(&[
        "--tls-cert", "tests/data/cert.pem",
        "--tls-key", "tests/data/key_pkcs1.pem",
]))]
fn tls_works(#[case] server: TestServer) -> Result<(), Error> {
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let resp = client.get(server.url()).send()?.error_for_status()?;
    assert_index_resp!(resp);
    Ok(())
}

/// Wrong path for cert throws error.
#[rstest]
fn wrong_path_cert() -> Result<(), Error> {
    Command::cargo_bin("duf")?
        .args(&["--tls-cert", "wrong", "--tls-key", "tests/data/key.pem"])
        .assert()
        .failure()
        .stderr(contains("error: Failed to access `wrong`"));

    Ok(())
}

/// Wrong paths for key throws errors.
#[rstest]
fn wrong_path_key() -> Result<(), Error> {
    Command::cargo_bin("duf")?
        .args(&["--tls-cert", "tests/data/cert.pem", "--tls-key", "wrong"])
        .assert()
        .failure()
        .stderr(contains("error: Failed to access `wrong`"));

    Ok(())
}
